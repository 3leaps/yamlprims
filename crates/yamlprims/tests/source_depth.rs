//! Source collection levels, yielded-error precedence and original-byte sites.
use yamlprims::*;

fn flow_sequence(n: usize) -> String {
    format!("{}0{}\n", "[".repeat(n), "]".repeat(n))
}
fn flow_mapping(n: usize) -> String {
    format!("{}0{}\n", "{x: ".repeat(n), "}".repeat(n))
}
fn block(n: usize, mapping: bool) -> String {
    let mut text = String::new();
    for i in 0..n {
        text += &"  ".repeat(i);
        text += match (mapping, i + 1 == n) {
            (true, true) => "x: 0\n",
            (true, false) => "x:\n",
            (false, true) => "- 0\n",
            (false, false) => "-\n",
        };
    }
    text
}
fn mixed(n: usize) -> String {
    format!("k:\n  {}", flow_sequence(n))
}
fn alias_insertion(n: usize) -> String {
    format!("{{a: &a [], b: {}*a{}}}", "[".repeat(n), "]".repeat(n))
}
fn unknown_aliases(n: usize) -> String {
    format!(
        "[{}]\n",
        (0..n)
            .map(|i| format!("*missing{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn check(
    input: &str,
    options: Options,
    stream: bool,
    documents: usize,
    kind: Option<ErrorKind>,
    site: Option<Location>,
    definition: Option<Span>,
) {
    let mut results = vec![
        load_all_with(input, &options),
        load_all_bytes(input.as_bytes(), &options),
    ];
    if !stream {
        results.push(load_with(input, &options).map(|d| vec![d]));
        results.push(load_bytes(input.as_bytes(), &options).map(|d| vec![d]));
        if kind.is_some() {
            results.push(from_str_with::<Value>(input, &options).map(|_| Vec::new()));
        } else {
            from_str_with::<Value>(input, &options).expect("typed value loading");
        }
    }
    for result in results {
        match &kind {
            None => {
                assert_eq!(result.unwrap().len(), documents);
            }
            Some(want) => {
                let error = result.unwrap_err();
                assert_eq!(&error.kind, want, "{input:?}");
                match site {
                    Some(at) => assert_eq!(error.site, Site::Source(at), "{input:?}"),
                    None => assert_eq!(error.site, Site::NotSource { last_known: None }),
                }
                if let Some(span) = definition {
                    assert_eq!(error.related.len(), 1);
                    assert_eq!(error.related[0].role, RelatedRole::AnchorDefinition);
                    assert_eq!(error.related[0].span, span);
                    assert_eq!(error.path.len(), options.limits.max_depth);
                    assert!(
                        matches!(&error.path[0], PathSegment::Key(k) if k.as_str() == Some("b"))
                    );
                    assert!(error.path[1..]
                        .iter()
                        .all(|p| matches!(p, PathSegment::Index(0))));
                } else {
                    assert!(error.path.is_empty(), "{input:?}: {:?}", error.path);
                    assert!(error.related.is_empty(), "{input:?}");
                }
                if !matches!(want, ErrorKind::InvalidOptions) {
                    encode_error(&error, &options).expect("bounded error encoding");
                }
            }
        }
    }
}

#[test]
fn flow_sequence_d1_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(&flow_sequence(1), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d1_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(&flow_mapping(1), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d1_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(&block(1, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d1_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(&block(1, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d1_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        &flow_sequence(2),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn flow_mapping_d1_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        &flow_mapping(2),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        }),
        None,
    );
}

#[test]
fn block_sequence_d1_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        &block(2, false),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 4,
            line: 2,
            column: 3,
        }),
        None,
    );
}

#[test]
fn block_mapping_d1_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        &block(2, true),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 5,
            line: 2,
            column: 3,
        }),
        None,
    );
}

#[test]
fn mixed_d1_n0() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(&mixed(0), options, false, 1, None, None, None);
}

#[test]
fn mixed_d1_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        &mixed(1),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 5,
            line: 2,
            column: 3,
        }),
        None,
    );
}

#[test]
fn flow_sequence_d2_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&flow_sequence(1), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d2_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&flow_mapping(1), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d2_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&block(1, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d2_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&block(1, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d2_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&flow_sequence(2), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d2_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&flow_mapping(2), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d2_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&block(2, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d2_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&block(2, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d2_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        &flow_sequence(3),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        }),
        None,
    );
}

#[test]
fn flow_mapping_d2_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        &flow_mapping(3),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 8,
            line: 1,
            column: 9,
        }),
        None,
    );
}

