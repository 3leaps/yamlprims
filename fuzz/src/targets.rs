// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fuzz target bodies. libFuzzer binaries (`fuzz_targets/`, built with
//! the `libfuzzer` feature) and the stable replay test (`tests/replay.rs`)
//! call the same functions.

use yamlprims::{
    decode_wire, emit_all, encode, encode_error, encode_lint, encode_plain, lint_yaml11_bytes,
    load_all, load_all_bytes, load_bytes, ErrorKind, Options, Value, WireEnvelope,
};

use crate::{
    assert_json, assert_not_internal, assert_wire_round_trip, doc_eq, is_output_limit,
    select_options,
};

/// Mirrors Go FuzzLoad: error kinds are not Internal, and the single- and
/// multi-document entrypoints agree (§5.5).
pub fn fuzz_load(data: &[u8]) {
    let (opts, input) = select_options(data);
    let all = load_all_bytes(input, &opts);
    let one = load_bytes(input, &opts);
    if let Err(e) = &all {
        assert_not_internal("load_all_bytes", e);
    }
    if let Err(e) = &one {
        assert_not_internal("load_bytes", e);
    }
    let Ok(docs) = all else { return };
    match docs.len() {
        0 => {
            let doc = one.expect("stream with no documents: load_bytes failed");
            assert!(
                matches!(doc.root().value(), Value::Null),
                "no documents: root is not null"
            );
        }
        1 => {
            let doc = one.expect("one document: load_bytes failed");
            assert!(
                doc_eq(&doc, &docs[0]),
                "one document: load_bytes and load_all_bytes disagree"
            );
        }
        n => {
            let err = one
                .err()
                .unwrap_or_else(|| panic!("{n} documents: load_bytes succeeded"));
            assert!(
                matches!(err.kind, ErrorKind::MultipleDocuments),
                "{n} documents: load_bytes returned {err}, want MultipleDocuments"
            );
        }
    }
}

/// Mirrors Go FuzzDecodeWire: anything decode_wire accepts re-encodes, and that
/// encoder output round-trips byte-identically (§11.1).
pub fn fuzz_decode_wire(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let opts = Options::default();
    let Ok(env) = decode_wire(text, &opts) else {
        return;
    };
    let wire = match &env {
        WireEnvelope::Documents(docs) => encode(docs, &opts),
        WireEnvelope::Error(err) => encode_error(err, &opts),
    };
    match wire {
        Ok(wire) => assert_wire_round_trip(&wire),
        Err(e) => assert!(
            is_output_limit(&e),
            "encode of a decoded envelope failed: {e}"
        ),
    }
}

/// Mirrors Go FuzzEmitRoundTrip: the §10.2 self-round-trip invariant.
pub fn fuzz_emit_round_trip(data: &[u8]) {
    let (opts, input) = select_options(data);
    let Ok(docs) = load_all_bytes(input, &opts) else {
        return;
    };
    let emitted = match emit_all(&docs) {
        Ok(s) => s,
        Err(e) => {
            assert_not_internal("emit_all", &e);
            return;
        }
    };
    let again = load_all(&emitted).unwrap_or_else(|e| panic!("emitted YAML does not reload: {e}"));
    assert_eq!(again.len(), docs.len(), "document count changed on reload");
    for (i, (a, b)) in docs.iter().zip(again.iter()).enumerate() {
        assert!(doc_eq(a, b), "document {i} changed on reload");
    }
}

/// Mirrors Go FuzzLintYAML11: §7.2 coordinates and the §11.7 encode_lint
/// preflight accept what the linter produced.
pub fn fuzz_lint_yaml11(data: &[u8]) {
    let opts = Options::default();
    let res = match lint_yaml11_bytes(data, &opts) {
        Ok(r) => r,
        Err(e) => {
            assert_not_internal("lint_yaml11_bytes", &e);
            return;
        }
    };
    assert!(
        res.warning_count >= res.diagnostics.len(),
        "warning_count below diagnostics"
    );
    for (i, d) in res.diagnostics.iter().enumerate() {
        let (s, e) = (&d.span.start, &d.span.end);
        assert!(
            s.line >= 1 && s.column >= 1 && e.line >= 1 && e.column >= 1,
            "diagnostic {i}: coordinates are 1-based"
        );
        assert!(
            s.byte_offset <= e.byte_offset && e.byte_offset <= data.len(),
            "diagnostic {i}: span out of bounds"
        );
    }
    match encode_lint(&res, &opts) {
        Ok(out) => assert_json("lint envelope", &out),
        Err(e) => assert!(
            is_output_limit(&e),
            "encode_lint rejected the linter's result: {e}"
        ),
    }
}

/// Mirrors Go FuzzEncode: wire and plain output is JSON, and wire output
/// round-trips (§11.1).
pub fn fuzz_encode(data: &[u8]) {
    let (opts, input) = select_options(data);
    let docs = match load_all_bytes(input, &opts) {
        Ok(d) => d,
        Err(err) => {
            match encode_error(&err, &opts) {
                Ok(wire) => {
                    assert_json("error envelope", &wire);
                    assert_wire_round_trip(&wire);
                }
                Err(e) => assert!(is_output_limit(&e), "encode_error failed: {e}"),
            }
            return;
        }
    };
    match encode(&docs, &opts) {
        Ok(wire) => {
            assert_json("wire envelope", &wire);
            assert_wire_round_trip(&wire);
        }
        Err(e) => assert!(is_output_limit(&e), "encode failed: {e}"),
    }
    for (i, doc) in docs.iter().enumerate() {
        match encode_plain(doc.root(), &opts) {
            Ok(plain) => assert_json(&format!("document {i} plain output"), &plain),
            Err(e) => assert_not_internal("encode_plain", &e),
        }
    }
}
