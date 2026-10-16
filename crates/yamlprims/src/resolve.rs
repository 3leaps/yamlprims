//! Scalar and tag resolution (spec §4).

use crate::error::ErrorKind;
use crate::value::{Integer, Tag, Value};
use granit_parser::{ScalarStyle, Tag as GTag};

const YAML_NS: &str = "tag:yaml.org,2002:";

pub(crate) enum TagClass {
    None,
    NonSpecific,
    Core(&'static str),
    Merge,
    Other(Tag),
}

pub(crate) fn classify(tag: Option<&GTag>) -> TagClass {
    let Some(t) = tag else { return TagClass::None };
    if t.handle().is_empty() && t.suffix() == "!" {
        return TagClass::NonSpecific;
    }
    if let Some(core) = t.core_suffix() {
        let c: &'static str = match core {
            "bool" => "bool",
            "float" => "float",
            "int" => "int",
            "map" => "map",
            "null" => "null",
            "seq" => "seq",
            _ => "str",
        };
        return TagClass::Core(c);
    }
    if t.suffix_in_namespace(YAML_NS).as_deref() == Some("merge") {
        return TagClass::Merge;
    }
    TagClass::Other(Tag {
        uri: t.to_string(),
        handle: t.original_handle().to_owned(),
        suffix: t.suffix().to_owned(),
    })
}