#[test]
fn block_sequence_d2_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        &block(3, false),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 10,
            line: 3,
            column: 5,
        }),
        None,
    );
}

#[test]
fn block_mapping_d2_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        &block(3, true),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 12,
            line: 3,
            column: 5,
        }),
        None,
    );
}

#[test]
fn mixed_d2_n0() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&mixed(0), options, false, 1, None, None, None);
}

#[test]
fn mixed_d2_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(&mixed(1), options, false, 1, None, None, None);
}

#[test]
fn mixed_d2_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        &mixed(2),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 6,
            line: 2,
            column: 4,
        }),
        None,
    );
}

#[test]
fn flow_sequence_d3_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&flow_sequence(2), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d3_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&flow_mapping(2), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d3_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&block(2, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d3_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&block(2, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d3_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&flow_sequence(3), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d3_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&flow_mapping(3), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d3_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&block(3, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d3_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&block(3, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d3_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(
        &flow_sequence(4),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 3,
        }),
        Some(Location {
            byte_offset: 3,
            line: 1,
            column: 4,
        }),
        None,
    );
}

#[test]
fn flow_mapping_d3_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(
        &flow_mapping(4),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 3,
        }),
        Some(Location {
            byte_offset: 12,
            line: 1,
            column: 13,
        }),
        None,
    );
}

#[test]
fn block_sequence_d3_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(
        &block(4, false),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 3,
        }),
        Some(Location {
            byte_offset: 18,
            line: 4,
            column: 7,
        }),
        None,
    );
}

#[test]
fn block_mapping_d3_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(
        &block(4, true),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 3,
        }),
        Some(Location {
            byte_offset: 21,
            line: 4,
            column: 7,
        }),
        None,
    );
}

#[test]
fn mixed_d3_n1() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&mixed(1), options, false, 1, None, None, None);
}

#[test]
fn mixed_d3_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(&mixed(2), options, false, 1, None, None, None);
}

#[test]
fn mixed_d3_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(
        &mixed(3),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 3,
        }),
        Some(Location {
            byte_offset: 7,
            line: 2,
            column: 5,
        }),
        None,
    );
}

#[test]
fn flow_sequence_d4_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&flow_sequence(3), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d4_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&flow_mapping(3), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d4_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&block(3, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d4_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&block(3, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d4_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&flow_sequence(4), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d4_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&flow_mapping(4), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d4_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&block(4, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d4_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&block(4, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d4_n5() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(
        &flow_sequence(5),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 4,
        }),
        Some(Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        }),
        None,
    );
}

#[test]
fn flow_mapping_d4_n5() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(
        &flow_mapping(5),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 4,
        }),
        Some(Location {
            byte_offset: 16,
            line: 1,
            column: 17,
        }),
        None,
    );
}

#[test]
fn block_sequence_d4_n5() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(
        &block(5, false),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 4,
        }),
        Some(Location {
            byte_offset: 28,
            line: 5,
            column: 9,
        }),
        None,
    );
}

#[test]
fn block_mapping_d4_n5() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(
        &block(5, true),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 4,
        }),
        Some(Location {
            byte_offset: 32,
            line: 5,
            column: 9,
        }),
        None,
    );
}

#[test]
fn mixed_d4_n2() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&mixed(2), options, false, 1, None, None, None);
}

#[test]
fn mixed_d4_n3() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(&mixed(3), options, false, 1, None, None, None);
}

#[test]
fn mixed_d4_n4() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    check(
        &mixed(4),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 4,
        }),
        Some(Location {
            byte_offset: 8,
            line: 2,
            column: 6,
        }),
        None,
    );
}

#[test]
fn flow_sequence_d16_n15() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&flow_sequence(15), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d16_n15() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&flow_mapping(15), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d16_n15() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&block(15, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d16_n15() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&block(15, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d16_n16() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&flow_sequence(16), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d16_n16() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&flow_mapping(16), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d16_n16() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&block(16, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d16_n16() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&block(16, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d16_n17() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(
        &flow_sequence(17),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 16,
        }),
        Some(Location {
            byte_offset: 16,
            line: 1,
            column: 17,
        }),
        None,
    );
}

#[test]
fn flow_mapping_d16_n17() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(
        &flow_mapping(17),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 16,
        }),
        Some(Location {
            byte_offset: 64,
            line: 1,
            column: 65,
        }),
        None,
    );
}

