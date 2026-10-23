//! Anchor-cache bytes, completion order, binding values and pre-copy admission (§§5–7).
//! The assertions are fixed expectations; tests never regenerate them from a loader.
use serde_json::{json, Value as Json};
use yamlprims::*;

enum Mode {
    All,
    Stream,
    Single,
}
struct Origin {
    document: usize,
    path: &'static [usize],
    definition: Span,
    use_site: Span,
}
enum Expected {
    Value(&'static str, &'static [Origin]),
    Error(ErrorKind, Location, &'static [usize]),
}

fn shape(v: &Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i.to_i64().expect("small expected integer")),
        Value::String(s) => json!(s),
        Value::Sequence(items) => Json::Array(items.iter().map(|n| shape(n.value())).collect()),
        Value::Mapping(m) => {
            json!({"map": m.iter().map(|(k,v)| [shape(k.value()), shape(v.value())]).collect::<Vec<_>>() })
        }
        Value::Tagged(t) => {
            json!({"tag": [t.tag().uri(), t.tag().handle(), t.tag().suffix()], "value": shape(t.value().value())})
        }
        other => panic!("unexpected value in bounded fixture: {other:?}"),
    }
}

fn check(input: &str, o: Options, mode: Mode, expected: Expected) {
    let mut results = Vec::new();
    if !matches!(mode, Mode::Single) {
        results.push(load_all_with(input, &o));
        results.push(load_all_bytes(input.as_bytes(), &o));
    }
    if !matches!(mode, Mode::Stream) {
        results.push(load_with(input, &o).map(|d| vec![d]));
        results.push(load_bytes(input.as_bytes(), &o).map(|d| vec![d]));
        if matches!(&expected, Expected::Error(..)) {
            results.push(from_str_with::<Value>(input, &o).map(|_| Vec::new()));
        }
    }
    for result in results {
        match &expected {
            Expected::Value(values, origins) => {
                let docs = result.expect(input);
                let wanted: Json = serde_json::from_str(values).unwrap();
                assert_eq!(
                    Json::Array(docs.iter().map(|d| shape(d.root().value())).collect()),
                    wanted,
                    "{input:?}"
                );
                for origin in *origins {
                    let mut node = docs[origin.document].root();
                    for i in origin.path {
                        node = &node.value().as_sequence().unwrap()[*i];
                    }
                    assert_eq!(
                        node.span(),
                        &NodeSpan::Source(origin.definition),
                        "alias definition: {input:?}"
                    );
                    assert_eq!(
                        node.via(),
                        Some(&Via::Alias {
                            use_site: origin.use_site
                        }),
                        "alias use: {input:?}"
                    );
                }
            }
            Expected::Error(kind, at, path) => {
                let error = result.expect_err(input);
                assert_eq!(&error.kind, kind, "{input:?}: {error:?}");
                assert_eq!(error.site, Site::Source(*at), "{input:?}");
                // Exact path and empty related list characterize the existing builder.
                // Registration's kind, configured limit and source start are normative.
                assert_eq!(error.path.len(), path.len(), "{input:?}: {:?}", error.path);
                for (got, want) in error.path.iter().zip(*path) {
                    assert!(
                        matches!(got, PathSegment::Index(i) if i == want),
                        "{input:?}: {got:?}"
                    );
                }
                assert!(error.related.is_empty(), "{input:?}: {:?}", error.related);
            }
        }
    }
}

fn options(
    owned: usize,
    nodes: usize,
    expanded: usize,
    depth: usize,
    scalar: usize,
    tags: UnknownTags,
) -> Options {
    let mut o = Options::default();
    o.merge_keys = MergeKeys::Reject;
    o.unknown_tags = tags;
    o.limits.max_input_bytes = 4096;
    o.limits.max_depth = depth;
    o.limits.max_nodes = nodes;
    o.limits.max_expanded_nodes = expanded;
    o.limits.max_alias_events = 32;
    o.limits.max_owned_bytes = owned;
    o.limits.max_scalar_bytes = scalar;
    o.limits.max_key_bytes = 16;
    o.limits.max_mapping_entries = 16;
    o.limits.max_documents = 4;
    o.limits.max_output_bytes = 4096;
    o.limits.max_warnings = 1;
    o.validate().expect("complete valid, noncompeting options");
    o
}

