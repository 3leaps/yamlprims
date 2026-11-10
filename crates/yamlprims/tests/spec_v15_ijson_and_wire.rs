use yamlprims::{
    decode_wire, encode, encode_error, encode_ijson, load_all_with, load_ijson, load_ijson_bytes,
    load_ijson_with, load_with, Document, ErrorKind, Integer, LimitKind, Mapping, MergeKeys, Node,
    Options, Site, SyntaxKind, Tag, Tagged, UnknownTags, Value, WireEnvelope,
};

#[test]
fn checked_constructors_and_tag_validation() {
    let local = Tag::try_new("!local").expect("valid local tag");
    assert_eq!(local.uri(), "!local");
    assert_eq!(local.handle(), "");
    assert_eq!(local.suffix(), "!local");

    let empty_ssp = Tag::try_new("x:").expect("RFC 3986 empty scheme-specific URI");
    assert_eq!(empty_ssp.uri(), "x:");

    let comma_uri =
        Tag::try_new("tag:example.com,2026:v\n1").expect("global tag with comma and newline");
    assert_eq!(comma_uri.uri(), "tag:example.com,2026:v\n1");

    let long_uri = format!("!{}", "a".repeat(2050));
    let long_tag =
        Tag::try_new(long_uri.clone()).expect("constructors have no fixed 1024/4096 byte cap");
    assert_eq!(long_tag.uri(), long_uri);

    let sp1 = Tag::with_spelling("!widget", "!", "widget").expect("local handle");
    assert_eq!(sp1.handle(), "!");
    assert_eq!(sp1.suffix(), "widget");
    let sp2 = Tag::with_spelling("tag:yaml.org,2002:timestamp", "!!", "timestamp")
        .expect("secondary handle");
    assert_eq!(sp2.handle(), "!!");
    let sp3 = Tag::with_spelling("tag:example.com,2026:item", "!e!", "item").expect("named handle");
    assert_eq!(sp3.handle(), "!e!");
    let sp4 = Tag::with_spelling("tag:example.com,2026:item", "!", "item")
        .expect("redefined primary handle");
    assert_eq!(sp4.uri(), "tag:example.com,2026:item");

    for bad in [
        "",
        "!",
        "$:?",
        "1http:foo",
        "noscheme",
        "tag:yaml.org,2002:null",
        "tag:yaml.org,2002:bool",
        "tag:yaml.org,2002:int",
        "tag:yaml.org,2002:float",
        "tag:yaml.org,2002:str",
        "tag:yaml.org,2002:seq",
        "tag:yaml.org,2002:map",
    ] {
        let err = Tag::try_new(bad).expect_err(bad);
        assert_eq!(err.kind, ErrorKind::InvalidOptions, "bad uri: {bad}");
        assert_eq!(err.site, Site::NotSource { last_known: None });
    }

    for (u, h, s) in [
        ("!foo", "", "foo"),
        ("!foo", "!", ""),
        ("!foo", "!", "bar"),
        ("!foo", "!bad_handle!", "foo"),
        ("!foo", "!", "!foo"),
    ] {
        let err = Tag::with_spelling(u, h, s).expect_err("bad spelling");
        assert_eq!(err.kind, ErrorKind::InvalidOptions);
    }

    let str_node = Node::synthetic(Value::String("hello".into()));
    let seq_node = Node::synthetic(Value::Sequence(vec![]));
    let map_node = Node::synthetic(Value::Mapping(Mapping::new()));
    assert!(Tagged::new(local.clone(), str_node.clone()).is_ok());
    assert!(Tagged::new(local.clone(), seq_node.clone()).is_ok());
    assert!(Tagged::new(local.clone(), map_node.clone()).is_ok());

    let merge_tag = Tag::try_new("tag:yaml.org,2002:merge").unwrap();
    assert!(Tagged::new(merge_tag.clone(), str_node.clone()).is_ok());
    assert_eq!(
        Tagged::new(merge_tag.clone(), seq_node.clone())
            .unwrap_err()
            .kind,
        ErrorKind::TagKindMismatch
    );
    assert_eq!(
        Tagged::new(merge_tag, map_node.clone()).unwrap_err().kind,
        ErrorKind::TagKindMismatch
    );

    for bad_inner in [
        Node::synthetic(Value::Null),
        Node::synthetic(Value::Bool(true)),
        Node::synthetic(Value::Int(Integer::from_i128(1))),
        Node::synthetic(Value::Float(1.0)),
        Node::synthetic(Value::Tagged(Box::new(
            Tagged::new(local.clone(), str_node.clone()).unwrap(),
        ))),
    ] {
        let err = Tagged::new(local.clone(), bad_inner).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TagKindMismatch);
    }

    let d0 = Document::new(str_node.clone());
    assert_eq!(d0.yaml_version(), None);
    assert!(!d0.explicit_start());

    let d1 = Document::with_metadata(str_node.clone(), Some((1, 2)), true).unwrap();
    assert_eq!(d1.yaml_version(), Some((1, 2)));
    assert!(d1.explicit_start());

    assert_eq!(
        Document::with_metadata(str_node.clone(), Some((2, 0)), false)
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedYamlVersion
    );
    assert_eq!(
        Document::with_metadata(str_node, Some((1, 1_000_000_000)), false)
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedYamlVersion
    );
}

