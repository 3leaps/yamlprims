//! Merge-source validity, shadowing, capacity and logical-copy admission (§§5–7).
use yamlprims::*;

fn check_error(
    input: &str,
    options: &Options,
    stream: bool,
    kind: ErrorKind,
    at: Location,
    path: &[PathSegment],
    related: &[(RelatedRole, Span)],
) {
    options.validate().expect("valid positive limits");
    let mut errors = vec![
        load_all_with(input, options).unwrap_err(),
        load_all_bytes(input.as_bytes(), options).unwrap_err(),
    ];
    if !stream {
        errors.push(load_with(input, options).unwrap_err());
        errors.push(load_bytes(input.as_bytes(), options).unwrap_err());
        errors.push(from_str_with::<Value>(input, options).unwrap_err());
    }
    for error in errors {
        assert_eq!(error.kind, kind, "{input:?}: {error:?}");
        assert_eq!(error.site, Site::Source(at), "{input:?}");
        assert_eq!(error.path.len(), path.len(), "{input:?}: {:?}", error.path);
        for (got, want) in error.path.iter().zip(path) {
            match (got, want) {
                (PathSegment::Index(a), PathSegment::Index(b)) => assert_eq!(a, b),
                (PathSegment::Key(a), PathSegment::Key(b)) => assert!(key_eq(a, b)),
                _ => panic!("path kind: {got:?} / {want:?}"),
            }
        }
        assert_eq!(
            error.related.len(),
            related.len(),
            "{input:?}: {:?}",
            error.related
        );
        for (got, (role, span)) in error.related.iter().zip(related) {
            assert_eq!(got.role, *role);
            assert_eq!(got.span, *span);
        }
        encode_error(&error, options).expect("bounded diagnostic encodes");
    }
}

fn check_success(
    input: &str,
    options: &Options,
    path: &[PathSegment],
    entries: &[(&str, i64)],
    merged: &[&str],
    directive: Span,
) {
    options.validate().unwrap();
    let mut documents = vec![
        load_with(input, options).unwrap(),
        load_bytes(input.as_bytes(), options).unwrap(),
    ];
    documents.extend(load_all_with(input, options).unwrap());
    documents.extend(load_all_bytes(input.as_bytes(), options).unwrap());
    from_str_with::<Value>(input, options).expect("typed path shares successful admission");
    assert_eq!(documents.len(), 4);
    for document in documents {
        let mut node = document.root();
        for segment in path {
            node = match segment {
                PathSegment::Index(i) => &node.value().as_sequence().unwrap()[*i],
                PathSegment::Key(k) => node.value().as_mapping().unwrap().get_value(k).unwrap(),
            };
        }
        let map = node.value().as_mapping().unwrap();
        assert_eq!(map.len(), entries.len());
        for ((k, v), (name, number)) in map.iter().zip(entries) {
            assert_eq!(k.value().as_str(), Some(*name));
            assert_eq!(v.value().as_integer().unwrap().to_i64(), Some(*number));
            if merged.contains(name) {
                assert_eq!(k.via(), Some(&Via::Merge { directive }));
                assert_eq!(v.via(), Some(&Via::Merge { directive }));
            } else {
                assert_eq!(k.via(), None);
                assert_eq!(v.via(), None);
            }
        }
    }
}

#[test]
fn capacity_before_nodes() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 8;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn capacity_before_expanded() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_expanded_nodes = 1;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn capacity_before_owned() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_owned_bytes = 676;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn capacity_before_all_copy() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 8;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 676;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn capacity_before_exact_prefix() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 7;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 524;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn capacity_before_no_competing_budget() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_nodes_only() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 8;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 8,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_expanded_only() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_expanded_nodes = 1;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxExpandedNodes,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_owned_only() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_owned_bytes = 676;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxOwnedBytes,
            configured: 676,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_nodes_expanded() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 8;
    options.limits.max_expanded_nodes = 1;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 8,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_nodes_owned() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 8;
    options.limits.max_owned_bytes = 676;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 8,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_expanded_owned() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 676;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxExpandedNodes,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_all_copy() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 8;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 676;
    check_error(
        "{x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 8,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn copy_exact_or_above_0() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 9;
    options.limits.max_expanded_nodes = 2;
    options.limits.max_owned_bytes = 677;
    check_success(
        "{x: 0, <<: {a: 1}}",
        &options,
        &[],
        &[("x", 0), ("a", 1)],
        &["a"],
        Span {
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
    );
}

#[test]
fn copy_exact_or_above_1() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 10;
    options.limits.max_expanded_nodes = 3;
    options.limits.max_owned_bytes = 678;
    check_success(
        "{x: 0, <<: {a: 1}}",
        &options,
        &[],
        &[("x", 0), ("a", 1)],
        &["a"],
        Span {
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
    );
}

#[test]
fn two_explicit_at_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, y: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 2,
        },
        Location {
            byte_offset: 13,
            line: 1,
            column: 14,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn explicit_after_directive() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{<<: {a: 1}, x: 0}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 1,
                    line: 1,
                    column: 2,
                },
                end: Location {
                    byte_offset: 3,
                    line: 1,
                    column: 4,
                },
            },
        )],
    );
}

