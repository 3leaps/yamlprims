use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use yamlprims::{
    emit, emit_all, emit_with, from_str, key_eq, load, load_all, to_node, to_string, CompatQuoting,
    Document, EmitOptions, ErrorKind, Integer, LimitKind, Mapping, Node, Spanned, Tag, Tagged,
    TaggedValue, Value,
};

#[test]
fn emit_scalar_styles_and_compat_quoting_classes() {
    let safe_opts = EmitOptions::default();
    let mut core_opts = EmitOptions::default();
    core_opts.compat_quoting = CompatQuoting::CoreOnly;

    // Null, Bool, Int, Float
    assert_eq!(emit(&Node::synthetic(Value::Null)).unwrap(), "null\n");
    assert_eq!(emit(&Node::synthetic(Value::Bool(true))).unwrap(), "true\n");
    assert_eq!(
        emit(&Node::synthetic(Value::Int(Integer::from_i128(i128::MIN)))).unwrap(),
        "-170141183460469231731687303715884105728\n"
    );
    assert_eq!(
        emit(&Node::synthetic(Value::Int(Integer::from_u128(u128::MAX)))).unwrap(),
        "340282366920938463463374607431768211455\n"
    );
    assert_eq!(emit(&Node::synthetic(Value::Float(0.0))).unwrap(), "0.0\n");
    assert_eq!(
        emit(&Node::synthetic(Value::Float(-0.0))).unwrap(),
        "-0.0\n"
    );
    assert_eq!(
        emit(&Node::synthetic(Value::Float(f64::INFINITY))).unwrap(),
        ".inf\n"
    );
    assert_eq!(
        emit(&Node::synthetic(Value::Float(f64::NEG_INFINITY))).unwrap(),
        "-.inf\n"
    );
    assert_eq!(
        emit(&Node::synthetic(Value::Float(f64::NAN))).unwrap(),
        ".nan\n"
    );
    assert_eq!(
        emit(&Node::synthetic(Value::Float(1e21))).unwrap(),
        "1.0e+21\n"
    );
    assert_eq!(
        emit(&Node::synthetic(Value::Float(1.5e-7))).unwrap(),
        "1.5e-7\n"
    );

    // Core Schema decimal integers (including "08") are quoted under both Yaml11Safe and CoreOnly
    let zero_eight = Node::synthetic(Value::String("08".to_owned()));
    assert_eq!(emit_with(&zero_eight, &safe_opts).unwrap(), "'08'\n");
    assert_eq!(emit_with(&zero_eight, &core_opts).unwrap(), "'08'\n");

    // YAML 1.1 hazard classes: quoted under Yaml11Safe, plain under CoreOnly
    for hazard in [
        "y",
        "N",
        "yes",
        "No",
        "ON",
        "off",
        "0_7",
        "0b1010",
        "0x1_a",
        "1_000",
        "1,000",
        "1.2.3",
        "1_0.5_0",
        "10:20",
        "1_0:20",
        "1:05:30.5_0",
        "2026-11-09",
        "2026-11-09T12:00:00Z",
    ] {
        let n = Node::synthetic(Value::String(hazard.to_owned()));
        let safe_yaml = emit_with(&n, &safe_opts).unwrap();
        assert_eq!(
            safe_yaml,
            format!("'{hazard}'\n"),
            "expected Yaml11Safe to quote {hazard}"
        );
        let core_yaml = emit_with(&n, &core_opts).unwrap();
        assert_eq!(
            core_yaml,
            format!("{hazard}\n"),
            "expected CoreOnly to keep {hazard} plain"
        );
        // Both reload as String(hazard) in yamlprims
        assert_eq!(
            load(&safe_yaml).unwrap().root().value(),
            &Value::String(hazard.to_owned())
        );
        assert_eq!(
            load(&core_yaml).unwrap().root().value(),
            &Value::String(hazard.to_owned())
        );
    }

    // Multiline block scalars vs Style 1 root leading space/newline
    let block_clip = Node::synthetic(Value::String("line1\nline2\n".to_owned()));
    assert_eq!(emit(&block_clip).unwrap(), "|\n  line1\n  line2\n");
    assert!(key_eq(
        load(&emit(&block_clip).unwrap()).unwrap().root().value(),
        block_clip.value()
    ));

    let block_strip = Node::synthetic(Value::String("line1\n\nline2".to_owned()));
    assert_eq!(emit(&block_strip).unwrap(), "|-\n  line1\n\n  line2\n");
    assert!(key_eq(
        load(&emit(&block_strip).unwrap()).unwrap().root().value(),
        block_strip.value()
    ));

    let block_keep = Node::synthetic(Value::String("line1\nline2\n\n".to_owned()));
    assert_eq!(emit(&block_keep).unwrap(), "|+\n  line1\n  line2\n\n");
    assert!(key_eq(
        load(&emit(&block_keep).unwrap()).unwrap().root().value(),
        block_keep.value()
    ));

    // Root multiline starting with space uses Style 1 double-quoted; inside sequence uses |2-
    let leading_space = Node::synthetic(Value::String(" indented\nline".to_owned()));
    assert_eq!(emit(&leading_space).unwrap(), "\" indented\\nline\"\n");
    let seq_with_leading = Node::synthetic(Value::Sequence(vec![leading_space.clone()]));
    assert_eq!(
        emit(&seq_with_leading).unwrap(),
        "- |2-\n   indented\n  line\n"
    );
    assert!(key_eq(
        load(&emit(&seq_with_leading).unwrap())
            .unwrap()
            .root()
            .value(),
        seq_with_leading.value()
    ));
}