#[test]
fn deep_caller_tree_drop_clone_eq_debug_on_2mib_stack() {
    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut cur = Node::synthetic(Value::String("leaf".into()));
            for i in 0..12_000usize {
                if i % 3 == 0 {
                    cur = Node::synthetic(Value::Sequence(vec![cur]));
                } else if i % 3 == 1 {
                    let mut m = Mapping::new();
                    m.insert_new(Node::synthetic(Value::String("k".into())), cur)
                        .unwrap();
                    cur = Node::synthetic(Value::Mapping(m));
                } else {
                    let t = Tagged::new(Tag::try_new("!t").unwrap(), cur).unwrap();
                    cur = Node::synthetic(Value::Tagged(Box::new(t)));
                }
            }
            let cloned = cur.clone();
            assert_eq!(cur, cloned);
            let dbg = format!("{cur:?}");
            assert!(dbg.contains("..."));
            let val = cur.into_value();
            drop(Node::synthetic(val));
            drop(cloned);
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn decode_wire_roundtrip_limits_and_malformed_controls() {
    let opts = Options::default();
    let docs = load_all_with(
        "%YAML 1.2\n---\n!custom\na: [1, -0.0, .nan, true, null, \"hello\"]\n",
        &opts,
    )
    .unwrap();
    let wire = encode(&docs, &opts).unwrap();
    let WireEnvelope::Documents(decoded) = decode_wire(&wire, &opts).unwrap() else {
        panic!("expected documents envelope");
    };
    let reencoded = encode(&decoded, &opts).unwrap();
    assert_eq!(wire, reencoded);

    let long_tag_yaml = format!("!{} hello\n", "a".repeat(2050));
    let long_docs = load_all_with(&long_tag_yaml, &opts).unwrap();
    let long_wire = encode(&long_docs, &opts).unwrap();
    let WireEnvelope::Documents(long_dec) = decode_wire(&long_wire, &opts).unwrap() else {
        panic!("expected documents envelope");
    };
    assert_eq!(encode(&long_dec, &opts).unwrap(), long_wire);

    let syn_err = load_with("[\n", &opts).unwrap_err();
    let err_wire = encode_error(&syn_err, &opts).unwrap();
    let WireEnvelope::Error(dec_err) = decode_wire(&err_wire, &opts).unwrap() else {
        panic!("expected error envelope");
    };
    assert_eq!(encode_error(&dec_err, &opts).unwrap(), err_wire);

    let doc_empty_nested = load_all_with("[[]]\n", &opts).unwrap();
    let doc_wire = encode(&doc_empty_nested, &opts).unwrap();
    let mut d1_opts = Options::default();
    d1_opts.limits.max_depth = 1;
    let d1_err = decode_wire(&doc_wire, &d1_opts).unwrap_err();
    assert_eq!(
        d1_err.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1
        }
    );

    let path_key_wire = r#"{"format":"yamlprims-value","version":1,"error":{"kind":"custom","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[{"key":{"kind":"sequence","items":[{"kind":"sequence","items":[],"span":null,"via":null,"tag_start":null}],"span":null,"via":null,"tag_start":null}}],"message":"ok"}}"#;
    assert!(decode_wire(path_key_wire, &d1_opts).is_ok());

    let mut n1_opts = Options::default();
    n1_opts.limits.max_nodes = 1;
    assert_eq!(
        decode_wire(path_key_wire, &n1_opts).unwrap_err().kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 1
        }
    );

    for bad_wire in [
        r#"{"format":"other","version":1,"documents":[]}"#,
        r#"{"format":"yamlprims-value","version":2,"documents":[]}"#,
        r#"{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"tagged","tag":{"uri":"!a","handle":"!","suffix":"a"},"value":{"kind":"null","span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null}}]}"#,
        r#"{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"tagged","tag":{"uri":"tag:yaml.org,2002:str","handle":"!!","suffix":"str"},"value":{"kind":"string","value":"x","span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null}}]}"#,
        r#"{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"unknown_code"},"site":{"source":[0,1,1]},"related":[],"path":[],"message":"m"}}"#,
    ] {
        let err = decode_wire(bad_wire, &opts).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Internal, "bad_wire: {bad_wire}");
    }
}