#[test]
fn core_map_source() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: !!map {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn tagged_receiver() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "!t {x: 0, <<: {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 10,
            line: 1,
            column: 11,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 10,
                    line: 1,
                    column: 11,
                },
                end: Location {
                    byte_offset: 12,
                    line: 1,
                    column: 13,
                },
            },
        )],
    );
}

#[test]
fn explicit_merge_tag() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 256;
    check_error(
        "{x: 0, !!merge \"<<\": {a: 1}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 15,
            line: 1,
            column: 16,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 15,
                    line: 1,
                    column: 16,
                },
                end: Location {
                    byte_offset: 19,
                    line: 1,
                    column: 20,
                },
            },
        )],
    );
}

#[test]
fn partial_capacity_nodes() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 14;
    check_error(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 2,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn partial_capacity_expanded() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_expanded_nodes = 3;
    check_error(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 2,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn partial_capacity_owned() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_owned_bytes = 1110;
    check_error(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 2,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn partial_capacity_exact_first_copy() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 13;
    options.limits.max_expanded_nodes = 2;
    options.limits.max_owned_bytes = 958;
    check_error(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 2,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn earlier_copy_nodes() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 12;
    check_error(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 12,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn earlier_copy_expanded() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_expanded_nodes = 1;
    check_error(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxExpandedNodes,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn earlier_copy_owned() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_owned_bytes = 957;
    check_error(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxOwnedBytes,
            configured: 957,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn partial_all_fit() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 3;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 15;
    options.limits.max_expanded_nodes = 4;
    options.limits.max_owned_bytes = 1111;
    check_success(
        "{x: 0, <<: [{a: 1}, {b: 2}]}",
        &options,
        &[],
        &[("x", 0), ("a", 1), ("b", 2)],
        &["a", "b"],
        Span {
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
    );
}

#[test]
fn shadow_before() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 7;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 524;
    check_success(
        "{x: 0, <<: {x: 1}}",
        &options,
        &[],
        &[("x", 0)],
        &[],
        Span {
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
    );
}

#[test]
fn shadow_after() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 7;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 524;
    check_success(
        "{<<: {x: 1}, x: 0}",
        &options,
        &[],
        &[("x", 0)],
        &[],
        Span {
            start: Location {
                byte_offset: 1,
                line: 1,
                column: 2,
            },
            end: Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
        },
    );
}

#[test]
fn shadow_then_unshadowed_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 11;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 805;
    check_error(
        "{x: 0, <<: [{x: 1}, {a: 2}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn shadow_then_unshadowed_fit() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 13;
    options.limits.max_expanded_nodes = 2;
    options.limits.max_owned_bytes = 958;
    check_success(
        "{x: 0, <<: [{x: 1}, {a: 2}]}",
        &options,
        &[],
        &[("x", 0), ("a", 2)],
        &["a"],
        Span {
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
    );
}

#[test]
fn earlier_source_shadow_at_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 11;
    options.limits.max_expanded_nodes = 2;
    options.limits.max_owned_bytes = 805;
    check_success(
        "{<<: [{a: 1}, {a: 2}]}",
        &options,
        &[],
        &[("a", 1)],
        &["a"],
        Span {
            start: Location {
                byte_offset: 1,
                line: 1,
                column: 2,
            },
            end: Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
        },
    );
}

#[test]
fn earlier_shadow_then_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 14;
    options.limits.max_expanded_nodes = 2;
    options.limits.max_owned_bytes = 1022;
    check_error(
        "{<<: [{a: 1}, {a: 2}, {b: 3}]}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 1,
                    line: 1,
                    column: 2,
                },
                end: Location {
                    byte_offset: 3,
                    line: 1,
                    column: 4,
                },
            },
        )],
    );
}

#[test]
fn ordered_sources_fit() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 16;
    options.limits.max_expanded_nodes = 4;
    options.limits.max_owned_bytes = 1175;
    check_success(
        "{<<: [{a: 1}, {a: 2}, {b: 3}]}",
        &options,
        &[],
        &[("a", 1), ("b", 3)],
        &["a", "b"],
        Span {
            start: Location {
                byte_offset: 1,
                line: 1,
                column: 2,
            },
            end: Location {
                byte_offset: 3,
                line: 1,
                column: 4,
            },
        },
    );
}

