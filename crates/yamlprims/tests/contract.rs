//! Contract tests for the scalar and key contract (+ errata): locations, budgets, anchors,
//! documents, typed paths, and wire format.

use serde::Deserialize;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use yamlprims::*;

// ---- allocation instrumentation ------------------------------
struct Counting;
static TRACK: AtomicBool = AtomicBool::new(false);
static THRESHOLD: AtomicUsize = AtomicUsize::new(usize::MAX);
static BIG: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) && l.size() >= THRESHOLD.load(Ordering::Relaxed) {
            BIG.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) && n >= THRESHOLD.load(Ordering::Relaxed) && l.size() < n {
            BIG.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;
static ALLOC_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn opts() -> Options {
    Options::default()
}

fn err(r: Result<Document, Error>) -> Error {
    r.expect_err("expected an error")
}

fn src(e: &Error) -> Location {
    match e.site {
        Site::Source(l) => l,
        Site::NotSource { .. } => panic!("expected a source site: {e:?}"),
    }
}

/// Independent reference scan (LF, CRLF, lone CR; columns in chars; BOM not a column).
fn reference(input: &str, byte: usize) -> (usize, usize) {
    let (bom, body) = match input.strip_prefix('\u{feff}') {
        Some(b) => (3, b),
        None => (0, input),
    };
    let prefix = &body[..byte - bom];
    let (mut line, mut col) = (1, 1);
    let mut it = prefix.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\n' || c == '\r' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(n);
    match n.value() {
        Value::Sequence(s) => s.iter().for_each(|c| walk(c, out)),
        Value::Mapping(m) => m.iter().for_each(|(k, v)| {
            walk(k, out);
            walk(v, out)
        }),
        Value::Tagged(t) => walk(t.value(), out),
        _ => {}
    }
}

/// a parsed node's span lies inside its parent's, except for alias copies and
/// merged-in entries, which keep their definition spans.
fn check_contained(n: &Node, parent: Option<Span>, input: &str) {
    let mut own = None;
    if let NodeSpan::Source(s) = n.span() {
        // A copy is exempt only from its insertion parent; its children still lie inside
        // its definition span.
        if let (Some(p), None) = (parent, n.via()) {
            assert!(
                p.start.byte_offset <= s.start.byte_offset
                    && s.end.byte_offset <= p.end.byte_offset,
                "{input:?}: {s:?} outside {p:?}"
            );
        }
        own = Some(*s);
    }
    match n.value() {
        Value::Sequence(items) => items.iter().for_each(|c| check_contained(c, own, input)),
        Value::Mapping(m) => m.iter().for_each(|(k, v)| {
            check_contained(k, own, input);
            check_contained(v, own, input);
        }),
        Value::Tagged(t) => check_contained(t.value(), own, input),
        _ => {}
    }
}

fn check_locations(input: &str) {
    let docs = load_all(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
    for d in &docs {
        check_contained(d.root(), None, input);
        let mut nodes = Vec::new();
        walk(d.root(), &mut nodes);
        for n in nodes {
            let NodeSpan::Source(s) = n.span() else {
                panic!("parsed node without source span")
            };
            assert!(
                s.start.byte_offset <= s.end.byte_offset && s.end.byte_offset <= input.len(),
                "{input:?} {s:?}"
            );
            assert!(
                input.is_char_boundary(s.start.byte_offset)
                    && input.is_char_boundary(s.end.byte_offset),
                "{input:?} {s:?}"
            );
            for l in [s.start, s.end] {
                assert_eq!(
                    reference(input, l.byte_offset),
                    (l.line, l.column),
                    "{input:?} at {l:?}"
                );
            }
            if n.via().is_none() {
                if let Value::String(t) = n.value() {
                    // Plain single-line scalars: the source slice is the text.
                    if !t.is_empty()
                        && !t.contains(['\n', '"', '\'', '#', '\\'])
                        && n.tag_start().is_none()
                    {
                        let slice = &input[s.start.byte_offset..s.end.byte_offset];
                        if !slice.starts_with(['"', '\'', '|', '>']) {
                            assert_eq!(slice, t, "{input:?}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn locations_over_unicode_bom_crlf_tabs() {
    for input in [
        "a: 1\nb: [x, y]\nc: {d: e}\n",
        "\u{feff}k: v\nk2: v2\n",
        "k: v\r\nk2: v2\r\n",
        "k: v\rk2: v2\r",
        "k:\tv\n",
        "ключ: значение\n名前: 値\n",
        "k: \"é\"\nl: 'ü'\n",
        "- 😀 a\n- b\n",
        "a:\nb: ~\n",
        "? |\n  key\n: v\n",
        "a: &x [1, 2]\nb: *x\n",
        "k: !custom v\n",
        "---\na: 1\n---\nb: 2\n",
    ] {
        check_locations(input);
    }
}

#[test]
fn empty_and_comment_only_input() {
    for input in ["", "# c\n", "\u{feff}", "  \n"] {
        let d = load(input).unwrap();
        let NodeSpan::Source(s) = d.root().span() else {
            panic!()
        };
        assert_eq!(s.start, s.end);
        assert_eq!(s.start.byte_offset, input.len(), "{input:?}");
        assert!(matches!(d.root().value(), Value::Null));
    }
}

#[test]
fn invalid_utf8_location() {
    let e = load_bytes(b"\xEF\xBB\xBFa: 1\nk: \xC3\x28\n", &opts()).unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::InvalidUtf8 {
            invalid_len: Some(1)
        }
    );
    assert_eq!(
        src(&e),
        Location {
            byte_offset: 3 + 5 + 3,
            line: 2,
            column: 4
        }
    );
    let e = load_bytes(b"k: \xE2\x82", &opts()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidUtf8 { invalid_len: None });
}

#[test]
fn eof_syntax_error_location() {
    let input = "a: [1, 2\n";
    let e = err(load(input));
    assert!(matches!(e.kind, ErrorKind::Syntax(_)), "{e:?}");
    let l = src(&e);
    assert!(l.byte_offset <= input.len());
    assert_eq!(reference(input, l.byte_offset), (l.line, l.column));
}

#[test]
fn syntax_error_takes_precedence() {
    // A resolution error earlier in event order loses to the syntax error.
    let e = err(load("a: !!int x\nb: [\n"));
    assert!(matches!(e.kind, ErrorKind::Syntax(_)), "{e:?}");
}

// ---- budgets ---------------------------------------------------------

fn alias_bomb(scalar: usize, aliases: usize) -> String {
    let mut s = format!("a: &x \"{}\"\nb:\n", "z".repeat(scalar));
    for _ in 0..aliases {
        s.push_str("  - *x\n");
    }
    s
}

#[test]
fn large_scalar_many_aliases_rejected_before_copy() {
    let _g = ALLOC_TEST.lock().unwrap();
    let scalar = 64 << 10;
    let input = alias_bomb(scalar, 200);
    let mut o = opts();
    o.limits.max_owned_bytes = 10 * scalar;
    o.limits.max_input_bytes = 1 << 20;
    o.limits.max_scalar_bytes = 2 * scalar;
    o.limits.max_key_bytes = 1024;
    BIG.store(0, Ordering::SeqCst);
    THRESHOLD.store(scalar, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let r = load_with(&input, &o);
    TRACK.store(false, Ordering::SeqCst);
    let e = err(r);
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxOwnedBytes,
            configured: (10 * scalar) as u64
        }
    );
    let big = BIG.load(Ordering::SeqCst);
    // Without the pre-copy check there would be ~200 scalar-sized copies.
    assert!(big <= 14, "{big} scalar-sized allocations");
}

#[test]
fn alias_events_and_expanded_nodes_limits() {
    let mut o = opts();
    o.limits.max_alias_events = 3;
    let e = err(load_with("a: &x 1\nb: [*x, *x, *x, *x]\n", &o));
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxAliasEvents,
            configured: 3
        }
    );
    let mut o = opts();
    o.limits.max_expanded_nodes = 5;
    let e = err(load_with("a: &x [1, 2]\nb: [*x, *x]\n", &o));
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxExpandedNodes,
            configured: 5
        }
    );
}

#[test]
fn materialized_depth_counts_alias_height() {
    let mut o = opts();
    o.limits.max_depth = 4;
    // Anchored subtree height 2, used at depth 3 -> 5 > 4.
    let e = err(load_with("a: &x [[1]]\nb: [[*x]]\n", &o));
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 4
        }
    );
    assert!(e
        .related
        .iter()
        .any(|r| r.role == RelatedRole::AnchorDefinition));
    // Same alias one level shallower fits.
    load_with("a: &x [[1]]\nb: [*x]\n", &o).unwrap();
}

#[test]
fn lexical_depth_127_128_129() {
    let nest = |n: usize| format!("{}1{}", "[".repeat(n), "]".repeat(n));
    load(&nest(127)).unwrap();
    load(&nest(128)).unwrap();
    let e = err(load(&nest(129)));
    assert!(
        matches!(
            e.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxDepth,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn other_limits() {
    let mut o = opts();
    o.limits.max_input_bytes = 4;
    assert!(matches!(
        err(load_with("a: 12\n", &o)).kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxInputBytes,
            ..
        }
    ));
    let mut o = opts();
    o.limits.max_scalar_bytes = 3;
    assert!(matches!(
        err(load_with("a: 1234\n", &o)).kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxScalarBytes,
            ..
        }
    ));
    let mut o = opts();
    o.limits.max_mapping_entries = 2;
    assert!(matches!(
        err(load_with("a: 1\nb: 2\nc: 3\n", &o)).kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            ..
        }
    ));
    let mut o = opts();
    o.limits.max_documents = 2;
    assert!(matches!(
        load_all_with("1\n---\n2\n---\n3\n", &o).unwrap_err().kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDocuments,
            ..
        }
    ));
    let mut o = opts();
    o.limits.max_nodes = 3;
    assert!(matches!(
        err(load_with("[1, 2, 3]\n", &o)).kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            ..
        }
    ));
    let mut o = opts();
    o.limits.max_key_bytes = 70;
    assert!(matches!(
        err(load_with("? [a, b]\n: 1\n", &o)).kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            ..
        }
    ));
}

