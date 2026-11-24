//! Plain JSON float text (spec revision 1.7 §11.3, `1.7/E-5`).
//!
//! Expected strings are derived from the rule, not from an implementation: the
//! fewest significant digits that read back as the same binary64 value, the
//! nearest such decimal, and an even final digit when two are equally near.

use yamlprims::{Node, Options, Value};

fn plain(f: f64) -> String {
    let n = Node::synthetic(Value::Float(f));
    yamlprims::encode_plain(&n, &Options::default()).unwrap()
}

#[test]
fn exact_ties_choose_the_even_final_digit() {
    // x = 10^15 + 1/4 (bits 430c6bf526340002): the adjacent binary64 values
    // are x - 1/8 and x + 1/8, so the rounding midpoints are x +/- 1/16. Both
    // 17-digit candidates .2 and .3 lie inside them and each is 1/20 from x;
    // no 16-digit (integer) candidate does.
    let x: f64 = 1e15 + 0.25;
    assert_eq!(x.to_bits(), 0x430c_6bf5_2634_0002);
    assert_eq!(plain(x), "1000000000000000.2");
    assert_eq!(plain(-x), "-1000000000000000.2");
    // 10^15 + 3/4: candidates .7 and .8, equally near; .8 is even.
    let y: f64 = 1e15 + 0.75;
    assert_eq!(plain(y), "1000000000000000.8");
    assert_eq!(plain(-y), "-1000000000000000.8");
}

#[test]
fn layout_boundaries_are_unchanged() {
    let cases: &[(f64, &str)] = &[
        (0.0, "0.0"),
        (-0.0, "-0.0"),
        (0.1, "0.1"),
        (123.456, "123.456"),
        (1e15, "1000000000000000.0"),
        (9_999_999_999_999_998.0, "9999999999999998.0"),
        (1e16, "1e16"),
        (1.5e16, "1.5e16"),
        (0.0001, "0.0001"),
        (0.00012, "0.00012"),
        (0.00001, "1e-5"),
        (1.5e-5, "1.5e-5"),
        (-2.5e-7, "-2.5e-7"),
        (5e-324, "5e-324"),
        (2.2250738585072014e-308, "2.2250738585072014e-308"),
        (1.7976931348623157e308, "1.7976931348623157e308"),
    ];
    for &(f, want) in cases {
        assert_eq!(plain(f), want, "bits {:016x}", f.to_bits());
    }
}

#[test]
fn every_output_reads_back_to_the_same_bits() {
    // Deterministic sweep over sign, exponent and mantissa patterns,
    // including subnormals.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    for _ in 0..200_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let f = f64::from_bits(state);
        if !f.is_finite() {
            continue;
        }
        let s = plain(f);
        let back: f64 = s.parse().unwrap();
        assert_eq!(back.to_bits(), f.to_bits(), "{s}");
        // Representation-size regression check only (not a proof of minimum
        // significant digits, nearest choice or even tie break): never longer
        // than Rust's own Debug form.
        assert!(s.len() <= format!("{f:?}").len(), "{s}");
    }
}