#[test]
fn empty_map() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 5;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 371;
    check_success(
        "{x: 0, <<: {}}",
        &options,
        &[],
        &[("x", 0)],
        &[],
        Span {
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
    );
}

#[test]
fn empty_list() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 5;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 371;
    check_success(
        "{x: 0, <<: []}",
        &options,
        &[],
        &[("x", 0)],
        &[],
        Span {
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
    );
}

#[test]
fn directive_slot_order() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 4;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_success(
        "{x: 0, <<: [{a: 1}, {b: 2}], y: 0}",
        &options,
        &[],
        &[("x", 0), ("a", 1), ("b", 2), ("y", 0)],
        &["a", "b"],
        Span {
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
    );
}

#[test]
fn aliased_map_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 11;
    options.limits.max_expanded_nodes = 3;
    options.limits.max_owned_bytes = 1022;
    check_error(
        "[&s {a: 1}, {x: 0, <<: *s}]",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 19,
            line: 1,
            column: 20,
        },
        &[PathSegment::Index(1)],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 19,
                    line: 1,
                    column: 20,
                },
                end: Location {
                    byte_offset: 21,
                    line: 1,
                    column: 22,
                },
            },
        )],
    );
}

#[test]
fn aliased_map_fit() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 13;
    options.limits.max_expanded_nodes = 5;
    options.limits.max_owned_bytes = 1175;
    check_success(
        "[&s {a: 1}, {x: 0, <<: *s}]",
        &options,
        &[PathSegment::Index(1)],
        &[("x", 0), ("a", 1)],
        &["a"],
        Span {
            start: Location {
                byte_offset: 19,
                line: 1,
                column: 20,
            },
            end: Location {
                byte_offset: 21,
                line: 1,
                column: 22,
            },
        },
    );
}

#[test]
fn aliased_list_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 13;
    options.limits.max_expanded_nodes = 4;
    options.limits.max_owned_bytes = 1214;
    check_error(
        "[&s [{a: 1}], {x: 0, <<: *s}]",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 21,
            line: 1,
            column: 22,
        },
        &[PathSegment::Index(1)],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 21,
                    line: 1,
                    column: 22,
                },
                end: Location {
                    byte_offset: 23,
                    line: 1,
                    column: 24,
                },
            },
        )],
    );
}

#[test]
fn aliased_list_fit() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 15;
    options.limits.max_expanded_nodes = 6;
    options.limits.max_owned_bytes = 1367;
    check_success(
        "[&s [{a: 1}], {x: 0, <<: *s}]",
        &options,
        &[PathSegment::Index(1)],
        &[("x", 0), ("a", 1)],
        &["a"],
        Span {
            start: Location {
                byte_offset: 21,
                line: 1,
                column: 22,
            },
            end: Location {
                byte_offset: 23,
                line: 1,
                column: 24,
            },
        },
    );
}

#[test]
fn nested_outer_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 11;
    options.limits.max_expanded_nodes = 2;
    options.limits.max_owned_bytes = 831;
    check_error(
        "{x: 0, <<: {<<: {a: 1}}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn nested_fit() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 2;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 13;
    options.limits.max_expanded_nodes = 4;
    options.limits.max_owned_bytes = 984;
    check_success(
        "{x: 0, <<: {<<: {a: 1}}}",
        &options,
        &[],
        &[("x", 0), ("a", 1)],
        &["a"],
        Span {
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
    );
}

#[test]
fn nested_inner_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{<<: {x: 0, <<: {a: 1}}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 12,
            line: 1,
            column: 13,
        },
        &[PathSegment::Key(Box::new(Value::String("<<".into())))],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn invalid_later_item_before_capacity_and_copy() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 9;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 652;
    check_error(
        "{x: 0, <<: [{a: 1}, 0]}",
        &options,
        false,
        ErrorKind::MergeSource,
        Location {
            byte_offset: 20,
            line: 1,
            column: 21,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn invalid_scalar_source_before_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: 1}",
        &options,
        false,
        ErrorKind::MergeSource,
        Location {
            byte_offset: 11,
            line: 1,
            column: 12,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn invalid_tagged_source_before_capacity() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: !t {a: 1}}",
        &options,
        false,
        ErrorKind::MergeSource,
        Location {
            byte_offset: 14,
            line: 1,
            column: 15,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
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
        )],
    );
}