#[test]
fn merge_expansion_is_budgeted() {
    let mut o = opts();
    o.merge_keys = MergeKeys::Apply;
    o.limits.max_expanded_nodes = 4;
    let e = err(load_with("b: &b {x: 1, y: 2, z: 3}\nm: {<<: *b}\n", &o));
    assert!(
        matches!(
            e.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxExpandedNodes,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn invalid_options() {
    let mut o = opts();
    o.limits.max_depth = 256;
    let e = err(load_with("a", &o));
    assert_eq!(e.kind, ErrorKind::InvalidOptions);
    assert!(matches!(e.site, Site::NotSource { .. }));
    let mut o = opts();
    o.limits.max_nodes = 0;
    assert_eq!(err(load_with("a", &o)).kind, ErrorKind::InvalidOptions);
}

// ---- anchors and documents --------------------------------------------------

#[test]
fn anchor_lifecycle() {
    let e = err(load("a: &x [1, *x]\n"));
    assert_eq!(e.kind, ErrorKind::RecursiveAlias);
    // Anchors are document-local.
    let e = load_all("a: &x 1\n---\nb: *x\n").unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnknownAnchor, "{e:?}");
    // Alias inside a later definition.
    let d = load("a: &x 1\nb: &y [*x, 2]\nc: *y\n").unwrap();
    let c = d.root().value().as_mapping().unwrap().get("c").unwrap();
    assert!(matches!(c.via(), Some(Via::Alias { .. })));
    assert_eq!(c.value().as_sequence().unwrap().len(), 2);
}

#[test]
fn alias_copy_keeps_definition_span_and_use_site() {
    let input = "a: &x hello\nb: *x\n";
    let d = load(input).unwrap();
    let b = d.root().value().as_mapping().unwrap().get("b").unwrap();
    let NodeSpan::Source(s) = b.span() else {
        panic!()
    };
    assert_eq!(&input[s.start.byte_offset..s.end.byte_offset], "hello");
    let Some(Via::Alias { use_site }) = b.via() else {
        panic!()
    };
    assert_eq!(
        &input[use_site.start.byte_offset..use_site.end.byte_offset],
        "*x"
    );
}

/// Found by fuzzing: a block collection ending in an alias took the anchor
/// definition's end, so its span ran backwards. Containing spans and error sites
/// use the alias use site.
#[test]
fn block_collection_ending_in_alias_spans_forward() {
    let input = "de: &d\nany:\n  <<:: *d\n";
    let d = load(input).unwrap();
    let any = d.root().value().as_mapping().unwrap().get("any").unwrap();
    let NodeSpan::Source(s) = any.span() else {
        panic!()
    };
    assert_eq!(&input[s.start.byte_offset..s.end.byte_offset], "<<:: *d");
    let NodeSpan::Source(r) = d.root().span() else {
        panic!()
    };
    assert_eq!(r.end.byte_offset, input.len() - 1);

    let seq = "a: &a 1\nb:\n- 2\n- *a\n";
    let d = load(seq).unwrap();
    let b = d.root().value().as_mapping().unwrap().get("b").unwrap();
    let NodeSpan::Source(s) = b.span() else {
        panic!()
    };
    // granit starts a block sequence at its first item's content, not at "-".
    assert_eq!(&seq[s.start.byte_offset..s.end.byte_offset], "2\n- *a");
}

fn related(e: &Error, role: RelatedRole) -> usize {
    e.related
        .iter()
        .find(|r| r.role == role)
        .unwrap_or_else(|| panic!("no {role:?} in {e:?}"))
        .span
        .start
        .byte_offset
}

/// §7.2: diagnostics on an alias copy report the definition site in `site` and
/// the use site in `related`; merged-in context adds the directive.
#[test]
fn alias_and_merge_origin_in_related() {
    let dup = "x: &k a\nm:\n  *k : 1\n  *k : 2\n";
    let e = err(load(dup));
    assert_eq!(e.kind, ErrorKind::DuplicateKey);
    assert_eq!(src(&e).byte_offset, dup.find(" a").unwrap() + 1);
    assert_eq!(related(&e, RelatedRole::AliasUse), dup.rfind("*k").unwrap());

    let mut o = opts();
    o.merge_keys = MergeKeys::Apply;
    let ms = "s: &s 1\nm:\n  <<: *s\n";
    let e = err(load_with(ms, &o));
    assert_eq!(e.kind, ErrorKind::MergeSource);
    assert_eq!(src(&e).byte_offset, ms.find(" 1").unwrap() + 1);
    assert_eq!(related(&e, RelatedRole::AliasUse), ms.find("*s").unwrap());
    assert_eq!(
        related(&e, RelatedRole::MergeDirective),
        ms.find("<<").unwrap()
    );

    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct P {
        base: u32,
        port: u16,
    }
    let ty = "base: &b 70000\nport: *b\n";
    let e = from_str::<P>(ty).unwrap_err();
    assert_eq!(e.kind, ErrorKind::IntegerOutOfRange);
    assert_eq!(src(&e).byte_offset, ty.find("70000").unwrap());
    assert_eq!(related(&e, RelatedRole::AliasUse), ty.find("*b").unwrap());

    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct M {
        b: std::collections::BTreeMap<String, u8>,
        m: std::collections::BTreeMap<String, u8>,
    }
    let mg = "b: &b {x: 1, y: 2}\nm:\n  <<: *b\n  x: 300\n";
    let e = from_str_with::<M>(mg, &o).unwrap_err();
    assert_eq!(src(&e).byte_offset, mg.find("300").unwrap());
    assert!(e.related.is_empty(), "explicit entry has no origin: {e:?}");
    let mg2 = "b: &b {x: 1, y: 300}\nm:\n  <<: *b\n";
    let e = from_str_with::<M>(mg2, &o).unwrap_err();
    assert_eq!(e.kind, ErrorKind::IntegerOutOfRange);
    let first = mg2.find("300").unwrap();
    assert_eq!(src(&e).byte_offset, first);
    // The first failing path is b.y (definition, no origin); m.y would carry the directive.
    let mg3 = "b: &b {x: 1}\nc: &c {y: 300}\nm:\n  <<: *c\n";
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct M3 {
        b: std::collections::BTreeMap<String, u8>,
        c: std::collections::BTreeMap<String, u16>,
        m: std::collections::BTreeMap<String, u8>,
    }
    let e = from_str_with::<M3>(mg3, &o).unwrap_err();
    assert_eq!(src(&e).byte_offset, mg3.find("300").unwrap());
    assert_eq!(
        related(&e, RelatedRole::MergeDirective),
        mg3.find("<<").unwrap()
    );
}

/// §7.2: a block collection ends at the exclusive end of its last content line,
/// excluding trailing line breaks, whatever the chomping of a final block scalar.
#[test]
fn block_collection_end_excludes_trailing_breaks() {
    for (input, expect) in [
        ("a:\n  b: |+\n    x\n\n\nc: 1\n", "b: |+\n    x"),
        ("a:\n  b: |-\n    x\n\nc: 1\n", "b: |-\n    x"),
        (
            "a:\n  b: >\n    x\n    y\n# note\nc: 1\n",
            "b: >\n    x\n    y",
        ),
        ("a:\r\n  b: |\r\n    x\r\n\r\nc: 1\r\n", "b: |\r\n    x"),
        // granit starts a block sequence at its first item, not at "-".
        ("a:\n- |\n  é\n", "|\n  é"),
    ] {
        let d = load(input).unwrap();
        let a = d.root().value().as_mapping().unwrap().get("a").unwrap();
        let NodeSpan::Source(s) = a.span() else {
            panic!()
        };
        let got = &input[s.start.byte_offset..s.end.byte_offset];
        assert_eq!(got, expect, "{input:?}");
        let (line, col) = reference(input, s.end.byte_offset);
        assert_eq!((s.end.line, s.end.column), (line, col), "{input:?}");
    }
}

#[test]
fn load_all_is_all_or_nothing() {
    let e = load_all("a: 1\n---\nb: [1, *x]\n---\nc: 3\n").unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnknownAnchor);
    assert_eq!(load_all("a: 1\n---\nb: 2\n").unwrap().len(), 2);
    assert_eq!(
        err(load("a: 1\n---\nb: 2\n")).kind,
        ErrorKind::MultipleDocuments
    );
}

#[test]
fn yaml_version_directive() {
    let d = load("%YAML 1.1\n---\nk: yes\n").unwrap();
    assert_eq!(d.yaml_version(), Some((1, 1)));
    assert_eq!(
        d.root()
            .value()
            .as_mapping()
            .unwrap()
            .get("k")
            .unwrap()
            .value()
            .as_str(),
        Some("yes")
    );
    assert_eq!(
        err(load("%YAML 2.0\n---\nk: v\n")).kind,
        ErrorKind::UnsupportedYamlVersion
    );
}

// ---- scalar and mapping regressions ---------------------------

#[test]
fn counterexamples() {
    // global and nested local tags.
    let d = load("!<tag:example.com,2026:thing> {a: 1}\n").unwrap();
    let Value::Tagged(t) = d.root().value() else {
        panic!()
    };
    assert_eq!(t.tag().uri(), "tag:example.com,2026:thing");
    let d = load("a: !custom [!inner x]\n").unwrap();
    let Value::Tagged(t) = d
        .root()
        .value()
        .as_mapping()
        .unwrap()
        .get("a")
        .unwrap()
        .value()
    else {
        panic!()
    };
    let Value::Tagged(inner) = t.value().value().as_sequence().unwrap()[0].value() else {
        panic!()
    };
    assert_eq!(inner.tag().uri(), "!inner");
    // u128::MAX exact.
    let d = load("340282366920938463463374607431768211455\n").unwrap();
    assert_eq!(
        d.root().value().as_integer().unwrap().to_u128(),
        Some(u128::MAX)
    );
    // Duplicate typed map.
    let e = from_str::<std::collections::HashMap<String, i64>>("x: 1\nx: 2\n").unwrap_err();
    assert_eq!(e.kind, ErrorKind::DuplicateKey);
    // Explicit-key grammar.
    load("[ ? ]\n").unwrap();
    assert!(matches!(err(load("[ ? ] ]\n")).kind, ErrorKind::Syntax(_)));
    // Boolean-word keys stay strings and do not collide.
    let d = load("y: 1\nn: 2\nyes: 3\nno: 4\nNO: 5\non: 6\noff: 7\n").unwrap();
    assert_eq!(d.root().value().as_mapping().unwrap().len(), 7);
}

// ---- typed paths (§8) ---------------------------------------------------------

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Service {
    name: String,
    port: u16,
    limits: Limits2,
    mode: Mode,
}
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct Limits2 {
    cpu: f64,
    #[serde(default)]
    tags: Vec<String>,
}
#[derive(Deserialize, Debug, PartialEq)]
enum Mode {
    Fast,
    Slow { factor: u8 },
    Tuned(u32),
}

#[test]
fn typed_errors_are_located_with_paths() {
    let input = "name: svc\nport: \"8080\"\nlimits: {cpu: 1}\nmode: Fast\n";
    let e = from_str::<Service>(input).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::Type { .. }));
    assert_eq!(
        src(&e),
        Location {
            byte_offset: 16,
            line: 2,
            column: 7
        }
    );
    assert!(matches!(&e.path[..], [PathSegment::Key(k)] if k.as_str() == Some("port")));

    let e = from_str::<Service>("name: svc\nport: 1\nlimits: {cpu: 1, tags: [a, 2]}\nmode: Fast\n")
        .unwrap_err();
    assert!(
        matches!(
            &e.path[..],
            [
                PathSegment::Key(_),
                PathSegment::Key(_),
                PathSegment::Index(1)
            ]
        ),
        "{:?}",
        e.path
    );
    assert_eq!(src(&e).line, 3);

    let e = from_str::<Service>("name: svc\nport: 1\nlimits: {cpu: 1}\nmode: Fast\nextra: 1\n")
        .unwrap_err();
    assert!(matches!(e.kind, ErrorKind::UnknownField(ref f) if f == "extra"));
    assert_eq!(src(&e).line, 5);

    let e = from_str::<Service>("name: svc\nlimits: {cpu: 1}\nmode: Fast\n").unwrap_err();
    assert!(matches!(e.kind, ErrorKind::MissingField(ref f) if f == "port"));
    assert_eq!(src(&e).line, 1);
}

