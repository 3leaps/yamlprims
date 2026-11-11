//! Strict RFC 7493 I-JSON projection (spec-v1.5 §11.6, ADR-0005).

use crate::budget::NODE_OVERHEAD;
use crate::build::Loader;
use crate::error::{Error, ErrorKind, LimitKind, RelatedRole, Site};
use crate::load::{gate_bytes, gate_str};
use crate::options::{MergeKeys, Options, UnknownTags};
use crate::value::{Node, NodeSpan, Value, Via};
use crate::wire::Out;
use std::fmt::Write as _;

const MAX_SAFE_INT: i128 = 9_007_199_254_740_991;
const MIN_SAFE_INT: i128 = -9_007_199_254_740_991;

/// Returns `true` when `c` is one of the 66 Unicode noncharacters prohibited by RFC 7493 §2.1:
/// `U+FDD0..=U+FDEF` (32 code points) and `U+..FFFE` / `U+..FFFF` across planes `0..=16` (34 code points).
pub(crate) fn is_unicode_noncharacter(c: char) -> bool {
    let cp = c as u32;
    (0xFDD0..=0xFDEF).contains(&cp) || ((cp & 0xFFFE) == 0xFFFE && cp <= 0x10FFFF)
}

#[derive(Clone, PartialEq, Eq)]
struct BigUint(Vec<u64>);

impl BigUint {
    fn from_u64(v: u64) -> Self {
        if v == 0 {
            Self(Vec::new())
        } else {
            Self(vec![v])
        }
    }

    fn mul_u64(&mut self, m: u64) {
        if m == 0 || self.0.is_empty() {
            self.0.clear();
            return;
        }
        let mut carry: u128 = 0;
        for w in &mut self.0 {
            let prod = (*w as u128) * (m as u128) + carry;
            *w = prod as u64;
            carry = prod >> 64;
        }
        if carry != 0 {
            self.0.push(carry as u64);
        }
    }

    fn pow5(mut exp: u32) -> Self {
        let mut res = Self::from_u64(1);
        while exp >= 27 {
            res.mul_u64(5u64.pow(27));
            exp -= 27;
        }
        if exp > 0 {
            res.mul_u64(5u64.pow(exp));
        }
        res
    }

    fn shl(&mut self, bits: u32) {
        if self.0.is_empty() || bits == 0 {
            return;
        }
        let word_shift = (bits / 64) as usize;
        let bit_shift = bits % 64;
        if bit_shift == 0 {
            self.0.splice(0..0, std::iter::repeat_n(0, word_shift));
            return;
        }
        let mut carry = 0u64;
        for w in &mut self.0 {
            let next = *w >> (64 - bit_shift);
            *w = (*w << bit_shift) | carry;
            carry = next;
        }
        if carry != 0 {
            self.0.push(carry);
        }
        if word_shift > 0 {
            self.0.splice(0..0, std::iter::repeat_n(0, word_shift));
        }
    }

    fn cmp_val(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match self.0.len().cmp(&other.0.len()) {
            Ordering::Equal => {
                for (a, b) in self.0.iter().rev().zip(other.0.iter().rev()) {
                    match a.cmp(b) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                }
                Ordering::Equal
            }
            ord => ord,
        }
    }
}

/// Compare `abs = m * 2^e2` against the exact decimal midpoint `(2 * d_low + 1) * 10^p / 2`.
fn cmp_abs_to_midpoint(m: u64, e2: i32, d_low: u64, p: i32) -> std::cmp::Ordering {
    let mid_odd = 2 * d_low + 1;
    let (mut lhs, mut rhs) = if p >= 0 {
        let lhs = BigUint::from_u64(m);
        let mut rhs = BigUint::pow5(p as u32);
        rhs.mul_u64(mid_odd);
        (lhs, rhs)
    } else {
        let mut lhs = BigUint::pow5((-p) as u32);
        lhs.mul_u64(m);
        let rhs = BigUint::from_u64(mid_odd);
        (lhs, rhs)
    };
    let shift = e2 - (p - 1);
    if shift >= 0 {
        lhs.shl(shift as u32);
    } else {
        rhs.shl((-shift) as u32);
    }
    lhs.cmp_val(&rhs)
}

