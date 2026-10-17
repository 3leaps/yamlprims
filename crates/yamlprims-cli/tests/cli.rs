use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "yamlprims-cli-{}-{time}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: impl AsRef<Path>, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run(args: &[impl AsRef<OsStr>], input: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_yamlprims"))
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(bytes) = input {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    child.wait_with_output().unwrap()
}
fn status(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
}
fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn wire_dump_empty_and_multidocument_stdin_matches_core() {
    for input in [b"".as_slice(), b"---\n...\n---\n", b"a: 1\n---\n[2]\n"] {
        let output = run(&["dump", "-"], Some(input));
        status(&output, 0);
        assert!(output.stderr.is_empty());
        let opts = yamlprims::Options::default();
        let docs = yamlprims::load_all_bytes(input, &opts).unwrap();
        let expected = yamlprims::encode(&docs, &opts).unwrap() + "\n";
        assert_eq!(output.stdout, expected.as_bytes());
        let value = json(&output);
        assert_eq!(value["documents"].as_array().unwrap().len(), docs.len());
    }
}

#[test]
fn wire_failure_uses_core_envelope_and_fails() {
    for input in [b"[".as_slice(), &[0xff], b"a: 1\na: 2\n"] {
        let output = run(&["dump", "-"], Some(input));
        status(&output, 1);
        assert!(!output.stderr.is_empty());
        let opts = yamlprims::Options::default();
        let error = yamlprims::load_all_bytes(input, &opts).unwrap_err();
        let expected = yamlprims::encode_error(&error, &opts).unwrap() + "\n";
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(json(&output)["error"].is_object());
    }
}

#[test]
fn plain_rejects_cardinality_parse_and_loss_without_stdout() {
    for input in [
        b"".as_slice(),
        b"1\n---\n2",
        b"[",
        b"!kind value",
        b".nan",
        &[0xff],
    ] {
        let output = run(&["dump", "--plain", "-"], Some(input));
        status(&output, 1);
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(!output.stderr.is_empty());
    }
    let output = run(&["dump", "--plain", "-"], Some(b"a: [true, 2]\n"));
    status(&output, 0);
    assert_eq!(json(&output), serde_json::json!({"a": [true, 2]}));
    assert!(output.stderr.is_empty());
}

#[test]
fn check_continues_across_files_and_reports_source_locations() {
    let files = Files::new();
    let empty = files.write("empty.yaml", b"");
    let bad = files.write("bad.yaml", b"key: [\n");
    let duplicate = files.write("duplicate.yaml", b"a: 1\na: 2\n");
    let output = run(
        &[
            OsStr::new("check"),
            empty.as_os_str(),
            OsStr::new("-"),
            bad.as_os_str(),
            duplicate.as_os_str(),
        ],
        Some(b"1\n---\n2"),
    );
    status(&output, 1);
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&format!("{}:1:6: syntax", bad.display())),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("{}:2:1: duplicate_key", duplicate.display())),
        "{stderr}"
    );
    assert!(!stderr.contains(&empty.display().to_string()));
    status(&run(&["check", "-"], Some(b"")), 0);
}

#[test]
fn io_failure_has_no_stdout_and_check_still_processes_later_files() {
    let files = Files::new();
    let missing = files.0.join("missing.yaml");
    let bad = files.write("bad.yaml", b"a: 1\na: 2\n");
    for mode in ["check", "dump"] {
        let output = run(&[OsStr::new(mode), missing.as_os_str()], None);
        status(&output, if mode == "check" { 1 } else { 2 });
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let output = run(
        &[OsStr::new("check"), missing.as_os_str(), bad.as_os_str()],
        None,
    );
    status(&output, 1);
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot read"));
    assert!(stderr.contains("duplicate_key"));
}

#[test]
fn oversized_file_fails_size_gate_before_invalid_utf8() {
    let files = Files::new();
    let mut input = vec![b' '; yamlprims::Options::default().limits.max_input_bytes + 2];
    input[0] = 0xff;
    let path = files.write("oversized.yaml", &input);
    let output = run(&[OsStr::new("dump"), path.as_os_str()], None);
    status(&output, 1);
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(stderr.contains("max_input_bytes"), "{stderr}");
    assert!(!stderr.contains("invalid_utf8"), "{stderr}");
    assert!(json(&output)["error"].is_object());
}

#[test]
fn merge_policies_and_strict_parsing_use_the_core() {
    let input = b"a: &a {x: 1}\nb: {<<: *a}\n";
    status(&run(&["check", "-"], Some(input)), 1);
    for mode in ["--merge=apply", "--merge=as_key"] {
        status(&run(&["check", mode, "-"], Some(input)), 0);
    }
    let output = run(&["dump", "--merge=apply", "--plain", "-"], Some(input));
    status(&output, 0);
    assert_eq!(
        json(&output),
        serde_json::json!({"a": {"x": 1}, "b": {"x": 1}})
    );
    status(&run(&["check", "-"], Some(b"a: [\n1,\n2]\n")), 1);
}

#[test]
fn usage_rejects_extra_dump_inputs_unknown_flags_and_repeated_stdin() {
    for args in [
        vec![],
        vec!["check"],
        vec!["dump"],
        vec!["other", "-"],
        vec!["dump", "a", "b"],
        vec!["dump", "--plain"],
        vec!["check", "--plain", "-"],
        vec!["check", "--unknown", "-"],
        vec!["check", "--merge=no", "-"],
        vec!["check", "-", "-"],
        vec!["dump", "--plain", "--plain", "-"],
        vec!["dump", "--merge=apply", "--merge=reject", "-"],
        vec!["check", "--indentation=compatible", "-"],
    ] {
        let output = run(&args, None);
        status(&output, 2);
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(String::from_utf8(output.stderr).unwrap().contains("usage:"));
    }
    let output = run(&["--help"], None);
    status(&output, 0);
    assert!(output.stdout.starts_with(b"usage:"));
    assert!(output.stderr.is_empty());
}

#[test]
fn double_dash_admits_option_shaped_filename() {
    let files = Files::new();
    files.write("--plain", b"yes\n");
    let output = Command::new(env!("CARGO_BIN_EXE_yamlprims"))
        .current_dir(&files.0)
        .args(["dump", "--plain", "--", "--plain"])
        .output()
        .unwrap();
    status(&output, 0);
    assert_eq!(json(&output), "yes");
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_filename_is_opened_without_string_conversion() {
    use std::os::unix::ffi::OsStrExt;
    let files = Files::new();
    let path = files.write(OsStr::from_bytes(b"name-\xff.yaml"), b"true");
    let output = run(
        &[OsStr::new("dump"), OsStr::new("--plain"), path.as_os_str()],
        None,
    );
    status(&output, 0);
    assert_eq!(json(&output), true);
}

#[test]
fn unicode_filename_with_spaces_is_opened() {
    let files = Files::new();
    let path = files.write("données test.yaml", b"true");
    let output = run(
        &[OsStr::new("dump"), OsStr::new("--plain"), path.as_os_str()],
        None,
    );
    status(&output, 0);
    assert_eq!(json(&output), true);
}