#[test]
fn enums_and_tags() {
    #[derive(Deserialize)]
    struct M {
        m: Mode,
    }
    assert_eq!(from_str::<M>("m: Fast\n").unwrap().m, Mode::Fast);
    assert_eq!(
        from_str::<M>("m: {Slow: {factor: 2}}\n").unwrap().m,
        Mode::Slow { factor: 2 }
    );
    assert_eq!(from_str::<M>("m: !Tuned 7\n").unwrap().m, Mode::Tuned(7));
    assert!(from_str::<M>("m: !<tag:x.com,2026:Tuned> 7\n").is_err());
    // Tags never stripped silently.
    #[derive(Deserialize)]
    struct S {
        #[allow(dead_code)]
        s: Option<String>,
    }
    assert!(from_str::<S>("s: !custom x\n").is_err());
    assert!(from_str::<S>("s: ~\n").unwrap().s.is_none());
    // Tuple length.
    #[derive(Deserialize)]
    struct T {
        #[allow(dead_code)]
        t: (u8, u8),
    }
    assert!(from_str::<T>("t: [1, 2, 3]\n").is_err());
    assert!(from_str::<T>("t: [1, 2]\n").is_ok());
}

#[test]
fn synthetic_nodes_report_not_source() {
    let parsed = load("k: 1\n").unwrap();
    let e = from_node::<String>(
        parsed
            .root()
            .value()
            .as_mapping()
            .unwrap()
            .get("k")
            .unwrap(),
        &opts(),
    )
    .unwrap_err();
    assert!(matches!(e.site, Site::Source(_)));
    let n = Node::synthetic(Value::Bool(true));
    let e = from_node::<String>(&n, &opts()).unwrap_err();
    assert!(
        matches!(e.site, Site::NotSource { last_known: None }),
        "{e:?}"
    );
}

// ---- wire format (§11) ----------------------------------------------------------

