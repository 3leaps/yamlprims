//! Final merged-subtree metrics and cumulative work are independent (§§5.3–6).

use yamlprims::{load_all_with, load_with, ErrorKind, LimitKind, MergeKeys, Options};

fn options() -> Options {
    let mut options = Options::default();
    options.merge_keys = MergeKeys::Apply;
    options.limits.max_scalar_bytes = 8;
    options.limits.max_key_bytes = 256;
    options
}

fn boundary(input: &str, limit: LimitKind, want: usize, documents: usize) {
    // All configured limits must be positive: zero expansion uses a success
    // control at one, and a threshold of one has no admissible below-limit row.
    for configured in want.saturating_sub(1)..=want + 1 {
        if configured == 0 {
            continue;
        }
        let mut options = options();
        match limit {
            LimitKind::MaxDepth => options.limits.max_depth = configured,
            LimitKind::MaxNodes => options.limits.max_nodes = configured,
            LimitKind::MaxExpandedNodes => options.limits.max_expanded_nodes = configured,
            LimitKind::MaxOwnedBytes => {
                options.limits.max_owned_bytes = configured;
                options.limits.max_key_bytes = configured.min(256);
            }
            LimitKind::MaxKeyBytes => options.limits.max_key_bytes = configured,
            _ => panic!("unsupported boundary {limit:?}"),
        }
        options.validate().expect("valid noncompeting options");
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
                    configured: configured as u64
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

macro_rules! merge_boundaries {
    ($( $name:ident: $input:expr => [$depth:expr, $nodes:expr, $expanded:expr, $owned:expr, $documents:expr], )*) => {
        $(mod $name {
            use super::*;
            #[test]
            fn depth() { boundary($input, LimitKind::MaxDepth, $depth, $documents); }
            #[test]
            fn nodes() { boundary($input, LimitKind::MaxNodes, $nodes, $documents); }
            #[test]
            fn expanded() { boundary($input, LimitKind::MaxExpandedNodes, $expanded, $documents); }
            #[test]
            fn owned() { boundary($input, LimitKind::MaxOwnedBytes, $owned, $documents); }
        })*
    };
}

// Fixed totals include construction, registration and logical copies, without
// refunding removed directives or shadowed source nodes. Later copies and cache
// registration use only the final mapping's nodes/weight/height.
merge_boundaries! {
    basic_1: "a: &a {k: 1}\nb: &b {<<: *a}\nc: [*b]\n" => [3,18,8,1787,1],
    basic_2: "a: &a {k: 1}\nb: &b {<<: *a}\nc: [*b, *b]\n" => [3,21,11,2004,1],
    basic_3: "a: &a {k: 1}\nb: &b {<<: *a}\nc: [*b, *b, *b]\n" => [3,24,14,2221,1],
    fully_shadowed_1: "a: &a {k: 1, m: 2}\nb: &b {k: 0, m: 0, <<: *a}\nc: [*b]\n" => [3,26,10,2705,1],
    fully_shadowed_3: "a: &a {k: 1, m: 2}\nb: &b {k: 0, m: 0, <<: *a}\nc: [*b, *b, *b]\n" => [3,36,20,3445,1],
    partly_shadowed_1: "a: &a {k: 1, m: 2}\nb: &b {k: 0, <<: *a}\nc: [*b]\n" => [3,26,12,2705,1],
    partly_shadowed_3: "a: &a {k: 1, m: 2}\nb: &b {k: 0, <<: *a}\nc: [*b, *b, *b]\n" => [3,36,22,3445,1],
    all_merged_1: "a: &a {k: 1, m: 2}\nb: &b {<<: *a}\nc: [*b]\n" => [3,26,14,2705,1],
    all_merged_3: "a: &a {k: 1, m: 2}\nb: &b {<<: *a}\nc: [*b, *b, *b]\n" => [3,36,24,3445,1],
    empty_1: "a: &a {}\nb: &b {<<: *a}\nc: [*b]\n" => [3,10,2,869,1],
    empty_3: "a: &a {}\nb: &b {<<: *a}\nc: [*b, *b, *b]\n" => [3,12,4,997,1],
    tagged_empty_1: "a: &a {}\nb: &b !t {<<: *a}\nc: [*b]\n" => [3,12,3,1073,1],
    tagged_empty_3: "a: &a {}\nb: &b !t {<<: *a}\nc: [*b, *b, *b]\n" => [3,16,7,1337,1],
    registration_without_later_copy: "&b {<<: {}}\n" => [2,3,0,282,1],
    inline_empty_source: "[&b {<<: {}}, *b]\n" => [3,5,1,410,1],
    shadowed_deep_source_1: "b: &b {k: 0, <<: {k: [[[1]]]}}\nc: [*b]\n" => [6,17,3,1456,1],
    shadowed_deep_source_5: "b: &b {k: 0, <<: {k: [[[1]]]}}\nc: [[[[[*b]]]]]\n" => [7,21,3,1712,1],
    source_list_earlier_wins: "a: &a {k: 1}\nb: &b {<<: [*a, {k: 2, m: 3}]}\nc: [*b]\n" => [4,28,12,2680,1],
    source_list_with_explicit: "a: &a {k: 1}\nb: &b {k: 0, <<: [*a, {k: 2, m: 3}]}\nc: [*b]\n" => [4,28,10,2680,1],
    merged_collection_key: "s: &s {? [1]: 2}\nt: &t {<<: *s}\nu: [[*t]]\n" => [5,23,11,2229,1],
    merged_tagged_value: "s: &s {k: !t [1]}\nt: &t {<<: *s}\nu: [[*t]]\n" => [5,27,14,2643,1],
    tagged_merged_mapping: "a: &a {k: 1}\nb: &b !t {<<: *a}\nc: [*b]\n" => [3,20,9,1991,1],
    successive_merged_anchors: "a: &a {k: 1}\nb: &b {<<: *a}\nd: &d {<<: *b}\ne: [*d]\n" => [3,26,13,2617,1],
    redefined_anchor_keeps_older_merged_target: "a: &x {k: 1}\nb: &y {<<: *x}\nc: &x {m: [2]}\nd: [*y, *x]\n" => [4,27,12,2719,1],
    nested_anchor_in_merged_source: "[&x {<<: {k: &y [1]}}, *x, *y]\n" => [4,16,9,1534,1],
    nested_anchor_in_shadowed_source: "[&x {k: 0, <<: {k: &y [1]}}, *x, *y]\n" => [4,14,5,1342,1],
    stream_registration_and_work_do_not_reset: "---\na: &a {k: 1}\nb: &b {<<: *a}\nc: [*b]\n---\na: &a {k: 1}\nb: &b {<<: *a}\nc: [*b]\n" => [3,36,16,3574,2],
    completed_merged_key: "? {<<: {k: 1}}\n: 0\n" => [3,9,2,676,1],
    merged_empty_value: "a: &a {k: []}\nb: &b {<<: *a}\nc: [[*b]]\n" => [5,19,8,1851,1],
    merged_tagged_empty_value: "a: &a {k: !t []}\nb: &b {<<: *a}\nc: [[*b]]\n" => [5,23,11,2259,1],
}

#[test]
fn completed_merged_key_uses_its_final_weight() {
    boundary("? {<<: {k: 1}}\n: 0\n", LimitKind::MaxKeyBytes, 217, 1);
}

#[test]
fn merged_alias_at_default_and_maximum_depth_boundaries() {
    for depth in [128, 255] {
        let mut options = options();
        options.limits.max_depth = depth;
        for sequences in [depth - 3, depth - 2, depth - 1] {
            let input = format!(
                "a: &a {{k: 1}}\nb: &b {{<<: *a}}\nc: {}*b{}\n",
                "[".repeat(sequences),
                "]".repeat(sequences),
            );
            let result = load_with(&input, &options);
            if sequences + 2 <= depth {
                result.expect("final target H=1 fits the alias position");
            } else {
                assert_eq!(
                    result.unwrap_err().kind,
                    ErrorKind::LimitExceeded {
                        limit: LimitKind::MaxDepth,
                        configured: depth as u64,
                    }
                );
            }
        }
    }
}

#[test]
fn deep_merged_values_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            for depth in [128, 255] {
                for tagged in [false, true] {
                    for sequences in [depth - 4, depth - 3, depth - 2] {
                        let open = if tagged { "!t [" } else { "[" };
                        let input = format!(
                            "s: &s {{k: {}0{}}}\nt: &t {{<<: *s}}\nu: *t\n",
                            open.repeat(sequences),
                            "]".repeat(sequences),
                        );
                        let mut options = options();
                        options.limits.max_depth = depth;
                        let result = load_with(&input, &options);
                        // H(s)=sequences+1; the merge-source alias is at p=2.
                        if sequences + 3 <= depth {
                            result.expect("deep merged value fits");
                        } else {
                            assert_eq!(
                                result.unwrap_err().kind,
                                ErrorKind::LimitExceeded {
                                    limit: LimitKind::MaxDepth,
                                    configured: depth as u64,
                                }
                            );
                        }
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn source_precedence_and_explicit_order_are_preserved() {
    let input = "a: &a {k: 1}\nb: &b {k: 0, <<: [*a, {k: 2, m: 3}]}\nc: [*b]\n";
    let document = load_with(input, &options()).unwrap();
    let root = document.root().value().as_mapping().unwrap();
    let b = root.get("b").unwrap().value().as_mapping().unwrap();
    assert_eq!(
        b.iter()
            .map(|(k, _)| k.value().as_str().unwrap())
            .collect::<Vec<_>>(),
        ["k", "m"]
    );
    assert_eq!(
        b.get("k").unwrap().value().as_integer().unwrap().to_i64(),
        Some(0)
    );
    assert_eq!(
        b.get("m").unwrap().value().as_integer().unwrap().to_i64(),
        Some(3)
    );
    let c = root.get("c").unwrap().value().as_sequence().unwrap();
    let copied = c[0].value().as_mapping().unwrap();
    assert_eq!(copied.len(), 2);
    assert_eq!(
        copied
            .get("k")
            .unwrap()
            .value()
            .as_integer()
            .unwrap()
            .to_i64(),
        Some(0)
    );
    assert_eq!(
        copied
            .get("m")
            .unwrap()
            .value()
            .as_integer()
            .unwrap()
            .to_i64(),
        Some(3)
    );
}

#[test]
fn redefinitions_leave_the_older_merged_target_intact() {
    let input = "a: &x {k: 1}\nb: &y {<<: *x}\nc: &x {m: [2]}\nd: [*y, *x]\n";
    let document = load_with(input, &options()).unwrap();
    let root = document.root().value().as_mapping().unwrap();
    let d = root.get("d").unwrap().value().as_sequence().unwrap();
    let older = d[0].value().as_mapping().unwrap();
    assert_eq!(older.len(), 1);
    assert_eq!(
        older
            .get("k")
            .unwrap()
            .value()
            .as_integer()
            .unwrap()
            .to_i64(),
        Some(1)
    );
    let newer = d[1].value().as_mapping().unwrap();
    assert_eq!(newer.len(), 1);
    let sequence = newer.get("m").unwrap().value().as_sequence().unwrap();
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].value().as_integer().unwrap().to_i64(), Some(2));
}