#[test]
fn block_sequence_d16_n17() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(
        &block(17, false),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 16,
        }),
        Some(Location {
            byte_offset: 304,
            line: 17,
            column: 33,
        }),
        None,
    );
}

#[test]
fn block_mapping_d16_n17() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(
        &block(17, true),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 16,
        }),
        Some(Location {
            byte_offset: 320,
            line: 17,
            column: 33,
        }),
        None,
    );
}

#[test]
fn mixed_d16_n14() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&mixed(14), options, false, 1, None, None, None);
}

#[test]
fn mixed_d16_n15() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(&mixed(15), options, false, 1, None, None, None);
}

#[test]
fn mixed_d16_n16() {
    let mut options = Options::default();
    options.limits.max_depth = 16;
    check(
        &mixed(16),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 16,
        }),
        Some(Location {
            byte_offset: 20,
            line: 2,
            column: 18,
        }),
        None,
    );
}

#[test]
fn flow_sequence_d128_n127() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&flow_sequence(127), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d128_n127() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&flow_mapping(127), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d128_n127() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&block(127, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d128_n127() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&block(127, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d128_n128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&flow_sequence(128), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d128_n128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&flow_mapping(128), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d128_n128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&block(128, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d128_n128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&block(128, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d128_n129() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &flow_sequence(129),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 128,
            line: 1,
            column: 129,
        }),
        None,
    );
}

#[test]
fn flow_mapping_d128_n129() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &flow_mapping(129),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 512,
            line: 1,
            column: 513,
        }),
        None,
    );
}

#[test]
fn block_sequence_d128_n129() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &block(129, false),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 16768,
            line: 129,
            column: 257,
        }),
        None,
    );
}

#[test]
fn block_mapping_d128_n129() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &block(129, true),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 16896,
            line: 129,
            column: 257,
        }),
        None,
    );
}

#[test]
fn mixed_d128_n126() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&mixed(126), options, false, 1, None, None, None);
}

#[test]
fn mixed_d128_n127() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&mixed(127), options, false, 1, None, None, None);
}

#[test]
fn mixed_d128_n128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &mixed(128),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 132,
            line: 2,
            column: 130,
        }),
        None,
    );
}

#[test]
fn flow_sequence_d255_n254() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&flow_sequence(254), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d255_n254() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&flow_mapping(254), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d255_n254() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&block(254, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d255_n254() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&block(254, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d255_n255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&flow_sequence(255), options, false, 1, None, None, None);
}

#[test]
fn flow_mapping_d255_n255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&flow_mapping(255), options, false, 1, None, None, None);
}

#[test]
fn block_sequence_d255_n255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&block(255, false), options, false, 1, None, None, None);
}

#[test]
fn block_mapping_d255_n255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&block(255, true), options, false, 1, None, None, None);
}

#[test]
fn flow_sequence_d255_n256() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &flow_sequence(256),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 255,
            line: 1,
            column: 256,
        }),
        None,
    );
}

#[test]
fn flow_mapping_d255_n256() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &flow_mapping(256),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 1020,
            line: 1,
            column: 1021,
        }),
        None,
    );
}

#[test]
fn block_sequence_d255_n256() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &block(256, false),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 65790,
            line: 256,
            column: 511,
        }),
        None,
    );
}

#[test]
fn block_mapping_d255_n256() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &block(256, true),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 66045,
            line: 256,
            column: 511,
        }),
        None,
    );
}

#[test]
fn mixed_d255_n253() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&mixed(253), options, false, 1, None, None, None);
}

#[test]
fn mixed_d255_n254() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&mixed(254), options, false, 1, None, None, None);
}

#[test]
fn mixed_d255_n255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &mixed(255),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 259,
            line: 2,
            column: 257,
        }),
        None,
    );
}

#[test]
fn lookahead_d1() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        &flow_sequence(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn mixed_lookahead_d1() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        &mixed(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 5,
            line: 2,
            column: 3,
        }),
        None,
    );
}

#[test]
fn lookahead_d2() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        &flow_sequence(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        }),
        None,
    );
}

#[test]
fn mixed_lookahead_d2() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        &mixed(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 6,
            line: 2,
            column: 4,
        }),
        None,
    );
}

#[test]
fn lookahead_d128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &flow_sequence(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 128,
            line: 1,
            column: 129,
        }),
        None,
    );
}

#[test]
fn mixed_lookahead_d128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &mixed(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 132,
            line: 2,
            column: 130,
        }),
        None,
    );
}

#[test]
fn lookahead_d255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &flow_sequence(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 255,
            line: 1,
            column: 256,
        }),
        None,
    );
}