#[test]
fn wire_envelopes() {
    let o = opts();
    let d =
        load_all("a: .nan\nb: -0.0\nc: 340282366920938463463374607431768211455\nd: \"q\\u0001\"\n")
            .unwrap();
    let w = encode(&d, &o).unwrap();
    let v: serde_json::Value = serde_json::from_str(&w).unwrap();
    assert_eq!(v["format"], "yamlprims-value");
    assert_eq!(v["version"], 1);
    let e = &v["documents"][0]["root"]["entries"];
    assert_eq!(e[0]["value"]["bits"], "7ff8000000000000");
    assert_eq!(e[1]["value"]["bits"], "8000000000000000");
    assert_eq!(
        e[2]["value"]["value"],
        "340282366920938463463374607431768211455"
    );
    assert!(w.contains(r#""q\u0001""#));
    // Error envelope.
    let er = err(load("a: 1\na: 2\n"));
    let w = encode_error(&er, &o).unwrap();
    let v: serde_json::Value = serde_json::from_str(&w).unwrap();
    assert_eq!(v["error"]["kind"], "duplicate_key");
    assert_eq!(v["error"]["site"]["source"], serde_json::json!([5, 2, 1]));
    assert_eq!(v["error"]["related"][0]["role"], "first_definition");
    // Output limit.
    let mut small = opts();
    small.limits.max_output_bytes = 50;
    assert!(matches!(
        encode(&d, &small).unwrap_err().kind,
        ErrorKind::OutputLimitExceeded { .. }
    ));
    // Plain JSON loss refusal.
    let d = load("? [1]\n: x\n").unwrap();
    assert!(encode_plain(d.root(), &o).is_err());
    let d = load("a: .inf\n").unwrap();
    assert!(encode_plain(d.root(), &o).is_err());
    let d = load("__proto__: 1\nb: -0.0\n").unwrap();
    assert_eq!(
        encode_plain(d.root(), &o).unwrap(),
        r#"{"__proto__":1,"b":-0.0}"#
    );
}

#[test]
fn mapping_api_keeps_index_valid() {
    let d = load("a: 1\nb: 2\n").unwrap();
    let mut m = d.root().value().as_mapping().unwrap().clone();
    assert!(m
        .insert_new(
            Node::synthetic(Value::String("a".into())),
            Node::synthetic(Value::Null)
        )
        .is_err());
    m.remove(&Value::String("a".into())).unwrap();
    assert!(m.get("a").is_none());
    assert!(m.get("b").is_some());
    m.insert_new(
        Node::synthetic(Value::String("a".into())),
        Node::synthetic(Value::Null),
    )
    .unwrap();
    assert!(m.get("a").is_some());
}

// ---- Contract regressions ----

fn limited(f: impl Fn(&mut Limits)) -> Options {
    let mut o = opts();
    o.limits.max_scalar_bytes = 100;
    o.limits.max_key_bytes = 100;
    f(&mut o.limits);
    o
}

fn limit_kind(r: Result<Document, Error>) -> Option<LimitKind> {
    match r {
        Ok(_) => None,
        Err(e) => match e.kind {
            ErrorKind::LimitExceeded { limit, .. } => Some(limit),
            _ => panic!("unexpected error {e:?}"),
        },
    }
}

/// Smallest limit value that accepts `input`, checked by scanning; asserts it equals `want`
/// and that one below is rejected by that limit (below/at boundary).
fn boundary(
    input: &str,
    want: usize,
    kind: LimitKind,
    set: fn(&mut Limits, usize),
    base: &Options,
) {
    let mut at = base.clone();
    set(&mut at.limits, want);
    assert_eq!(
        limit_kind(load_all_with(input, &at).map(|mut d| d.remove(0))),
        None,
        "{input:?} at {want}"
    );
    let mut below = base.clone();
    set(&mut below.limits, want - 1);
    assert_eq!(
        limit_kind(load_all_with(input, &below).map(|mut d| d.remove(0))),
        Some(kind),
        "{input:?} at {}",
        want - 1
    );
}

/// a retained tag is a wrapper node with its own weight (64 + tag bytes), for
/// scalars and collections, nested tags, and alias copies of tagged collections.
#[test]
fn tag_wrappers_are_counted_and_charged() {
    let set_nodes: fn(&mut Limits, usize) = |l, v| l.max_nodes = v;
    let set_owned: fn(&mut Limits, usize) = |l, v| l.max_owned_bytes = v;
    let base = limited(|_| {});
    // `!x`: uri "!x" + handle "!" + suffix "x" = 4 tag bytes.
    boundary("!x hi", 2, LimitKind::MaxNodes, set_nodes, &base);
    boundary(
        "!x hi",
        64 + 4 + 64 + 2,
        LimitKind::MaxOwnedBytes,
        set_owned,
        &base,
    );
    boundary("!x []", 2, LimitKind::MaxNodes, set_nodes, &base);
    boundary(
        "!x []",
        64 + 4 + 64,
        LimitKind::MaxOwnedBytes,
        set_owned,
        &base,
    );
    boundary("!x {}", 2, LimitKind::MaxNodes, set_nodes, &base);
    boundary("!x [!y [z]]", 5, LimitKind::MaxNodes, set_nodes, &base);
    boundary(
        "!x [!y [z]]",
        (64 + 4 + 64) * 2 + 65,
        LimitKind::MaxOwnedBytes,
        set_owned,
        &base,
    );
    // Lane B's case: a long collection tag alone exceeds a small owned-byte budget.
    let long = format!("!{} []", "x".repeat(1024));
    let small = limited(|l| l.max_owned_bytes = 256);
    assert_eq!(
        limit_kind(load_with(&long, &small)),
        Some(LimitKind::MaxOwnedBytes)
    );
    // Alias copy of a tagged collection: root 1, keys 2, tagged seq 2, copy 2.
    let alias = "a: &t !x []\nb: *t\n";
    boundary(alias, 7, LimitKind::MaxNodes, set_nodes, &base);
    // Weight: root 64, keys 65+65, entries 24+24, tagged seq 132, anchor storage 132, copy 132.
    boundary(
        alias,
        64 + 65 * 2 + 24 * 2 + 132 * 3,
        LimitKind::MaxOwnedBytes,
        set_owned,
        &base,
    );
    let mut exp = base.clone();
    exp.limits.max_expanded_nodes = 1;
    assert_eq!(
        limit_kind(load_with(alias, &exp)),
        Some(LimitKind::MaxExpandedNodes)
    );
    exp.limits.max_expanded_nodes = 2;
    assert_eq!(limit_kind(load_with(alias, &exp)), None);
    // Core tags are resolved, not retained: no wrapper.
    assert_eq!(
        limit_kind(load_with("!!seq []", &limited(|l| l.max_nodes = 1))),
        None
    );
    // Tag errors are reported at the start event, ahead of errors inside the collection.
    let mut rej = opts();
    rej.unknown_tags = UnknownTags::Reject;
    assert_eq!(
        err(load_with("!x {a: 1, a: 2}", &rej)).kind,
        ErrorKind::UnknownTag
    );
    assert_eq!(
        err(load("!!seq {a: 1, a: 2}")).kind,
        ErrorKind::TagKindMismatch
    );
}

/// merged-in entries are copies charged to the expansion budgets (§5.3 rule 6),
/// inline or aliased, nested or not, independent of the alias guard.
#[test]
fn inline_and_nested_merges_are_charged() {
    let set_exp: fn(&mut Limits, usize) = |l, v| l.max_expanded_nodes = v;
    let set_nodes: fn(&mut Limits, usize) = |l, v| l.max_nodes = v;
    let mut base = limited(|_| {});
    base.merge_keys = MergeKeys::Apply;
    boundary(
        "{<<: {a: 1}}",
        2,
        LimitKind::MaxExpandedNodes,
        set_exp,
        &base,
    );
    boundary("{<<: {a: 1}}", 7, LimitKind::MaxNodes, set_nodes, &base);
    boundary(
        "{<<: {a: 1, b: 2}}",
        4,
        LimitKind::MaxExpandedNodes,
        set_exp,
        &base,
    );
    // Shadowed entries are not copied.
    boundary(
        "{a: 0, <<: {a: 1, b: 2}}",
        2,
        LimitKind::MaxExpandedNodes,
        set_exp,
        &base,
    );
    // Nested: the inner merge copies a:1, then the outer merge copies it again.
    boundary(
        "{<<: {<<: {a: 1}}}",
        4,
        LimitKind::MaxExpandedNodes,
        set_exp,
        &base,
    );
    // Sequence of sources, tagged values inside a source.
    boundary(
        "{<<: [{a: 1}, {b: !x 2}]}",
        5,
        LimitKind::MaxExpandedNodes,
        set_exp,
        &base,
    );
    // Aliased source: alias copy (3 nodes) plus the merged entry (2).
    boundary(
        "s: &s {a: 1}\nm: {<<: *s}\n",
        5,
        LimitKind::MaxExpandedNodes,
        set_exp,
        &base,
    );
    let mut o = base.clone();
    o.limits.max_expanded_nodes = 1;
    let e = err(load_with("{<<: {a: 1}}", &o));
    assert_eq!(related(&e, RelatedRole::MergeDirective), 1);
}

/// the public checked constructor rejects keys containing NaN at any depth.
#[test]
fn public_mapping_rejects_nan_keys() {
    let mut m = Mapping::new();
    let null = || Node::synthetic(Value::Null);
    assert_eq!(
        m.insert_new(Node::synthetic(Value::Float(f64::NAN)), null()),
        Err(InsertError::InvalidKey)
    );
    let nested = Value::Sequence(vec![
        Node::synthetic(Value::Int(Integer::from_u128(1))),
        Node::synthetic(Value::Float(f64::NAN)),
    ]);
    assert_eq!(
        m.insert_new(Node::synthetic(nested), null()),
        Err(InsertError::InvalidKey)
    );
    let mut inner = Mapping::new();
    inner
        .insert_new(
            Node::synthetic(Value::String("k".into())),
            Node::synthetic(Value::Float(f64::NAN)),
        )
        .unwrap();
    assert_eq!(
        m.insert_new(Node::synthetic(Value::Mapping(inner)), null()),
        Err(InsertError::InvalidKey),
        "NaN in a nested value of a key"
    );
    assert!(m.is_empty());
    // NaN as a value is fine, and ±0 and infinities are ordinary keys.
    m.insert_new(
        Node::synthetic(Value::String("v".into())),
        Node::synthetic(Value::Float(f64::NAN)),
    )
    .unwrap();
    m.insert_new(Node::synthetic(Value::Float(f64::INFINITY)), null())
        .unwrap();
    assert_eq!(
        m.insert_new(Node::synthetic(Value::String("v".into())), null()),
        Err(InsertError::Duplicate { existing: 0 })
    );
    let tree = Node::synthetic(Value::Mapping(m));
    assert!(encode_plain(&tree, &opts()).is_err(), "non-string key");
}

/// from_node limit errors keep the offending parsed node's source site; synthetic
/// nodes report NotSource with the nearest source ancestor.
#[test]
fn from_node_limit_sites() {
    let input = "[1, 2]";
    let d = load(input).unwrap();
    let o = limited(|l| l.max_nodes = 1);
    let e = from_node::<Vec<u64>>(d.root(), &o).unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 1
        }
    );
    assert_eq!(src(&e).byte_offset, input.find('1').unwrap());

    let deep = load("[[[1]]]").unwrap();
    let o = limited(|l| l.max_depth = 2);
    let e = from_node::<Vec<Vec<Vec<u8>>>>(deep.root(), &o).unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2
        }
    );
    assert!(matches!(e.site, Site::Source(_)));

    let synthetic = Node::synthetic(Value::Sequence(vec![
        Node::synthetic(Value::Null),
        Node::synthetic(Value::Null),
    ]));
    let e = from_node::<Vec<()>>(&synthetic, &limited(|l| l.max_nodes = 1)).unwrap_err();
    assert_eq!(e.site, Site::NotSource { last_known: None });
}

