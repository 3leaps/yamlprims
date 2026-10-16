//! Conformance tests for public spec revision 1.3 (`spec-v1.3`), §13.2 "Revision 1.3
//! anchors", "EOF keep chomping", "UTF-8 input surface" and the f32 conversion path.
//! Kept apart from `spec_v12.rs`: these expectations bind to 1.3, not to 1.2 profiles.
//! Every expected value is copied from the 1.3 text (§1, §7.2, §8), not from the candidate.

use yamlprims::*;

/// Independent line/column scan (§7.2): LF, CRLF and lone CR are breaks; the BOM is not a
/// column; columns count scalar values.
fn reference(input: &[u8], byte: usize) -> (usize, usize) {
    let bom = if input.starts_with(b"\xef\xbb\xbf") {
        3
    } else {
        0
    };
    let text = std::str::from_utf8(&input[bom..byte]).expect("valid prefix");
    let (mut line, mut col) = (1, 1);
    let mut it = text.chars().peekable();
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

/// Coordinates of every node, and recursive containment of parsed (non-copied) children.
fn contained(n: &Node, parent: Option<Span>, input: &str) {
    let s = source(n);
    for l in [s.start, s.end] {
        assert_eq!(
            reference(input.as_bytes(), l.byte_offset),
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
    // Only the copy's link to its insertion parent is exempt; its children still lie
    // inside its (definition) span.
    let own = Some(s);
    match n.value() {
        Value::Sequence(items) => items.iter().for_each(|c| contained(c, own, input)),
        Value::Mapping(m) => m.iter().for_each(|(k, v)| {
            contained(k, own, input);
            contained(v, own, input);
        }),
        Value::Tagged(t) => contained(t.value(), own, input),
        _ => {}
    }
}

fn entry(n: &Node, i: usize, key: bool) -> (Node, usize) {
    match n.value() {
        Value::Mapping(m) => {
            let (k, v) = m.iter().nth(i).unwrap();
            if key {
                let floor = if i == 0 {
                    source(n).start
                } else {
                    source(m.iter().nth(i - 1).unwrap().1).end
                };
                (k.clone(), floor.byte_offset)
            } else {
                (v.clone(), source(k).end.byte_offset)
            }
        }
        v => panic!("not a mapping: {v:?}"),
    }
}

/// The 1.3 §7.2 rows: (input, path of (entry index, key?) steps, span, check of the value).
#[test]
fn revision_1_3_anchor_rows() {
    type Check = fn(&Value) -> bool;
    let empty_str: Check = |v| v.as_str() == Some("");
    let null: Check = |v| matches!(v, Value::Null);
    let mapping: Check = |v| matches!(v, Value::Mapping(_));
    type Row = (
        &'static str,
        &'static [(usize, bool)],
        usize,
        Option<&'static str>,
        Check,
    );
    let rows: [Row; 10] = [
        ("\u{feff}k: |\n\nx: 1\n", &[(0, false)], 7, None, empty_str),
        ("k: |\r\rx: 1\r", &[(0, false)], 4, None, empty_str),
        ("k: !t |\n\nx: 1\n", &[(0, false)], 7, Some("!t"), empty_str),
        ("k: !!str |\n\nx: 1\n", &[(0, false)], 10, None, empty_str),
        ("k: &a |\n\nx: 1\n", &[(0, false)], 7, None, empty_str),
        ("a: 0\nk: |\n\nx: 1\n", &[(1, false)], 9, None, empty_str),
        (
            "p:\n  ? a\n  ? b\n",
            &[(0, false), (0, false)],
            8,
            None,
            null,
        ),
        (
            "p:\n  k: |\n\n  x: 1\n",
            &[(0, false), (0, false)],
            9,
            None,
            empty_str,
        ),
        ("?\n\n :\n", &[(0, true)], 4, None, mapping),
        ("?\n !m\n :\n", &[(0, true)], 7, Some("!m"), mapping),
    ];
    for (input, route, at, tag, check) in rows {
        let d = load(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        contained(d.root(), None, input);
        let mut n = d.root().clone();
        let mut floor = 0;
        for &(i, key) in route {
            (n, floor) = entry(&n, i, key);
        }
        let s = source(&n);
        assert_eq!(
            (s.start.byte_offset, s.end.byte_offset),
            (at, at),
            "{input:?}"
        );
        assert!(at >= floor, "{input:?}: below floor {floor}");
        let inner = match (n.value(), tag) {
            (Value::Tagged(t), Some(want)) => {
                assert_eq!(t.tag().uri(), want, "{input:?}");
                assert_eq!(source(t.value()), s, "{input:?}: wrapper and inner differ");
                t.value().value().clone()
            }
            (Value::Tagged(_), None) => panic!("{input:?}: unexpected tag"),
            (_, Some(_)) => panic!("{input:?}: tag not retained"),
            (v, None) => v.clone(),
        };
        assert!(check(&inner), "{input:?}: {inner:?}");
    }
}

/// §13.2: nested anchors inside alias and merge copies keep the definition's spans
/// (copies are not re-normalized), and the copied subtree matches its definition.
#[test]
fn nested_anchors_inside_copies() {
    let input = "p: &x\n  ? a\n  ? b\nq: *x\n";
    let d = load(input).unwrap();
    contained(d.root(), None, input);
    let (p, _) = entry(d.root(), 0, false);
    let (q, _) = entry(d.root(), 1, false);
    let spans = |n: &Node| -> Vec<(usize, usize)> {
        let Value::Mapping(m) = n.value() else {
            panic!()
        };
        m.iter()
            .flat_map(|(k, v)| [source(k), source(v)])
            .map(|s| (s.start.byte_offset, s.end.byte_offset))
            .collect()
    };
    // "p: &x\n" + "  ? a\n" + "  ? b\n": omitted values at the ends of the `?` lines.
    assert_eq!(spans(&p), [(10, 11), (11, 11), (16, 17), (17, 17)]);
    assert_eq!(spans(&q), spans(&p));
    let mut o = Options::default();
    o.merge_keys = MergeKeys::Apply;
    let input = "base: &b\n  ? a\nm:\n  <<: *b\n  c: 1\n";
    let d = load_with(input, &o).unwrap();
    contained(d.root(), None, input);
    let (base, _) = entry(d.root(), 0, false);
    let (merged, _) = entry(d.root(), 1, false);
    let (bv, _) = entry(&base, 0, false);
    let (mv, _) = entry(&merged, 0, false);
    // "base: &b\n" + "  ? a\n": the omitted value is at 14, in the definition and the copy.
    assert_eq!(source(&bv).start.byte_offset, 14);
    assert_eq!(source(&mv), source(&bv));
}

/// §7.2 keep chomping at EOF: a physical final break counts, an unterminated spaces-only
/// tail does not; CRLF body breaks normalize to LF.
#[test]
fn keep_chomping_at_eof() {
    for (input, want) in [
        ("k: |+\n\n   ", "\n"),
        ("k: |+\n\n   \n", "\n\n"),
        ("k: |+\r\n\r\n   ", "\n"),
    ] {
        let d = load(input).unwrap();
        let (v, _) = entry(d.root(), 0, false);
        assert_eq!(v.value().as_str(), Some(want), "{input:?}");
    }
}

/// §1, §7.2: the UTF-8 input surface. Invalid UTF-8 at the first invalid original byte;
/// valid UTF-8 containing raw NULs reaches YAML syntax validation.
#[test]
fn utf8_input_surface() {
    let o = Options::default();
    let at = |r: Result<Document, Error>| -> (ErrorKind, usize) {
        let e = r.expect_err("must fail");
        let Site::Source(l) = e.site else {
            panic!("{:?}", e.site)
        };
        (e.kind.clone(), l.byte_offset)
    };
    let (k, b) = at(load_bytes(b"\xff\xfek\x00", &o));
    assert!(
        matches!(k, ErrorKind::InvalidUtf8 { .. }) && b == 0,
        "{k:?} {b}"
    );
    let (k, b) = at(load_bytes(b"\x00\x00\xfe\xff\x00\x00\x00k", &o));
    assert!(
        matches!(k, ErrorKind::InvalidUtf8 { .. }) && b == 2,
        "{k:?} {b}"
    );
    // BOM-less UTF-16LE for "k: v\n".
    let (k, b) = at(load_bytes(b"\x6b\x00\x3a\x00\x20\x00\x76\x00\x0a\x00", &o));
    assert!(matches!(k, ErrorKind::Syntax(_)) && b == 1, "{k:?} {b}");
    // A raw NUL is a syntax error; an escaped `\0` is a valid NUL character.
    let (k, b) = at(load_bytes(b"k: a\x00\n", &o));
    assert!(matches!(k, ErrorKind::Syntax(_)) && b == 4, "{k:?} {b}");
    let d = load_bytes(b"k: \"a\\0b\"\n", &o).unwrap();
    assert_eq!(
        entry(d.root(), 0, false).0.value().as_str(),
        Some("a\u{0}b")
    );
    // The optional UTF-8 BOM is accepted and counted in offsets.
    let d = load_bytes(b"\xef\xbb\xbfk: v\n", &o).unwrap();
    assert_eq!(source(&entry(d.root(), 0, false).0).start.byte_offset, 6);
    // Input size: the gate runs on the raw length, before UTF-8 validation.
    let input = b"\xef\xbb\xbfk: v\n";
    let mut o = Options::default();
    o.limits.max_input_bytes = input.len();
    assert!(load_bytes(input, &o).is_ok());
    o.limits.max_input_bytes = input.len() - 1;
    let e = load_bytes(input, &o).unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxInputBytes,
            configured: input.len() as u64 - 1
        }
    );
    let mut o = Options::default();
    o.limits.max_input_bytes = 2;
    let e = load_bytes(b"\xff\xfe\xff", &o).unwrap_err();
    assert!(
        matches!(
            e.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxInputBytes,
                ..
            }
        ),
        "size gate first: {:?}",
        e.kind
    );
}

/// §8: f32 is the binary64 Value rounded to binary32, through both typed paths; the
/// literal is not reparsed. Int-to-f32 still needs exact representability.
#[test]
fn f32_double_rounding_both_paths() {
    let lit = "1.00000005960464477539062500000000000001";
    let direct_next = f32::from_bits(1.0f32.to_bits() + 1);
    assert_eq!(
        lit.parse::<f32>().unwrap(),
        direct_next,
        "direct decimal-to-binary32 rounds up"
    );
    let via_str: f32 = from_str(lit).unwrap();
    assert_eq!(via_str, 1.0f32);
    let d = load(lit).unwrap();
    let via_node: f32 = from_node(d.root(), &Options::default()).unwrap();
    assert_eq!(via_node, 1.0f32);
    assert_eq!(from_str::<f32>("16777216").unwrap(), 16777216.0);
    assert!(
        from_str::<f32>("16777217").is_err(),
        "Int not exactly representable"
    );
}
