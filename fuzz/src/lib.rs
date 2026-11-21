// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared helpers for the fuzz targets. The targets mirror the Go `Fuzz*`
//! functions in `yaml/fuzz_test.go` and check only properties the
//! specification states (see `fuzz/README.md`).

pub mod targets;

use yamlprims::{
    decode_wire, encode, encode_error, Document, Error, ErrorKind, MergeKeys, Options, Value,
    WireEnvelope,
};

/// Picks the load options from the first byte, so one corpus covers the
/// default, the I-JSON default and both opt-in merge-key policies. The
/// selector byte is not part of the YAML input. Same mapping as the Go targets.
pub fn select_options(data: &[u8]) -> (Options, &[u8]) {
    let Some((&sel, rest)) = data.split_first() else {
        return (Options::default(), data);
    };
    let mut opts = Options::default();
    match sel % 4 {
        1 => opts = Options::ijson_default(),
        2 => opts.merge_keys = MergeKeys::AsKey,
        3 => opts.merge_keys = MergeKeys::Apply,
        _ => {}
    }
    (opts, rest)
}

/// Prefix of every implementation-quality failure. Replay and the smoke
/// wrapper report these with their own kind, apart from property failures.
pub const QUALITY_PREFIX: &str = "implementation-quality:";

/// Implementation-quality check, not a specification property: the
/// specification allows `Internal` (for example when the engine cannot supply
/// a byte offset, §7.2), so an `Internal` from an in-memory call is reported
/// as a quality failure for review rather than as non-conformance.
pub fn quality_not_internal(what: &str, err: &Error) {
    if matches!(err.kind, ErrorKind::Internal) {
        panic!("{QUALITY_PREFIX} internal error from {what}");
    }
}

pub fn is_output_limit(err: &Error) -> bool {
    matches!(err.kind, ErrorKind::OutputLimitExceeded { .. })
}

/// §5.1 `key_eq` extended as the §10.2 self-round-trip requires: NaN equals
/// NaN and -0.0 keeps its sign bit. Mapping entries are compared in order
/// (a `Mapping` preserves insertion order and the emitter writes it).
pub fn value_eq(a: &Value, b: &Value) -> bool {
    let mut stack = vec![(a, b)];
    while let Some((a, b)) = stack.pop() {
        match (a, b) {
            (Value::Null, Value::Null) => {}
            (Value::Bool(x), Value::Bool(y)) if x == y => {}
            (Value::Int(x), Value::Int(y)) if x == y => {}
            (Value::Float(x), Value::Float(y)) if x.to_bits() == y.to_bits() => {}
            (Value::String(x), Value::String(y)) if x == y => {}
            (Value::Tagged(x), Value::Tagged(y)) => {
                if x.tag().uri() != y.tag().uri() {
                    return false;
                }
                stack.push((x.value().value(), y.value().value()));
            }
            (Value::Sequence(x), Value::Sequence(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                stack.extend(x.iter().zip(y.iter()).map(|(p, q)| (p.value(), q.value())));
            }
            (Value::Mapping(x), Value::Mapping(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                for ((ka, va), (kb, vb)) in x.iter().zip(y.iter()) {
                    stack.push((ka.value(), kb.value()));
                    stack.push((va.value(), vb.value()));
                }
            }
            _ => return false,
        }
    }
    true
}

pub fn doc_eq(a: &Document, b: &Document) -> bool {
    a.yaml_version() == b.yaml_version() && value_eq(a.root().value(), b.root().value())
}

/// The §11.1 round trip for an envelope the encoder produced: when it fits
/// the decoder's input budget, `decode_wire` then `encode` is byte-identical.
pub fn assert_wire_round_trip(wire: &str) {
    let opts = Options::default();
    if wire.len() > opts.limits.max_input_bytes {
        return;
    }
    let env = decode_wire(wire, &opts)
        .unwrap_or_else(|e| panic!("decode_wire rejected encoder output: {e}"));
    let again = match &env {
        WireEnvelope::Documents(docs) => encode(docs, &opts),
        WireEnvelope::Error(err) => encode_error(err, &opts),
    }
    .unwrap_or_else(|e| panic!("re-encode of decoded encoder output failed: {e}"));
    assert!(
        again == wire,
        "decode_wire then encode is not byte-identical"
    );
}

pub fn assert_json(what: &str, text: &str) {
    assert!(
        serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok(),
        "{what} is not JSON"
    );
}