/// every public encoder validates its options.
#[test]
fn encoders_validate_options() {
    let d = load("ok").unwrap();
    let mut o = opts();
    o.limits.max_depth = 0;
    assert_eq!(
        encode(std::slice::from_ref(&d), &o).unwrap_err().kind,
        ErrorKind::InvalidOptions
    );
    assert_eq!(
        encode_plain(d.root(), &o).unwrap_err().kind,
        ErrorKind::InvalidOptions
    );
    let e = err(load("a: [")).clone();
    assert_eq!(
        encode_error(&e, &o).unwrap_err().kind,
        ErrorKind::InvalidOptions
    );
}

/// the document error depends on the API, not on the configured limit's value.
#[test]
fn document_limit_by_api() {
    let o = limited(|l| l.max_documents = 1);
    assert_eq!(
        load_all_with("1\n---\n2", &o).unwrap_err().kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDocuments,
            configured: 1
        }
    );
    assert_eq!(
        err(load_with("1\n---\n2", &o)).kind,
        ErrorKind::MultipleDocuments
    );
    let o = limited(|l| l.max_documents = 2);
    assert_eq!(
        err(load_with("1\n---\n2", &o)).kind,
        ErrorKind::MultipleDocuments
    );
    assert_eq!(load_all_with("1\n---\n2", &o).unwrap().len(), 2);
    assert!(matches!(
        load_all_with("1\n---\n2\n---\n3", &o).unwrap_err().kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDocuments,
            ..
        }
    ));
}