#[test]
fn scalar_definition_owned_191() {
    check(
        "[&a 1]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn scalar_definition_owned_192() {
    check(
        "[&a 1]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn scalar_definition_owned_193() {
    check(
        "[&a 1]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn scalar_definition_nodes_1() {
    check(
        "[&a 1]",
        options(4096, 1, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxNodes,
                configured: 1,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn scalar_definition_nodes_2() {
    check(
        "[&a 1]",
        options(4096, 2, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn scalar_definition_nodes_3() {
    check(
        "[&a 1]",
        options(4096, 3, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn scalar_definition_expanded_1() {
    check(
        "[&a 1]",
        options(4096, 128, 1, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn scalar_definition_registration_1_below() {
    check(
        "[&a 1]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn scalar_definition_registration_1_at() {
    check(
        "[&a 1]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn scalar_definition_registration_1_above() {
    check(
        "[&a 1]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn scalar_alias_owned_255() {
    check(
        "[&a 1, *a]",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 8,
            },
            &[1],
        ),
    );
}

#[test]
fn scalar_alias_owned_256() {
    check(
        "[&a 1, *a]",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 5,
                        line: 1,
                        column: 6,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 7,
                        line: 1,
                        column: 8,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
            }],
        ),
    );
}

#[test]
fn scalar_alias_owned_257() {
    check(
        "[&a 1, *a]",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 5,
                        line: 1,
                        column: 6,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 7,
                        line: 1,
                        column: 8,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
            }],
        ),
    );
}

#[test]
fn scalar_alias_nodes_2() {
    check(
        "[&a 1, *a]",
        options(4096, 2, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxNodes,
                configured: 2,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 8,
            },
            &[1],
        ),
    );
}

#[test]
fn scalar_alias_nodes_3() {
    check(
        "[&a 1, *a]",
        options(4096, 3, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 5,
                        line: 1,
                        column: 6,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 7,
                        line: 1,
                        column: 8,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
            }],
        ),
    );
}

#[test]
fn scalar_alias_nodes_4() {
    check(
        "[&a 1, *a]",
        options(4096, 4, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 5,
                        line: 1,
                        column: 6,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 7,
                        line: 1,
                        column: 8,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
            }],
        ),
    );
}

#[test]
fn scalar_alias_expanded_1() {
    check(
        "[&a 1, *a]",
        options(4096, 128, 1, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 5,
                        line: 1,
                        column: 6,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 7,
                        line: 1,
                        column: 8,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
            }],
        ),
    );
}

#[test]
fn scalar_alias_expanded_2() {
    check(
        "[&a 1, *a]",
        options(4096, 128, 2, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 5,
                        line: 1,
                        column: 6,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 7,
                        line: 1,
                        column: 8,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
            }],
        ),
    );
}

#[test]
fn scalar_alias_registration_1_below() {
    check(
        "[&a 1, *a]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn scalar_alias_registration_1_at() {
    check(
        "[&a 1, *a]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 192,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 8,
            },
            &[1],
        ),
    );
}

#[test]
fn scalar_alias_registration_1_above() {
    check(
        "[&a 1, *a]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 193,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 8,
            },
            &[1],
        ),
    );
}

#[test]
fn redefinition_owned_383() {
    check(
        "[&a 1, &a 2, *a]",
        options(383, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 383,
            },
            Location {
                byte_offset: 13,
                line: 1,
                column: 14,
            },
            &[2],
        ),
    );
}

#[test]
fn redefinition_owned_384() {
    check(
        "[&a 1, &a 2, *a]",
        options(384, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,2,2]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                    end: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 13,
                        line: 1,
                        column: 14,
                    },
                    end: Location {
                        byte_offset: 15,
                        line: 1,
                        column: 16,
                    },
                },
            }],
        ),
    );
}

#[test]
fn redefinition_owned_385() {
    check(
        "[&a 1, &a 2, *a]",
        options(385, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,2,2]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                    end: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 13,
                        line: 1,
                        column: 14,
                    },
                    end: Location {
                        byte_offset: 15,
                        line: 1,
                        column: 16,
                    },
                },
            }],
        ),
    );
}

#[test]
fn redefinition_nodes_3() {
    check(
        "[&a 1, &a 2, *a]",
        options(4096, 3, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxNodes,
                configured: 3,
            },
            Location {
                byte_offset: 13,
                line: 1,
                column: 14,
            },
            &[2],
        ),
    );
}

#[test]
fn redefinition_nodes_4() {
    check(
        "[&a 1, &a 2, *a]",
        options(4096, 4, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,2,2]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                    end: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 13,
                        line: 1,
                        column: 14,
                    },
                    end: Location {
                        byte_offset: 15,
                        line: 1,
                        column: 16,
                    },
                },
            }],
        ),
    );
}

#[test]
fn redefinition_nodes_5() {
    check(
        "[&a 1, &a 2, *a]",
        options(4096, 5, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,2,2]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                    end: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 13,
                        line: 1,
                        column: 14,
                    },
                    end: Location {
                        byte_offset: 15,
                        line: 1,
                        column: 16,
                    },
                },
            }],
        ),
    );
}