fn decimal_round_trips_to(d: u64, p: i32, target: f64) -> bool {
    if d == 0 {
        return false;
    }
    format!("{d}e{p}").parse::<f64>() == Ok(target)
}

/// Decompose a finite positive `f64` into its IEEE 754 shortest round-trip decimal
/// significand digits (`1..=17` ASCII digits without trailing zeros, breaking exact
/// halfway ties to even per ECMAScript `Number::toString` / Ryū) and decimal exponent `k`
/// such that `abs = 0.<digits> * 10^k` (`sci_exp = k - 1`).
pub(crate) fn decompose_shortest_float(abs: f64) -> (String, i32) {
    debug_assert!(abs > 0.0 && abs.is_finite());
    let sci = format!("{abs:e}");
    let (mant, exp_str) = sci.split_once('e').expect("scientific notation has 'e'");
    let sci_exp: i32 = exp_str.parse().expect("valid exponent");
    let mut digits = String::with_capacity(mant.len());
    for b in mant.bytes() {
        if b != b'.' {
            digits.push(b as char);
        }
    }
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    let n = digits.len() as i32;
    let k = sci_exp + 1;
    let p = k - n;
    let Ok(mut d) = digits.parse::<u64>() else {
        return (digits, k);
    };

    let bits = abs.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let raw_mant = bits & ((1u64 << 52) - 1);
    let (m, e2) = if raw_exp == 0 {
        (raw_mant, -1074)
    } else {
        (raw_mant | (1u64 << 52), raw_exp - 1023 - 52)
    };

    if d > 1 && decimal_round_trips_to(d - 1, p, abs) {
        match cmp_abs_to_midpoint(m, e2, d - 1, p) {
            std::cmp::Ordering::Less => d -= 1,
            std::cmp::Ordering::Equal if (d - 1).is_multiple_of(2) => d -= 1,
            _ => {}
        }
    } else if decimal_round_trips_to(d + 1, p, abs) {
        match cmp_abs_to_midpoint(m, e2, d, p) {
            std::cmp::Ordering::Greater => d += 1,
            std::cmp::Ordering::Equal if (d + 1).is_multiple_of(2) => d += 1,
            _ => {}
        }
    }

    let mut out_digits = d.to_string();
    let new_k = p + (out_digits.len() as i32);
    while out_digits.len() > 1 && out_digits.ends_with('0') {
        out_digits.pop();
    }
    (out_digits, new_k)
}