/// Value, Node, Spanned<T> and TaggedValue<T> through this crate's deserializer
/// (lossless), and their documented degradation through a foreign one.
#[test]
fn owned_and_wrapper_types() {
    let v: Value = from_str("!custom x").unwrap();
    let Value::Tagged(t) = &v else {
        panic!("{v:?}")
    };
    assert_eq!(t.tag().uri(), "!custom");
    assert_eq!(t.value().value().as_str(), Some("x"));

    let input = "a: &x [1, 2]\nb: *x\n";
    let n: Node = from_str(input).unwrap();
    let b = n.value().as_mapping().unwrap().get("b").unwrap();
    assert!(
        matches!(b.via(), Some(Via::Alias { .. })),
        "Node keeps provenance"
    );

    #[derive(Debug, Deserialize)]
    struct Cfg {
        name: Spanned<String>,
        extra: Value,
        raw: Node,
        port: Option<Spanned<u16>>,
        kind: TaggedValue<String>,
        flags: Vec<Spanned<bool>>,
        // §8 says `Option<TaggedValue<()>>` works, but a non-core tag's empty inner is the
        // String "" (§4.4), so `()` cannot accept it.
        opt: Option<TaggedValue<String>>,
        nested: Spanned<Spanned<u8>>,
    }
    let input = "name: svc\nextra: {k: [1, !t y]}\nraw: 1e3\nport: 8080\nkind: !Kind fast\nflags: [true, false]\nopt: !n\nnested: 7\n";
    let c: Cfg = from_str(input).unwrap();
    let at = |s: &Option<Span>| s.map(|s| &input[s.start.byte_offset..s.end.byte_offset]);
    assert_eq!(
        (c.name.value.as_str(), at(&c.name.span)),
        ("svc", Some("svc"))
    );
    assert_eq!(at(&c.port.as_ref().unwrap().span), Some("8080"));
    assert_eq!(c.port.unwrap().value, 8080);
    assert_eq!((c.kind.tag.uri(), c.kind.value.as_str()), ("!Kind", "fast"));
    assert_eq!(
        c.flags.iter().map(|f| at(&f.span)).collect::<Vec<_>>(),
        vec![Some("true"), Some("false")]
    );
    let opt = c.opt.unwrap();
    assert_eq!((opt.tag.uri(), opt.value.as_str()), ("!n", ""));
    assert!(from_str::<Option<TaggedValue<()>>>("!n").is_err());
    assert_eq!(
        (
            c.nested.value.value,
            at(&c.nested.span),
            at(&c.nested.value.span)
        ),
        (7, Some("7"), Some("7"))
    );
    assert!(matches!(c.raw.value(), Value::Float(f) if *f == 1000.0));
    assert!(matches!(c.raw.span(), NodeSpan::Source(_)));
    let Value::Mapping(m) = &c.extra else {
        panic!()
    };
    assert!(matches!(
        m.get("k").unwrap().value().as_sequence().unwrap()[1].value(),
        Value::Tagged(_)
    ));

    // Strictness is unchanged: wrappers are transparent, tags are not stripped.
    assert!(from_str::<Spanned<String>>("!t x").is_err());
    assert_eq!(
        from_str::<TaggedValue<String>>("x").unwrap_err().kind,
        ErrorKind::Type {
            expected: "tagged value".into(),
            found: "string".into()
        }
    );
    // A non-core tag keeps its raw String inner (§4.4); only enums re-resolve.
    assert!(matches!(
        from_str::<TaggedValue<u32>>("!x 7").unwrap_err().kind,
        ErrorKind::Type { .. }
    ));
    let e = from_str::<Spanned<u8>>("300").unwrap_err();
    assert_eq!(
        (&e.kind, src(&e).byte_offset),
        (&ErrorKind::IntegerOutOfRange, 0)
    );
    // Hand-built: span None.
    let s: Spanned<u8> =
        from_node(&Node::synthetic(Value::Int(Integer::from_u128(5))), &opts()).unwrap();
    assert_eq!(
        s,
        Spanned {
            value: 5,
            span: None
        }
    );

    // Foreign deserializer: synthetic Node/Value, Spanned without span, TaggedValue refused.
    let n: Node = serde_json::from_str(r#"{"a": [1, -2, 1.5, "s", null, true]}"#).unwrap();
    assert!(matches!(n.span(), NodeSpan::Synthetic));
    assert_eq!(
        n.value()
            .as_mapping()
            .unwrap()
            .get("a")
            .unwrap()
            .value()
            .as_sequence()
            .unwrap()
            .len(),
        6
    );
    assert!(
        serde_json::from_str::<Value>(r#"{"a": 1, "a": 2}"#).is_err(),
        "duplicate keys stay errors"
    );
    let s: Spanned<u8> = serde_json::from_str("5").unwrap();
    assert_eq!(
        s,
        Spanned {
            value: 5,
            span: None
        }
    );
    assert!(serde_json::from_str::<TaggedValue<u8>>("5").is_err());
}

/// The protocols are bounded by the same depth guard and leave no state behind.
#[test]
fn protocol_depth_and_isolation() {
    let deep = format!("{}1{}", "[".repeat(120), "]".repeat(120));
    let o = limited(|l| l.max_depth = 100);
    assert!(matches!(
        from_str_with::<Value>(&deep, &o).unwrap_err().kind,
        ErrorKind::LimitExceeded { .. }
    ));
    // A failed conversion must not leak a parked node into the next call.
    assert!(from_str::<TaggedValue<u8>>("!x 300").is_err());
    let s: Spanned<String> = serde_json::from_str("\"x\"").unwrap();
    assert_eq!(s.span, None);
    let v: Value = serde_json::from_str("1").unwrap();
    assert!(matches!(v, Value::Int(_)));
}

/// an ordinary block sequence starts at its first `-`;
/// an indentless sequence (a mapping value at its key's own indentation) starts at the
/// first token after its first `-`, as granit marks it.
#[test]
fn block_sequence_start() {
    let cases: &[(&str, &str)] = &[
        // Ordinary (indented, top-level, or nested in a sequence): the first dash.
        ("k:\n  - x\n", "- x"),
        ("k:\n  - !t x\n", "- !t x"),
        ("k:\n  -\n  - x\n", "-\n  - x"),
        ("k:\n  - - x\n", "- - x"),
        ("k:\n  - # c\n    x\n", "- # c\n    x"),
        ("- x\n", "- x"),
        ("- - x\n", "- - x"),
        ("k: &s\n  - x\n", "- x"),
        // Indentless: the first token after the first dash.
        ("k:\n- x\n", "x"),
        ("k:\n- !t x\n", "!t x"),
        ("k:\n- &a x\n- *a\n", "&a x\n- *a"),
        ("a: &a x\nk:\n- *a\n", "*a"),
        ("k:\n-\n- x\n", "\n- x"),
        ("k:\n- - x\n", "- x"),
        ("k:\n- # c\n  x\n", "\n  x"),
        ("k: &s\n- x\n", "x"),
        // Flow: the bracket.
        ("k: []\n", "[]"),
    ];
    for (input, want) in cases {
        let d = load(input).unwrap();
        let k = d
            .root()
            .value()
            .as_mapping()
            .and_then(|m| m.get("k"))
            .unwrap_or(d.root());
        let NodeSpan::Source(s) = k.span() else {
            panic!()
        };
        assert_eq!(
            &input[s.start.byte_offset..s.end.byte_offset],
            *want,
            "{input:?}"
        );
    }
    // An indentless sequence inside a sequence item's mapping.
    let input = "- k:\n  - x\n";
    let d = load(input).unwrap();
    let k = d.root().value().as_sequence().unwrap()[0]
        .value()
        .as_mapping()
        .unwrap()
        .get("k")
        .unwrap();
    let NodeSpan::Source(s) = k.span() else {
        panic!()
    };
    assert_eq!(&input[s.start.byte_offset..s.end.byte_offset], "x");
    // A sequence's own properties are reported through tag_start, not the span.
    let input = "k: !t\n- x\n";
    let d = load(input).unwrap();
    let k = d.root().value().as_mapping().unwrap().get("k").unwrap();
    assert_eq!(
        k.tag_start().unwrap().byte_offset,
        input.find("!t").unwrap()
    );
}

/// Foreign deserialization fallbacks hold for deserializers that visit
/// values directly (serde's own value deserializers), not only for newtype-wrapping ones.
#[test]
fn foreign_direct_visits() {
    use serde::de::value::{Error as VE, *};
    use serde::de::IntoDeserializer;
    let n = Node::deserialize(U128Deserializer::<VE>::new(u128::MAX)).unwrap();
    assert_eq!(
        (n.value().as_integer().unwrap().to_u128(), n.span()),
        (Some(u128::MAX), &NodeSpan::Synthetic)
    );
    let v = Value::deserialize(I128Deserializer::<VE>::new(i128::MIN)).unwrap();
    assert_eq!(v.as_integer().unwrap().to_i128(), Some(i128::MIN));
    for (d, want) in [(I64Deserializer::<VE>::new(-3).into_deserializer(), "-3")] {
        assert_eq!(
            Value::deserialize(d)
                .unwrap()
                .as_integer()
                .unwrap()
                .to_string(),
            want
        );
    }
    assert!(matches!(
        Value::deserialize(UnitDeserializer::<VE>::new()).unwrap(),
        Value::Null
    ));
    assert_eq!(
        Value::deserialize(StringDeserializer::<VE>::new("s".into()))
            .unwrap()
            .as_str(),
        Some("s")
    );
    let seq = Value::deserialize(SeqDeserializer::<_, VE>::new(vec![1u64, 2].into_iter())).unwrap();
    assert_eq!(seq.as_sequence().unwrap().len(), 2);
    let map =
        Value::deserialize(MapDeserializer::<_, VE>::new(vec![("a", 1u8)].into_iter())).unwrap();
    assert!(map.as_mapping().unwrap().get("a").is_some());
    let dup = MapDeserializer::<_, VE>::new(vec![("a", 1u8), ("a", 2)].into_iter());
    assert!(
        Value::deserialize(dup).is_err(),
        "duplicate keys stay errors"
    );

    // Spanned: scalar, string, unit, collections, nested wrappers; always span None.
    assert_eq!(
        Spanned::<u64>::deserialize(U64Deserializer::<VE>::new(5)).unwrap(),
        Spanned {
            value: 5,
            span: None
        }
    );
    assert_eq!(
        Spanned::<u128>::deserialize(U128Deserializer::<VE>::new(u128::MAX))
            .unwrap()
            .value,
        u128::MAX
    );
    assert_eq!(
        Spanned::<bool>::deserialize(BoolDeserializer::<VE>::new(true))
            .unwrap()
            .value,
        true
    );
    assert_eq!(
        Spanned::<String>::deserialize(StrDeserializer::<VE>::new("x"))
            .unwrap()
            .value,
        "x"
    );
    assert_eq!(
        Spanned::<()>::deserialize(UnitDeserializer::<VE>::new())
            .unwrap()
            .value,
        ()
    );
    let v =
        Spanned::<Vec<u8>>::deserialize(SeqDeserializer::<_, VE>::new(vec![1u8, 2].into_iter()))
            .unwrap();
    assert_eq!(
        v,
        Spanned {
            value: vec![1, 2],
            span: None
        }
    );
    let m = Spanned::<std::collections::BTreeMap<String, u8>>::deserialize(
        MapDeserializer::<_, VE>::new(vec![("a", 1u8)].into_iter()),
    )
    .unwrap();
    assert_eq!(m.value.get("a"), Some(&1));
    let nested = Spanned::<Spanned<Node>>::deserialize(U64Deserializer::<VE>::new(7)).unwrap();
    assert_eq!((nested.span, nested.value.span), (None, None));
    assert_eq!(
        nested.value.value.value().as_integer().unwrap().to_u64(),
        Some(7)
    );
    let wrapped = Spanned::<Vec<Spanned<u8>>>::deserialize(SeqDeserializer::<_, VE>::new(
        vec![1u8].into_iter(),
    ))
    .unwrap();
    assert_eq!(
        wrapped.value,
        vec![Spanned {
            value: 1,
            span: None
        }]
    );
    // serde_json routes: Option targets and nesting.
    let o: Spanned<Option<u8>> = serde_json::from_str("null").unwrap();
    assert_eq!(o.value, None);
    let o: Spanned<Option<u8>> = serde_json::from_str("4").unwrap();
    assert_eq!(o.value, Some(4));
    // TaggedValue stays a documented error through foreign deserializers.
    assert!(TaggedValue::<u64>::deserialize(U64Deserializer::<VE>::new(1)).is_err());
    // Native calls after foreign ones still see their own spans.
    let s: Spanned<u8> = from_str("  9").unwrap();
    assert_eq!(s.span.unwrap().start.byte_offset, 2);
}

/// from_node budget errors keep the alias use (and merge directive) context.
#[test]
fn from_node_budget_errors_keep_origin() {
    let input = "a: &a [1]\nb: [*a]\n";
    let d = load(input).unwrap();
    let o = limited(|l| l.max_depth = 2);
    let e = from_node::<Node>(d.root(), &o).unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2
        }
    );
    // The collection whose children would exceed the depth: the alias copy of [1].
    assert_eq!(src(&e).byte_offset, input.find("[1]").unwrap());
    assert_eq!(
        related(&e, RelatedRole::AliasUse),
        input.find("*a").unwrap()
    );

    // root, m, the merged map, then a (merged in): the fourth node exceeds 3.
    let mut o = limited(|l| l.max_nodes = 3);
    o.merge_keys = MergeKeys::Apply;
    let input = "m: {<<: {a: 1}}\n";
    let d = load_with(input, &opts_apply()).unwrap();
    let e = from_node::<Node>(d.root(), &o).unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 3
        }
    );
    assert_eq!(
        related(&e, RelatedRole::MergeDirective),
        input.find("<<").unwrap()
    );

    // No origin: related stays empty.
    let d = load("[[1]]").unwrap();
    let e = from_node::<Node>(d.root(), &limited(|l| l.max_depth = 1)).unwrap_err();
    assert!(e.related.is_empty());
}

fn opts_apply() -> Options {
    let mut o = opts();
    o.merge_keys = MergeKeys::Apply;
    o
}