#[test]
fn mixed_lookahead_d255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &mixed(2048),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 259,
            line: 2,
            column: 257,
        }),
        None,
    );
}

#[test]
fn empty_sequence_at() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check("[]", options, false, 1, None, None, None);
}

#[test]
fn empty_mapping_at() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check("{}", options, false, 1, None, None, None);
}

#[test]
fn nested_empty_sequence_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("[[]]", options, false, 1, None, None, None);
}

#[test]
fn nested_empty_sequence_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[[]]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn nested_empty_mapping_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("[{}]", options, false, 1, None, None, None);
}

#[test]
fn nested_empty_mapping_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[{}]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn collection_key_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("? []\n: 0\n", options, false, 1, None, None, None);
}

#[test]
fn collection_key_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "? []\n: 0\n",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        }),
        None,
    );
}

#[test]
fn flow_collection_key_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("{? []: 0}", options, false, 1, None, None, None);
}

#[test]
fn flow_collection_key_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "{? []: 0}",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 3,
            line: 1,
            column: 4,
        }),
        None,
    );
}

#[test]
fn implicit_mapping_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("[a: b]", options, false, 1, None, None, None);
}

#[test]
fn implicit_mapping_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[a: b]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn implicit_mapping_key_at() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check("? [x: y]\n: 0\n", options, false, 1, None, None, None);
}

#[test]
fn implicit_mapping_key_over() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "? [x: y]\n: 0\n",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 3,
            line: 1,
            column: 4,
        }),
        None,
    );
}

#[test]
fn indentless_sequence_at() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check("k:\n- []\n", options, false, 1, None, None, None);
}

#[test]
fn indentless_sequence_over() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "k:\n- []\n",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 5,
            line: 2,
            column: 3,
        }),
        None,
    );
}

#[test]
fn tagged_empty_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("[!t []]", options, false, 1, None, None, None);
}

#[test]
fn tagged_empty_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[!t []]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        }),
        None,
    );
}

#[test]
fn core_tagged_empty_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("[!!seq []]", options, false, 1, None, None, None);
}

#[test]
fn core_tagged_empty_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[!!seq []]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 7,
            line: 1,
            column: 8,
        }),
        None,
    );
}

#[test]
fn anchored_empty_at() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check("[&a []]", options, false, 1, None, None, None);
}

#[test]
fn anchored_empty_over() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[&a []]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        }),
        None,
    );
}

#[test]
fn tagged_root_at() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check("!t [0]", options, false, 1, None, None, None);
}

#[test]
fn core_tagged_root_at() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check("!!seq [0]", options, false, 1, None, None, None);
}

#[test]
fn shadowed_merge_at() {
    let mut options = Options::default();
    options.limits.max_depth = 6;
    options.merge_keys = MergeKeys::Apply;
    check(
        "{m: {x: 0, <<: {x: [[[1]]]}}}",
        options,
        false,
        1,
        None,
        None,
        None,
    );
}

#[test]
fn shadowed_merge_over() {
    let mut options = Options::default();
    options.limits.max_depth = 5;
    options.merge_keys = MergeKeys::Apply;
    check(
        "{m: {x: 0, <<: {x: [[[1]]]}}}",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 5,
        }),
        Some(Location {
            byte_offset: 21,
            line: 1,
            column: 22,
        }),
        None,
    );
}

#[test]
fn merge_source_list_at() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    options.merge_keys = MergeKeys::Apply;
    check(
        "{m: {x: 0, <<: [{x: 1}]}}",
        options,
        false,
        1,
        None,
        None,
        None,
    );
}

#[test]
fn merge_source_list_over() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    options.merge_keys = MergeKeys::Apply;
    check(
        "{m: {x: 0, <<: [{x: 1}]}}",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 3,
        }),
        Some(Location {
            byte_offset: 16,
            line: 1,
            column: 17,
        }),
        None,
    );
}

#[test]
fn retained_merge_value_at() {
    let mut options = Options::default();
    options.limits.max_depth = 4;
    options.merge_keys = MergeKeys::Apply;
    check(
        "{m: {x: 0, <<: {x: !t []}}}",
        options,
        false,
        1,
        None,
        None,
        None,
    );
}

#[test]
fn retained_merge_value_over() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    options.merge_keys = MergeKeys::Apply;
    check(
        "{m: {x: 0, <<: {x: !t []}}}",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 3,
        }),
        Some(Location {
            byte_offset: 22,
            line: 1,
            column: 23,
        }),
        None,
    );
}