/// Format a finite `f64` per RFC 8785 §3.2.2.3 (ECMAScript `Number::toString`).
/// `-0.0` formats as `"0"`.
pub(crate) fn format_ijson_float(f: f64) -> String {
    debug_assert!(f.is_finite());
    if f == 0.0 {
        return "0".to_string();
    }
    let mut out = String::with_capacity(24);
    if f.is_sign_negative() {
        out.push('-');
    }
    let (digits, k) = decompose_shortest_float(f.abs());
    let n = digits.len() as i32;
    if k > 0 && k <= 21 {
        if n <= k {
            out.push_str(&digits);
            for _ in 0..(k - n) {
                out.push('0');
            }
        } else {
            let ku = k as usize;
            out.push_str(&digits[..ku]);
            out.push('.');
            out.push_str(&digits[ku..]);
        }
    } else if (-5..=0).contains(&k) {
        out.push_str("0.");
        for _ in 0..(-k) {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let sci_exp = k - 1;
        if sci_exp >= 0 {
            let _ = write!(out, "e+{sci_exp}");
        } else {
            let _ = write!(out, "e{sci_exp}");
        }
    }
    out
}

fn node_site(n: &Node) -> Site {
    match n.span {
        NodeSpan::Source(s) => Site::Source(s.start),
        NodeSpan::Synthetic => Site::NotSource { last_known: None },
    }
}

fn with_node_origin(n: &Node, e: Error) -> Error {
    match n.via() {
        Some(Via::Alias { use_site }) => e.with_related(RelatedRole::AliasUse, *use_site),
        Some(Via::Merge { directive }) => e.with_related(RelatedRole::MergeDirective, *directive),
        None => e,
    }
}

fn node_err(n: &Node, kind: ErrorKind, msg: impl std::fmt::Display) -> Error {
    with_node_origin(n, Error::new(kind, node_site(n), msg))
}

enum Frame<'a> {
    Node {
        node: &'a Node,
        depth: usize,
        is_key: bool,
    },
    Literal(&'static str),
    Seq {
        items: &'a [Node],
        idx: usize,
        depth: usize,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
        depth: usize,
    },
}

/// Project `node` to strict RFC 7493 I-JSON text (spec-v1.5 §11.6).
pub fn encode_ijson(node: &Node, opts: &Options) -> Result<String, Error> {
    opts.validate()?;
    let mut out = Out::new(opts.limits.max_output_bytes);
    let mut nodes_seen: usize = 0;
    let mut stack: Vec<Frame<'_>> = vec![Frame::Node {
        node,
        depth: 0,
        is_key: false,
    }];

    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Literal(lit) => out.push(lit)?,
            Frame::Seq { items, idx, depth } => {
                if idx >= items.len() {
                    out.push("]")?;
                } else {
                    if idx > 0 {
                        out.push(",")?;
                    }
                    stack.push(Frame::Seq {
                        items,
                        idx: idx + 1,
                        depth,
                    });
                    stack.push(Frame::Node {
                        node: &items[idx],
                        depth,
                        is_key: false,
                    });
                }
            }
            Frame::Map {
                entries,
                idx,
                depth,
            } => {
                if idx >= entries.len() {
                    out.push("}")?;
                } else {
                    if idx > 0 {
                        out.push(",")?;
                    }
                    let (k, v) = &entries[idx];
                    stack.push(Frame::Map {
                        entries,
                        idx: idx + 1,
                        depth,
                    });
                    stack.push(Frame::Node {
                        node: v,
                        depth,
                        is_key: false,
                    });
                    stack.push(Frame::Literal(":"));
                    stack.push(Frame::Node {
                        node: k,
                        depth,
                        is_key: true,
                    });
                }
            }
            Frame::Node {
                node: n,
                depth,
                is_key,
            } => {
                nodes_seen += 1;
                if nodes_seen > opts.limits.max_nodes {
                    return Err(node_err(
                        n,
                        ErrorKind::LimitExceeded {
                            limit: LimitKind::MaxNodes,
                            configured: opts.limits.max_nodes as u64,
                        },
                        "I-JSON node count exceeds max_nodes",
                    ));
                }
                if matches!(&n.value, Value::Tagged(_)) {
                    let site = match n.tag_start() {
                        Some(loc) => Site::Source(loc),
                        None => node_site(n),
                    };
                    return Err(with_node_origin(
                        n,
                        Error::new(
                            ErrorKind::UnknownTag,
                            site,
                            "tagged values are rejected in RFC 7493 I-JSON projection",
                        ),
                    ));
                }
                if is_key {
                    let Value::String(s) = &n.value else {
                        return Err(node_err(
                            n,
                            ErrorKind::Type {
                                expected: "I-JSON string key".into(),
                                found: "non_string_key".into(),
                            },
                            "mapping key must be a String in RFC 7493 I-JSON projection",
                        ));
                    };
                    let key_weight = NODE_OVERHEAD.saturating_add(s.len() as u64);
                    if key_weight > opts.limits.max_key_bytes as u64 {
                        return Err(node_err(
                            n,
                            ErrorKind::LimitExceeded {
                                limit: LimitKind::MaxKeyBytes,
                                configured: opts.limits.max_key_bytes as u64,
                            },
                            "mapping key exceeds max_key_bytes",
                        ));
                    }
                }
                match &n.value {
                    Value::Null => out.push("null")?,
                    Value::Bool(b) => out.push(if *b { "true" } else { "false" })?,
                    Value::Int(i) => {
                        let Some(iv) = i.to_i128() else {
                            return Err(node_err(
                                n,
                                ErrorKind::IntegerOutOfRange,
                                "integer exceeds RFC 7493 I-JSON safe range ±(2^53 - 1)",
                            ));
                        };
                        if !(MIN_SAFE_INT..=MAX_SAFE_INT).contains(&iv) {
                            return Err(node_err(
                                n,
                                ErrorKind::IntegerOutOfRange,
                                "integer exceeds RFC 7493 I-JSON safe range ±(2^53 - 1)",
                            ));
                        }
                        out.push(&i.to_string())?;
                    }
                    Value::Float(f) => {
                        if !f.is_finite() {
                            return Err(node_err(
                                n,
                                ErrorKind::FloatOutOfRange,
                                "non-finite float cannot be represented in RFC 7493 I-JSON",
                            ));
                        }
                        out.push(&format_ijson_float(*f))?;
                    }
                    Value::String(s) => {
                        if s.len() > opts.limits.max_scalar_bytes {
                            return Err(node_err(
                                n,
                                ErrorKind::LimitExceeded {
                                    limit: LimitKind::MaxScalarBytes,
                                    configured: opts.limits.max_scalar_bytes as u64,
                                },
                                "string exceeds max_scalar_bytes",
                            ));
                        }
                        if s.chars().any(is_unicode_noncharacter) {
                            return Err(node_err(
                                n,
                                ErrorKind::Type {
                                    expected: "RFC 7493 I-JSON string".into(),
                                    found: "noncharacter".into(),
                                },
                                "string contains a Unicode noncharacter prohibited by RFC 7493",
                            ));
                        }
                        out.string(s)?;
                    }
                    Value::Sequence(items) => {
                        if depth + 1 > opts.limits.max_depth {
                            return Err(node_err(
                                n,
                                ErrorKind::LimitExceeded {
                                    limit: LimitKind::MaxDepth,
                                    configured: opts.limits.max_depth as u64,
                                },
                                "sequence nesting exceeds max_depth",
                            ));
                        }
                        out.push("[")?;
                        stack.push(Frame::Seq {
                            items,
                            idx: 0,
                            depth: depth + 1,
                        });
                    }
                    Value::Mapping(m) => {
                        if depth + 1 > opts.limits.max_depth {
                            return Err(node_err(
                                n,
                                ErrorKind::LimitExceeded {
                                    limit: LimitKind::MaxDepth,
                                    configured: opts.limits.max_depth as u64,
                                },
                                "mapping nesting exceeds max_depth",
                            ));
                        }
                        if m.len() > opts.limits.max_mapping_entries {
                            return Err(node_err(
                                n,
                                ErrorKind::LimitExceeded {
                                    limit: LimitKind::MaxMappingEntries,
                                    configured: opts.limits.max_mapping_entries as u64,
                                },
                                "mapping exceeds max_mapping_entries",
                            ));
                        }
                        out.push("{")?;
                        stack.push(Frame::Map {
                            entries: &m.entries,
                            idx: 0,
                            depth: depth + 1,
                        });
                    }
                    Value::Tagged(_) => unreachable!("checked above"),
                }
            }
        }
    }
    Ok(out.into_string())
}