#[test]
fn emit_stream_boundaries_mapping_keys_and_tags_roundtrip() {
    assert_eq!(emit_all(&[]).unwrap(), "");

    let d0 = Document::with_metadata(
        Node::synthetic(Value::String("first".to_owned())),
        Some((1, 2)),
        true,
    )
    .unwrap();
    let d1 = Document::with_metadata(Node::synthetic(Value::Null), None, false).unwrap();
    let d2 =
        Document::with_metadata(Node::synthetic(Value::Bool(false)), Some((1, 2)), true).unwrap();
    let stream = emit_all(&[d0, d1, d2]).unwrap();
    assert_eq!(
        stream,
        "%YAML 1.2\n---\nfirst\n---\nnull\n...\n%YAML 1.2\n---\nfalse\n"
    );
    let reloaded = load_all(&stream).unwrap();
    assert_eq!(reloaded.len(), 3);
    assert_eq!(reloaded[0].yaml_version(), Some((1, 2)));
    assert_eq!(reloaded[1].yaml_version(), None);
    assert_eq!(reloaded[2].yaml_version(), Some((1, 2)));

    // Implicit and explicit mapping keys + tags
    let mut m = Mapping::new();
    m.insert_new(
        Node::synthetic(Value::Null),
        Node::synthetic(Value::Bool(true)),
    )
    .unwrap();
    m.insert_new(
        Node::synthetic(Value::Int(Integer::from_i128(1))),
        Node::synthetic(Value::String("x".to_owned())),
    )
    .unwrap();
    m.insert_new(
        Node::synthetic(Value::Sequence(vec![Node::synthetic(Value::Int(
            Integer::from_i128(2),
        ))])),
        Node::synthetic(Value::String("seq-key".to_owned())),
    )
    .unwrap();
    let bracket_tag = Tag::try_new("x:[").unwrap();
    let tagged_val = Tagged::new(
        bracket_tag,
        Node::synthetic(Value::String("payload".to_owned())),
    )
    .unwrap();
    m.insert_new(
        Node::synthetic(Value::String("tagged".to_owned())),
        Node::synthetic(Value::Tagged(Box::new(tagged_val))),
    )
    .unwrap();

    let root = Node::synthetic(Value::Mapping(m));
    let yaml_out = emit(&root).unwrap();
    assert!(yaml_out.contains("!<x:%5B> payload\n"), "{yaml_out}");
    assert!(yaml_out.contains("?\n  - 2\n: seq-key\n"), "{yaml_out}");
    let reloaded_doc = load(&yaml_out).unwrap();
    assert!(key_eq(reloaded_doc.root().value(), root.value()));
}