#[test]
fn redefinition_expanded_1() {
    check(
        "[&a 1, &a 2, *a]",
        options(4096, 128, 1, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,2,2]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                    end: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 13,
                        line: 1,
                        column: 14,
                    },
                    end: Location {
                        byte_offset: 15,
                        line: 1,
                        column: 16,
                    },
                },
            }],
        ),
    );
}

#[test]
fn redefinition_expanded_2() {
    check(
        "[&a 1, &a 2, *a]",
        options(4096, 128, 2, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[1,2,2]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                    end: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 13,
                        line: 1,
                        column: 14,
                    },
                    end: Location {
                        byte_offset: 15,
                        line: 1,
                        column: 16,
                    },
                },
            }],
        ),
    );
}

#[test]
fn redefinition_registration_1_below() {
    check(
        "[&a 1, &a 2, *a]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn redefinition_registration_1_at() {
    check(
        "[&a 1, &a 2, *a]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 192,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn redefinition_registration_1_above() {
    check(
        "[&a 1, &a 2, *a]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 193,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn redefinition_registration_2_below() {
    check(
        "[&a 1, &a 2, *a]",
        options(319, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 319,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn redefinition_registration_2_at() {
    check(
        "[&a 1, &a 2, *a]",
        options(320, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 320,
            },
            Location {
                byte_offset: 13,
                line: 1,
                column: 14,
            },
            &[2],
        ),
    );
}

#[test]
fn redefinition_registration_2_above() {
    check(
        "[&a 1, &a 2, *a]",
        options(321, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 321,
            },
            Location {
                byte_offset: 13,
                line: 1,
                column: 14,
            },
            &[2],
        ),
    );
}

#[test]
fn nested_distinct_owned_511() {
    check(
        "[&a [&b 1], *a]",
        options(511, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 511,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_distinct_owned_512() {
    check(
        "[&a [&b 1], *a]",
        options(512, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],[1]]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_distinct_owned_513() {
    check(
        "[&a [&b 1], *a]",
        options(513, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],[1]]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_distinct_nodes_4() {
    check(
        "[&a [&b 1], *a]",
        options(4096, 4, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxNodes,
                configured: 4,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_distinct_nodes_5() {
    check(
        "[&a [&b 1], *a]",
        options(4096, 5, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],[1]]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_distinct_nodes_6() {
    check(
        "[&a [&b 1], *a]",
        options(4096, 6, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],[1]]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_distinct_expanded_1() {
    check(
        "[&a [&b 1], *a]",
        options(4096, 128, 1, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxExpandedNodes,
                configured: 1,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_distinct_expanded_2() {
    check(
        "[&a [&b 1], *a]",
        options(4096, 128, 2, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],[1]]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_distinct_expanded_3() {
    check(
        "[&a [&b 1], *a]",
        options(4096, 128, 3, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],[1]]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 4,
                        line: 1,
                        column: 5,
                    },
                    end: Location {
                        byte_offset: 10,
                        line: 1,
                        column: 11,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_distinct_registration_1_below() {
    check(
        "[&a [&b 1], *a]",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 8,
                line: 1,
                column: 9,
            },
            &[0, 0],
        ),
    );
}

#[test]
fn nested_distinct_registration_1_at() {
    check(
        "[&a [&b 1], *a]",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 256,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn nested_distinct_registration_1_above() {
    check(
        "[&a [&b 1], *a]",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 257,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn nested_distinct_registration_2_below() {
    check(
        "[&a [&b 1], *a]",
        options(383, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 383,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn nested_distinct_registration_2_at() {
    check(
        "[&a [&b 1], *a]",
        options(384, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 384,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_distinct_registration_2_above() {
    check(
        "[&a [&b 1], *a]",
        options(385, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 385,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_same_name_owned_447() {
    check(
        "[&a [&a 1], *a]",
        options(447, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 447,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_same_name_owned_448() {
    check(
        "[&a [&a 1], *a]",
        options(448, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 8,
                        line: 1,
                        column: 9,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_same_name_owned_449() {
    check(
        "[&a [&a 1], *a]",
        options(449, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 8,
                        line: 1,
                        column: 9,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_same_name_nodes_3() {
    check(
        "[&a [&a 1], *a]",
        options(4096, 3, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxNodes,
                configured: 3,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_same_name_nodes_4() {
    check(
        "[&a [&a 1], *a]",
        options(4096, 4, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 8,
                        line: 1,
                        column: 9,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_same_name_nodes_5() {
    check(
        "[&a [&a 1], *a]",
        options(4096, 5, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 8,
                        line: 1,
                        column: 9,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_same_name_expanded_1() {
    check(
        "[&a [&a 1], *a]",
        options(4096, 128, 1, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 8,
                        line: 1,
                        column: 9,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_same_name_expanded_2() {
    check(
        "[&a [&a 1], *a]",
        options(4096, 128, 2, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[1],1]]",
            &[Origin {
                document: 0,
                path: &[1],
                definition: Span {
                    start: Location {
                        byte_offset: 8,
                        line: 1,
                        column: 9,
                    },
                    end: Location {
                        byte_offset: 9,
                        line: 1,
                        column: 10,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 12,
                        line: 1,
                        column: 13,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
            }],
        ),
    );
}

#[test]
fn nested_same_name_registration_1_below() {
    check(
        "[&a [&a 1], *a]",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 8,
                line: 1,
                column: 9,
            },
            &[0, 0],
        ),
    );
}

#[test]
fn nested_same_name_registration_1_at() {
    check(
        "[&a [&a 1], *a]",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 256,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn nested_same_name_registration_1_above() {
    check(
        "[&a [&a 1], *a]",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 257,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn nested_same_name_registration_2_below() {
    check(
        "[&a [&a 1], *a]",
        options(383, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 383,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn nested_same_name_registration_2_at() {
    check(
        "[&a [&a 1], *a]",
        options(384, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 384,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn nested_same_name_registration_2_above() {
    check(
        "[&a [&a 1], *a]",
        options(385, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 385,
            },
            Location {
                byte_offset: 12,
                line: 1,
                column: 13,
            },
            &[1],
        ),
    );
}

#[test]
fn retained_scalar_alias_owned_462() {
    check(
        "[&a !t x, *a]",
        options(462, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 462,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn retained_scalar_alias_owned_463() {
    check("[&a !t x, *a]", options(463, 128, 64, 16, 16, UnknownTags::Preserve), Mode::All, Expected::Value("[[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"},{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]]", &[Origin { document: 0, path: &[1], definition: Span { start: Location { byte_offset: 7, line: 1, column: 8 }, end: Location { byte_offset: 8, line: 1, column: 9 } }, use_site: Span { start: Location { byte_offset: 10, line: 1, column: 11 }, end: Location { byte_offset: 12, line: 1, column: 13 } } }]));
}

#[test]
fn retained_scalar_alias_owned_464() {
    check("[&a !t x, *a]", options(464, 128, 64, 16, 16, UnknownTags::Preserve), Mode::All, Expected::Value("[[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"},{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]]", &[Origin { document: 0, path: &[1], definition: Span { start: Location { byte_offset: 7, line: 1, column: 8 }, end: Location { byte_offset: 8, line: 1, column: 9 } }, use_site: Span { start: Location { byte_offset: 10, line: 1, column: 11 }, end: Location { byte_offset: 12, line: 1, column: 13 } } }]));
}

#[test]
fn retained_scalar_alias_nodes_4() {
    check(
        "[&a !t x, *a]",
        options(4096, 4, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxNodes,
                configured: 4,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn retained_scalar_alias_nodes_5() {
    check("[&a !t x, *a]", options(4096, 5, 64, 16, 16, UnknownTags::Preserve), Mode::All, Expected::Value("[[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"},{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]]", &[Origin { document: 0, path: &[1], definition: Span { start: Location { byte_offset: 7, line: 1, column: 8 }, end: Location { byte_offset: 8, line: 1, column: 9 } }, use_site: Span { start: Location { byte_offset: 10, line: 1, column: 11 }, end: Location { byte_offset: 12, line: 1, column: 13 } } }]));
}

#[test]
fn retained_scalar_alias_nodes_6() {
    check("[&a !t x, *a]", options(4096, 6, 64, 16, 16, UnknownTags::Preserve), Mode::All, Expected::Value("[[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"},{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]]", &[Origin { document: 0, path: &[1], definition: Span { start: Location { byte_offset: 7, line: 1, column: 8 }, end: Location { byte_offset: 8, line: 1, column: 9 } }, use_site: Span { start: Location { byte_offset: 10, line: 1, column: 11 }, end: Location { byte_offset: 12, line: 1, column: 13 } } }]));
}

#[test]
fn retained_scalar_alias_expanded_1() {
    check(
        "[&a !t x, *a]",
        options(4096, 128, 1, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxExpandedNodes,
                configured: 1,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn retained_scalar_alias_expanded_2() {
    check("[&a !t x, *a]", options(4096, 128, 2, 16, 16, UnknownTags::Preserve), Mode::All, Expected::Value("[[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"},{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]]", &[Origin { document: 0, path: &[1], definition: Span { start: Location { byte_offset: 7, line: 1, column: 8 }, end: Location { byte_offset: 8, line: 1, column: 9 } }, use_site: Span { start: Location { byte_offset: 10, line: 1, column: 11 }, end: Location { byte_offset: 12, line: 1, column: 13 } } }]));
}

#[test]
fn retained_scalar_alias_expanded_3() {
    check("[&a !t x, *a]", options(4096, 128, 3, 16, 16, UnknownTags::Preserve), Mode::All, Expected::Value("[[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"},{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]]", &[Origin { document: 0, path: &[1], definition: Span { start: Location { byte_offset: 7, line: 1, column: 8 }, end: Location { byte_offset: 8, line: 1, column: 9 } }, use_site: Span { start: Location { byte_offset: 10, line: 1, column: 11 }, end: Location { byte_offset: 12, line: 1, column: 13 } } }]));
}

#[test]
fn retained_scalar_alias_registration_1_below() {
    check(
        "[&a !t x, *a]",
        options(329, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 329,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 8,
            },
            &[0],
        ),
    );
}

#[test]
fn retained_scalar_alias_registration_1_at() {
    check(
        "[&a !t x, *a]",
        options(330, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 330,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn retained_scalar_alias_registration_1_above() {
    check(
        "[&a !t x, *a]",
        options(331, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 331,
            },
            Location {
                byte_offset: 10,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn stream_owned_255() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 16,
                line: 2,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn stream_owned_256() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Value("[1,2]", &[]),
    );
}

#[test]
fn stream_owned_257() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Value("[1,2]", &[]),
    );
}

#[test]
fn stream_nodes_1() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(4096, 1, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxNodes,
                configured: 1,
            },
            Location {
                byte_offset: 16,
                line: 2,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn stream_nodes_2() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(4096, 2, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Value("[1,2]", &[]),
    );
}

#[test]
fn stream_nodes_3() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(4096, 3, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Value("[1,2]", &[]),
    );
}

#[test]
fn stream_expanded_1() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(4096, 128, 1, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Value("[1,2]", &[]),
    );
}

#[test]
fn stream_registration_1_below() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn stream_registration_1_at() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 128,
            },
            Location {
                byte_offset: 16,
                line: 2,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn stream_registration_1_above() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 129,
            },
            Location {
                byte_offset: 16,
                line: 2,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn stream_registration_2_below() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 16,
                line: 2,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn stream_registration_2_at() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Value("[1,2]", &[]),
    );
}

#[test]
fn stream_registration_2_above() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Value("[1,2]", &[]),
    );
}

#[test]
fn root_scalar_owned_127() {
    check(
        "&a 1",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn root_scalar_owned_128() {
    check(
        "&a 1",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn root_scalar_owned_129() {
    check(
        "&a 1",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn root_scalar_registration_1_below() {
    check(
        "&a 1",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn root_scalar_registration_1_at() {
    check(
        "&a 1",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn root_scalar_registration_1_above() {
    check(
        "&a 1",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn core_scalar_owned_127() {
    check(
        "&a !!int 1",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 9,
                line: 1,
                column: 10,
            },
            &[],
        ),
    );
}

#[test]
fn core_scalar_owned_128() {
    check(
        "&a !!int 1",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn core_scalar_owned_129() {
    check(
        "&a !!int 1",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn core_scalar_registration_1_below() {
    check(
        "&a !!int 1",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 9,
                line: 1,
                column: 10,
            },
            &[],
        ),
    );
}

#[test]
fn core_scalar_registration_1_at() {
    check(
        "&a !!int 1",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn core_scalar_registration_1_above() {
    check(
        "&a !!int 1",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[1]", &[]),
    );
}

#[test]
fn empty_sequence_owned_127() {
    check(
        "&a []",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn empty_sequence_owned_128() {
    check(
        "&a []",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[]]", &[]),
    );
}

#[test]
fn empty_sequence_owned_129() {
    check(
        "&a []",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[]]", &[]),
    );
}

#[test]
fn empty_sequence_registration_1_below() {
    check(
        "&a []",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn empty_sequence_registration_1_at() {
    check(
        "&a []",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[]]", &[]),
    );
}

#[test]
fn empty_sequence_registration_1_above() {
    check(
        "&a []",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[]]", &[]),
    );
}

#[test]
fn empty_mapping_owned_127() {
    check(
        "&a {}",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn empty_mapping_owned_128() {
    check(
        "&a {}",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"map\":[]}]", &[]),
    );
}

#[test]
fn empty_mapping_owned_129() {
    check(
        "&a {}",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"map\":[]}]", &[]),
    );
}

#[test]
fn empty_mapping_registration_1_below() {
    check(
        "&a {}",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn empty_mapping_registration_1_at() {
    check(
        "&a {}",
        options(128, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"map\":[]}]", &[]),
    );
}

#[test]
fn empty_mapping_registration_1_above() {
    check(
        "&a {}",
        options(129, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"map\":[]}]", &[]),
    );
}

#[test]
fn retained_scalar_owned_265() {
    check(
        "&a !t x",
        options(265, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 265,
            },
            Location {
                byte_offset: 6,
                line: 1,
                column: 7,
            },
            &[],
        ),
    );
}

#[test]
fn retained_scalar_owned_266() {
    check(
        "&a !t x",
        options(266, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]", &[]),
    );
}

#[test]
fn retained_scalar_owned_267() {
    check(
        "&a !t x",
        options(267, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]", &[]),
    );
}

#[test]
fn retained_scalar_registration_1_below() {
    check(
        "&a !t x",
        options(265, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 265,
            },
            Location {
                byte_offset: 6,
                line: 1,
                column: 7,
            },
            &[],
        ),
    );
}

#[test]
fn retained_scalar_registration_1_at() {
    check(
        "&a !t x",
        options(266, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]", &[]),
    );
}

#[test]
fn retained_scalar_registration_1_above() {
    check(
        "&a !t x",
        options(267, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":\"x\"}]", &[]),
    );
}

#[test]
fn utf8_scalar_owned_139() {
    check(
        "&a \"é🙂\"",
        options(139, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 139,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn utf8_scalar_owned_140() {
    check(
        "&a \"é🙂\"",
        options(140, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[\"é🙂\"]", &[]),
    );
}

#[test]
fn utf8_scalar_owned_141() {
    check(
        "&a \"é🙂\"",
        options(141, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[\"é🙂\"]", &[]),
    );
}

#[test]
fn utf8_scalar_registration_1_below() {
    check(
        "&a \"é🙂\"",
        options(139, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 139,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn utf8_scalar_registration_1_at() {
    check(
        "&a \"é🙂\"",
        options(140, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[\"é🙂\"]", &[]),
    );
}

#[test]
fn utf8_scalar_registration_1_above() {
    check(
        "&a \"é🙂\"",
        options(141, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[\"é🙂\"]", &[]),
    );
}

#[test]
fn sequence_owned_255() {
    check(
        "&a [1]",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn sequence_owned_256() {
    check(
        "&a [1]",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn sequence_owned_257() {
    check(
        "&a [1]",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn sequence_registration_1_below() {
    check(
        "&a [1]",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
            &[],
        ),
    );
}

#[test]
fn sequence_registration_1_at() {
    check(
        "&a [1]",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn sequence_registration_1_above() {
    check(
        "&a [1]",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn core_sequence_owned_255() {
    check(
        "&a !!seq [1]",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 9,
                line: 1,
                column: 10,
            },
            &[],
        ),
    );
}

#[test]
fn core_sequence_owned_256() {
    check(
        "&a !!seq [1]",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn core_sequence_owned_257() {
    check(
        "&a !!seq [1]",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn core_sequence_registration_1_below() {
    check(
        "&a !!seq [1]",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 255,
            },
            Location {
                byte_offset: 9,
                line: 1,
                column: 10,
            },
            &[],
        ),
    );
}

#[test]
fn core_sequence_registration_1_at() {
    check(
        "&a !!seq [1]",
        options(256, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn core_sequence_registration_1_above() {
    check(
        "&a !!seq [1]",
        options(257, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn retained_sequence_owned_391() {
    check(
        "&a !t [1]",
        options(391, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 391,
            },
            Location {
                byte_offset: 6,
                line: 1,
                column: 7,
            },
            &[],
        ),
    );
}

#[test]
fn retained_sequence_owned_392() {
    check(
        "&a !t [1]",
        options(392, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":[1]}]", &[]),
    );
}

#[test]
fn retained_sequence_owned_393() {
    check(
        "&a !t [1]",
        options(393, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":[1]}]", &[]),
    );
}

#[test]
fn retained_sequence_registration_1_below() {
    check(
        "&a !t [1]",
        options(391, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 391,
            },
            Location {
                byte_offset: 6,
                line: 1,
                column: 7,
            },
            &[],
        ),
    );
}

#[test]
fn retained_sequence_registration_1_at() {
    check(
        "&a !t [1]",
        options(392, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":[1]}]", &[]),
    );
}

#[test]
fn retained_sequence_registration_1_above() {
    check(
        "&a !t [1]",
        options(393, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[{\"tag\":[\"!t\",\"!\",\"t\"],\"value\":[1]}]", &[]),
    );
}

#[test]
fn collection_redefinition_owned_575() {
    check(
        "[&a [], &a [2], *a]",
        options(575, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 575,
            },
            Location {
                byte_offset: 16,
                line: 1,
                column: 17,
            },
            &[2],
        ),
    );
}

#[test]
fn collection_redefinition_owned_576() {
    check(
        "[&a [], &a [2], *a]",
        options(576, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[],[2],[2]]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 16,
                        line: 1,
                        column: 17,
                    },
                    end: Location {
                        byte_offset: 18,
                        line: 1,
                        column: 19,
                    },
                },
            }],
        ),
    );
}

#[test]
fn collection_redefinition_owned_577() {
    check(
        "[&a [], &a [2], *a]",
        options(577, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value(
            "[[[],[2],[2]]]",
            &[Origin {
                document: 0,
                path: &[2],
                definition: Span {
                    start: Location {
                        byte_offset: 11,
                        line: 1,
                        column: 12,
                    },
                    end: Location {
                        byte_offset: 14,
                        line: 1,
                        column: 15,
                    },
                },
                use_site: Span {
                    start: Location {
                        byte_offset: 16,
                        line: 1,
                        column: 17,
                    },
                    end: Location {
                        byte_offset: 18,
                        line: 1,
                        column: 19,
                    },
                },
            }],
        ),
    );
}

#[test]
fn collection_redefinition_registration_1_below() {
    check(
        "[&a [], &a [2], *a]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn collection_redefinition_registration_1_at() {
    check(
        "[&a [], &a [2], *a]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 192,
            },
            Location {
                byte_offset: 11,
                line: 1,
                column: 12,
            },
            &[1],
        ),
    );
}

#[test]
fn collection_redefinition_registration_1_above() {
    check(
        "[&a [], &a [2], *a]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 193,
            },
            Location {
                byte_offset: 11,
                line: 1,
                column: 12,
            },
            &[1],
        ),
    );
}

#[test]
fn collection_redefinition_registration_2_below() {
    check(
        "[&a [], &a [2], *a]",
        options(447, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 447,
            },
            Location {
                byte_offset: 11,
                line: 1,
                column: 12,
            },
            &[1],
        ),
    );
}

#[test]
fn collection_redefinition_registration_2_at() {
    check(
        "[&a [], &a [2], *a]",
        options(448, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 448,
            },
            Location {
                byte_offset: 16,
                line: 1,
                column: 17,
            },
            &[2],
        ),
    );
}

#[test]
fn collection_redefinition_registration_2_above() {
    check(
        "[&a [], &a [2], *a]",
        options(449, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 449,
            },
            Location {
                byte_offset: 16,
                line: 1,
                column: 17,
            },
            &[2],
        ),
    );
}

#[test]
fn bom_owned_191() {
    check(
        "\u{feff}[&a 1]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn bom_owned_192() {
    check(
        "\u{feff}[&a 1]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn bom_owned_193() {
    check(
        "\u{feff}[&a 1]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn bom_registration_1_below() {
    check(
        "\u{feff}[&a 1]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn bom_registration_1_at() {
    check(
        "\u{feff}[&a 1]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn bom_registration_1_above() {
    check(
        "\u{feff}[&a 1]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn unicode_prefix_owned_261() {
    check(
        "[\"é🙂\", &a 1]",
        options(261, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 261,
            },
            Location {
                byte_offset: 14,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn unicode_prefix_owned_262() {
    check(
        "[\"é🙂\", &a 1]",
        options(262, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn unicode_prefix_owned_263() {
    check(
        "[\"é🙂\", &a 1]",
        options(263, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn unicode_prefix_registration_1_below() {
    check(
        "[\"é🙂\", &a 1]",
        options(261, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 261,
            },
            Location {
                byte_offset: 14,
                line: 1,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn unicode_prefix_registration_1_at() {
    check(
        "[\"é🙂\", &a 1]",
        options(262, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn unicode_prefix_registration_1_above() {
    check(
        "[\"é🙂\", &a 1]",
        options(263, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn lf_owned_191() {
    check(
        "[\n &a 1\n]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 6,
                line: 2,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn lf_owned_192() {
    check(
        "[\n &a 1\n]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn lf_owned_193() {
    check(
        "[\n &a 1\n]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn lf_registration_1_below() {
    check(
        "[\n &a 1\n]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 6,
                line: 2,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn lf_registration_1_at() {
    check(
        "[\n &a 1\n]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn lf_registration_1_above() {
    check(
        "[\n &a 1\n]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn crlf_owned_191() {
    check(
        "[\r\n &a 1\r\n]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 7,
                line: 2,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn crlf_owned_192() {
    check(
        "[\r\n &a 1\r\n]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn crlf_owned_193() {
    check(
        "[\r\n &a 1\r\n]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn crlf_registration_1_below() {
    check(
        "[\r\n &a 1\r\n]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 7,
                line: 2,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn crlf_registration_1_at() {
    check(
        "[\r\n &a 1\r\n]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn crlf_registration_1_above() {
    check(
        "[\r\n &a 1\r\n]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn cr_owned_191() {
    check(
        "[\r &a 1\r]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 6,
                line: 2,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn cr_owned_192() {
    check(
        "[\r &a 1\r]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn cr_owned_193() {
    check(
        "[\r &a 1\r]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn cr_registration_1_below() {
    check(
        "[\r &a 1\r]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 6,
                line: 2,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn cr_registration_1_at() {
    check(
        "[\r &a 1\r]",
        options(192, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn cr_registration_1_above() {
    check(
        "[\r &a 1\r]",
        options(193, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[1]]", &[]),
    );
}

#[test]
fn bom_unicode_crlf_owned_261() {
    check(
        "\u{feff}[\"é🙂\",\r\n &a !!int 1\r\n]",
        options(261, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 261,
            },
            Location {
                byte_offset: 25,
                line: 2,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn bom_unicode_crlf_owned_262() {
    check(
        "\u{feff}[\"é🙂\",\r\n &a !!int 1\r\n]",
        options(262, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn bom_unicode_crlf_owned_263() {
    check(
        "\u{feff}[\"é🙂\",\r\n &a !!int 1\r\n]",
        options(263, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn bom_unicode_crlf_registration_1_below() {
    check(
        "\u{feff}[\"é🙂\",\r\n &a !!int 1\r\n]",
        options(261, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 261,
            },
            Location {
                byte_offset: 25,
                line: 2,
                column: 11,
            },
            &[1],
        ),
    );
}

#[test]
fn bom_unicode_crlf_registration_1_at() {
    check(
        "\u{feff}[\"é🙂\",\r\n &a !!int 1\r\n]",
        options(262, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn bom_unicode_crlf_registration_1_above() {
    check(
        "\u{feff}[\"é🙂\",\r\n &a !!int 1\r\n]",
        options(263, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Value("[[\"é🙂\",1]]", &[]),
    );
}

#[test]
fn construction_precedes_registration() {
    check(
        "[&a 1]",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 127,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn second_document_construction() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 191,
            },
            Location {
                byte_offset: 16,
                line: 2,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn scalar_limit_before_registration() {
    check(
        "[&a 1234]",
        options(191, 128, 64, 16, 3, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxScalarBytes,
                configured: 3,
            },
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn tag_before_registration() {
    check(
        "[&a !t x]",
        options(329, 128, 64, 16, 16, UnknownTags::Reject),
        Mode::All,
        Expected::Error(
            ErrorKind::UnknownTag,
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn resolution_before_registration() {
    check(
        "[&a !!int nope]",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::TagValueMismatch,
            Location {
                byte_offset: 4,
                line: 1,
                column: 5,
            },
            &[0],
        ),
    );
}

#[test]
fn syntax_before_registration() {
    check(
        "[&a 1\n",
        options(191, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::Syntax("unclosed_flow_collection".into()),
            Location {
                byte_offset: 0,
                line: 1,
                column: 1,
            },
            &[],
        ),
    );
}

#[test]
fn source_depth_before_registration() {
    check(
        "[&a 1, [[0]]]",
        options(191, 128, 64, 1, 16, UnknownTags::Preserve),
        Mode::All,
        Expected::Error(
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxDepth,
                configured: 1,
            },
            Location {
                byte_offset: 7,
                line: 1,
                column: 8,
            },
            &[],
        ),
    );
}

#[test]
fn bindings_are_document_local() {
    check(
        "--- &a 1\n--- *a\n",
        options(4096, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::UnknownAnchor,
            Location {
                byte_offset: 13,
                line: 2,
                column: 5,
            },
            &[],
        ),
    );
}

#[test]
fn terminal_unknown_before_registration() {
    check(
        "--- &a 1\n--- *a\n",
        options(127, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Stream,
        Expected::Error(
            ErrorKind::UnknownAnchor,
            Location {
                byte_offset: 13,
                line: 2,
                column: 5,
            },
            &[],
        ),
    );
}

#[test]
fn single_document_cardinality() {
    check(
        "--- &a 1\n--- &a 2\n",
        options(255, 128, 64, 16, 16, UnknownTags::Preserve),
        Mode::Single,
        Expected::Error(
            ErrorKind::MultipleDocuments,
            Location {
                byte_offset: 9,
                line: 2,
                column: 1,
            },
            &[],
        ),
    );
}
