//! Revision 1.7 resolved tag identity and `%TAG` separation (spec §4.4, §7.1, §11.1).
//! Expected kinds and source coordinates are written from the specification text,
//! not captured from the implementation, except the two cases labelled as pinned
//! engine/profile compatibility expectations, whose codes come from the pinned
//! engine and whose coordinates were still counted by hand.
use yamlprims::{
    decode_wire, emit_all, encode, encode_error, load_all_with, ErrorKind, LimitKind, Location,
    Options, Site, SyntaxKind, UnknownTags, Value, WireEnvelope,
};

fn opts(reject: bool, max_depth: Option<usize>) -> Options {
    let mut o = Options::default();
    if reject {
        o.unknown_tags = UnknownTags::Reject;
    }
    if let Some(d) = max_depth {
        o.limits.max_depth = d;
    }
    o
}

fn at(byte_offset: usize, line: usize, column: usize) -> Site {
    Site::Source(Location {
        byte_offset,
        line,
        column,
    })
}

const TAG: ErrorKind = ErrorKind::Syntax(SyntaxKind::InvalidGlobalTagCharacter);
const GAP: ErrorKind = ErrorKind::Syntax(SyntaxKind::ExpectedWhitespace);
const BANG: ErrorKind = ErrorKind::Syntax(SyntaxKind::ExpectedTagDirectiveBang);
const TERM: ErrorKind = ErrorKind::Syntax(SyntaxKind::InvalidTagTerminator);

fn depth2() -> ErrorKind {
    ErrorKind::LimitExceeded {
        limit: LimitKind::MaxDepth,
        configured: 2,
    }
}

