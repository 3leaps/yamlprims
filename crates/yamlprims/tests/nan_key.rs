//! Completed ordinary keys are admitted before diagnostic cloning and value construction.
//! Expectations follow specification sections 5.1, 5.2, 6 and 7.2.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use yamlprims::*;

const PAYLOAD_BYTES: usize = 131_071;
thread_local! {
    static LARGE_ALLOCS: Cell<Option<usize>> = const { Cell::new(None) };
}

struct Counting;

fn count(size: usize) {
    if size >= PAYLOAD_BYTES {
        let _ = LARGE_ALLOCS.try_with(|counter| {
            if let Some(n) = counter.get() {
                counter.set(Some(n + 1));
            }
        });
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if size > layout.size() {
            count(size);
        }
        unsafe { System.realloc(p, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn measured(input: &str, key_bytes: usize) -> (Error, usize) {
    let mut options = Options::default();
    options.limits.max_key_bytes = key_bytes;
    options.validate().unwrap();
    LARGE_ALLOCS.with(|counter| counter.set(Some(0)));
    let result = load_with(input, &options);
    let allocations = LARGE_ALLOCS.with(|counter| counter.replace(None).unwrap());
    (result.unwrap_err(), allocations)
}

#[test]
fn nan_rejection_does_not_clone_the_diagnostic_key() {
    // One sequence and two scalar nodes: 3 * 64 + the decoded String bytes.
    // Both rejection paths parse and build exactly the same key. The valid-key
    // control proves that the large diagnostic clone is visible to the counter.
    let weight = 192 + PAYLOAD_BYTES;
    let input = format!(
        "? [.nan, \"{}\"]\n: !!int nope\n",
        "x".repeat(PAYLOAD_BYTES)
    );
    let valid_key = input.replacen(".nan", "null", 1);
    let (oversized, before_path) = measured(&input, weight - 1);
    let (invalid, invalid_allocations) = measured(&input, weight);
    let (value_error, with_path) = measured(&valid_key, weight);
    assert_eq!(
        oversized.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            configured: (weight - 1) as u64,
        }
    );
    assert_eq!(invalid.kind, ErrorKind::InvalidKey);
    assert_eq!(value_error.kind, ErrorKind::TagValueMismatch);
    assert_eq!(invalid_allocations, before_path, "no diagnostic-key clone");
    assert!(
        with_path > before_path,
        "allocation counter must detect a diagnostic-key clone"
    );
}

#[test]
fn bounded_parser_keys_reject_nan_on_2mib_stack() {
    for max_depth in [128, 255] {
        for tagged in [false, true] {
            let levels = max_depth - 1; // The root mapping consumes one level.
            let opening = if tagged { "!t [" } else { "[" };
            let input = format!(
                "? {}.nan{}\n: !!int nope\n",
                opening.repeat(levels),
                "]".repeat(levels)
            );
            let at = if tagged { 5 } else { 2 };
            std::thread::Builder::new()
                .stack_size(2 << 20)
                .spawn(move || {
                    let mut options = Options::default();
                    options.limits.max_depth = max_depth;
                    check(
                        &input,
                        &options,
                        false,
                        ErrorKind::InvalidKey,
                        Location {
                            byte_offset: at,
                            line: 1,
                            column: at + 1,
                        },
                        &[],
                        &[],
                    );
                })
                .unwrap()
                .join()
                .unwrap();
        }
    }
}

fn check(
    input: &str,
    options: &Options,
    stream: bool,
    want: ErrorKind,
    at: Location,
    path: &[PathSegment],
    related: &[(RelatedRole, Span)],
) {
    options.validate().expect("valid noncompeting options");
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
        assert_eq!(error.kind, want, "{input:?}: {error:?}");
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

#[test]
fn scalar_nan0() {
    let options = Options::default();
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn scalar_nan1() {
    let options = Options::default();
    check(
        ".NaN: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn scalar_nan2() {
    let options = Options::default();
    check(
        ".NAN: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn core_float() {
    let options = Options::default();
    check(
        "!!float .nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 8,
            line: 1,
            column: 9,
        },
        &[],
        &[],
    );
}

#[test]
fn sequence() {
    let options = Options::default();
    check(
        "? [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn nested_sequence() {
    let options = Options::default();
    check(
        "? [[.nan]]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn mapping_value() {
    let options = Options::default();
    check(
        "? {a: .nan}\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn tagged_sequence() {
    let options = Options::default();
    check(
        "? !t [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 5,
            line: 1,
            column: 6,
        },
        &[],
        &[],
    );
}

#[test]
fn tagged_mapping() {
    let options = Options::default();
    check(
        "? !t {a: .nan}\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 5,
            line: 1,
            column: 6,
        },
        &[],
        &[],
    );
}

#[test]
fn nested_path() {
    let options = Options::default();
    check(
        "outer: [0, {.nan: !!int nope}]\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 12,
            line: 1,
            column: 13,
        },
        &[
            PathSegment::Key(Box::new(Value::String("outer".into()))),
            PathSegment::Index(1),
        ],
        &[],
    );
}

#[test]
fn bom_unicode_crlf() {
    let options = Options::default();
    check(
        "\u{feff}é: 0\r\nouter: { .nan: !!int nope }\r\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 19,
            line: 2,
            column: 10,
        },
        &[PathSegment::Key(Box::new(Value::String("outer".into())))],
        &[],
    );
}

#[test]
fn full_mapping() {
    let mut options = Options::default();
    options.limits.max_mapping_entries = 1;
    check(
        "a: 0\n.nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 5,
            line: 2,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn value_scalar_limit() {
    let mut options = Options::default();
    options.limits.max_scalar_bytes = 4;
    check(
        ".nan: abcde\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn value_node_limit() {
    let mut options = Options::default();
    options.limits.max_nodes = 2;
    check(
        ".nan: [0]\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn value_owned_limit() {
    let mut options = Options::default();
    options.limits.max_owned_bytes = 128;
    options.limits.max_key_bytes = 64;
    options.limits.max_scalar_bytes = 4;
    check(
        ".nan: 0\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn entry_charge_limit() {
    let mut options = Options::default();
    options.limits.max_owned_bytes = 192;
    options.limits.max_key_bytes = 64;
    options.limits.max_scalar_bytes = 4;
    check(
        ".nan: 0\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn earlier_key_node_limit() {
    let mut options = Options::default();
    options.limits.max_nodes = 1;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 1,
        },
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn earlier_key_owned_limit() {
    let mut options = Options::default();
    options.limits.max_owned_bytes = 127;
    options.limits.max_key_bytes = 64;
    options.limits.max_scalar_bytes = 4;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxOwnedBytes,
            configured: 127,
        },
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn earlier_key_scalar_limit() {
    let mut options = Options::default();
    options.limits.max_scalar_bytes = 3;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxScalarBytes,
            configured: 3,
        },
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn incomplete_key_resolution() {
    let options = Options::default();
    check(
        "? [.nan, !!int nope]\n: 0\n",
        &options,
        false,
        ErrorKind::TagValueMismatch,
        Location {
            byte_offset: 9,
            line: 1,
            column: 10,
        },
        &[PathSegment::Index(1)],
        &[],
    );
}

#[test]
fn incomplete_key_node_limit() {
    let mut options = Options::default();
    options.limits.max_nodes = 3;
    check(
        "? [.nan, 0]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxNodes,
            configured: 3,
        },
        Location {
            byte_offset: 9,
            line: 1,
            column: 10,
        },
        &[PathSegment::Index(1)],
        &[],
    );
}

#[test]
fn incomplete_key_recursive_alias() {
    let options = Options::default();
    check(
        "? &a [.nan, *a]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::RecursiveAlias,
        Location {
            byte_offset: 12,
            line: 1,
            column: 13,
        },
        &[PathSegment::Index(1)],
        &[],
    );
}

#[test]
fn inner_key_before_outer_complete() {
    let options = Options::default();
    check(
        "? {.nan: !!int nope}\n: 0\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 3,
            line: 1,
            column: 4,
        },
        &[],
        &[],
    );
}

#[test]
fn tag_policy_before_children() {
    let mut options = Options::default();
    options.unknown_tags = UnknownTags::Reject;
    check(
        "? !t [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::UnknownTag,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn source_syntax_before_nan() {
    let options = Options::default();
    check(
        ".nan: [\n",
        &options,
        false,
        ErrorKind::Syntax("unclosed_flow_collection".into()),
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[],
        &[],
    );
}

#[test]
fn alias_scalar() {
    let options = Options::default();
    check(
        "[&n .nan, {*n : !!int nope}]\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        },
        &[PathSegment::Index(1)],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 11,
                    line: 1,
                    column: 12,
                },
                end: Location {
                    byte_offset: 13,
                    line: 1,
                    column: 14,
                },
            },
        )],
    );
}

#[test]
fn alias_sequence() {
    let options = Options::default();
    check(
        "a: &n [.nan]\n? *n\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 15,
                    line: 2,
                    column: 3,
                },
                end: Location {
                    byte_offset: 17,
                    line: 2,
                    column: 5,
                },
            },
        )],
    );
}

#[test]
fn alias_mapping() {
    let options = Options::default();
    check(
        "a: &n {a: .nan}\n? *n\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 18,
                    line: 2,
                    column: 3,
                },
                end: Location {
                    byte_offset: 20,
                    line: 2,
                    column: 5,
                },
            },
        )],
    );
}