fn is_null(t: &str) -> bool {
    matches!(t, "~" | "null" | "Null" | "NULL" | "")
}
fn as_bool(t: &str) -> Option<bool> {
    match t {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

fn all(s: &str, f: impl Fn(u8) -> bool) -> bool {
    !s.is_empty() && s.bytes().all(f)
}

/// Rows 3-5 of the Core table. Ok(None) = not an integer literal.
fn parse_int(t: &str) -> Result<Option<Integer>, ErrorKind> {
    let radix_digits = if let Some(r) = t.strip_prefix("0o") {
        Some((8, r, all(r, |b| (b'0'..=b'7').contains(&b))))
    } else if let Some(r) = t.strip_prefix("0x") {
        Some((16, r, all(r, |b| b.is_ascii_hexdigit())))
    } else {
        None
    };
    if let Some((radix, digits, ok)) = radix_digits {
        if !ok {
            return Ok(None);
        }
        return u128::from_str_radix(digits, radix)
            .map(|v| Some(Integer::from_u128(v)))
            .map_err(|_| ErrorKind::IntegerOutOfRange);
    }
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if !all(digits, |b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let mut mag: u128 = 0;
    for b in digits.bytes() {
        mag = mag
            .checked_mul(10)
            .and_then(|m| m.checked_add((b - b'0') as u128))
            .ok_or(ErrorKind::IntegerOutOfRange)?;
    }
    if !neg || mag == 0 {
        return Ok(Some(Integer::from_u128(mag)));
    }
    const MIN_MAG: u128 = 1 << 127;
    if mag > MIN_MAG {
        return Err(ErrorKind::IntegerOutOfRange);
    }
    Ok(Some(Integer::from_i128(if mag == MIN_MAG {
        i128::MIN
    } else {
        -(mag as i128)
    })))
}

/// Rows 6-8 of the Core table. Ok(None) = not a float literal.
fn parse_float(t: &str) -> Result<Option<f64>, ErrorKind> {
    let unsigned = t.strip_prefix(['-', '+']).unwrap_or(t);
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") {
        return Ok(Some(if t.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        }));
    }
    if matches!(t, ".nan" | ".NaN" | ".NAN") {
        return Ok(Some(f64::from_bits(0x7ff8_0000_0000_0000)));
    }
    // [-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?
    let (mant, exp) = match unsigned.find(['e', 'E']) {
        Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
        None => (unsigned, None),
    };
    let mant_ok = match mant.split_once('.') {
        Some(("", frac)) => all(frac, |b| b.is_ascii_digit()),
        Some((int, frac)) => {
            all(int, |b| b.is_ascii_digit()) && frac.bytes().all(|b| b.is_ascii_digit())
        }
        None => all(mant, |b| b.is_ascii_digit()),
    };
    let exp_ok = match exp {
        None => true,
        Some(e) => all(e.strip_prefix(['-', '+']).unwrap_or(e), |b| {
            b.is_ascii_digit()
        }),
    };
    if !mant_ok || !exp_ok {
        return Ok(None);
    }
    let x: f64 = t.parse().map_err(|_| ErrorKind::Internal)?;
    if x.is_infinite() {
        return Err(ErrorKind::FloatOutOfRange);
    }
    Ok(Some(x))
}

pub(crate) fn core_plain(t: &str) -> Result<Value, ErrorKind> {
    if is_null(t) {
        return Ok(Value::Null);
    }
    if let Some(b) = as_bool(t) {
        return Ok(Value::Bool(b));
    }
    if let Some(i) = parse_int(t)? {
        return Ok(Value::Int(i));
    }
    if let Some(f) = parse_float(t)? {
        return Ok(Value::Float(f));
    }
    Ok(Value::String(t.to_owned()))
}

pub(crate) enum Resolved {
    Plain(Value),
    Tagged(Tag, Value),
}

/// Resolve one scalar (§4.1, §4.3, §4.4). `Merge` is handled by the caller in key position.
pub(crate) fn resolve_scalar(
    text: &str,
    style: ScalarStyle,
    class: TagClass,
) -> Result<Resolved, ErrorKind> {
    let plain = matches!(style, ScalarStyle::Plain);
    let v = match class {
        TagClass::None if plain => core_plain(text)?,
        TagClass::None | TagClass::NonSpecific => Value::String(text.to_owned()),
        TagClass::Core("str") => Value::String(text.to_owned()),
        TagClass::Core("null") => {
            if is_null(text) {
                Value::Null
            } else {
                return Err(ErrorKind::TagValueMismatch);
            }
        }
        TagClass::Core("bool") => Value::Bool(as_bool(text).ok_or(ErrorKind::TagValueMismatch)?),
        TagClass::Core("int") => Value::Int(parse_int(text)?.ok_or(ErrorKind::TagValueMismatch)?),
        TagClass::Core("float") => {
            if is_int_literal(text) {
                // An integer literal under !!float is not bound by the integer range;
                // it must be exactly representable as f64.
                Value::Float(int_literal_to_f64_exact(text).ok_or(ErrorKind::TagValueMismatch)?)
            } else {
                Value::Float(parse_float(text)?.ok_or(ErrorKind::TagValueMismatch)?)
            }
        }
        TagClass::Core(_) => return Err(ErrorKind::TagKindMismatch), // map/seq on a scalar
        TagClass::Merge => {
            // Outside key position `!!merge` is an ordinary non-core tag.
            return Ok(Resolved::Tagged(
                Tag {
                    uri: format!("{YAML_NS}merge"),
                    handle: "!!".into(),
                    suffix: "merge".into(),
                },
                Value::String(text.to_owned()),
            ));
        }
        TagClass::Other(tag) => return Ok(Resolved::Tagged(tag, Value::String(text.to_owned()))),
    };
    Ok(Resolved::Plain(v))
}

fn is_int_literal(t: &str) -> bool {
    if let Some(r) = t.strip_prefix("0o") {
        return all(r, |b| (b'0'..=b'7').contains(&b));
    }
    if let Some(r) = t.strip_prefix("0x") {
        return all(r, |b| b.is_ascii_hexdigit());
    }
    let d = t.strip_prefix(['-', '+']).unwrap_or(t);
    all(d, |b| b.is_ascii_digit())
}

/// Exact f64 value of an integer literal of any size, or None if it would round.
fn int_literal_to_f64_exact(t: &str) -> Option<f64> {
    let (bits_per, digits) = if let Some(r) = t.strip_prefix("0o") {
        (3, r)
    } else if let Some(r) = t.strip_prefix("0x") {
        (4, r)
    } else {
        // Decimal: the correctly rounded f64 is exact iff it prints back to the same integer.
        let neg = t.starts_with('-');
        let d = t
            .strip_prefix(['-', '+'])
            .unwrap_or(t)
            .trim_start_matches('0');
        let d = if d.is_empty() { "0" } else { d };
        let f: f64 = d.parse().ok()?;
        if !f.is_finite() || format!("{f:.0}") != d {
            return None;
        }
        // The integer value is converted, so `-0` is +0.0.
        return Some(if neg && f != 0.0 { -f } else { f });
    };
    // Power-of-two radix: exact iff the significant bits span at most 53.
    let mut bits = String::new();
    for c in digits.chars() {
        let v = c.to_digit(1 << bits_per)?;
        for i in (0..bits_per).rev() {
            bits.push(if v >> i & 1 == 1 { '1' } else { '0' });
        }
    }
    let bits = bits.trim_start_matches('0');
    if bits.is_empty() {
        return Some(0.0);
    }
    let span = bits.trim_end_matches('0').len();
    if span > 53 || bits.len() > 1024 {
        return None;
    }
    let mant = u64::from_str_radix(&bits[..span], 2).ok()? as f64;
    Some(mant * 2f64.powi((bits.len() - span) as i32))
}