/// Key validity is checked before mapping capacity: a NaN key in a full mapping is
/// `InvalidKey`, not `max_mapping_entries`.
#[test]
fn nan_key_checked_before_entry_limit() {
    let o = limited(|l| l.max_mapping_entries = 1);
    let input = "a: 1\n.nan: 2\n";
    let e = err(load_with(input, &o));
    assert_eq!(e.kind, ErrorKind::InvalidKey);
    assert_eq!(src(&e).byte_offset, input.find(".nan").unwrap());
}

/// Findings from the yaml-test-suite run. Block scalars end at their last
/// non-blank line; an omitted value after an explicit key sits on the key's line; every
/// span is inside its parent's; a stream with no documents gives no documents.
#[test]
fn yaml_test_suite_spans_and_empty_streams() {
    let scalar = |input: &str, key: &str| {
        let d = load(input).unwrap();
        let n = d
            .root()
            .value()
            .as_mapping()
            .unwrap()
            .get(key)
            .unwrap()
            .clone();
        let NodeSpan::Source(s) = n.span() else {
            panic!()
        };
        input[s.start.byte_offset..s.end.byte_offset].to_string()
    };
    assert_eq!(scalar("a: |\n  x\nb: 1\n", "a"), "x");
    assert_eq!(
        scalar("a: |\n  x  \n\n", "a"),
        "x  ",
        "trailing spaces on the last line stay"
    );
    assert_eq!(scalar("a: |+\n  x\n\n\nb: 1\n", "a"), "x");
    assert_eq!(scalar("a: >-\n  x\n  y\n", "a"), "x\n  y");
    assert_eq!(scalar("a: |2\n    x\n  \nb: 1\n", "a"), "  x");
    assert_eq!(scalar("a: |\r\n  x\r\n\r\nb: 1\r\n", "a"), "x");
    assert_eq!(scalar("foo: |-\n \tbar\n", "foo"), "\tbar");
    // No non-blank line: the end of the start's line.
    let input = "- |+\n\n\n";
    let d = load(input).unwrap();
    let NodeSpan::Source(s) = d.root().value().as_sequence().unwrap()[0].span() else {
        panic!()
    };
    assert_eq!(&input[s.start.byte_offset..s.end.byte_offset], "|+");

    // Omitted value after an explicit key.
    let input = "? a\n? b\n";
    let d = load(input).unwrap();
    let ends: Vec<usize> = d
        .root()
        .value()
        .as_mapping()
        .unwrap()
        .iter()
        .map(|(_, v)| match v.span() {
            NodeSpan::Source(s) => {
                assert_eq!(s.start, s.end);
                s.start.byte_offset
            }
            _ => panic!(),
        })
        .collect();
    assert_eq!(ends, [3, 7]);

    // Containment, with locations checked against the reference scan.
    for input in [
        "- aaa: |2\n    xxx\n  bbb: |\n    xxx\n",
        "- ? : x\n",
        "x:\n  ? a\n",
        "- |+\n\n\n",
        "a: >2\n   more\n  regular\nb: >2\n\n\n   more\n  regular\n",
        "k:\n  ? - a\n  : >\n    b\nl: 1\n",
        "s: |\r\n  é\r\n\r\n",
        // Bare `?` (empty key and value) on the last line: found by fuzzing.
        "a:\n ?\n",
        "- ?\n",
        "-  \t?\n",
        "? ? ? : ? ?\n",
        "x:\n  ?\n\n\n",
        "? !t\n",
        "- ? !t\n",
        " \n?px:  \n \n\n\n ?\n",
    ] {
        check_locations(input);
    }

    // Streams without documents ; `load` still gives Null (§5.5).
    for input in ["", "# c\n", "...\n", "# c\n...\n", "  \n\n"] {
        assert_eq!(load_all(input).unwrap().len(), 0, "{input:?}");
        assert!(matches!(load(input).unwrap().root().value(), Value::Null));
    }
    assert_eq!(load_all("---\n").unwrap().len(), 1);
    assert_eq!(load_all("---\n...\n---\n").unwrap().len(), 2);
}

/// Indentation is strict in every parsing pass (yaml-test-suite 9C9N).
#[test]
fn indentation_strict() {
    let input = "---\nflow: [a,\nb,\nc]\n";
    let e = err(load(input));
    assert!(matches!(e.kind, ErrorKind::Syntax(_)), "{e:?}");
    assert!(matches!(e.site, Site::Source(_)));
    // the strict pre-pass reports the syntax error ahead of an earlier duplicate key.
    let e = err(load("a: 1\na: 2\nflow: [a,\nb]\n"));
    assert!(matches!(e.kind, ErrorKind::Syntax(_)), "{e:?}");
    let ok = "flow: [a,\n  b,\n  c]\n";
    assert!(load(ok).is_ok());
}

/// Spec §5.5 : an explicit empty document's root sits at the boundary ending the
/// document: the first byte of `...`, the next `---`, or EOF. Zero-document streams keep the
/// EOF rule, and collections are not moved.
#[test]
fn explicit_empty_document_root_at_boundary() {
    let at = |n: &Node| match n.span() {
        NodeSpan::Source(s) => {
            assert_eq!(s.start, s.end, "empty root");
            (s.start.byte_offset, s.start.line, s.start.column)
        }
        NodeSpan::Synthetic => panic!("synthetic root"),
    };
    for (input, want) in [
        ("---\n", (4, 2, 1)),
        ("---\n...\n", (4, 2, 1)),
        ("---\n# c\n\n...\n", (9, 4, 1)),
        ("--- # c\n", (8, 2, 1)),
        ("---", (3, 1, 4)),
        ("--- &a\n...\n", (7, 2, 1)),
        ("%YAML 1.2\n---\n", (14, 3, 1)),
        ("---\r\n...\r\n", (5, 2, 1)),
        ("---\r...\r", (4, 2, 1)),
    ] {
        let d = load(input).unwrap();
        assert!(d.explicit_start());
        assert_eq!(at(d.root()), want, "{input:?}");
        let b = load_bytes(input.as_bytes(), &opts()).unwrap();
        assert_eq!(at(b.root()), want, "bytes {input:?}");
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice(input.as_bytes());
        let b = load_bytes(&bom, &opts()).unwrap();
        assert_eq!(at(b.root()), (want.0 + 3, want.1, want.2), "BOM {input:?}");
    }
    // The consecutive explicit-empty example.
    let docs = load_all("---\n...\n---\n").unwrap();
    assert_eq!(
        docs.iter().map(|d| at(d.root())).collect::<Vec<_>>(),
        [(4, 2, 1), (12, 4, 1)]
    );
    assert!(docs.iter().all(|d| d.explicit_start()
        && d.yaml_version().is_none()
        && matches!(d.root().value(), Value::Null)));
    let e = err(load("---\n...\n---\n"));
    assert_eq!(e.kind, ErrorKind::MultipleDocuments);
    assert_eq!(
        (src(&e).byte_offset, src(&e).line, src(&e).column),
        (8, 3, 1)
    );
    let docs = load_all("---\n---\n").unwrap();
    assert_eq!(
        docs.iter().map(|d| at(d.root())).collect::<Vec<_>>(),
        [(4, 2, 1), (8, 3, 1)]
    );
    // A retained tag: the wrapper and its empty inner node move together; tag_start stays.
    let d = load("--- !t\n...\n").unwrap();
    assert_eq!(at(d.root()), (7, 2, 1));
    assert_eq!(d.root().tag_start().map(|l| l.byte_offset), Some(4));
    let Value::Tagged(t) = d.root().value() else {
        panic!("tagged")
    };
    assert_eq!(at(t.value()), (7, 2, 1));
    // Not explicit, or not empty: unchanged.
    assert_eq!(at(load("# c\n").unwrap().root()), (4, 2, 1));
    assert!(load_all("# c\n...\n").unwrap().is_empty());
    let d = load("--- ~\n").unwrap();
    assert!(
        matches!(d.root().span(), NodeSpan::Source(s) if s.start.byte_offset == 4 && s.end.byte_offset == 5)
    );
    let d = load("---\n:\n").unwrap();
    assert!(d.root().value().as_mapping().is_some());
}