#[test]
fn alias_descendant_in_key() {
    let options = Options::default();
    check(
        "a: &n .nan\n? [*n]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 13,
            line: 2,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn alias_inner_key() {
    let options = Options::default();
    check(
        "a: &n .nan\n? {*n : !!int nope}\n: 0\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 14,
                    line: 2,
                    column: 4,
                },
                end: Location {
                    byte_offset: 16,
                    line: 2,
                    column: 6,
                },
            },
        )],
    );
}

#[test]
fn stream_second_document() {
    let options = Options::default();
    check(
        "---\nok: 1\n---\n.nan: !!int nope\n",
        &options,
        true,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 14,
            line: 4,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn ordinary_in_reject() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Reject;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn ordinary_in_apply() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn ordinary_in_askey() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::AsKey;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_scalar_below() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 63;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            configured: 63,
        },
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_scalar_at() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 64;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_scalar_above() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 65;
    check(
        ".nan: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_sequence_below() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 127;
    check(
        "? [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            configured: 127,
        },
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_sequence_at() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 128;
    check(
        "? [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_sequence_above() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 129;
    check(
        "? [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_mapping_below() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 216;
    check(
        "? {a: .nan}\n: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            configured: 216,
        },
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_mapping_at() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 217;
    check(
        "? {a: .nan}\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_mapping_above() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 218;
    check(
        "? {a: .nan}\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_tagged_sequence_below() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 195;
    check(
        "? !t [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            configured: 195,
        },
        Location {
            byte_offset: 5,
            line: 1,
            column: 6,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_tagged_sequence_at() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 196;
    check(
        "? !t [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 5,
            line: 1,
            column: 6,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_tagged_sequence_above() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 197;
    check(
        "? !t [.nan]\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 5,
            line: 1,
            column: 6,
        },
        &[],
        &[],
    );
}

#[test]
fn weight_alias_scalar_below() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 63;
    check(
        "[&n .nan, {*n : !!int nope}]\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            configured: 63,
        },
        Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        },
        &[PathSegment::Index(1)],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 11,
                    line: 1,
                    column: 12,
                },
                end: Location {
                    byte_offset: 13,
                    line: 1,
                    column: 14,
                },
            },
        )],
    );
}

