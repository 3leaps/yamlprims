//! Data-layer YAML 1.2.2 Core emitter (spec-v1.5 §10.2, ADR-0005).

use crate::budget::{ENTRY_OVERHEAD, NODE_OVERHEAD};
use crate::error::{Error, ErrorKind, LimitKind, Site};
use crate::ijson::decompose_shortest_float;
use crate::value::{is_valid_non_core_tag_uri, Document, Node, Tag, Value};
use crate::wire::Out;
use std::fmt::Write as _;

/// Plain-scalar compatibility quoting policy (spec-v1.5 §10.2, ADR-0005 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum CompatQuoting {
    /// Quote YAML 1.1 scalar hazards (`y`/`n`/`yes`/`no`/`on`/`off`, underscore/comma numbers,
    /// `0[0-9_]+`, `0b[01_]+`, sexagesimals, ISO dates) in addition to §4.1 Core non-strings.
    #[default]
    Yaml11Safe,
    /// Quote only §4.1 Core non-strings and YAML 1.2.2 lexical hazards.
    CoreOnly,
}

/// Resource bounds for [`emit_with`] and [`emit_all_with`] (spec-v1.5 §10.2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmitLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_scalar_bytes: usize,
    pub max_key_bytes: usize,
    pub max_mapping_entries: usize,
    pub max_tag_bytes: usize,
    pub max_documents: usize,
    pub max_output_bytes: usize,
}