/// Spec §7.1 : `Error::new` is public and formats its message into a bounded
/// buffer, truncating with `…` on a character boundary, without first building the whole
/// text. Serde's constructors and payload strings are bounded the same way.
#[test]
fn error_new_bounded_formatting() {
    struct Chunks(usize, &'static str);
    impl std::fmt::Display for Chunks {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for _ in 0..self.0 {
                f.write_str(self.1)?;
            }
            Ok(())
        }
    }
    let _g = ALLOC_TEST.lock().unwrap();
    BIG.store(0, Ordering::SeqCst);
    THRESHOLD.store(16 << 10, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let loc = Location {
        byte_offset: 3,
        line: 1,
        column: 4,
    };
    let a = Error::new(ErrorKind::Custom, Site::Source(loc), Chunks(1 << 20, "é"));
    let b = <Error as serde::de::Error>::custom(Chunks(1 << 20, "abc"));
    let c = <Error as serde::de::Error>::unknown_field(&"k".repeat(15 << 10), &["a"]);
    TRACK.store(false, Ordering::SeqCst);
    assert_eq!(
        BIG.load(Ordering::SeqCst),
        0,
        "a large buffer was allocated"
    );
    for e in [&a, &b] {
        assert!(
            e.message.len() <= 4096 && e.message.ends_with('…'),
            "{}",
            e.message.len()
        );
    }
    assert!(
        c.message.len() < 512 && c.message.contains('…'),
        "{}",
        c.message
    );
    assert!(a.message.trim_end_matches('…').chars().all(|ch| ch == 'é'));
    assert_eq!(a.location(), Some(loc));
    assert!(a.related.is_empty() && a.path.is_empty() && a.engine_detail.is_none());
    assert_eq!(b.location(), None);
    assert!(matches!(&c.kind, ErrorKind::UnknownField(f) if f.len() <= 1024 && f.ends_with('…')));
    // Short messages are kept whole; a message of exactly the bound is not truncated.
    let d = Error::new(
        ErrorKind::Internal,
        Site::NotSource { last_known: None },
        format_args!("x{}", 1),
    );
    assert_eq!(d.message, "x1");
    let exact = "z".repeat(4096);
    assert_eq!(
        Error::new(ErrorKind::Custom, Site::Source(loc), &exact).message,
        exact
    );
    let over = Error::new(ErrorKind::Custom, Site::Source(loc), "z".repeat(4097))
        .into_detail()
        .message;
    assert!(over.len() <= 4096 && over.ends_with('…'));
}

/// Spec §§6, 7.1, 10 : `encode_error` rechecks a caller-mutated error against the
/// current limits. Descriptive text is truncated; structured identity (the path) is rejected
/// when out of bounds, never cut; a malformed `syntax_kind` is `Internal`. No JSON on error.
#[test]
fn encode_error_rechecks_mutated_errors() {
    let j = |w: String| serde_json::from_str::<serde_json::Value>(&w).unwrap();
    let base = || {
        Error::new(
            ErrorKind::Custom,
            Site::NotSource { last_known: None },
            "small",
        )
    };
    let limit_kind = |e: &Error| match e.kind {
        ErrorKind::LimitExceeded { limit, .. } => Some(limit),
        _ => None,
    };
    let not_source = |e: &Error| matches!(e.site, Site::NotSource { last_known: None });

    // Message: truncated on a character boundary, within 4096 bytes.
    let mut e = base();
    e.message = "é".repeat(3000);
    let w = j(encode_error(&e, &opts()).unwrap());
    let m = w["error"]["message"].as_str().unwrap();
    assert!(
        m.len() <= 4096 && m.ends_with('…') && m.trim_end_matches('…').chars().all(|c| c == 'é'),
        "{}",
        m.len()
    );
    // Type descriptions and field names: truncated at 1024 bytes.
    let mut e = base();
    e.kind = ErrorKind::Type {
        expected: "x".repeat(5000),
        found: "y".into(),
    };
    let w = j(encode_error(&e, &opts()).unwrap());
    assert_eq!(w["error"]["detail"]["found"], "y");
    let x = w["error"]["detail"]["expected"].as_str().unwrap();
    assert!(x.len() <= 1024 && x.ends_with('…'));
    e.kind = ErrorKind::UnknownField("f".repeat(5000));
    let w = j(encode_error(&e, &opts()).unwrap());
    assert!(w["error"]["detail"]["field"].as_str().unwrap().len() <= 1024);

    // Path length: max_depth segments are fine; one more is rejected.
    let mut o = opts();
    o.limits.max_depth = 2;
    let mut e = base();
    e.path = vec![PathSegment::Index(0), PathSegment::Index(1)];
    assert_eq!(
        j(encode_error(&e, &o).unwrap())["error"]["path"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    e.path.push(PathSegment::Index(2));
    let r = encode_error(&e, &o).unwrap_err();
    assert_eq!(limit_kind(&r), Some(LimitKind::MaxDepth));
    assert!(not_source(&r));

    // Path keys: exact identity when within bounds; weight and nesting are checked.
    let mut e = base();
    e.path = vec![PathSegment::Key(Box::new(Value::String(
        "k\u{1}\"\n".into(),
    )))];
    let w = encode_error(&e, &opts()).unwrap();
    assert!(w.contains(r#""path":[{"key":{"kind":"string","value":"k\u0001\"\n","span":null,"via":null,"tag_start":null}}]"#), "{w}");
    e.path = vec![PathSegment::Key(Box::new(Value::String(
        "z".repeat(70 << 10),
    )))];
    assert_eq!(
        limit_kind(&encode_error(&e, &opts()).unwrap_err()),
        Some(LimitKind::MaxKeyBytes)
    );
    let mut nested = Value::Null;
    for _ in 0..3 {
        nested = Value::Sequence(vec![Node::synthetic(nested)]);
    }
    e.path = vec![PathSegment::Key(Box::new(nested.clone()))];
    let mut o = opts();
    o.limits.max_depth = 3;
    assert!(encode_error(&e, &o).is_ok());
    o.limits.max_depth = 2;
    assert_eq!(
        limit_kind(&encode_error(&e, &o).unwrap_err()),
        Some(LimitKind::MaxDepth)
    );
    let wide = Value::Sequence((0..2000).map(|_| Node::synthetic(Value::Null)).collect());
    e.path = vec![PathSegment::Key(Box::new(wide))];
    assert_eq!(
        limit_kind(&encode_error(&e, &opts()).unwrap_err()),
        Some(LimitKind::MaxKeyBytes)
    );

    // syntax_kind must be a snake_case code.
    let mut e = base();
    e.kind = ErrorKind::Syntax("Not A Code".into());
    let r = encode_error(&e, &opts()).unwrap_err();
    assert_eq!(r.kind, ErrorKind::Internal);
    assert!(not_source(&r));
    e.kind = ErrorKind::Syntax("unexpected_token".into());
    assert!(encode_error(&e, &opts()).is_ok());
}

/// Strings are escaped straight into the output under the output budget; no escaped copy
/// of a large scalar is built first (spec §11.1).
#[test]
fn encode_streams_string_escapes() {
    let _g = ALLOC_TEST.lock().unwrap_or_else(|p| p.into_inner());
    let big = format!("k: \"{}\"\n", "a\\t".repeat(200_000));
    let docs = load_all(&big).unwrap();
    let mut o = opts();
    o.limits.max_output_bytes = 64 << 10;
    BIG.store(0, Ordering::SeqCst);
    THRESHOLD.store(128 << 10, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let r = encode(&docs, &o);
    TRACK.store(false, Ordering::SeqCst);
    assert!(matches!(
        r.unwrap_err().kind,
        ErrorKind::OutputLimitExceeded { .. }
    ));
    assert_eq!(
        BIG.load(Ordering::SeqCst),
        0,
        "a large temporary was allocated"
    );
    let w = encode(&load_all("k: \"a\\u0001\\t\\\"\\\\é\"\n").unwrap(), &opts()).unwrap();
    assert!(w.contains(r#""value":"a\u0001\t\"\\é""#), "{w}");
}

/// an empty (blank-only) block scalar keeps granit's start marker; a zero-width one
/// takes the empty-node rule. Pinned examples, with their values.
#[test]
fn empty_block_scalar_anchors() {
    for (input, value, start, end) in [
        ("k: |\n  \n  \nx: 1\n", "", 10, 10),
        ("k: |\n\nx: 1\n", "", 4, 4),
        ("k: |\n\n", "", 3, 4),
        ("k: |\n", "", 3, 4),
        ("k: >-\n\nx: 1\n", "", 5, 5),
        ("k: |+\n\n\nx: 1\n", "\n\n", 5, 5),
        ("k: |\r\n\r\nx: 1\r\n", "", 4, 4),
        ("k: | # c\n\nx: 1\n", "", 8, 8),
        ("- |\n\n- a\n", "", 3, 3),
    ] {
        let d = load(input).unwrap();
        let n = match d.root().value() {
            Value::Mapping(m) => m.iter().next().unwrap().1,
            Value::Sequence(s) => &s[0],
            v => panic!("{v:?}"),
        };
        assert_eq!(n.value().as_str(), Some(value), "{input:?}");
        let NodeSpan::Source(s) = n.span() else {
            panic!("synthetic")
        };
        assert_eq!(
            (s.start.byte_offset, s.end.byte_offset),
            (start, end),
            "{input:?}"
        );
    }
}
