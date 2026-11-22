//! Malformed-wire refusals (spec revision 1.7 §11.1, `1.7/E-6`): one public
//! message in every binding, the specific reason kept in the bounded,
//! non-serialized `engine_detail`.

use yamlprims::{decode_lint_wire, decode_wire, encode_error, ErrorKind, Options};

const REFUSAL: &str = "invalid yamlprims wire payload";

#[test]
fn malformed_wire_uses_the_common_message_and_keeps_the_reason() {
    let d = Options::default();
    for text in [
        "nope",
        "{}",
        "[]",
        r#"{"format":"other","version":1,"documents":[]}"#,
        r#"{"format":"yamlprims-value","version":2,"documents":[]}"#,
    ] {
        let e = decode_wire(text, &d).unwrap_err();
        assert!(matches!(e.kind, ErrorKind::Internal), "{text}");
        assert_eq!(e.message, REFUSAL, "{text}");
        let detail = e.engine_detail.as_deref().unwrap_or("");
        assert!(!detail.is_empty(), "{text}: reason must be kept");
        let wire = encode_error(&e, &d).unwrap();
        assert!(
            !wire.contains(detail),
            "{text}: reason must not be serialized"
        );
    }
}

#[test]
fn reason_is_bounded() {
    let key = "k".repeat(20_000);
    let text = format!(r#"{{"format":"yamlprims-value","version":1,"{key}":0,"documents":[]}}"#);
    let e = decode_wire(&text, &Options::default()).unwrap_err();
    assert_eq!(e.message, REFUSAL);
    assert!(e.engine_detail.as_deref().unwrap().len() <= 4096);
}

#[test]
fn decoded_error_envelopes_keep_the_caller_message() {
    let d = Options::default();
    let text = r#"{"format":"yamlprims-value","version":1,"error":{"kind":"internal","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"caller text"}}"#;
    let env = decode_wire(text, &d).unwrap();
    let yamlprims::WireEnvelope::Error(e) = env else {
        panic!("expected an error envelope");
    };
    assert_eq!(e.message, "caller text");
    assert_eq!(encode_error(&e, &d).unwrap(), text);
}

#[test]
fn lint_wire_shares_only_the_json_level_refusal() {
    // decode_lint_wire shares the JSON reader with decode_wire: its JSON-level
    // refusals carry the common message; its own envelope checks keep
    // InvalidOptions and their wording.
    let e = decode_lint_wire("nope", &Options::default()).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::Internal));
    assert_eq!(e.message, REFUSAL);
    let e = decode_lint_wire("{}", &Options::default()).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::InvalidOptions));
    assert_ne!(e.message, REFUSAL);
}

#[test]
fn limits_and_options_are_not_folded_into_the_refusal() {
    let mut o = Options::default();
    o.limits.max_input_bytes = 3;
    let e = decode_wire("nope", &o).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::LimitExceeded { .. }));
    assert_eq!(e.message, "wire payload exceeds max_input_bytes (3)");
    let mut bad = Options::default();
    bad.limits.max_depth = 0;
    let e = decode_wire("nope", &bad).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::InvalidOptions));
}

#[test]
fn refusal_envelope_fits_an_exact_output_budget_and_not_one_less() {
    let d = Options::default();
    let e = decode_wire("{}", &d).unwrap_err();
    let full = encode_error(&e, &d).unwrap();
    assert_eq!(
        full,
        r#"{"format":"yamlprims-value","version":1,"error":{"kind":"internal","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"invalid yamlprims wire payload"}}"#
    );
    let mut exact = Options::default();
    exact.limits.max_output_bytes = full.len();
    assert_eq!(encode_error(&e, &exact).unwrap(), full);
    let mut less = Options::default();
    less.limits.max_output_bytes = full.len() - 1;
    let over = encode_error(&e, &less).unwrap_err();
    assert!(matches!(over.kind, ErrorKind::OutputLimitExceeded { .. }));
}

#[test]
fn constructor_refusals_match_the_shared_table() {
    use yamlprims::{Document, Node, Tag, Tagged, Value};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/vectors/constructor-error-cases.json");
    let table: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let d = Options::default();
    let local = Tag::try_new("!t").unwrap();
    let merge = Tag::try_new("tag:yaml.org,2002:merge");
    for case in table["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let err = match id {
            "tag.empty-uri" => Tag::try_new("").unwrap_err(),
            "tag.core-uri" => Tag::try_new("tag:yaml.org,2002:str").unwrap_err(),
            "tag.inconsistent-spelling" => Tag::with_spelling("!x", "!!", "y").unwrap_err(),
            "tagged.bool-inner" => {
                Tagged::new(local.clone(), Node::synthetic(Value::Bool(true))).unwrap_err()
            }
            "tagged.merge-sequence-inner" => Tagged::new(
                merge.clone().expect("merge tag is constructible"),
                Node::synthetic(Value::Sequence(vec![])),
            )
            .unwrap_err(),
            "document.version-2-0" => {
                Document::with_metadata(Node::synthetic(Value::Null), Some((2, 0)), false)
                    .unwrap_err()
            }
            other => panic!("unknown case {other}"),
        };
        let wire = encode_error(&err, &d).unwrap();
        let want = format!(
            r#"{{"format":"yamlprims-value","version":1,"error":{{"kind":"{}","detail":{{}},"site":{{"not_source":{{"last_known":null}}}},"related":[],"path":[],"message":"{}"}}}}"#,
            case["kind"].as_str().unwrap(),
            case["message"].as_str().unwrap()
        );
        assert_eq!(wire, want, "{id}");
    }
}

#[test]
fn out_of_range_metadata_is_refused_as_the_shared_table_says() {
    use yamlprims::{Error, LimitKind, Location, PathSegment, Site};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/vectors/constructor-error-cases.json");
    let table: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let big: usize = 1 << 53;
    let ns = || Site::NotSource { last_known: None };
    for case in table["encode_error_cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let err = match id {
            "path-index-2^53" => {
                let mut e = Error::new(ErrorKind::Internal, ns(), "m");
                e.path.push(PathSegment::Index(big));
                e
            }
            "site-offset-2^53" => Error::new(
                ErrorKind::Internal,
                Site::Source(Location {
                    byte_offset: big,
                    line: 1,
                    column: 1,
                }),
                "m",
            ),
            "configured-2^53" => Error::new(
                ErrorKind::LimitExceeded {
                    limit: LimitKind::MaxNodes,
                    configured: big as u64,
                },
                ns(),
                "m",
            ),
            "configured-zero" => Error::new(
                ErrorKind::LimitExceeded {
                    limit: LimitKind::MaxNodes,
                    configured: 0,
                },
                ns(),
                "m",
            ),
            other => panic!("unknown case {other}"),
        };
        let refused = encode_error(&err, &Options::default()).unwrap_err();
        assert!(matches!(refused.kind, ErrorKind::Internal), "{id}");
        assert_eq!(refused.message, case["message"].as_str().unwrap(), "{id}");
        assert!(
            matches!(refused.site, Site::NotSource { last_known: None }),
            "{id}"
        );
    }
}