#[test]
fn weight_alias_scalar_at() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 64;
    check(
        "[&n .nan, {*n : !!int nope}]\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        },
        &[PathSegment::Index(1)],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 11,
                    line: 1,
                    column: 12,
                },
                end: Location {
                    byte_offset: 13,
                    line: 1,
                    column: 14,
                },
            },
        )],
    );
}

#[test]
fn weight_alias_scalar_above() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 65;
    check(
        "[&n .nan, {*n : !!int nope}]\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        },
        &[PathSegment::Index(1)],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 11,
                    line: 1,
                    column: 12,
                },
                end: Location {
                    byte_offset: 13,
                    line: 1,
                    column: 14,
                },
            },
        )],
    );
}

#[test]
fn weight_alias_sequence_below() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 127;
    check(
        "a: &n [.nan]\n? *n\n: !!int nope\n",
        &options,
        false,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxKeyBytes,
            configured: 127,
        },
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 15,
                    line: 2,
                    column: 3,
                },
                end: Location {
                    byte_offset: 17,
                    line: 2,
                    column: 5,
                },
            },
        )],
    );
}

#[test]
fn weight_alias_sequence_at() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 128;
    check(
        "a: &n [.nan]\n? *n\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 15,
                    line: 2,
                    column: 3,
                },
                end: Location {
                    byte_offset: 17,
                    line: 2,
                    column: 5,
                },
            },
        )],
    );
}

#[test]
fn weight_alias_sequence_above() {
    let mut options = Options::default();
    options.limits.max_key_bytes = 129;
    check(
        "a: &n [.nan]\n? *n\n: !!int nope\n",
        &options,
        false,
        ErrorKind::InvalidKey,
        Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        },
        &[],
        &[(
            RelatedRole::AliasUse,
            Span {
                start: Location {
                    byte_offset: 15,
                    line: 2,
                    column: 3,
                },
                end: Location {
                    byte_offset: 17,
                    line: 2,
                    column: 5,
                },
            },
        )],
    );
}

#[test]
fn quoted_nan() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::AsKey;
    load_with("\".nan\": 0\n", &options).unwrap();
}

#[test]
fn core_string_nan() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::AsKey;
    load_with("!!str .nan: 0\n", &options).unwrap();
}

#[test]
fn local_tag_scalar_is_string() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::AsKey;
    load_with("!t .nan: 0\n", &options).unwrap();
}

#[test]
fn nan_is_allowed_as_value() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::AsKey;
    load_with("a: .nan\n", &options).unwrap();
}

#[test]
fn finite_and_infinite_keys() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::AsKey;
    load_with("{.inf: 0, -.inf: 0, 0.0: 0}\n", &options).unwrap();
}

#[test]
fn as_key_merge_string() {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::AsKey;
    load_with("<<: .nan\n", &options).unwrap();
}