#[test]
fn ijson_six_rules_and_all_66_noncharacters() {
    for empty in ["", "   \n  ", "# comment only\n", "...\n"] {
        let err = load_ijson(empty).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::Type {
                expected: "single YAML document".into(),
                found: "empty_stream".into()
            },
            "input: {empty:?}"
        );
    }
    assert_eq!(load_ijson("---\n").unwrap(), "null");
    assert_eq!(load_ijson("null\n").unwrap(), "null");
    assert_eq!(load_ijson("~\n").unwrap(), "null");

    let mut multi_opts = Options::ijson_default();
    multi_opts.limits.max_documents = 10;
    assert_eq!(
        load_ijson_with("null\n---\n1\n", &multi_opts)
            .unwrap_err()
            .kind,
        ErrorKind::MultipleDocuments
    );
    assert_eq!(
        load_ijson_with("null\n---\n[\n", &multi_opts)
            .unwrap_err()
            .kind,
        ErrorKind::Syntax(SyntaxKind::UnclosedFlowCollection)
    );
    let mut d2_opts = Options::ijson_default();
    d2_opts.limits.max_depth = 2;
    assert_eq!(
        load_ijson_with("null\n---\n[[[]]]\n", &d2_opts)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2
        }
    );

    assert_eq!(
        load_ijson("9007199254740991\n").unwrap(),
        "9007199254740991"
    );
    assert_eq!(
        load_ijson("-9007199254740991\n").unwrap(),
        "-9007199254740991"
    );
    for unsafe_int in [
        "9007199254740992\n",
        "-9007199254740992\n",
        "-9223372036854775808\n",
        "18446744073709551615\n",
    ] {
        assert_eq!(
            load_ijson(unsafe_int).unwrap_err().kind,
            ErrorKind::IntegerOutOfRange,
            "input: {unsafe_int}"
        );
    }

    for non_finite in [".nan\n", ".inf\n", "-.inf\n"] {
        assert_eq!(
            load_ijson(non_finite).unwrap_err().kind,
            ErrorKind::FloatOutOfRange
        );
    }
    assert_eq!(load_ijson("-0.0\n").unwrap(), "0");
    assert_eq!(load_ijson("1.0\n").unwrap(), "1");
    assert_eq!(load_ijson("1.0e+20\n").unwrap(), "100000000000000000000");
    assert_eq!(load_ijson("1.0e+21\n").unwrap(), "1e+21");
    assert_eq!(
        load_ijson("[0.1, 1.5e-7, 123456789.0, 1e20, -2.5]\n").unwrap(),
        "[0.1,1.5e-7,123456789,100000000000000000000,-2.5]"
    );

    for bad_key in [
        "true: 1\n",
        "1: a\n",
        "1.5: a\n",
        "null: a\n",
        "? [1]\n: a\n",
        "? {a: 1}\n: b\n",
    ] {
        assert_eq!(
            load_ijson(bad_key).unwrap_err().kind,
            ErrorKind::Type {
                expected: "I-JSON string key".into(),
                found: "non_string_key".into()
            },
            "bad_key: {bad_key}"
        );
    }

    let mut preserve_opts = Options::default();
    preserve_opts.unknown_tags = UnknownTags::Preserve;
    assert_eq!(
        load_ijson_with("!custom hello\n", &preserve_opts)
            .unwrap_err()
            .kind,
        ErrorKind::UnknownTag
    );

    let tagged_key = Node::synthetic(Value::Tagged(Box::new(
        Tagged::new(
            Tag::try_new("!k").unwrap(),
            Node::synthetic(Value::Sequence(vec![])),
        )
        .unwrap(),
    )));
    let mut m = Mapping::new();
    m.insert_new(tagged_key, Node::synthetic(Value::Null))
        .unwrap();
    let map_node = Node::synthetic(Value::Mapping(m));
    assert_eq!(
        encode_ijson(&map_node, &Options::ijson_default())
            .unwrap_err()
            .kind,
        ErrorKind::UnknownTag
    );

    let merge_yaml = "base: &b\n  x: 1\n  y: 2\nderived:\n  <<: *b\n  y: 3\n";
    assert_eq!(
        load_ijson(merge_yaml).unwrap_err().kind,
        ErrorKind::MergeRejected
    );
    let mut apply_opts = Options::ijson_default();
    apply_opts.merge_keys = MergeKeys::Apply;
    assert_eq!(
        load_ijson_with(merge_yaml, &apply_opts).unwrap(),
        r#"{"base":{"x":1,"y":2},"derived":{"x":1,"y":3}}"#
    );
    let mut as_key_opts = Options::ijson_default();
    as_key_opts.merge_keys = MergeKeys::AsKey;
    assert_eq!(
        load_ijson_with(merge_yaml, &as_key_opts).unwrap_err().kind,
        ErrorKind::InvalidOptions
    );

    let mut nonchars = Vec::new();
    for cp in 0xFDD0u32..=0xFDEFu32 {
        nonchars.push(char::from_u32(cp).unwrap());
    }
    for plane in 0u32..=16u32 {
        let base = plane << 16;
        nonchars.push(char::from_u32(base | 0xFFFE).unwrap());
        nonchars.push(char::from_u32(base | 0xFFFF).unwrap());
    }
    assert_eq!(nonchars.len(), 66);
    for ch in nonchars {
        let val_node = Node::synthetic(Value::String(format!("a{ch}b")));
        let err = encode_ijson(&val_node, &Options::ijson_default()).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::Type {
                expected: "RFC 7493 I-JSON string".into(),
                found: "noncharacter".into()
            },
            "nonchar U+{:04X}",
            ch as u32
        );

        let mut km = Mapping::new();
        km.insert_new(
            Node::synthetic(Value::String(ch.to_string())),
            Node::synthetic(Value::Null),
        )
        .unwrap();
        let key_node = Node::synthetic(Value::Mapping(km));
        let kerr = encode_ijson(&key_node, &Options::ijson_default()).unwrap_err();
        assert_eq!(
            kerr.kind,
            ErrorKind::Type {
                expected: "RFC 7493 I-JSON string".into(),
                found: "noncharacter".into()
            }
        );
    }

    assert_eq!(
        load_ijson("[\"\\ufdcf\", \"\\ufdf0\", \"\\ufffd\", \"\\U0010fffd\"]\n").unwrap(),
        "[\"\u{fdcf}\",\"\u{fdf0}\",\"\u{fffd}\",\"\u{10fffd}\"]"
    );

    let mut small_out = Options::ijson_default();
    small_out.limits.max_output_bytes = 4;
    small_out.limits.max_scalar_bytes = 4;
    small_out.limits.max_key_bytes = 4;
    assert_eq!(
        load_ijson_bytes(b"[1, 2, 3]\n", &small_out)
            .unwrap_err()
            .kind,
        ErrorKind::OutputLimitExceeded { configured: 4 }
    );

    // Exact decimal midpoint tie-breaking (subnormal 5e-324 and 1e23)
    let subnormal_node = Node::synthetic(Value::Float(f64::from_bits(1)));
    assert_eq!(
        encode_ijson(&subnormal_node, &Options::ijson_default()).unwrap(),
        "5e-324"
    );
    let e23_node = Node::synthetic(Value::Float(1e23));
    assert_eq!(
        encode_ijson(&e23_node, &Options::ijson_default()).unwrap(),
        "1e+23"
    );

    // decode_wire accepts arbitrary JSON object member ordering and rejects invalid tag URI chars
    let reordered_wire = r#"{"documents":[{"root":{"value":"ok","tag_start":null,"via":null,"span":null,"kind":"string"},"explicit_start":false,"yaml_version":null}],"version":1,"format":"yamlprims-value"}"#;
    let env = decode_wire(reordered_wire, &Options::default()).unwrap();
    match env {
        WireEnvelope::Documents(docs) => {
            assert_eq!(docs.len(), 1);
            assert_eq!(docs[0].root().value(), &Value::String("ok".into()));
        }
        other => panic!("expected Documents, got {other:?}"),
    }
    let bad_uri_wire = r#"{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"tagged","tag":{"uri":"$:?" ,"handle":"","suffix":"$:?"},"value":{"kind":"string","value":"x","span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null}}]}"#;
    assert_eq!(
        decode_wire(bad_uri_wire, &Options::default())
            .unwrap_err()
            .kind,
        ErrorKind::Internal
    );
}