#[test]
fn refused_identities_and_separation_select_kind_and_site() {
    let depth = ErrorKind::LimitExceeded {
        limit: LimitKind::MaxDepth,
        configured: 2,
    };
    let cases: Vec<(&str, &str, Options, ErrorKind, Site)> = vec![
        (
            "example 6.25",
            "!<$:?> value\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "digit scheme",
            "key: !<9ab:c> value\n",
            opts(false, None),
            TAG,
            at(5, 1, 6),
        ),
        (
            "no colon",
            "- !<abc> value\n",
            opts(false, None),
            TAG,
            at(2, 1, 3),
        ),
        (
            "bad scheme char",
            "!<a*b:c> value\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "sequence",
            "!<abc> [1, 2]\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "mapping",
            "!<abc> {k: v}\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "expansion",
            "%TAG !n! 12\n--- !n!3 v\n",
            opts(false, None),
            TAG,
            at(16, 2, 5),
        ),
        (
            "secondary redefined",
            "%TAG !! 12\n--- !!3 v\n",
            opts(false, None),
            TAG,
            at(15, 2, 5),
        ),
        (
            "reject policy",
            "!<abc> value\n",
            opts(true, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "reject: valid unknown first, invalid later",
            "a: !<tag:example.com,2026:t> x\nb: !<abc> y\n",
            opts(true, None),
            TAG,
            at(34, 2, 4),
        ),
        (
            "before later syntax",
            "a: !<abc> 1\nb: [\n",
            opts(false, None),
            TAG,
            at(3, 1, 4),
        ),
        (
            "depth first",
            "[[!<abc> [1]]]\n",
            opts(false, Some(2)),
            depth,
            at(9, 1, 10),
        ),
        (
            "bom and multibyte",
            "\u{feff}\u{e9}: !<abc> x\n",
            opts(false, None),
            TAG,
            at(7, 1, 4),
        ),
        (
            "crlf",
            "a: 1\r\nb: !<abc> x\r\n",
            opts(false, None),
            TAG,
            at(9, 2, 4),
        ),
        (
            "gap, used",
            "%TAG !e!tag:example.com,2026:\n--- !e!a b\n",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        (
            "gap, unused",
            "%TAG !e!tag:example.com,2026:\n--- b\n",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        (
            "gap, secondary handle",
            "%TAG !!tag:example.com,2026:\n--- b\n",
            opts(false, None),
            GAP,
            at(7, 1, 8),
        ),
        (
            "gap, primary handle",
            "%TAG !:x\n--- b\n",
            opts(false, None),
            GAP,
            at(6, 1, 7),
        ),
        (
            "gap, second directive, crlf",
            "%TAG !e! tag:a,1:\r\n%TAG !f!tag:b,1:\r\n--- x\r\n",
            opts(false, None),
            GAP,
            at(27, 2, 9),
        ),
        (
            "gap, bom",
            "\u{feff}%TAG !e!tag:example.com,2026:\n--- b\n",
            opts(false, None),
            GAP,
            at(11, 1, 9),
        ),
        (
            "gap before later syntax",
            "%TAG !e!tag:a,1:\n--- [\n",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        (
            "gap, second document",
            "a\n...\n%TAG !e!tag:a,1:\n--- b\n",
            opts(false, None),
            GAP,
            at(14, 3, 9),
        ),
        (
            "handle error wins",
            "%TAG !e0 a:\n%TAG !f!b:\n--- x\n",
            opts(false, None),
            ErrorKind::Syntax(SyntaxKind::ExpectedTagDirectiveBang),
            at(0, 1, 1),
        ),
        (
            "empty prefix unchanged",
            "%TAG !e!\n--- x\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "earlier tag identity wins",
            "!<abc> x\n...\n%TAG !e!x:\n--- y\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "earlier depth wins",
            "[[[1]]]\n...\n%TAG !e!x:\n--- y\n",
            opts(false, Some(2)),
            depth2(),
            at(2, 1, 3),
        ),
        // Pinned engine/profile compatibility expectation, not a YAML 1.2.2
        // diagnostic or independent format evidence. The syntax code is the one
        // the pinned engine reports, granit-parser 2.0.0 (crates.io, Cargo.lock
        // checksum da55feca5d1625466bae068f9fd4b87892caeae70c3fe671ddb0f50d41aebf37),
        // mapped 1:1 by `engine::syntax_kind` (granit `ErrorKind::ExpectedBlockSequenceEntry` ->
        // `SyntaxKind::ExpectedBlockSequenceEntry`; the Go twin's `scanErrExpectedBlockSequenceEntry` -> `SyntaxExpectedBlockSequenceEntry`).
        // The control detects that the earlier parser error wins over the directive check; the
        // source coordinate was counted by hand: `- a\n` is bytes 0-3, so `b` is byte 4, line 2, column 1.
        (
            "earlier parser error wins",
            "- a\nb: c\n...\n%TAG !e!x:\n--- y\n",
            opts(false, None),
            ErrorKind::Syntax(SyntaxKind::ExpectedBlockSequenceEntry),
            at(4, 2, 1),
        ),
        (
            "gap before later tag identity",
            "%TAG !e!x:\n--- !<abc> y\n",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        (
            "gap before later depth",
            "%TAG !e!x:\n--- [[[1]]]\n",
            opts(false, Some(2)),
            GAP,
            at(8, 1, 9),
        ),
        (
            "gap before later syntax",
            "%TAG !e!x:\n--- [\n",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        (
            "gap then end of input",
            "%TAG !e!x:",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        (
            "gap then NUL",
            "%TAG !e!x:\0--- y\n",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        // Pinned engine/profile compatibility expectation, not a YAML 1.2.2
        // diagnostic or independent format evidence. The syntax code is the one
        // the pinned engine reports, granit-parser 2.0.0 (crates.io, Cargo.lock
        // checksum da55feca5d1625466bae068f9fd4b87892caeae70c3fe671ddb0f50d41aebf37),
        // mapped 1:1 by `engine::syntax_kind` (granit `ErrorKind::InvalidDirectiveTerminator` ->
        // `SyntaxKind::InvalidDirectiveTerminator`; the Go twin's `scanErrInvalidDirectiveTerminator` -> `SyntaxInvalidDirectiveTerminator`).
        // The control detects that the scanner failure inside the same directive wins over the directive check; the
        // source coordinate was counted by hand: the directive starts at byte 0, line 1, column 1.
        (
            "terminator error wins over gap",
            "%TAG !e!x: ]\n--- y\n",
            opts(false, None),
            ErrorKind::Syntax(SyntaxKind::InvalidDirectiveTerminator),
            at(0, 1, 1),
        ),
        (
            "gap before later duplicate key",
            "%TAG !e!x:\n--- a: 1\na: 2\n",
            opts(false, None),
            GAP,
            at(8, 1, 9),
        ),
        (
            "duplicate handle before gap",
            "%TAG !e! a:\n%TAG !e!b:\n--- x\n",
            opts(false, None),
            ErrorKind::Syntax(SyntaxKind::DuplicateTagDirective),
            at(12, 2, 1),
        ),
        (
            "prefix scan error wins over gap",
            "%TAG !e!%zz\n--- x\n",
            opts(false, None),
            ErrorKind::Syntax(SyntaxKind::InvalidTagEscape),
            at(0, 1, 1),
        ),
        (
            "underscore handle, used",
            "%TAG !a_b! tag:example.com,2026:\n--- !a_b!y z\n",
            opts(false, None),
            BANG,
            at(0, 1, 1),
        ),
        (
            "underscore handle, unused",
            "%TAG !a_b! tag:example.com,2026:\n--- z\n",
            opts(false, None),
            BANG,
            at(0, 1, 1),
        ),
        (
            "underscore handle and gap",
            "%TAG !a_b!x:\n--- z\n",
            opts(false, None),
            BANG,
            at(0, 1, 1),
        ),
        (
            "underscore handle use",
            "--- !a_b!y z\n",
            opts(false, None),
            TERM,
            at(4, 1, 5),
        ),
        (
            "underscore handle use in flow",
            "k: [ !a_b!y z ]\n",
            opts(false, None),
            TERM,
            at(5, 1, 6),
        ),
        (
            "verbatim bang",
            "!<!> x\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "verbatim bang in sequence",
            "- !<!> x\n",
            opts(false, None),
            TAG,
            at(2, 1, 3),
        ),
        (
            "verbatim bang on collection",
            "!<!> [1]\n",
            opts(false, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "verbatim bang, reject",
            "!<!> x\n",
            opts(true, None),
            TAG,
            at(0, 1, 1),
        ),
        (
            "verbatim bang on collection, reject",
            "!<!> {a: 1}\n",
            opts(true, None),
            TAG,
            at(0, 1, 1),
        ),
    ];
    for (name, input, o, kind, site) in cases {
        let err = load_all_with(input, &o).expect_err(name);
        assert_eq!(err.kind, kind, "{name}");
        assert_eq!(err.site, site, "{name}");
    }
}

#[test]
fn refused_identity_wire_bytes() {
    let err = load_all_with("!<$:?> value\n", &Options::default()).unwrap_err();
    assert_eq!(
        encode_error(&err, &Options::default()).unwrap(),
        r#"{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"invalid_global_tag_character"},"site":{"source":[0,1,1]},"related":[],"path":[],"message":"syntax error: resolved tag is neither a local tag nor a global tag URI"}}"#
    );
    let err = load_all_with("%TAG !!tag:a,1:\n--- b\n", &Options::default()).unwrap_err();
    assert_eq!(
        encode_error(&err, &Options::default()).unwrap(),
        r#"{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"expected_whitespace"},"site":{"source":[7,1,8]},"related":[],"path":[],"message":"syntax error: expected whitespace"}}"#
    );
}

#[test]
fn accepted_identities_close_through_wire_and_emit() {
    let cases: &[(&str, &str)] = &[
        ("!<x:> value\n", "x:"),
        (
            "!<tag:example.com,2026:item> value\n",
            "tag:example.com,2026:item",
        ),
        ("!local value\n", "!local"),
        (
            "%TAG !p! ta\n--- !p!g:example.com/x v\n",
            "tag:example.com/x",
        ),
        ("!<%74ag:example.com,2026:x> v\n", "tag:example.com,2026:x"),
        ("!local%21x v\n", "!local!x"),
        ("!<tag:example.com,2026:s> [1]\n", "tag:example.com,2026:s"),
        ("--- !a_b z\n", "!a_b"),
        (
            "%TAG !e! tag:example.com,2026:\n--- !e!a_b z\n",
            "tag:example.com,2026:a_b",
        ),
        (
            "!<tag:example.com,2026:a_b> z\n",
            "tag:example.com,2026:a_b",
        ),
        (
            "%TAG !a-1! tag:example.com,2026:\n--- !a-1!y z\n",
            "tag:example.com,2026:y",
        ),
        (
            "%TAG !e! tag:example.com,2026:\n--- !e!a b\n",
            "tag:example.com,2026:a",
        ),
        (
            "%TAG !e!\ttag:example.com,2026:\n--- !e!a b\n",
            "tag:example.com,2026:a",
        ),
    ];
    let o = Options::default();
    for (input, uri) in cases {
        let docs = load_all_with(input, &o).unwrap_or_else(|e| panic!("{input:?}: {e:?}"));
        match &docs[0].root().value() {
            Value::Tagged(t) => assert_eq!(t.tag().uri(), *uri, "{input:?}"),
            other => panic!("{input:?}: expected Tagged, got {other:?}"),
        }
        let wire = encode(&docs, &o).unwrap();
        let WireEnvelope::Documents(back) = decode_wire(&wire, &o).unwrap() else {
            panic!("{input:?}: decode_wire returned an error envelope");
        };
        assert_eq!(encode(&back, &o).unwrap(), wire, "{input:?}");
        let emitted = emit_all(&docs).unwrap();
        let reloaded = load_all_with(&emitted, &o).unwrap();
        match reloaded[0].root().value() {
            Value::Tagged(t) => assert_eq!(t.tag().uri(), *uri, "{input:?} reload"),
            other => panic!("{input:?}: reload lost the tag: {other:?}"),
        }
    }
    // An unused scheme-less prefix and the non-specific / Core tags load unchanged.
    for input in [
        "%TAG !n! 12\n--- v\n",
        "- !!str 1\n- ! 2\n- !!int 3\n",
        "! [1]\n",
    ] {
        let docs = load_all_with(input, &o).unwrap();
        let mut reject = Options::default();
        reject.unknown_tags = UnknownTags::Reject;
        if input.starts_with('!') {
            assert!(
                load_all_with(input, &reject).is_ok(),
                "{input:?} under Reject"
            );
        }
        let wire = encode(&docs, &o).unwrap();
        assert!(matches!(
            decode_wire(&wire, &o).unwrap(),
            WireEnvelope::Documents(_)
        ));
    }
}

#[test]
fn scanner_failure_at_end_of_input_with_directive_text_is_an_error() {
    // A scanner failure reported at the end of input, after a line break, in
    // input that contains `%TAG`, is returned as an error (never a panic).
    for input in ["a: '%TAG\n", "%TAG: [\n"] {
        assert!(
            load_all_with(input, &Options::default()).is_err(),
            "{input:?}"
        );
    }
}
