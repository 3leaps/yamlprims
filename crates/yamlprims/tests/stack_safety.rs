//! Stack safety of parser-created trees : at the largest admitted
//! depth, loading, cleanup and the tree operations finish on a 2 MiB thread, in debug and
//! release builds, and one level more is LimitExceeded(MaxDepth). A stack overflow aborts
//! the test binary, which fails the run. These regressions cover parser-created trees only, not arbitrary hand-built depth.

use yamlprims::*;

fn flow_seq(d: usize) -> String {
    format!("{}1{}", "[".repeat(d), "]".repeat(d))
}

/// Inputs `d` collection levels deep (the root counts as one).
fn shapes(d: usize) -> Vec<(&'static str, String)> {
    let mut block_map = String::new();
    for i in 0..d {
        block_map += &" ".repeat(i);
        block_map += if i + 1 == d { "a: 1\n" } else { "a:\n" };
    }
    vec![
        ("flow_seq", flow_seq(d)),
        (
            "flow_map",
            format!("{}1{}", "{a: ".repeat(d), "}".repeat(d)),
        ),
        ("block_seq", format!("{}1\n", "- ".repeat(d))),
        ("block_map", block_map),
        ("tagged", format!("{}1{}", "!t [".repeat(d), "]".repeat(d))),
        ("alias", format!("a: &x {}\nb: *x\n", flow_seq(d - 1))),
        (
            "merge",
            format!("a: &x {{k: {}}}\nb: {{<<: *x}}\n", flow_seq(d - 3)),
        ),
        ("key", format!("? {}\n: 1\n", flow_seq(d - 1))),
    ]
}

fn opts(max_depth: usize) -> Options {
    let mut o = Options::default();
    o.limits.max_depth = max_depth;
    o.merge_keys = MergeKeys::Apply;
    o
}

fn on_2mib<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn check(max_depth: usize) {
    for (shape, text) in shapes(max_depth) {
        let t = text.clone();
        on_2mib(move || {
            let o = opts(max_depth);
            let d = load_with(&t, &o).unwrap_or_else(|e| panic!("{shape}: {e}"));
            let copy = d.root().clone();
            assert!(key_eq(copy.value(), d.root().value()));
            let _ = format!("{copy:?}");
            encode(std::slice::from_ref(&d), &o).unwrap();
            let _: Value = from_node(d.root(), &o).unwrap();
            drop((copy, d));
        });
    }
    for (shape, text) in shapes(max_depth + 1) {
        let e = on_2mib(move || load_with(&text, &opts(max_depth)).unwrap_err());
        assert_eq!(
            e.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxDepth,
                configured: max_depth as u64
            },
            "{shape}"
        );
    }
}

#[test]
fn default_depth_on_2mib_thread() {
    check(128);
}

#[test]
fn largest_depth_on_2mib_thread() {
    check(255);
}

/// The iterative builder unwinds the error path as the recursive one did: completed
/// siblings and their subtrees leave no segments behind.
#[test]
fn error_path_after_completed_siblings() {
    let e = load("- [1, [2]]\n- {k: [3]}\n- {a: 1, a: 2}\n").unwrap_err();
    assert_eq!(e.kind, ErrorKind::DuplicateKey);
    assert!(
        matches!(&e.path[..], [PathSegment::Index(2)]),
        "{:?}",
        e.path
    );
    let e = load("x: [1, [2]]\ny: {z: [0, {a: 1, a: 2}]}\n").unwrap_err();
    assert_eq!(e.kind, ErrorKind::DuplicateKey);
    let keys: Vec<String> = e
        .path
        .iter()
        .map(|p| match p {
            PathSegment::Key(k) => k.as_str().unwrap().to_string(),
            PathSegment::Index(i) => i.to_string(),
        })
        .collect();
    assert_eq!(keys, ["y", "z", "1"]);
}