#[test]
fn invalid_aliased_item() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "[&bad 0, {x: 0, <<: [{a: 1}, *bad]}]",
        &options,
        false,
        ErrorKind::MergeSource,
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[PathSegment::Index(1)],
        &[
            (
                RelatedRole::AliasUse,
                Span {
                    start: Location {
                        byte_offset: 29,
                        line: 1,
                        column: 30,
                    },
                    end: Location {
                        byte_offset: 33,
                        line: 1,
                        column: 34,
                    },
                },
            ),
            (
                RelatedRole::MergeDirective,
                Span {
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
            ),
        ],
    );
}

#[test]
fn invalid_item_in_aliased_list() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "[&s [{a: 1}, 0], {x: 0, <<: *s}]",
        &options,
        false,
        ErrorKind::MergeSource,
        Location {
            byte_offset: 13,
            line: 1,
            column: 14,
        },
        &[PathSegment::Index(1)],
        &[
            (
                RelatedRole::AliasUse,
                Span {
                    start: Location {
                        byte_offset: 28,
                        line: 1,
                        column: 29,
                    },
                    end: Location {
                        byte_offset: 30,
                        line: 1,
                        column: 31,
                    },
                },
            ),
            (
                RelatedRole::MergeDirective,
                Span {
                    start: Location {
                        byte_offset: 24,
                        line: 1,
                        column: 25,
                    },
                    end: Location {
                        byte_offset: 26,
                        line: 1,
                        column: 27,
                    },
                },
            ),
        ],
    );
}

#[test]
fn source_ordinary_capacity_is_earlier() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: {a: 1, b: 2}}",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 18,
            line: 1,
            column: 19,
        },
        &[PathSegment::Key(Box::new(Value::String("<<".into())))],
        &[],
    );
}

#[test]
fn source_key_nan_is_earlier() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: {.nan: !!int nope}}",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 12,
            line: 1,
            column: 13,
        },
        &[PathSegment::Key(Box::new(Value::String("<<".into())))],
        &[],
    );
}

#[test]
fn source_value_resolution_is_earlier() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: {a: !!int nope}}",
        &options,
        false,
        ErrorKind::TagValueMismatch,
        Location {
            byte_offset: 15,
            line: 1,
            column: 16,
        },
        &[
            PathSegment::Key(Box::new(Value::String("<<".into()))),
            PathSegment::Key(Box::new(Value::String("a".into()))),
        ],
        &[],
    );
}

#[test]
fn second_directive_is_earlier() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "{x: 0, <<: {a: 1}, <<: 0}",
        &options,
        false,
        ErrorKind::DuplicateKey,
        Location {
            byte_offset: 19,
            line: 1,
            column: 20,
        },
        &[],
        &[(
            RelatedRole::FirstDefinition,
            Span {
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
        )],
    );
}

#[test]
fn bom_unicode_crlf() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    check_error(
        "\u{feff}é: 0\r\nout:\r\n  x: 0\r\n  <<: {a: 1}\r\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 26,
            line: 4,
            column: 3,
        },
        &[PathSegment::Key(Box::new(Value::String("out".into())))],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 26,
                    line: 4,
                    column: 3,
                },
                end: Location {
                    byte_offset: 28,
                    line: 4,
                    column: 5,
                },
            },
        )],
    );
}

#[test]
fn stream_cumulative_prefix() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_mapping_entries = 1;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 128;
    options.limits.max_nodes = 8;
    options.limits.max_expanded_nodes = 1;
    options.limits.max_owned_bytes = 588;
    check_error(
        "--- 0\n--- {x: 0, <<: {a: 1}}\n",
        &options,
        true,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxMappingEntries,
            configured: 1,
        },
        Location {
            byte_offset: 17,
            line: 2,
            column: 12,
        },
        &[],
        &[(
            RelatedRole::MergeDirective,
            Span {
                start: Location {
                    byte_offset: 17,
                    line: 2,
                    column: 12,
                },
                end: Location {
                    byte_offset: 19,
                    line: 2,
                    column: 14,
                },
            },
        )],
    );
}