#[test]
fn alias_materialized_d128_level127() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&alias_insertion(125), options, false, 1, None, None, None);
}

#[test]
fn alias_materialized_d128_level128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(&alias_insertion(126), options, false, 1, None, None, None);
}

#[test]
fn alias_materialized_d128_level129() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &alias_insertion(127),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 128,
        }),
        Some(Location {
            byte_offset: 141,
            line: 1,
            column: 142,
        }),
        Some(Span {
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
        }),
    );
}

#[test]
fn alias_materialized_d255_level254() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&alias_insertion(252), options, false, 1, None, None, None);
}

#[test]
fn alias_materialized_d255_level255() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(&alias_insertion(253), options, false, 1, None, None, None);
}

#[test]
fn alias_materialized_d255_level256() {
    let mut options = Options::default();
    options.limits.max_depth = 255;
    check(
        &alias_insertion(254),
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 255,
        }),
        Some(Location {
            byte_offset: 268,
            line: 1,
            column: 269,
        }),
        Some(Span {
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
        }),
    );
}

#[test]
fn coordinates_lf_plain() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "é: 0\nb: [[]]\n",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 10,
            line: 2,
            column: 5,
        }),
        None,
    );
}

#[test]
fn coordinates_lf_bom() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "\u{feff}é: 0\nb: [[]]\n",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 13,
            line: 2,
            column: 5,
        }),
        None,
    );
}

#[test]
fn coordinates_crlf_plain() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "é: 0\r\nb: [[]]\r\n",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 11,
            line: 2,
            column: 5,
        }),
        None,
    );
}

#[test]
fn coordinates_crlf_bom() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "\u{feff}é: 0\r\nb: [[]]\r\n",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 14,
            line: 2,
            column: 5,
        }),
        None,
    );
}

#[test]
fn coordinates_cr_plain() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "é: 0\rb: [[]]\r",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 10,
            line: 2,
            column: 5,
        }),
        None,
    );
}

#[test]
fn coordinates_cr_bom() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "\u{feff}é: 0\rb: [[]]\r",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 13,
            line: 2,
            column: 5,
        }),
        None,
    );
}

#[test]
fn scalar_root() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check("0", options, false, 1, None, None, None);
}

#[test]
fn tagged_scalar() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check("!t 0", options, false, 1, None, None, None);
}

#[test]
fn reset_documents() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check("---\n[]\n---\n{}\n", options, true, 2, None, None, None);
}

#[test]
fn second_document_depth() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "---\n[]\n---\n[[]]\n",
        options,
        true,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 12,
            line: 4,
            column: 2,
        }),
        None,
    );
}

#[test]
fn invalid_depth_zero() {
    let mut options = Options::default();
    options.limits.max_depth = 0;
    check(
        "[[0]]",
        options,
        false,
        1,
        Some(ErrorKind::InvalidOptions),
        None,
        None,
    );
}

#[test]
fn invalid_depth_256() {
    let mut options = Options::default();
    options.limits.max_depth = 256;
    check(
        "[[0]]",
        options,
        false,
        1,
        Some(ErrorKind::InvalidOptions),
        None,
        None,
    );
}

#[test]
fn duplicate_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        "a: 1\na: 2\nb: [\n",
        options,
        false,
        1,
        Some(ErrorKind::Syntax("unclosed_flow_collection".into())),
        Some(Location {
            byte_offset: 13,
            line: 3,
            column: 4,
        }),
        None,
    );
}

#[test]
fn unknown_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        "a: *missing\nb: [\n",
        options,
        false,
        1,
        Some(ErrorKind::UnknownAnchor),
        Some(Location {
            byte_offset: 3,
            line: 1,
            column: 4,
        }),
        None,
    );
}

#[test]
fn version_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        "%YAML 2.0\n---\n[\n",
        options,
        false,
        1,
        Some(ErrorKind::UnsupportedYamlVersion),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}

#[test]
fn materialized_depth_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 3;
    check(
        "a: &a []\nb: [[*a]]\nc: [\n",
        options,
        false,
        1,
        Some(ErrorKind::Syntax("unclosed_flow_collection".into())),
        Some(Location {
            byte_offset: 22,
            line: 3,
            column: 4,
        }),
        None,
    );
}

#[test]
fn key_budget_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    options.limits.max_key_bytes = 1;
    check(
        "? ab\n: [\n",
        options,
        false,
        1,
        Some(ErrorKind::Syntax("unclosed_flow_collection".into())),
        Some(Location {
            byte_offset: 7,
            line: 2,
            column: 3,
        }),
        None,
    );
}