fn prepare_ijson_load_options(opts: &Options) -> Result<Options, Error> {
    opts.validate()?;
    if opts.merge_keys == MergeKeys::AsKey {
        return Err(Error::not_source(
            ErrorKind::InvalidOptions,
            None,
            "MergeKeys::AsKey is not permitted in RFC 7493 I-JSON projection",
        ));
    }
    let mut eff = opts.clone();
    eff.unknown_tags = UnknownTags::Reject;
    Ok(eff)
}

/// Load a single non-empty YAML document with default options (16 MiB output cap)
/// and project it to strict RFC 7493 I-JSON text (spec-v1.5 §11.6).
pub fn load_ijson(input: &str) -> Result<String, Error> {
    load_ijson_with(input, &Options::ijson_default())
}

/// Load a single non-empty YAML document with `opts` (`UnknownTags::Reject` enforced)
/// and project it to strict RFC 7493 I-JSON text (spec-v1.5 §11.6).
pub fn load_ijson_with(input: &str, opts: &Options) -> Result<String, Error> {
    let eff = prepare_ijson_load_options(opts)?;
    let (s, bom) = gate_str(input, &eff)?;
    let doc = Loader::new(s, bom, &eff).load_single_non_empty()?;
    encode_ijson(&doc.root, &eff)
}

/// Load a single non-empty YAML document from raw UTF-8 bytes with `opts`
/// (`UnknownTags::Reject` enforced) and project it to strict RFC 7493 I-JSON text.
pub fn load_ijson_bytes(input: &[u8], opts: &Options) -> Result<String, Error> {
    let eff = prepare_ijson_load_options(opts)?;
    let (s, bom) = gate_bytes(input, &eff)?;
    let doc = Loader::new(s, bom, &eff).load_single_non_empty()?;
    encode_ijson(&doc.root, &eff)
}
