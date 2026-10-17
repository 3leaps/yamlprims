//! File/stdin handling and presentation over the shared YAML core.
#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use yamlprims::{Error, MergeKeys, Options, Site};

const USAGE: &str = "usage: yamlprims [--merge=reject|apply|as_key] check FILE...\n       yamlprims [--merge=reject|apply|as_key] dump [--plain] FILE\nUse - for stdin, and -- before filenames beginning with -.";

struct Command {
    check: bool,
    plain: bool,
    files: Vec<PathBuf>,
    opts: Options,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Option<Command> {
    let mut command = None;
    let mut plain = false;
    let mut merge = None;
    let mut files = Vec::new();
    let mut literal = false;
    for arg in args {
        if !literal && arg == "--" {
            literal = true;
            continue;
        }
        if !literal && arg == "--plain" {
            if plain {
                return None;
            }
            plain = true;
        } else if !literal && arg.to_str().is_some_and(|a| a.starts_with("--merge=")) {
            if merge.is_some() {
                return None;
            }
            merge = Some(match arg.to_str()?.strip_prefix("--merge=")? {
                "reject" => MergeKeys::Reject,
                "apply" => MergeKeys::Apply,
                "as_key" => MergeKeys::AsKey,
                _ => return None,
            });
        } else if command.is_none() {
            command = Some(match arg.to_str()? {
                "check" => true,
                "dump" => false,
                _ => return None,
            });
        } else {
            if !literal && arg != "-" && arg.as_encoded_bytes().starts_with(b"-") {
                return None;
            }
            files.push(PathBuf::from(arg));
        }
    }
    let check = command?;
    if files.is_empty()
        || (!check && files.len() != 1)
        || (check && plain)
        || files.iter().filter(|p| p.as_os_str() == "-").count() > 1
    {
        return None;
    }
    let mut opts = Options::default();
    opts.merge_keys = merge.unwrap_or(MergeKeys::Reject);
    Some(Command {
        check,
        plain,
        files,
        opts,
    })
}

/// Read one byte beyond the raw-input budget so the core reports MaxInputBytes.
fn read_limited(reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read(path: &Path, opts: &Options) -> io::Result<Vec<u8>> {
    if path.as_os_str() == "-" {
        read_limited(io::stdin().lock(), opts.limits.max_input_bytes)
    } else {
        read_limited(std::fs::File::open(path)?, opts.limits.max_input_bytes)
    }
}

fn report(path: &Path, e: &Error) -> String {
    let path = path.display();
    match e.site {
        Site::Source(l) => format!(
            "{path}:{}:{}: {}: {}",
            l.line,
            l.column,
            e.kind.name(),
            e.message
        ),
        Site::NotSource { .. } => format!("{path}: {}: {}", e.kind.name(), e.message),
    }
}

fn diagnostic(message: impl std::fmt::Display) {
    let _ = writeln!(io::stderr().lock(), "{message}");
}

/// Serialization has completed before this function is called. An I/O error can
/// still leave a prefix on the destination; it is reported as a failed command.
fn emit(mut writer: impl Write, text: &str) -> io::Result<()> {
    writer.write_all(text.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn dump(path: &Path, bytes: &[u8], command: &Command, writer: impl Write) -> u8 {
    let opts = &command.opts;
    let (text, status) = match yamlprims::load_all_bytes(bytes, opts) {
        Ok(docs) => {
            let result = if command.plain {
                match docs.as_slice() {
                    [doc] => yamlprims::encode_plain(doc.root(), opts),
                    _ => {
                        diagnostic(format!(
                            "{}: --plain needs exactly one document",
                            path.display()
                        ));
                        return 1;
                    }
                }
            } else {
                yamlprims::encode(&docs, opts)
            };
            match result {
                Ok(text) => (text, 0),
                Err(e) => {
                    diagnostic(report(path, &e));
                    return 1;
                }
            }
        }
        Err(e) => {
            diagnostic(report(path, &e));
            if command.plain {
                return 1;
            }
            match yamlprims::encode_error(&e, opts) {
                Ok(text) => (text, 1),
                Err(encoding) => {
                    diagnostic(report(path, &encoding));
                    return 1;
                }
            }
        }
    };
    if let Err(e) = emit(writer, &text) {
        diagnostic(format!("cannot write stdout: {e}"));
        return 2;
    }
    status
}

fn run(command: Command) -> u8 {
    let mut status = 0;
    for path in &command.files {
        let bytes = match read(path, &command.opts) {
            Ok(bytes) => bytes,
            Err(e) => {
                diagnostic(format!("{}: cannot read: {e}", path.display()));
                status = if command.check { 1 } else { 2 };
                continue;
            }
        };
        if command.check {
            if let Err(e) = yamlprims::load_all_bytes(&bytes, &command.opts) {
                diagnostic(report(path, &e));
                status = status.max(1);
            }
        } else {
            return dump(path, &bytes, &command, io::stdout().lock());
        }
    }
    status
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return ExitCode::from(match emit(io::stdout().lock(), USAGE) {
            Ok(()) => 0,
            Err(e) => {
                diagnostic(format!("cannot write stdout: {e}"));
                2
            }
        });
    }
    match parse(args) {
        Some(command) => ExitCode::from(run(command)),
        None => {
            diagnostic(USAGE);
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_stops_one_byte_past_limit() {
        let mut reader = &b"0123456789"[..];
        assert_eq!(read_limited(&mut reader, 3).unwrap(), b"0123");
        assert_eq!(reader, b"456789");
        assert!(read_limited(&b""[..], 3).unwrap().is_empty());
    }

    #[test]
    fn read_failure_is_not_a_partial_input() {
        struct FailingReader(bool);
        impl Read for FailingReader {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    bytes[0] = b'a';
                    Ok(1)
                } else {
                    Err(io::Error::other("read failed"))
                }
            }
        }
        assert!(read_limited(FailingReader(false), 3).is_err());
    }

    #[test]
    fn failed_encoding_emits_no_prefix_or_empty_fallback() {
        let mut command = parse([OsString::from("dump"), OsString::from("-")]).unwrap();
        command.opts.limits.max_output_bytes = 1;
        for input in [b"a: 1".as_slice(), b"[".as_slice()] {
            let mut output = Vec::new();
            assert_eq!(dump(Path::new("-"), input, &command, &mut output), 1);
            assert!(output.is_empty());
        }
    }

    #[test]
    fn output_failure_is_propagated() {
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(emit(FailingWriter, "{}").is_err());
        let command = parse([OsString::from("dump"), OsString::from("-")]).unwrap();
        assert_eq!(dump(Path::new("-"), b"true", &command, FailingWriter), 2);
    }
}