impl Default for EmitLimits {
    fn default() -> Self {
        Self {
            max_depth: 128,
            max_nodes: 250_000,
            max_scalar_bytes: 1024 * 1024,
            max_key_bytes: 64 * 1024,
            max_mapping_entries: 100_000,
            max_tag_bytes: 1_024,
            max_documents: 1_024,
            max_output_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Options for [`emit_with`], [`emit_all_with`], [`crate::to_node`], and [`crate::to_string_with`] (spec-v1.5 §10.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct EmitOptions {
    pub compat_quoting: CompatQuoting,
    pub limits: EmitLimits,
}

impl EmitOptions {
    /// Validate emitter options before processing any input tree or writing output.
    pub fn validate(&self) -> Result<(), Error> {
        let l = &self.limits;
        let bad = |msg: &str| {
            Err(Error::new(
                ErrorKind::InvalidOptions,
                Site::NotSource { last_known: None },
                msg,
            ))
        };
        if l.max_depth == 0 || l.max_depth > 255 {
            return bad("max_depth must be in 1..=255");
        }
        for (name, v) in [
            ("max_nodes", l.max_nodes),
            ("max_scalar_bytes", l.max_scalar_bytes),
            ("max_key_bytes", l.max_key_bytes),
            ("max_mapping_entries", l.max_mapping_entries),
            ("max_tag_bytes", l.max_tag_bytes),
            ("max_documents", l.max_documents),
            ("max_output_bytes", l.max_output_bytes),
        ] {
            if v == 0 {
                return bad(&format!("{name} must be >= 1"));
            }
        }
        if l.max_scalar_bytes > l.max_output_bytes {
            return bad("max_scalar_bytes must be <= max_output_bytes");
        }
        if l.max_key_bytes > l.max_output_bytes {
            return bad("max_key_bytes must be <= max_output_bytes");
        }
        Ok(())
    }
}

fn emit_err(kind: ErrorKind, msg: impl std::fmt::Display) -> Error {
    Error::new(kind, Site::NotSource { last_known: None }, msg)
}

fn emit_limit_err(limit: LimitKind, configured: usize, msg: &str) -> Error {
    emit_err(
        ErrorKind::LimitExceeded {
            limit,
            configured: configured as u64,
        },
        format!("{msg} ({configured})"),
    )
}

enum KeyWeightFrame<'a> {
    Node(&'a Node),
    Seq {
        items: &'a [Node],
        idx: usize,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
        next_val: Option<&'a Node>,
    },
}

fn check_emit_key_weight(key_root: &Node, max_key_bytes: usize) -> Result<(), Error> {
    let mut total: usize = 0;
    let mut stack: Vec<KeyWeightFrame<'_>> = vec![KeyWeightFrame::Node(key_root)];
    while let Some(frame) = stack.pop() {
        let n = match frame {
            KeyWeightFrame::Node(n) => n,
            KeyWeightFrame::Seq { items, idx } => {
                let Some(item) = items.get(idx) else {
                    continue;
                };
                if idx + 1 < items.len() {
                    stack.push(KeyWeightFrame::Seq {
                        items,
                        idx: idx + 1,
                    });
                }
                item
            }
            KeyWeightFrame::Map {
                entries,
                idx,
                next_val: Some(v),
            } => {
                if idx + 1 < entries.len() {
                    stack.push(KeyWeightFrame::Map {
                        entries,
                        idx: idx + 1,
                        next_val: None,
                    });
                }
                v
            }
            KeyWeightFrame::Map {
                entries,
                idx,
                next_val: None,
            } => {
                let Some((k, v)) = entries.get(idx) else {
                    continue;
                };
                stack.push(KeyWeightFrame::Map {
                    entries,
                    idx,
                    next_val: Some(v),
                });
                k
            }
        };
        total = total.saturating_add(NODE_OVERHEAD as usize);
        if total > max_key_bytes {
            return Err(emit_limit_err(
                LimitKind::MaxKeyBytes,
                max_key_bytes,
                "mapping key exceeds max_key_bytes",
            ));
        }
        match &n.value {
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => {}
            Value::String(s) => {
                total = total.saturating_add(s.len());
                if total > max_key_bytes {
                    return Err(emit_limit_err(
                        LimitKind::MaxKeyBytes,
                        max_key_bytes,
                        "mapping key exceeds max_key_bytes",
                    ));
                }
            }
            Value::Sequence(items) => {
                if !items.is_empty() {
                    stack.push(KeyWeightFrame::Seq { items, idx: 0 });
                }
            }
            Value::Mapping(map) => {
                total = total.saturating_add(map.len().saturating_mul(ENTRY_OVERHEAD as usize));
                if total > max_key_bytes {
                    return Err(emit_limit_err(
                        LimitKind::MaxKeyBytes,
                        max_key_bytes,
                        "mapping key exceeds max_key_bytes",
                    ));
                }
                if !map.entries.is_empty() {
                    stack.push(KeyWeightFrame::Map {
                        entries: &map.entries,
                        idx: 0,
                        next_val: None,
                    });
                }
            }
            Value::Tagged(t) => {
                total = total.saturating_add(t.tag.uri.len().saturating_mul(2));
                if total > max_key_bytes {
                    return Err(emit_limit_err(
                        LimitKind::MaxKeyBytes,
                        max_key_bytes,
                        "mapping key exceeds max_key_bytes",
                    ));
                }
                stack.push(KeyWeightFrame::Node(&t.value));
            }
        }
    }
    Ok(())
}

enum PreflightFrame<'a> {
    Node {
        node: &'a Node,
        depth: usize,
        in_key: bool,
        is_key_root: bool,
    },
    Seq {
        items: &'a [Node],
        idx: usize,
        depth: usize,
        in_key: bool,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
        next_val: Option<&'a Node>,
        depth: usize,
        in_key: bool,
    },
}

pub(crate) fn preflight_documents(docs: &[Document], opts: &EmitOptions) -> Result<(), Error> {
    opts.validate()?;
    if docs.len() > opts.limits.max_documents {
        return Err(emit_limit_err(
            LimitKind::MaxDocuments,
            opts.limits.max_documents,
            "document count exceeds max_documents",
        ));
    }

    let mut nodes_visited: usize = 0;
    let mut stack: Vec<PreflightFrame<'_>> = Vec::new();

    for doc in docs {
        if let Some((major, minor)) = doc.yaml_version {
            if major != 1 || minor > 999_999_999 {
                return Err(emit_err(
                    ErrorKind::UnsupportedYamlVersion,
                    format!("unsupported YAML directive version {major}.{minor}"),
                ));
            }
        }
        preflight_node(&doc.root, opts, &mut nodes_visited, &mut stack)?;
    }
    Ok(())
}

fn preflight_node<'a>(
    root: &'a Node,
    opts: &EmitOptions,
    nodes_visited: &mut usize,
    stack: &mut Vec<PreflightFrame<'a>>,
) -> Result<(), Error> {
    stack.clear();
    stack.push(PreflightFrame::Node {
        node: root,
        depth: 0,
        in_key: false,
        is_key_root: false,
    });

    while let Some(step) = stack.pop() {
        let (node, depth, in_key, is_key_root) = match step {
            PreflightFrame::Node {
                node,
                depth,
                in_key,
                is_key_root,
            } => (node, depth, in_key, is_key_root),
            PreflightFrame::Seq {
                items,
                idx,
                depth,
                in_key,
            } => {
                let Some(item) = items.get(idx) else {
                    continue;
                };
                if idx + 1 < items.len() {
                    stack.push(PreflightFrame::Seq {
                        items,
                        idx: idx + 1,
                        depth,
                        in_key,
                    });
                }
                (item, depth, in_key, false)
            }
            PreflightFrame::Map {
                entries,
                idx,
                next_val: Some(v),
                depth,
                in_key,
            } => {
                if idx + 1 < entries.len() {
                    stack.push(PreflightFrame::Map {
                        entries,
                        idx: idx + 1,
                        next_val: None,
                        depth,
                        in_key,
                    });
                }
                (v, depth, in_key, false)
            }
            PreflightFrame::Map {
                entries,
                idx,
                next_val: None,
                depth,
                in_key,
            } => {
                let Some((k, v)) = entries.get(idx) else {
                    continue;
                };
                stack.push(PreflightFrame::Map {
                    entries,
                    idx,
                    next_val: Some(v),
                    depth,
                    in_key,
                });
                (k, depth, true, true)
            }
        };

        *nodes_visited += 1;
        if *nodes_visited > opts.limits.max_nodes {
            return Err(emit_limit_err(
                LimitKind::MaxNodes,
                opts.limits.max_nodes,
                "node count exceeds max_nodes",
            ));
        }

        if is_key_root {
            check_emit_key_weight(node, opts.limits.max_key_bytes)?;
        }

        match &node.value {
            Value::Null | Value::Bool(_) | Value::Int(_) => {}
            Value::Float(f) => {
                if in_key && f.is_nan() {
                    return Err(emit_err(
                        ErrorKind::InvalidKey,
                        "NaN float is not valid inside a mapping key",
                    ));
                }
            }
            Value::String(s) => {
                if s.len() > opts.limits.max_scalar_bytes {
                    return Err(emit_limit_err(
                        LimitKind::MaxScalarBytes,
                        opts.limits.max_scalar_bytes,
                        "scalar byte length exceeds max_scalar_bytes",
                    ));
                }
            }
            Value::Tagged(t) => {
                if t.tag.uri.len() > opts.limits.max_tag_bytes
                    || !is_valid_non_core_tag_uri(&t.tag.uri)
                {
                    return Err(emit_err(
                        ErrorKind::InvalidOptions,
                        "Tagged node contains invalid or oversized Tag::uri",
                    ));
                }
                if t.tag.uri == "tag:yaml.org,2002:merge" {
                    if in_key || !matches!(t.value.value, Value::String(_)) {
                        return Err(emit_err(
                            ErrorKind::TagKindMismatch,
                            "tag:yaml.org,2002:merge may only wrap a non-key String node",
                        ));
                    }
                } else if !matches!(
                    t.value.value,
                    Value::String(_) | Value::Sequence(_) | Value::Mapping(_)
                ) {
                    return Err(emit_err(
                        ErrorKind::TagKindMismatch,
                        "Tagged node must wrap String, Sequence, or Mapping",
                    ));
                }
                stack.push(PreflightFrame::Node {
                    node: &t.value,
                    depth,
                    in_key,
                    is_key_root: false,
                });
            }
            Value::Sequence(items) => {
                let next_depth = depth + 1;
                if next_depth > opts.limits.max_depth {
                    return Err(emit_limit_err(
                        LimitKind::MaxDepth,
                        opts.limits.max_depth,
                        "collection nesting exceeds max_depth",
                    ));
                }
                if !items.is_empty() {
                    stack.push(PreflightFrame::Seq {
                        items,
                        idx: 0,
                        depth: next_depth,
                        in_key,
                    });
                }
            }
            Value::Mapping(map) => {
                let next_depth = depth + 1;
                if next_depth > opts.limits.max_depth {
                    return Err(emit_limit_err(
                        LimitKind::MaxDepth,
                        opts.limits.max_depth,
                        "collection nesting exceeds max_depth",
                    ));
                }
                if map.len() > opts.limits.max_mapping_entries {
                    return Err(emit_limit_err(
                        LimitKind::MaxMappingEntries,
                        opts.limits.max_mapping_entries,
                        "mapping entry count exceeds max_mapping_entries",
                    ));
                }
                if !map.entries.is_empty() {
                    stack.push(PreflightFrame::Map {
                        entries: &map.entries,
                        idx: 0,
                        next_val: None,
                        depth: next_depth,
                        in_key,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Format an IEEE 754 `f64` as a canonical YAML 1.2.2 Core float token (spec-v1.5 §10.2).
pub(crate) fn format_yaml_float(f: f64) -> String {
    if f.is_nan() {
        return ".nan".to_string();
    }
    if f.is_infinite() {
        return if f.is_sign_negative() {
            "-.inf".to_string()
        } else {
            ".inf".to_string()
        };
    }
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let mut out = String::with_capacity(24);
    if f.is_sign_negative() {
        out.push('-');
    }
    let (digits, k) = decompose_shortest_float(f.abs());
    let n = digits.len() as i32;
    let sci_exp = k - 1;
    if (-6..21).contains(&sci_exp) {
        if k >= n {
            out.push_str(&digits);
            for _ in 0..(k - n) {
                out.push('0');
            }
            out.push_str(".0");
        } else if k > 0 {
            let ku = k as usize;
            out.push_str(&digits[..ku]);
            out.push('.');
            out.push_str(&digits[ku..]);
        } else {
            out.push_str("0.");
            for _ in 0..(-k) {
                out.push('0');
            }
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        out.push('.');
        if digits.len() == 1 {
            out.push('0');
        } else {
            out.push_str(&digits[1..]);
        }
        if sci_exp >= 0 {
            let _ = write!(out, "e+{sci_exp}");
        } else {
            let _ = write!(out, "e{sci_exp}");
        }
    }
    out
}

fn is_escaped_codepoint(c: char) -> bool {
    let cp = c as u32;
    (cp <= 0x1F && cp != 0x0A)
        || (0x7F..=0x9F).contains(&cp)
        || matches!(cp, 0x2028 | 0x2029 | 0xFEFF | 0xFFFE | 0xFFFF)
}

fn needs_double_quoted_style1(s: &str, is_key: bool, enclosing_collections: usize) -> bool {
    if s.chars().any(is_escaped_codepoint) {
        return true;
    }
    if !s.is_empty() && s.bytes().all(|b| b == b'\n') {
        return true;
    }
    if s.contains('\n') {
        if is_key {
            return true;
        }
        if enclosing_collections == 0 && matches!(s.as_bytes().first(), Some(b' ' | b'\n')) {
            return true;
        }
    }
    false
}

fn render_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{000C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if is_escaped_codepoint(c) => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn is_unicode_whitespace(c: char) -> bool {
    matches!(
        c as u32,
        0x0009..=0x000D
            | 0x0020
            | 0x0085
            | 0x00A0
            | 0x1680
            | 0x2000..=0x200A
            | 0x2028
            | 0x2029
            | 0x202F
            | 0x205F
            | 0x3000
    )
}

fn matches_core_non_string(s: &str) -> bool {
    if matches!(
        s,
        "" | "~"
            | "null"
            | "Null"
            | "NULL"
            | "true"
            | "True"
            | "TRUE"
            | "false"
            | "False"
            | "FALSE"
            | ".nan"
            | ".NaN"
            | ".NAN"
    ) {
        return true;
    }
    if let Some(r) = s.strip_prefix("0o") {
        if !r.is_empty() && r.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
            return true;
        }
    }
    if let Some(r) = s.strip_prefix("0x") {
        if !r.is_empty() && r.bytes().all(|b| b.is_ascii_hexdigit()) {
            return true;
        }
    }
    let u = s.strip_prefix(['-', '+']).unwrap_or(s);
    if matches!(u, ".inf" | ".Inf" | ".INF") {
        return true;
    }
    let (mant, exp) = match u.find(['e', 'E']) {
        Some(i) => (&u[..i], Some(&u[i + 1..])),
        None => (u, None),
    };
    if let Some(e) = exp {
        let ed = e.strip_prefix(['-', '+']).unwrap_or(e);
        if ed.is_empty() || !ed.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    match mant.split_once('.') {
        Some(("", frac)) => !frac.is_empty() && frac.bytes().all(|b| b.is_ascii_digit()),
        Some((int_part, frac)) => {
            !int_part.is_empty()
                && int_part.bytes().all(|b| b.is_ascii_digit())
                && frac.bytes().all(|b| b.is_ascii_digit())
        }
        None => !mant.is_empty() && mant.bytes().all(|b| b.is_ascii_digit()),
    }
}

fn matches_yaml11_hazard(s: &str) -> bool {
    // Class 1: case-insensitive YAML 1.1 booleans / nulls
    for kw in ["y", "n", "yes", "no", "on", "off", "true", "false", "null"] {
        if s.eq_ignore_ascii_case(kw) {
            return true;
        }
    }
    if s.eq_ignore_ascii_case(".nan") {
        return true;
    }
    let u = s.strip_prefix(['-', '+']).unwrap_or(s);
    if u.eq_ignore_ascii_case(".inf") {
        return true;
    }

    // Class 2: radix, underscore/comma-grouped, or multi-dot numeric tokens
    if let Some(r) = u.strip_prefix("0b") {
        if !r.is_empty() && r.bytes().all(|b| matches!(b, b'0' | b'1' | b'_')) {
            return true;
        }
    }
    if let Some(r) = u.strip_prefix("0x") {
        if !r.is_empty() && r.bytes().all(|b| b.is_ascii_hexdigit() || b == b'_') {
            return true;
        }
    }
    if let Some(r) = u.strip_prefix('0') {
        if !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit() || b == b'_') {
            return true;
        }
    }
    let (mant, exp) = match u.find(['e', 'E']) {
        Some(i) => (&u[..i], Some(&u[i + 1..])),
        None => (u, None),
    };
    let exp_ok = match exp {
        None => true,
        Some(e) => {
            let ed = e.strip_prefix(['-', '+']).unwrap_or(e);
            !ed.is_empty() && ed.bytes().all(|b| b.is_ascii_digit() || b == b'_')
        }
    };
    if exp_ok {
        if mant.contains('.') && mant.bytes().any(|b| b.is_ascii_digit()) {
            let dot_ok = if let Some(rest) = mant.strip_prefix('.') {
                rest.as_bytes()
                    .first()
                    .is_some_and(|b| b.is_ascii_digit() || *b == b'_')
                    && rest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b == b'_' || b == b'.')
            } else if let Some((head, tail)) = mant.split_once('.') {
                !head.is_empty()
                    && head.bytes().all(|b| b.is_ascii_digit() || b == b'_')
                    && tail
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b == b'_' || b == b'.')
            } else {
                false
            };
            if dot_ok {
                return true;
            }
        }
        if mant.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
            && mant[1..]
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'_' || b == b',')
        {
            return true;
        }
    }

    // Class 3: sexagesimal [+-]?[0-9][0-9_]*(:[0-5]?[0-9])+([.][0-9_]*)?
    if u.contains(':') {
        let (sex_int, sex_frac_ok) = match u.split_once('.') {
            Some((head, frac)) => (head, frac.bytes().all(|b| b.is_ascii_digit() || b == b'_')),
            None => (u, true),
        };
        if sex_frac_ok {
            let mut parts = sex_int.split(':');
            if let Some(first) = parts.next() {
                let first_ok = first.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
                    && first[1..].bytes().all(|b| b.is_ascii_digit() || b == b'_');
                if first_ok {
                    let mut count = 0usize;
                    let mut all_ok = true;
                    for seg in parts {
                        count += 1;
                        let sb = seg.as_bytes();
                        let seg_ok = (sb.len() == 1 && sb[0].is_ascii_digit())
                            || (sb.len() == 2
                                && (b'0'..=b'5').contains(&sb[0])
                                && sb[1].is_ascii_digit());
                        if !seg_ok {
                            all_ok = false;
                            break;
                        }
                    }
                    if count >= 1 && all_ok {
                        return true;
                    }
                }
            }
        }
    }

    // Class 4: YAML 1.1 timestamp [0-9]{4}-[0-9]{1,2}-[0-9]{1,2}([Tt \t].*)?
    let b = s.as_bytes();
    if b.len() >= 8 && b[0..4].iter().all(|c| c.is_ascii_digit()) && b[4] == b'-' {
        let mut idx = 5;
        let m_start = idx;
        while idx < b.len() && idx - m_start < 2 && b[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx > m_start && idx < b.len() && b[idx] == b'-' {
            idx += 1;
            let d_start = idx;
            while idx < b.len() && idx - d_start < 2 && b[idx].is_ascii_digit() {
                idx += 1;
            }
            if idx > d_start && (idx == b.len() || matches!(b[idx], b'T' | b't' | b' ' | b'\t')) {
                return true;
            }
        }
    }

    false
}

fn can_emit_plain(s: &str, quoting: CompatQuoting) -> bool {
    if s.is_empty() || s.len() > 1024 || s.contains('\n') {
        return false;
    }
    let first_char = s.chars().next().unwrap();
    let last_char = s.chars().next_back().unwrap();
    if is_unicode_whitespace(first_char) || is_unicode_whitespace(last_char) {
        return false;
    }
    let b = s.as_bytes();
    if matches!(
        b[0],
        b',' | b'['
            | b']'
            | b'{'
            | b'}'
            | b'#'
            | b'&'
            | b'*'
            | b'!'
            | b'|'
            | b'>'
            | b'\''
            | b'"'
            | b'%'
            | b'@'
            | b'`'
    ) {
        return false;
    }
    if matches!(b[0], b'-' | b'?' | b':') && (b.len() == 1 || matches!(b[1], b' ' | b'\t')) {
        return false;
    }
    for marker in ["---", "..."] {
        if let Some(rest) = s.strip_prefix(marker) {
            if rest.is_empty() || rest.chars().next().is_some_and(is_unicode_whitespace) {
                return false;
            }
        }
    }
    if s.contains(": ") || s.contains(":\t") || s.contains(" #") || s.contains("\t#") {
        return false;
    }
    if s.ends_with(':') || s == "<<" || s == "=" {
        return false;
    }
    if matches_core_non_string(s) {
        return false;
    }
    if matches!(quoting, CompatQuoting::Yaml11Safe) && matches_yaml11_hazard(s) {
        return false;
    }
    true
}

enum RenderedString<'a> {
    SingleLine(String),
    LiteralBlock { header: &'static str, body: &'a str },
}

fn render_string<'a>(
    s: &'a str,
    is_key: bool,
    enclosing_collections: usize,
    quoting: CompatQuoting,
) -> RenderedString<'a> {
    // Style 1: Double-quoted when required
    if needs_double_quoted_style1(s, is_key, enclosing_collections) {
        return RenderedString::SingleLine(render_double_quoted(s));
    }
    // Style 2: Literal block for multiline non-key values
    if s.contains('\n') {
        let needs_indent_digit =
            enclosing_collections >= 1 && matches!(s.as_bytes().first(), Some(b' ' | b'\n'));
        let header = if !s.ends_with('\n') {
            if needs_indent_digit {
                "|2-"
            } else {
                "|-"
            }
        } else if s.ends_with("\n\n") {
            if needs_indent_digit {
                "|2+"
            } else {
                "|+"
            }
        } else if needs_indent_digit {
            "|2"
        } else {
            "|"
        };
        return RenderedString::LiteralBlock { header, body: s };
    }
    // Style 3: Plain
    if can_emit_plain(s, quoting) {
        return RenderedString::SingleLine(s.to_owned());
    }
    // Style 4: Single-quoted if no `'`, else double-quoted
    if !s.contains('\'') {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('\'');
        out.push_str(s);
        out.push('\'');
        RenderedString::SingleLine(out)
    } else {
        RenderedString::SingleLine(render_double_quoted(s))
    }
}

fn write_literal_block(
    out: &mut Out,
    header: &str,
    body: &str,
    body_indent: usize,
) -> Result<(), Error> {
    out.push(header)?;
    out.push("\n")?;
    let trimmed = body.strip_suffix('\n').unwrap_or(body);
    for line in trimmed.split('\n') {
        if !line.is_empty() {
            push_spaces(out, body_indent)?;
            out.push(line)?;
        }
        out.push("\n")?;
    }
    Ok(())
}

fn push_spaces(out: &mut Out, count: usize) -> Result<(), Error> {
    const SPACES: &str = "                                                                ";
    let mut rem = count;
    while rem > 0 {
        let chunk = rem.min(SPACES.len());
        out.push(&SPACES[..chunk])?;
        rem -= chunk;
    }
    Ok(())
}

fn is_ns_tag_char_no_pct(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'#' | b';'
                | b'/'
                | b'?'
                | b':'
                | b'@'
                | b'&'
                | b'='
                | b'+'
                | b'$'
                | b'_'
                | b'.'
                | b'~'
                | b'*'
                | b'\''
                | b'('
                | b')'
                | b'-'
        )
}

fn is_verbatim_uri_char_no_pct(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b';' | b'/'
                | b'?'
                | b':'
                | b'@'
                | b'&'
                | b'='
                | b'+'
                | b'$'
                | b'_'
                | b'.'
                | b'~'
                | b'*'
                | b'\''
                | b'('
                | b')'
                | b'-'
                | b','
                | b'!'
        )
}

fn percent_encode_into(out: &mut String, s: &str, allow: fn(u8) -> bool) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in s.as_bytes() {
        if allow(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
}

pub(crate) fn render_tag(tag: &Tag) -> String {
    let uri = &tag.uri;
    if let Some(rest) = uri.strip_prefix('!') {
        let mut out = String::with_capacity(uri.len() + 4);
        out.push('!');
        percent_encode_into(&mut out, rest, is_ns_tag_char_no_pct);
        out
    } else if let Some(rest) = uri.strip_prefix("tag:yaml.org,2002:") {
        if !rest.is_empty() {
            let mut out = String::with_capacity(rest.len() + 6);
            out.push_str("!!");
            percent_encode_into(&mut out, rest, is_ns_tag_char_no_pct);
            return out;
        }
        let mut out = String::with_capacity(uri.len() + 6);
        out.push_str("!<");
        percent_encode_into(&mut out, uri, is_verbatim_uri_char_no_pct);
        out.push('>');
        out
    } else {
        let mut out = String::with_capacity(uri.len() + 6);
        out.push_str("!<");
        percent_encode_into(&mut out, uri, is_verbatim_uri_char_no_pct);
        out.push('>');
        out
    }
}

fn render_scalar_single_line(
    v: &Value,
    is_key: bool,
    enclosing_collections: usize,
    quoting: CompatQuoting,
) -> Option<String> {
    match v {
        Value::Null => Some("null".to_string()),
        Value::Bool(true) => Some("true".to_string()),
        Value::Bool(false) => Some("false".to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(format_yaml_float(*f)),
        Value::String(s) => match render_string(s, is_key, enclosing_collections, quoting) {
            RenderedString::SingleLine(r) => Some(r),
            RenderedString::LiteralBlock { .. } => None,
        },
        Value::Sequence(items) if items.is_empty() => Some("[]".to_string()),
        Value::Mapping(map) if map.is_empty() => Some("{}".to_string()),
        _ => None,
    }
}

fn render_implicit_key(key: &Node, enclosing: usize, quoting: CompatQuoting) -> Option<String> {
    match &key.value {
        Value::Null => Some("null".to_string()),
        Value::Bool(true) => Some("true".to_string()),
        Value::Bool(false) => Some("false".to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(format_yaml_float(*f)),
        Value::String(s) if !s.contains('\n') => {
            if let RenderedString::SingleLine(r) = render_string(s, true, enclosing, quoting) {
                if r.len() <= 1024 {
                    return Some(r);
                }
            }
            None
        }
        _ => None,
    }
}

enum EmitFrame<'a> {
    Seq {
        items: &'a [Node],
        idx: usize,
        indent: usize,
        enclosing: usize,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
        indent: usize,
        enclosing: usize,
    },
    ExplicitMapValue {
        val: &'a Node,
        indent: usize,
        enclosing: usize,
    },
}

fn emit_after_prefix<'a>(
    out: &mut Out,
    stack: &mut Vec<EmitFrame<'a>>,
    indent: usize,
    prefix: &str,
    node: &'a Node,
    enclosing: usize,
    is_key: bool,
    quoting: CompatQuoting,
) -> Result<(), Error> {
    push_spaces(out, indent)?;
    out.push(prefix)?;

    if let Some(single) = render_scalar_single_line(&node.value, is_key, enclosing, quoting) {
        out.push(" ")?;
        out.push(&single)?;
        out.push("\n")?;
        return Ok(());
    }

    match &node.value {
        Value::String(s) => match render_string(s, is_key, enclosing, quoting) {
            RenderedString::SingleLine(single) => {
                out.push(" ")?;
                out.push(&single)?;
                out.push("\n")?;
            }
            RenderedString::LiteralBlock { header, body } => {
                out.push(" ")?;
                write_literal_block(out, header, body, indent + 2)?;
            }
        },
        Value::Sequence(items) => {
            out.push("\n")?;
            stack.push(EmitFrame::Seq {
                items,
                idx: 0,
                indent: indent + 2,
                enclosing: enclosing + 1,
            });
        }
        Value::Mapping(map) => {
            out.push("\n")?;
            stack.push(EmitFrame::Map {
                entries: &map.entries,
                idx: 0,
                indent: indent + 2,
                enclosing: enclosing + 1,
            });
        }
        Value::Tagged(t) => {
            let tag_str = render_tag(&t.tag);
            out.push(" ")?;
            out.push(&tag_str)?;
            if let Some(single) =
                render_scalar_single_line(&t.value.value, is_key, enclosing, quoting)
            {
                out.push(" ")?;
                out.push(&single)?;
                out.push("\n")?;
            } else {
                match &t.value.value {
                    Value::String(s) => match render_string(s, is_key, enclosing, quoting) {
                        RenderedString::SingleLine(single) => {
                            out.push(" ")?;
                            out.push(&single)?;
                            out.push("\n")?;
                        }
                        RenderedString::LiteralBlock { header, body } => {
                            out.push(" ")?;
                            write_literal_block(out, header, body, indent + 2)?;
                        }
                    },
                    Value::Sequence(items) => {
                        out.push("\n")?;
                        stack.push(EmitFrame::Seq {
                            items,
                            idx: 0,
                            indent: indent + 2,
                            enclosing: enclosing + 1,
                        });
                    }
                    Value::Mapping(map) => {
                        out.push("\n")?;
                        stack.push(EmitFrame::Map {
                            entries: &map.entries,
                            idx: 0,
                            indent: indent + 2,
                            enclosing: enclosing + 1,
                        });
                    }
                    _ => unreachable!("validated by preflight"),
                }
            }
        }
        _ => unreachable!("handled by render_scalar_single_line"),
    }
    Ok(())
}

fn emit_root_node(out: &mut Out, root: &Node, quoting: CompatQuoting) -> Result<(), Error> {
    if let Some(single) = render_scalar_single_line(&root.value, false, 0, quoting) {
        out.push(&single)?;
        out.push("\n")?;
        return Ok(());
    }

    let mut stack: Vec<EmitFrame<'_>> = Vec::new();
    match &root.value {
        Value::String(s) => match render_string(s, false, 0, quoting) {
            RenderedString::SingleLine(single) => {
                out.push(&single)?;
                out.push("\n")?;
            }
            RenderedString::LiteralBlock { header, body } => {
                write_literal_block(out, header, body, 2)?;
            }
        },
        Value::Sequence(items) => {
            stack.push(EmitFrame::Seq {
                items,
                idx: 0,
                indent: 0,
                enclosing: 1,
            });
        }
        Value::Mapping(map) => {
            stack.push(EmitFrame::Map {
                entries: &map.entries,
                idx: 0,
                indent: 0,
                enclosing: 1,
            });
        }
        Value::Tagged(t) => {
            let tag_str = render_tag(&t.tag);
            out.push(&tag_str)?;
            if let Some(single) = render_scalar_single_line(&t.value.value, false, 0, quoting) {
                out.push(" ")?;
                out.push(&single)?;
                out.push("\n")?;
            } else {
                match &t.value.value {
                    Value::String(s) => match render_string(s, false, 0, quoting) {
                        RenderedString::SingleLine(single) => {
                            out.push(" ")?;
                            out.push(&single)?;
                            out.push("\n")?;
                        }
                        RenderedString::LiteralBlock { header, body } => {
                            out.push(" ")?;
                            write_literal_block(out, header, body, 2)?;
                        }
                    },
                    Value::Sequence(items) => {
                        out.push("\n")?;
                        stack.push(EmitFrame::Seq {
                            items,
                            idx: 0,
                            indent: 0,
                            enclosing: 1,
                        });
                    }
                    Value::Mapping(map) => {
                        out.push("\n")?;
                        stack.push(EmitFrame::Map {
                            entries: &map.entries,
                            idx: 0,
                            indent: 0,
                            enclosing: 1,
                        });
                    }
                    _ => unreachable!("validated by preflight"),
                }
            }
        }
        _ => unreachable!("handled by render_scalar_single_line"),
    }

    while let Some(frame) = stack.pop() {
        match frame {
            EmitFrame::Seq {
                items,
                idx,
                indent,
                enclosing,
            } => {
                if idx < items.len() {
                    if idx + 1 < items.len() {
                        stack.push(EmitFrame::Seq {
                            items,
                            idx: idx + 1,
                            indent,
                            enclosing,
                        });
                    }
                    emit_after_prefix(
                        out,
                        &mut stack,
                        indent,
                        "-",
                        &items[idx],
                        enclosing,
                        false,
                        quoting,
                    )?;
                }
            }
            EmitFrame::Map {
                entries,
                idx,
                indent,
                enclosing,
            } => {
                if idx < entries.len() {
                    if idx + 1 < entries.len() {
                        stack.push(EmitFrame::Map {
                            entries,
                            idx: idx + 1,
                            indent,
                            enclosing,
                        });
                    }
                    let (k, v) = &entries[idx];
                    if let Some(key_str) = render_implicit_key(k, enclosing, quoting) {
                        let prefix = format!("{key_str}:");
                        emit_after_prefix(
                            out, &mut stack, indent, &prefix, v, enclosing, false, quoting,
                        )?;
                    } else {
                        stack.push(EmitFrame::ExplicitMapValue {
                            val: v,
                            indent,
                            enclosing,
                        });
                        emit_after_prefix(
                            out, &mut stack, indent, "?", k, enclosing, true, quoting,
                        )?;
                    }
                }
            }
            EmitFrame::ExplicitMapValue {
                val,
                indent,
                enclosing,
            } => {
                emit_after_prefix(out, &mut stack, indent, ":", val, enclosing, false, quoting)?;
            }
        }
    }

    Ok(())
}

/// Emit a single `Node` as canonical YAML 1.2.2 Core text under [`EmitOptions::default()`] (spec-v1.5 §10.2).
pub fn emit(node: &Node) -> Result<String, Error> {
    emit_with(node, &EmitOptions::default())
}

/// Emit a single `Node` as canonical YAML 1.2.2 Core text under `opts` (spec-v1.5 §10.2).
pub fn emit_with(node: &Node, opts: &EmitOptions) -> Result<String, Error> {
    opts.validate()?;
    let mut nodes_visited = 0usize;
    let mut stack = Vec::new();
    preflight_node(node, opts, &mut nodes_visited, &mut stack)?;
    let mut out = Out::new(opts.limits.max_output_bytes);
    emit_root_node(&mut out, node, opts.compat_quoting)?;
    Ok(out.into_string())
}

/// Emit a slice of `Document` values as a canonical YAML 1.2.2 Core stream under [`EmitOptions::default()`] (spec-v1.5 §10.2).
pub fn emit_all(docs: &[Document]) -> Result<String, Error> {
    emit_all_with(docs, &EmitOptions::default())
}

/// Emit a slice of `Document` values as a canonical YAML 1.2.2 Core stream under `opts` (spec-v1.5 §10.2).
pub fn emit_all_with(docs: &[Document], opts: &EmitOptions) -> Result<String, Error> {
    preflight_documents(docs, opts)?;
    if docs.is_empty() {
        return Ok(String::new());
    }
    let mut out = Out::new(opts.limits.max_output_bytes);
    for (idx, doc) in docs.iter().enumerate() {
        if idx == 0 {
            if let Some((_, minor)) = doc.yaml_version {
                let mut hdr = String::with_capacity(24);
                let _ = writeln!(hdr, "%YAML 1.{minor}");
                hdr.push_str("---\n");
                out.push(&hdr)?;
            } else if doc.explicit_start {
                out.push("---\n")?;
            }
        } else if let Some((_, minor)) = doc.yaml_version {
            let mut hdr = String::with_capacity(28);
            let _ = writeln!(hdr, "...\n%YAML 1.{minor}");
            hdr.push_str("---\n");
            out.push(&hdr)?;
        } else {
            out.push("---\n")?;
        }
        emit_root_node(&mut out, &doc.root, opts.compat_quoting)?;
    }
    Ok(out.into_string())
}
