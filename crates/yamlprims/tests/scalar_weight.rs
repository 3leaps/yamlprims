//! Scalar and key weights follow the logical formula in specification §6.

use yamlprims::{load_all_with, load_with, ErrorKind, LimitKind, Options};

fn boundary(input: &str, limit: LimitKind, want: usize, documents: usize) {
    for configured in [want - 1, want, want + 1] {
        let mut options = Options::default();
        match limit {
            LimitKind::MaxOwnedBytes => {
                options.limits.max_owned_bytes = configured;
                options.limits.max_scalar_bytes = configured.min(8);
                options.limits.max_key_bytes = configured.min(256);
            }
            LimitKind::MaxKeyBytes => options.limits.max_key_bytes = configured,
            LimitKind::MaxScalarBytes => options.limits.max_scalar_bytes = configured,
            LimitKind::MaxNodes => options.limits.max_nodes = configured,
            LimitKind::MaxExpandedNodes => options.limits.max_expanded_nodes = configured,
            _ => panic!("unsupported boundary {limit:?}"),
        }
        options.validate().expect("noncompeting valid options");
        let result = if documents == 1 {
            load_with(input, &options).map(|_| 1)
        } else {
            load_all_with(input, &options).map(|docs| docs.len())
        };
        if configured < want {
            assert_eq!(
                result.expect_err(input).kind,
                ErrorKind::LimitExceeded {
                    limit,
                    configured: configured as u64,
                },
                "{input:?}: {limit:?}={configured}",
            );
        } else {
            assert_eq!(
                result.expect(input),
                documents,
                "{input:?}: {limit:?}={configured}"
            );
        }
    }
}

macro_rules! weight_boundaries {
    ($limit:ident; $( $name:ident: $input:expr => $want:expr, )*) => {
        $(
            #[test]
            fn $name() {
                boundary($input, LimitKind::$limit, $want, 1);
            }
        )*
    };
}

// Non-String scalars have no text charge; core tags add no wrapper.
// String cases count decoded UTF-8 bytes, retained tags count their wrapper,
// and mapping entries add 24 bytes. Cached scalars and copies use that same weight.
weight_boundaries! { MaxOwnedBytes;
    owned_null: "null\n" => 64,
    owned_bool: "false\n" => 64,
    owned_int: "-123\n" => 64,
    owned_hex: "0x1F\n" => 64,
    owned_float: "1.5e3\n" => 64,
    owned_infinity: ".inf\n" => 64,
    owned_nan: ".nan\n" => 64,
    owned_core_null: "!!null null\n" => 64,
    owned_core_bool: "!!bool false\n" => 64,
    owned_core_int: "!!int 123\n" => 64,
    owned_core_float: "!!float 123\n" => 64,
    owned_plain_string: "abc\n" => 67,
    owned_quoted_number: "\"123\"\n" => 67,
    owned_core_string: "!!str 123\n" => 67,
    owned_nonspecific_string: "! 123\n" => 67,
    owned_escaped_string: "\"a\\tb\"\n" => 67,
    owned_utf8_string: "\"é🙂\"\n" => 70,
    owned_literal_string: "|-\n  abc\n" => 67,
    owned_tagged_string: "!t x\n" => 133,
    owned_tagged_number: "!t 123\n" => 135,
    owned_two_null: "[~, ~]\n" => 192,
    owned_two_int: "[1, 22]\n" => 192,
    owned_two_strings: "[\"1\", \"22\"]\n" => 195,
    owned_mapping: "{a: b}\n" => 218,
    owned_anchor_int: "&a 123\n" => 128,
    owned_alias_int: "[&a 1, *a]\n" => 256,
    owned_alias_tagged: "[&a !t x, *a]\n" => 463,
}

weight_boundaries! { MaxKeyBytes;
    key_key_int: "123: v\n" => 64,
    key_key_bool: "true: v\n" => 64,
    key_key_string: "\"123\": v\n" => 67,
    key_key_core_int: "!!int 123: v\n" => 64,
    key_key_core_string: "!!str 123: v\n" => 67,
    key_key_sequence: "? [1, 22]\n: v\n" => 192,
    key_key_string_sequence: "? [\"1\", \"22\"]\n: v\n" => 195,
    key_key_tagged: "!t 123: v\n" => 135,
}

#[test]
fn registration_charges_accumulate_across_stream_documents() {
    // Two scalar constructions and two cache registrations: 4 * 64.
    boundary("--- &a 1\n--- &a 2\n", LimitKind::MaxOwnedBytes, 256, 2);
}

#[test]
fn scalar_input_bytes_remain_independent_of_value_weight() {
    for input in ["1234\n", "!!int 1234\n", "\"1234\"\n", "!t 1234\n"] {
        boundary(input, LimitKind::MaxScalarBytes, 4, 1);
    }
}

#[test]
fn registration_adds_no_materialized_nodes() {
    // The sequence, definition and alias are three nodes; cache storage adds none.
    boundary("[&a 1, *a]\n", LimitKind::MaxNodes, 3, 1);
}

#[test]
fn copies_preserve_expanded_node_charges() {
    // Two scalar alias copies; construction and registration add no expanded nodes.
    boundary("[&a 1, *a, *a]\n", LimitKind::MaxExpandedNodes, 2, 1);
}