#[test]
fn emit_preflight_precedence_and_tagged_key_weight() {
    // Structural/input limit failures precede OutputLimitExceeded even when max_output_bytes = 1
    let mut m = Mapping::new();
    let tag = Tag::try_new("!custom-tag").unwrap();
    let tagged_key = Tagged::new(tag, Node::synthetic(Value::String("k".to_owned()))).unwrap();
    m.insert_new(
        Node::synthetic(Value::Tagged(Box::new(tagged_key))),
        Node::synthetic(Value::Null),
    )
    .unwrap();
    let root = Node::synthetic(Value::Mapping(m));

    // Weight of Tagged("!custom-tag", String("k")) is:
    // outer Tagged node: 64 + 2 * 11 = 86
    // inner String node: 64 + 1 = 65
    // Total = 151 bytes.
    let mut opts = EmitOptions::default();
    opts.limits.max_output_bytes = 200;
    opts.limits.max_scalar_bytes = 200;
    opts.limits.max_key_bytes = 150;
    let err = emit_with(&root, &opts).unwrap_err();
    assert!(
        matches!(
            err.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxKeyBytes,
                configured: 150
            }
        ),
        "got {err:?}"
    );

    opts.limits.max_key_bytes = 151;
    assert!(emit_with(&root, &opts).is_ok());

    // When max_output_bytes is too small to hold the output, preflight passes and OutputLimitExceeded fires
    let mut small_out = EmitOptions::default();
    small_out.limits.max_output_bytes = 3;
    small_out.limits.max_scalar_bytes = 3;
    small_out.limits.max_key_bytes = 3;
    let err = emit_with(&Node::synthetic(Value::Null), &small_out).unwrap_err();
    assert!(matches!(
        err.kind,
        ErrorKind::OutputLimitExceeded { configured: 3 }
    ));
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum SampleEnum {
    Unit,
    Newtype(u64),
    Tuple(String, bool),
    Struct { flag: bool, count: i32 },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct SampleConfig {
    name: String,
    items: Vec<SampleEnum>,
    tags: BTreeMap<String, TaggedValue<String>>,
    spanned: Spanned<u32>,
}

#[test]
fn serde_serialize_to_node_and_to_string_roundtrip() {
    let mut tags = BTreeMap::new();
    tags.insert(
        "primary".to_owned(),
        TaggedValue {
            tag: Tag::try_new("!env").unwrap(),
            value: "PROD".to_owned(),
        },
    );
    let cfg = SampleConfig {
        name: "yes".to_owned(),
        items: vec![
            SampleEnum::Unit,
            SampleEnum::Newtype(42),
            SampleEnum::Tuple("08".to_owned(), true),
            SampleEnum::Struct {
                flag: false,
                count: -7,
            },
        ],
        tags,
        spanned: Spanned {
            value: 99,
            span: None,
        },
    };

    let yaml = to_string(&cfg).unwrap();
    let decoded: SampleConfig = from_str(&yaml).unwrap();
    assert_eq!(decoded.name, cfg.name);
    assert_eq!(decoded.items, cfg.items);
    assert_eq!(decoded.tags, cfg.tags);
    assert_eq!(decoded.spanned.value, 99);
    assert!(decoded.spanned.span.is_some());

    // TaggedValue wrapping Int is rejected immediately by to_node with TagKindMismatch
    let bad_tagged = TaggedValue {
        tag: Tag::try_new("!num").unwrap(),
        value: 123i32,
    };
    let err = to_node(&bad_tagged, &EmitOptions::default()).unwrap_err();
    assert!(
        matches!(err.kind, ErrorKind::TagKindMismatch),
        "got {err:?}"
    );

    // Document implements Serialize (delegating to root Node)
    let doc = Document::new(Node::synthetic(Value::String("doc-root".into())));
    let doc_node = to_node(&doc, &EmitOptions::default()).unwrap();
    assert_eq!(doc_node, *doc.root());
    assert_eq!(to_string(&doc).unwrap(), "doc-root\n");

    // Borrowed Node / Value preflight in to_node rejects max_nodes before cloning
    let seq_node = Node::synthetic(Value::Sequence(vec![
        Node::synthetic(Value::Int(Integer::from_i128(1))),
        Node::synthetic(Value::Int(Integer::from_i128(2))),
    ]));
    let mut lim1 = EmitOptions::default();
    lim1.limits.max_nodes = 1;
    assert_eq!(
        to_node(&seq_node, &lim1).unwrap_err().kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 1
        }
    );
    assert_eq!(
        to_node(seq_node.value(), &lim1).unwrap_err().kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 1
        }
    );

    // Empty suffix after tag:yaml.org,2002: prefix emits verbatim !<tag:yaml.org,2002:>
    let empty_core_prefix_tag = Tag::try_new("tag:yaml.org,2002:").unwrap();
    let tagged_node = Node::synthetic(Value::Tagged(Box::new(
        Tagged::new(
            empty_core_prefix_tag,
            Node::synthetic(Value::String("v".into())),
        )
        .unwrap(),
    )));
    let emitted_tag = emit(&tagged_node).unwrap();
    assert_eq!(emitted_tag, "!<tag:yaml.org,2002:> v\n");

    // #[serde(flatten)] Value and #[serde(tag = "type")] Value inside to_node
    #[derive(Serialize)]
    struct FlattenHolder {
        head: String,
        #[serde(flatten)]
        rest: Value,
    }
    #[derive(Serialize)]
    #[serde(tag = "type")]
    enum TaggedEnumValue {
        Wrap(Value),
    }

    let mut rest_map = Mapping::new();
    rest_map
        .insert_new(
            Node::synthetic(Value::String("extra".into())),
            Node::synthetic(Value::Int(Integer::from_i128(7))),
        )
        .unwrap();
    let flat_holder = FlattenHolder {
        head: "first".into(),
        rest: Value::Mapping(rest_map.clone()),
    };
    let flat_yaml = to_string(&flat_holder).unwrap();
    assert_eq!(flat_yaml, "head: first\nextra: 7\n");

    let tagged_enum = TaggedEnumValue::Wrap(Value::Mapping(rest_map));
    let tagged_enum_yaml = to_string(&tagged_enum).unwrap();
    assert_eq!(tagged_enum_yaml, "type: Wrap\nextra: 7\n");

    // Custom Serialize invoking an inner Node/Value serialization inside an outer to_node
    struct NestedValueCaller;
    impl serde::Serialize for NestedValueCaller {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let inner = Node::synthetic(Value::Bool(true));
            let s = to_string(&inner).map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(s.trim())
        }
    }
    #[derive(Serialize)]
    struct OuterWithValueAndCustom {
        custom: NestedValueCaller,
        after: Value,
    }
    let outer_val = OuterWithValueAndCustom {
        custom: NestedValueCaller,
        after: Value::String("preserved".into()),
    };
    assert_eq!(
        to_string(&outer_val).unwrap(),
        "custom: 'true'\nafter: preserved\n"
    );
}
