//! Conformance tests for public spec revision 1.2 §13.2: "Revision 1.2 error encoding" and
//! "Revision 1.2 empty blocks". Expected values come from the spec text and its §7.2 table,
//! or from transformations of that table whose effect on offsets the spec fixes (a BOM
//! prefix, line-break substitution, a property before the indicator, a preceding sibling,
//! nesting). None is taken from the implementation's output.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use yamlprims::*;

// ---- allocation instrumentation --------------------------------------------
// BIG counts allocations of at least THRESHOLD bytes; EXACT counts allocations of exactly
// WATCH bytes (a clone of a String of that length allocates exactly its length).
struct Counting;
static TRACK: AtomicBool = AtomicBool::new(false);
static THRESHOLD: AtomicUsize = AtomicUsize::new(usize::MAX);
static WATCH: AtomicUsize = AtomicUsize::new(usize::MAX);
static BIG: AtomicUsize = AtomicUsize::new(0);
static EXACT: AtomicUsize = AtomicUsize::new(0);

fn note(size: usize) {
    if TRACK.load(Ordering::Relaxed) {
        if size >= THRESHOLD.load(Ordering::Relaxed) {
            BIG.fetch_add(1, Ordering::Relaxed);
        }
        if size == WATCH.load(Ordering::Relaxed) {
            EXACT.fetch_add(1, Ordering::Relaxed);
        }
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        note(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if l.size() < n {
            note(n);
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;
static ALLOC_TEST: Mutex<()> = Mutex::new(());

/// Runs `f` with allocation tracking; returns (allocations >= threshold, allocations == watch).
fn tracked<R>(threshold: usize, watch: usize, f: impl FnOnce() -> R) -> (R, usize, usize) {
    BIG.store(0, Ordering::SeqCst);
    EXACT.store(0, Ordering::SeqCst);
    THRESHOLD.store(threshold, Ordering::SeqCst);
    WATCH.store(watch, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let r = f();
    TRACK.store(false, Ordering::SeqCst);
    (r, BIG.load(Ordering::SeqCst), EXACT.load(Ordering::SeqCst))
}

// ---- error encoding ---------------------------------------------------------

fn opts() -> Options {
    Options::default()
}

fn with(f: impl FnOnce(&mut Limits)) -> Options {
    let mut o = Options::default();
    f(&mut o.limits);
    o
}

fn base() -> Error {
    Error::new(ErrorKind::Custom, Site::NotSource { last_known: None }, "m")
}

fn json(w: &str) -> serde_json::Value {
    serde_json::from_str(w).unwrap_or_else(|e| panic!("{e}: {w}"))
}

fn key(v: Value) -> PathSegment {
    PathSegment::Key(Box::new(v))
}

/// A rejection by the encoder: exact kind and payload, `NotSource { last_known: None }`,
/// and nothing that would need encoding in turn (no path, no related spans).
#[track_caller]
fn rejected(r: Result<String, Error>, want: ErrorKind) {
    let e = match r {
        Ok(w) => panic!("expected {want:?}, encoded {} bytes", w.len()),
        Err(e) => e,
    };
    assert_eq!(e.kind, want);
    assert!(
        matches!(e.site, Site::NotSource { last_known: None }),
        "{:?}",
        e.site
    );
    assert!(
        e.path.is_empty() && e.related.is_empty(),
        "rejection carries structure"
    );
}

fn limit(limit: LimitKind, configured: u64) -> ErrorKind {
    ErrorKind::LimitExceeded { limit, configured }
}

fn first_value(input: &str) -> Value {
    match load(input).unwrap().root().value() {
        Value::Mapping(m) => m.iter().next().unwrap().1.value().clone(),
        Value::Sequence(s) => s[0].value().clone(),
        v => v.clone(),
    }
}

/// `n` nested one-item sequences around `inner`: nesting depth `n` below the key root.
fn nest(n: usize, inner: Value) -> Value {
    (0..n).fold(inner, |v, _| Value::Sequence(vec![Node::synthetic(v)]))
}

/// §6, §11.1: a two-byte String key weighs 64 (root charge) + 2 = 66.
/// This is the negative control for the root charge: without it, 65 would accept.
#[test]
fn key_weight_includes_root_charge() {
    let mut e = base();
    e.path = vec![key(Value::String("ab".into()))];
    let w = encode_error(&e, &with(|l| l.max_key_bytes = 66)).unwrap();
    assert!(w.contains(r#""path":[{"key":{"kind":"string","value":"ab","span":null,"via":null,"tag_start":null}}]"#), "{w}");
    rejected(
        encode_error(&e, &with(|l| l.max_key_bytes = 65)),
        limit(LimitKind::MaxKeyBytes, 65),
    );
    // A sequence of two Null items: 64 + 2 * 64 = 192; a one-entry mapping {a: b}:
    // 64 + (64 + 64 + 24) + 1 + 1 = 218; a tag wrapper adds its tag bytes and 64.
    e.path = vec![key(Value::Sequence(vec![
        Node::synthetic(Value::Null),
        Node::synthetic(Value::Null),
    ]))];
    assert!(encode_error(&e, &with(|l| l.max_key_bytes = 192)).is_ok());
    rejected(
        encode_error(&e, &with(|l| l.max_key_bytes = 191)),
        limit(LimitKind::MaxKeyBytes, 191),
    );
    e.path = vec![key(first_value("- {a: b}"))];
    assert!(encode_error(&e, &with(|l| l.max_key_bytes = 218)).is_ok());
    rejected(
        encode_error(&e, &with(|l| l.max_key_bytes = 217)),
        limit(LimitKind::MaxKeyBytes, 217),
    );
    let tagged = first_value("!t x");
    let Value::Tagged(t) = &tagged else {
        panic!("{tagged:?}")
    };
    // `!t`: uri "!t", handle "!", suffix "t" (§6 counts all three) = 4 bytes.
    let tb = t.tag().uri().len() + t.tag().handle().len() + t.tag().suffix().len();
    assert_eq!(tb, 4, "{:?}", t.tag());
    e.path = vec![key(tagged)];
    let w = 64 + (tb + 64) + 1;
    assert!(encode_error(&e, &with(|l| l.max_key_bytes = w)).is_ok());
    rejected(
        encode_error(&e, &with(|l| l.max_key_bytes = w - 1)),
        limit(LimitKind::MaxKeyBytes, w as u64 - 1),
    );
}

/// A structured key longer than the 1,024-byte descriptive bound is identity, not
/// description: it is written in full when within `max_key_bytes`.
#[test]
fn long_structured_key_is_kept_whole() {
    let long = "é".repeat(1500); // 3,000 bytes
    let mut e = base();
    e.path = vec![key(Value::String(long.clone())), PathSegment::Index(7)];
    let w = json(&encode_error(&e, &opts()).unwrap());
    assert_eq!(w["error"]["path"][0]["key"]["value"], long.as_str());
    assert_eq!(w["error"]["path"][1]["index"], 7);
    // Within its budget at exactly 64 + 3,000; rejected, not cut, one byte below.
    assert!(encode_error(&e, &with(|l| l.max_key_bytes = 3064)).is_ok());
    rejected(
        encode_error(&e, &with(|l| l.max_key_bytes = 3063)),
        limit(LimitKind::MaxKeyBytes, 3063),
    );
}

/// Path length and key nesting are separate checks with exact boundaries. Root depth is
/// 0, a Sequence or Mapping child adds 1, a Tagged wrapper's child keeps its depth.
#[test]
fn path_length_and_key_nesting_boundaries() {
    let mut e = base();
    e.path = (0..4).map(PathSegment::Index).collect();
    assert!(encode_error(&e, &with(|l| l.max_depth = 4)).is_ok());
    rejected(
        encode_error(&e, &with(|l| l.max_depth = 3)),
        limit(LimitKind::MaxDepth, 3),
    );

    // Nesting 3 (three sequences around a scalar): accepted at max_depth 3, rejected at 2.
    for (inner, label) in [
        (Value::Null, "scalar"),
        (Value::Sequence(vec![]), "empty sequence"),
    ] {
        e.path = vec![key(nest(3, inner))];
        assert!(
            encode_error(&e, &with(|l| l.max_depth = 3)).is_ok(),
            "{label}"
        );
        rejected(
            encode_error(&e, &with(|l| l.max_depth = 2)),
            limit(LimitKind::MaxDepth, 2),
        );
    }
    // Mappings add depth on keys and values alike.
    for src in ["{a: {b: {c: d}}}", "{{a: {b: c}}: d}"] {
        e.path = vec![key(first_value(&format!("- {src}")))];
        assert!(
            encode_error(&e, &with(|l| l.max_depth = 3)).is_ok(),
            "{src}"
        );
        rejected(
            encode_error(&e, &with(|l| l.max_depth = 2)),
            limit(LimitKind::MaxDepth, 2),
        );
    }
    // Tag wrappers keep depth: `!t [!t [!t [x]]]` nests 3 like `[[[x]]]`.
    e.path = vec![key(first_value("- !t [!t [!t [x]]]"))];
    assert!(encode_error(&e, &with(|l| l.max_depth = 3)).is_ok());
    rejected(
        encode_error(&e, &with(|l| l.max_depth = 2)),
        limit(LimitKind::MaxDepth, 2),
    );
    // The two checks are independent: a full-length path with a maximally nested key passes.
    e.path = (0..2)
        .map(PathSegment::Index)
        .chain([key(nest(3, Value::Null))])
        .collect();
    assert!(encode_error(&e, &with(|l| l.max_depth = 3)).is_ok());
}

/// Every C0 control escape, quotes and backslashes, in messages, descriptions, syntax-free
/// structured keys; DEL and non-ASCII pass through.
#[test]
fn every_json_control_escape() {
    let text: String = (0u8..0x20)
        .map(char::from)
        .chain(['"', '\\', '\u{7f}', 'é', '/'])
        .collect();
    let mut want = String::from("\"");
    for b in 0u8..0x20 {
        want.push_str(&match b {
            0x08 => "\\b".into(),
            0x09 => "\\t".into(),
            0x0a => "\\n".into(),
            0x0c => "\\f".into(),
            0x0d => "\\r".into(),
            _ => format!("\\u{b:04x}"),
        });
    }
    want.push_str("\\\"\\\\\u{7f}é/\"");
    let mut e = base();
    e.message = text.clone();
    e.kind = ErrorKind::Type {
        expected: text.clone(),
        found: text.clone(),
    };
    e.path = vec![key(Value::String(text.clone()))];
    let w = encode_error(&e, &opts()).unwrap();
    assert_eq!(w.matches(want.as_str()).count(), 4, "{w}");
    assert_eq!(json(&w)["error"]["message"], text.as_str());
    // The value encoder too (DEL is not printable YAML, so it is left out of the input).
    let yaml = text.replace('\u{7f}', "");
    let v = encode(
        &load_all(&format!("k: {}\n", serde_json::to_string(&yaml).unwrap())).unwrap(),
        &opts(),
    )
    .unwrap();
    assert!(v.contains(&want.replace('\u{7f}', "")), "{v}");
}

/// Descriptive truncation is UTF-8 safe and the `…` marker counts against the bound.
#[test]
fn utf8_safe_truncation_with_marker() {
    // 4-byte characters around the 4,096 and 1,024 bounds, offset by one to two bytes.
    for pad in 0..4 {
        let s = format!("{}{}", "a".repeat(pad), "😀".repeat(2000));
        let mut e = base();
        e.message = s.clone();
        e.kind = ErrorKind::MissingField(s.clone());
        let w = json(&encode_error(&e, &opts()).unwrap());
        for (got, max) in [
            (&w["error"]["message"], 4096),
            (&w["error"]["detail"]["field"], 1024),
        ] {
            let got = got.as_str().unwrap();
            assert!(
                got.len() <= max && got.len() > max - 4 - 3,
                "{pad}: {}",
                got.len()
            );
            let body = got.strip_suffix('…').expect("marker");
            assert!(s.starts_with(body), "{pad}");
        }
    }
    // Exactly at a bound: not truncated, no marker.
    let mut e = base();
    e.message = "x".repeat(4096);
    e.kind = ErrorKind::UnknownField("y".repeat(1024));
    let w = json(&encode_error(&e, &opts()).unwrap());
    assert_eq!(w["error"]["message"].as_str().unwrap().len(), 4096);
    assert_eq!(
        w["error"]["detail"]["field"].as_str().unwrap(),
        "y".repeat(1024)
    );
}

/// The envelope fits exactly at its encoded length and is rejected one byte less, with the
/// configured value; text with escapes makes escaped and raw lengths differ.
#[test]
fn exact_encoded_length_boundary() {
    let mut e = Error::new(
        ErrorKind::Syntax("unexpected_token".into()),
        Site::Source(Location {
            byte_offset: 9,
            line: 2,
            column: 3,
        }),
        "a\u{1}\"b",
    );
    e.path = vec![key(Value::String("q\n".into())), PathSegment::Index(1)];
    let w = encode_error(&e, &opts()).unwrap();
    let n = w.len();
    assert_eq!(
        encode_error(&e, &with(|l| l.max_output_bytes = n)).unwrap(),
        w
    );
    rejected(
        encode_error(&e, &with(|l| l.max_output_bytes = n - 1)),
        ErrorKind::OutputLimitExceeded {
            configured: n as u64 - 1,
        },
    );
    for small in [1, 16, 64] {
        rejected(
            encode_error(&e, &with(|l| l.max_output_bytes = small)),
            ErrorKind::OutputLimitExceeded {
                configured: small as u64,
            },
        );
    }
}

/// After the kind changes, the detail is built from the current kind only, exactly.
#[test]
fn detail_follows_current_kind() {
    let mut e = Error::new(
        ErrorKind::Type {
            expected: "bool".into(),
            found: "string".into(),
        },
        Site::NotSource { last_known: None },
        "m",
    );
    let detail = |e: &Error| json(&encode_error(e, &opts()).unwrap())["error"]["detail"].clone();
    assert_eq!(
        detail(&e),
        serde_json::json!({"expected": "bool", "found": "string"})
    );
    for (kind, name, want) in [
        (ErrorKind::Custom, "custom", serde_json::json!({})),
        (
            limit(LimitKind::MaxNodes, 9),
            "limit_exceeded",
            serde_json::json!({"limit": "max_nodes", "configured": 9}),
        ),
        (
            ErrorKind::OutputLimitExceeded { configured: 5 },
            "output_limit_exceeded",
            serde_json::json!({"configured": 5}),
        ),
        (
            ErrorKind::MissingField("f".into()),
            "missing_field",
            serde_json::json!({"field": "f"}),
        ),
        (
            ErrorKind::Syntax("bad_indent".into()),
            "syntax",
            serde_json::json!({"syntax_kind": "bad_indent"}),
        ),
        (
            ErrorKind::InvalidUtf8 { invalid_len: None },
            "invalid_utf8",
            serde_json::json!({"invalid_len": null}),
        ),
        (
            ErrorKind::DuplicateKey,
            "duplicate_key",
            serde_json::json!({}),
        ),
    ] {
        e.kind = kind;
        let w = json(&encode_error(&e, &opts()).unwrap());
        assert_eq!(w["error"]["kind"], name);
        assert_eq!(w["error"]["detail"], want, "{name}");
    }
}

/// `syntax_kind` shape: 1–64 bytes of `[a-z0-9_]`; anything else is `Internal`.
#[test]
fn syntax_code_shape() {
    let mut e = base();
    for ok in ["a", "0", "_", "unexpected_token", &"a".repeat(64), "x9_"] {
        e.kind = ErrorKind::Syntax(ok.into());
        assert!(encode_error(&e, &opts()).is_ok(), "{ok:?}");
    }
    for bad in [
        "",
        &"a".repeat(65),
        "Upper",
        "a-b",
        "a b",
        "é",
        "a\n",
        "a.b",
        "\u{0}",
    ] {
        e.kind = ErrorKind::Syntax(bad.into());
        rejected(encode_error(&e, &opts()), ErrorKind::Internal);
    }
}

/// Encoding borrows: mutated fields, re-encoded under tighter options (accepted or
/// rejected), are unchanged afterwards, including untruncated long descriptions.
#[test]
fn borrowed_error_is_unchanged() {
    let mut e = base();
    e.message = "m".repeat(10_000);
    e.kind = ErrorKind::Type {
        expected: "e".repeat(3000),
        found: "f".into(),
    };
    e.path = vec![key(Value::String("k".repeat(100))), PathSegment::Index(3)];
    let before = format!("{e:?}");
    for o in [
        opts(),
        with(|l| l.max_depth = 1),
        with(|l| l.max_key_bytes = 100),
        with(|l| l.max_output_bytes = 50),
    ] {
        let _ = encode_error(&e, &o);
        assert_eq!(format!("{e:?}"), before);
    }
    assert!(encode_error(&e, &opts()).is_ok());
    assert_eq!(e.message.len(), 10_000);
}

/// Instrumented preflight and encoding: a rejected key is not traversed past the first
/// bound, accepted keys are not cloned, and no unbounded escaped temporary is built.
#[test]
fn instrumented_preflight_and_encoding() {
    let _g = ALLOC_TEST.lock().unwrap_or_else(|p| p.into_inner());
    // A key holding a node with 200,000 children: 64 * 200,000 is far over 64 KiB, so the
    // walk stops when that node is charged. Scheduling its children would need a 3 MB+ work
    // stack. The wide node is the last sibling, so it is visited while a sibling is pending.
    let wide = Value::Sequence((0..200_000).map(|_| Node::synthetic(Value::Null)).collect());
    let mut e = base();
    e.path = vec![key(Value::Sequence(vec![
        Node::synthetic(Value::Null),
        Node::synthetic(wide),
    ]))];
    let o = with(|l| l.max_key_bytes = 64 << 10);
    let (r, big, _) = tracked(256 << 10, usize::MAX, || encode_error(&e, &o));
    rejected(r, limit(LimitKind::MaxKeyBytes, 64 << 10));
    assert_eq!(big, 0, "traversal continued past the first bound");

    // A 300,001-byte key within budget is written by reference: no allocation of its size.
    let n = 300_001;
    let mut e = base();
    e.path = vec![key(Value::String("k".repeat(n)))];
    let o = with(|l| {
        l.max_key_bytes = 1 << 20;
        l.max_owned_bytes = 1 << 20;
    });
    let (r, _, exact) = tracked(usize::MAX, n, || encode_error(&e, &o));
    assert!(r.unwrap().len() > n);
    assert_eq!(exact, 0, "the key was cloned");

    // A message and descriptions of 1,000,000 control characters: each is bounded before
    // escaping, so nothing near the escaped size (6 MB) is allocated.
    let ctl = "\u{1}".repeat(1_000_000);
    let mut e = base();
    e.message = ctl.clone();
    e.kind = ErrorKind::Type {
        expected: ctl.clone(),
        found: ctl.clone(),
    };
    let (r, big, _) = tracked(256 << 10, usize::MAX, || encode_error(&e, &opts()));
    assert!(r.unwrap().len() < 64 << 10);
    assert_eq!(big, 0, "an escaped temporary was allocated");

    // A 1,000,000-character control key over a 64 KiB output budget fails before escaping.
    let mut e = base();
    e.path = vec![key(Value::String(ctl))];
    let o = with(|l| {
        l.max_key_bytes = 2 << 20;
        l.max_owned_bytes = 2 << 20;
        l.max_output_bytes = 64 << 10;
    });
    let (r, big, _) = tracked(256 << 10, usize::MAX, || encode_error(&e, &o));
    rejected(
        r,
        ErrorKind::OutputLimitExceeded {
            configured: 64 << 10,
        },
    );
    assert_eq!(big, 0, "an escaped temporary was allocated");
}

// ---- empty block scalars (§7.2) ---------------------------------------------

/// The §7.2 table: input, first mapping value or sequence item byte span, decoded value.
const TABLE: [(&str, usize, usize, &str); 9] = [
    ("k: |\n  \n  \nx: 1\n", 10, 10, ""),
    ("k: |\n\nx: 1\n", 4, 4, ""),
    ("k: |\n\n", 3, 4, ""),
    ("k: |\n", 3, 4, ""),
    ("k: >-\n\nx: 1\n", 5, 5, ""),
    ("k: |+\n\n\nx: 1\n", 5, 5, "\n\n"),
    ("k: |\r\n\r\nx: 1\r\n", 4, 4, ""),
    ("k: | # c\n\nx: 1\n", 8, 8, ""),
    ("- |\n\n- a\n", 3, 3, ""),
];

/// An independent line/column scan (§7.2): LF, CRLF and lone CR are breaks; the BOM is
/// not a column; columns count scalar values.
fn reference(input: &str, byte: usize) -> (usize, usize) {
    let (bom, body) = match input.strip_prefix('\u{feff}') {
        Some(b) => (3, b),
        None => (0, input),
    };
    let (mut line, mut col) = (1, 1);
    let mut it = body[..byte - bom].chars().peekable();
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

fn source(n: &Node) -> Span {
    match n.span() {
        NodeSpan::Source(s) => *s,
        NodeSpan::Synthetic => panic!("synthetic node"),
    }
}

fn contained(n: &Node, parent: Option<Span>, input: &str) {
    let s = source(n);
    for l in [s.start, s.end] {
        assert!(
            l.byte_offset <= input.len() && input.is_char_boundary(l.byte_offset),
            "{input:?} {l:?}"
        );
        assert_eq!(
            reference(input, l.byte_offset),
            (l.line, l.column),
            "{input:?} at {l:?}"
        );
    }
    if let (Some(p), None) = (parent, n.via()) {
        assert!(
            p.start.byte_offset <= s.start.byte_offset && s.end.byte_offset <= p.end.byte_offset,
            "{input:?}: {s:?} outside {p:?}"
        );
    }
    match n.value() {
        Value::Sequence(items) => items.iter().for_each(|c| contained(c, Some(s), input)),
        Value::Mapping(m) => m.iter().for_each(|(k, v)| {
            contained(k, Some(s), input);
            contained(v, Some(s), input);
        }),
        Value::Tagged(t) => contained(t.value(), Some(s), input),
        _ => {}
    }
}

/// Loads `input`, checks every node's coordinates and containment, and returns the
/// `nth` entry of the innermost collection along `route` (mapping value or sequence item)
/// with the floor below which its span may not move (preceding sibling or key end).
fn target(input: &str, route: &[usize]) -> (Node, usize) {
    let d = load(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
    contained(d.root(), None, input);
    let mut n = d.root().clone();
    let mut floor = 0;
    for &i in route {
        let (next, f) = match n.value() {
            Value::Mapping(m) => {
                let (k, v) = m.iter().nth(i).unwrap();
                (v.clone(), source(k).end.byte_offset)
            }
            Value::Sequence(s) => (
                s[i].clone(),
                if i == 0 {
                    source(&n).start.byte_offset
                } else {
                    source(&s[i - 1]).end.byte_offset
                },
            ),
            v => panic!("{input:?}: {v:?}"),
        };
        n = next;
        floor = f;
    }
    (n, floor)
}

#[track_caller]
fn expect(input: &str, route: &[usize], start: usize, end: usize, value: &str, tag: Option<&str>) {
    let (n, floor) = target(input, route);
    let s = source(&n);
    assert_eq!(
        (s.start.byte_offset, s.end.byte_offset),
        (start, end),
        "{input:?}"
    );
    assert!(
        s.start.byte_offset >= floor,
        "{input:?}: below floor {floor}"
    );
    let inner = match (n.value(), tag) {
        (Value::Tagged(t), Some(want)) => {
            assert_eq!(t.tag().uri(), want, "{input:?}");
            // The wrapper and its empty inner node share the span.
            assert_eq!(source(t.value()), s, "{input:?}");
            t.value().clone()
        }
        (v, None) => {
            assert!(!matches!(v, Value::Tagged(_)), "{input:?}");
            n.clone()
        }
        (v, Some(_)) => panic!("{input:?}: untagged {v:?}"),
    };
    assert_eq!(inner.value().as_str(), Some(value), "{input:?}");
}

/// Offset map for an edit that inserts `len` bytes at `at`: positions at or after `at` move.
fn shift(at: usize, len: usize) -> impl Fn(usize) -> usize {
    move |o| if o >= at { o + len } else { o }
}

#[test]
fn empty_block_table() {
    for (input, start, end, value) in TABLE {
        let route: &[usize] = &[0];
        expect(input, route, start, end, value, None);
    }
}

/// A UTF-8 BOM adds 3 to every byte offset; lines and columns are unchanged (§7.2).
#[test]
fn empty_block_bom() {
    for (input, start, end, value) in TABLE {
        expect(
            &format!("\u{feff}{input}"),
            &[0],
            start + 3,
            end + 3,
            value,
            None,
        );
    }
}

/// Line breaks: a lone CR keeps every offset of an LF row; CRLF moves each offset by the
/// number of breaks before it. Decoded values normalize breaks to LF.
#[test]
fn empty_block_line_breaks() {
    for (input, start, end, value) in TABLE.iter().filter(|r| !r.0.contains('\r')) {
        expect(&input.replace('\n', "\r"), &[0], *start, *end, value, None);
        let crlf = |o: usize| o + input[..o].matches('\n').count();
        expect(
            &input.replace('\n', "\r\n"),
            &[0],
            crlf(*start),
            crlf(*end),
            value,
            None,
        );
    }
}

/// Properties before the indicator (a local tag, a core `!!str` tag, an anchor, both)
/// shift positions at and after the indicator; a non-core tag keeps a Tagged wrapper whose
/// empty inner String shares its span.
#[test]
fn empty_block_properties() {
    for (input, start, end, value) in TABLE {
        let at = input.find(['|', '>']).unwrap();
        for (props, tag) in [
            ("!t ", Some("!t")),
            ("!!str ", None),
            ("&a ", None),
            ("&a !t ", Some("!t")),
        ] {
            let edited = format!("{}{props}{}", &input[..at], &input[at..]);
            let f = shift(at, props.len());
            expect(&edited, &[0], f(start), f(end), value, tag);
        }
    }
}

/// A preceding sibling moves the row by its length and becomes the floor.
#[test]
fn empty_block_after_sibling() {
    for (input, start, end, value) in TABLE {
        let sib = if input.starts_with('-') {
            "- 0\n"
        } else {
            "a: 0\n"
        };
        let f = shift(0, sib.len());
        expect(
            &format!("{sib}{input}"),
            &[1],
            f(start),
            f(end),
            value,
            None,
        );
    }
}

/// Nested one level under `p:`: every non-empty line is indented by two spaces, which
/// moves each offset by 3 plus 2 per non-empty line starting at or before it.
#[test]
fn empty_block_nested() {
    for (input, start, end, value) in TABLE {
        let mut out = String::from("p:\n");
        let mut starts = Vec::new(); // non-empty line starts in the original
        let mut pos = 0;
        for line in input.split_inclusive('\n') {
            if !matches!(line, "\n" | "\r\n") {
                out.push_str("  ");
                starts.push(pos);
            }
            out.push_str(line);
            pos += line.len();
        }
        let f = |o: usize| o + 3 + 2 * starts.iter().filter(|&&s| s <= o).count();
        expect(&out, &[0, 0], f(start), f(end), value, None);
    }
}

/// Beyond the table: the decoded value follows chomping alone (strip and clip give "",
/// keep gives one LF per blank line), for literal and folded styles, spaces-only lines,
/// comments and end of input. A lone spaces-only line at end of input with no line break
/// counts as one kept line, as in yaml-test-suite JEF9/02 (`- |+\n   ` is `"\n"`). Spans here are not pinned by the spec (the rule is anchored
/// to the engine's marker); they must satisfy coordinates, containment and floors.
#[test]
fn empty_block_chomping_values() {
    for style in ['|', '>'] {
        for (chomp, keep) in [("", false), ("-", false), ("+", true)] {
            for (body, blank) in [
                ("", 0),
                ("\n", 1),
                ("  \n", 1),
                ("\n\n", 2),
                ("  \n \n", 2),
                ("  ", 1),
            ] {
                for (tail, comment) in [("", ""), ("x: 1\n", ""), ("x: 1\n", " # c")] {
                    if !body.is_empty() && !body.ends_with('\n') && !tail.is_empty() {
                        continue;
                    }
                    let input = format!("k: {style}{chomp}{comment}\n{body}{tail}");
                    let want = if keep {
                        "\n".repeat(blank)
                    } else {
                        String::new()
                    };
                    let (n, floor) = target(&input, &[0]);
                    let s = source(&n);
                    assert_eq!(n.value().as_str(), Some(want.as_str()), "{input:?}");
                    assert!(
                        s.start.byte_offset >= floor && s.end.byte_offset <= input.len(),
                        "{input:?}"
                    );
                }
            }
        }
    }
}

/// §7.2 omitted children (`? a\n? b\n`: values at 3 and 7), nested one level and with a
/// BOM or CRLF. Nested entries are indented, so the next entry's marker follows that
/// indentation rather than the break itself.
#[test]
fn omitted_children_nested() {
    let values = |input: &str, route: &[usize]| -> Vec<usize> {
        let (n, _) = target(input, &route[..route.len() - 1]);
        let m = match n.value() {
            Value::Mapping(m) => m.clone(),
            v => panic!("{v:?}"),
        };
        m.iter().map(|(_, v)| source(v).start.byte_offset).collect()
    };
    let root = |input: &str| -> Vec<usize> {
        let d = load(input).unwrap();
        contained(d.root(), None, input);
        let Value::Mapping(m) = d.root().value() else {
            panic!()
        };
        m.iter().map(|(_, v)| source(v).start.byte_offset).collect()
    };
    assert_eq!(root("? a\n? b\n"), [3, 7]);
    assert_eq!(root("\u{feff}? a\n? b\n"), [6, 10]);
    assert_eq!(root("? a\r\n? b\r\n"), [3, 8]);
    // "p:\n" + "  ? a\n" + "  ? b\n": ends of the two `?` lines.
    assert_eq!(values("p:\n  ? a\n  ? b\n", &[0, 0]), [8, 14]);
    assert_eq!(values("p:\n  ? a\n  ? b\nq: 1\n", &[0, 0]), [8, 14]);
    assert_eq!(values("p:\n  ? a\r\n  ? b\r\n", &[0, 0]), [8, 15]);
    // Two levels: "p:\n" + "  r:\n" + "    ? a\n" + "  s: 1\n".
    assert_eq!(values("p:\n  r:\n    ? a\n  s: 1\n", &[0, 0, 0]), [15]);
    // An omitted value after `a:` stays at its floor, the end of its key.
    assert_eq!(values("p:\n  a:\n  b: 1\n", &[0, 0]), [6, 13]);
}

/// A zero-width block collection (a mapping of empty entries) keeps its place when it is
/// an omitted-looking child: its children share its point. Found by fuzzing, which
/// moved such a mapping back over indentation and breaks without its children.
#[test]
fn zero_width_collection_is_not_moved() {
    for input in [
        "?\n\n :\n\n\n?\n\n",
        "?\n\n :\n",
        "---\na:\r:\r\r :",
        "p:\n  ?\n\n   :\n",
    ] {
        let d = load(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        contained(d.root(), None, input);
    }
    let d = load("?\n\n :\n").unwrap();
    let Value::Mapping(m) = d.root().value() else {
        panic!()
    };
    let (k, _) = m.iter().next().unwrap();
    assert!(matches!(k.value(), Value::Mapping(_)));
    assert_eq!(
        (source(k).start.byte_offset, source(k).end.byte_offset),
        (4, 4)
    );
    // Tagged zero-width collections keep their place too (wrapper and inner mapping), as
    // keys, sequence items and nested keys: the `:` after the indentation.
    for (input, route, at) in [
        ("?\n !m\n :\n", &[][..], 7),
        ("?\n\n !m\n :\n", &[][..], 8),
        ("- !m\n  :\n", &[][..], 7),
        ("p:\n  ?\n\n   !m\n   :\n", &[0][..], 17),
    ] {
        let d = load(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        contained(d.root(), None, input);
        let mut n = d.root().clone();
        for &i in route {
            let Value::Mapping(m) = n.value() else {
                panic!("{input:?}")
            };
            let next = m.iter().nth(i).unwrap().1.clone();
            n = next;
        }
        let first = match n.value() {
            Value::Mapping(m) => m.iter().next().unwrap().0.clone(),
            Value::Sequence(s) => s[0].clone(),
            v => panic!("{input:?}: {v:?}"),
        };
        let Value::Tagged(t) = first.value() else {
            panic!("{input:?}: untagged")
        };
        assert!(matches!(t.value().value(), Value::Mapping(_)), "{input:?}");
        for node in [&first, t.value()] {
            assert_eq!(
                (source(node).start.byte_offset, source(node).end.byte_offset),
                (at, at),
                "{input:?}"
            );
        }
    }
}