#[test]
fn rejected_tag_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    options.unknown_tags = UnknownTags::Reject;
    check(
        "a: !t 1\nb: [\n",
        options,
        false,
        1,
        Some(ErrorKind::Syntax("unclosed_flow_collection".into())),
        Some(Location {
            byte_offset: 11,
            line: 2,
            column: 4,
        }),
        None,
    );
}

#[test]
fn recursive_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        "&a [*a]\n---\n[\n",
        options,
        true,
        1,
        Some(ErrorKind::Syntax("unclosed_flow_collection".into())),
        Some(Location {
            byte_offset: 12,
            line: 3,
            column: 1,
        }),
        None,
    );
}

#[test]
fn syntax_before_depth() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "]\n---\n[[0]]\n",
        options,
        true,
        1,
        Some(ErrorKind::Syntax("misplaced_flow_collection_end".into())),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}

#[test]
fn depth_before_late_syntax() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "---\n[[0]]\n---\n[\n",
        options,
        true,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 5,
            line: 2,
            column: 2,
        }),
        None,
    );
}

#[test]
fn depth_before_unknown() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[[0]]\n---\n*missing\n",
        options,
        true,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn unknown_before_depth() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "*missing\n---\n[[0]]\n",
        options,
        true,
        1,
        Some(ErrorKind::UnknownAnchor),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}

#[test]
fn lookahead_mismatch() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    check(
        "[[[}\n",
        options,
        false,
        1,
        Some(ErrorKind::Syntax("mismatched_flow_collection_end".into())),
        Some(Location {
            byte_offset: 2,
            line: 1,
            column: 3,
        }),
        None,
    );
}

#[test]
fn unclosed_native_opener() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        "[\n---\n0\n",
        options,
        false,
        1,
        Some(ErrorKind::Syntax("unclosed_flow_collection".into())),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}

#[test]
fn depth_before_construction() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    options.unknown_tags = UnknownTags::Reject;
    check(
        "!t [[0]]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 4,
            line: 1,
            column: 5,
        }),
        None,
    );
}

#[test]
fn depth_before_nodes() {
    let mut options = Options::default();
    options.limits.max_depth = 1;
    options.limits.max_nodes = 1;
    check(
        "[[]]",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 1,
        }),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn depth_before_merge_policy() {
    let mut options = Options::default();
    options.limits.max_depth = 2;
    check(
        "{<<: [[]]}",
        options,
        false,
        1,
        Some(ErrorKind::LimitExceeded {
            limit: LimitKind::MaxDepth,
            configured: 2,
        }),
        Some(Location {
            byte_offset: 6,
            line: 1,
            column: 7,
        }),
        None,
    );
}

#[test]
fn many_unknown_1() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &unknown_aliases(1),
        options,
        false,
        1,
        Some(ErrorKind::UnknownAnchor),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn many_unknown_2() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &unknown_aliases(2),
        options,
        false,
        1,
        Some(ErrorKind::UnknownAnchor),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn many_unknown_16() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &unknown_aliases(16),
        options,
        false,
        1,
        Some(ErrorKind::UnknownAnchor),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn many_unknown_128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &unknown_aliases(128),
        options,
        false,
        1,
        Some(ErrorKind::UnknownAnchor),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn many_unknown_1024() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &unknown_aliases(1024),
        options,
        false,
        1,
        Some(ErrorKind::UnknownAnchor),
        Some(Location {
            byte_offset: 1,
            line: 1,
            column: 2,
        }),
        None,
    );
}

#[test]
fn many_version_1() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &"%YAML 2.0\n---\n0\n...\n".repeat(1),
        options,
        true,
        1,
        Some(ErrorKind::UnsupportedYamlVersion),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}

#[test]
fn many_version_2() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &"%YAML 2.0\n---\n0\n...\n".repeat(2),
        options,
        true,
        1,
        Some(ErrorKind::UnsupportedYamlVersion),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}

#[test]
fn many_version_16() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &"%YAML 2.0\n---\n0\n...\n".repeat(16),
        options,
        true,
        1,
        Some(ErrorKind::UnsupportedYamlVersion),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}

#[test]
fn many_version_128() {
    let mut options = Options::default();
    options.limits.max_depth = 128;
    check(
        &"%YAML 2.0\n---\n0\n...\n".repeat(128),
        options,
        true,
        1,
        Some(ErrorKind::UnsupportedYamlVersion),
        Some(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        }),
        None,
    );
}
