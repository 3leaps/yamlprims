//! YAML 1.1 → YAML 1.2.2 Core migration lint (`spec-v1.6` §10.3, §11.7).

use crate::budget::{limit_err, tag_bytes, Budget, ENTRY_OVERHEAD, NODE_OVERHEAD};
use crate::error::{bounded, Error, ErrorKind, LimitKind, PathSegment, MAX_FIELD, MAX_MESSAGE};
use crate::load::{gate_bytes, gate_str, read_bounded};
use crate::options::{Limits, MergeKeys, Options};
use crate::value::{Location, Mapping, Node, NodeSpan, Span, Tagged, Value};
use crate::wire::{preflight_path, write_path_segments, Out, MAX_WIRE_META_UINT};
use std::fmt;

/// Closed vocabulary of YAML 1.1 migration lint codes (`spec-v1.6` §10.3, §11.7).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LintCode {
    Yaml11Boolean,     // wire code: "yaml11_boolean"
    Yaml11Number,      // wire code: "yaml11_number"
    Yaml11Sexagesimal, // wire code: "yaml11_sexagesimal"
    Yaml11MergeKey,    // wire code: "yaml11_merge_key"
}

impl LintCode {
    /// Canonical `yamlprims-lint/1` wire code string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            LintCode::Yaml11Boolean => "yaml11_boolean",
            LintCode::Yaml11Number => "yaml11_number",
            LintCode::Yaml11Sexagesimal => "yaml11_sexagesimal",
            LintCode::Yaml11MergeKey => "yaml11_merge_key",
        }
    }
}

impl fmt::Display for LintCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One YAML 1.1 compatibility hazard recorded during `lint_yaml11*` (`spec-v1.6` §10.3).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintDiagnostic {
    pub code: LintCode,
    pub span: Span,
    pub document_index: usize,
    pub path: Vec<PathSegment>,
    pub raw: String,
    pub yaml12_resolution: String,
    pub yaml11_note: String,
    pub message: String,
}

impl LintDiagnostic {
    /// Construct a [`LintDiagnostic`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        code: LintCode,
        span: Span,
        document_index: usize,
        path: Vec<PathSegment>,
        raw: impl Into<String>,
        yaml12_resolution: impl Into<String>,
        yaml11_note: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            span,
            document_index,
            path,
            raw: raw.into(),
            yaml12_resolution: yaml12_resolution.into(),
            yaml11_note: yaml11_note.into(),
            message: message.into(),
        }
    }
}

/// Complete stream lint result returned by `lint_yaml11*` (`spec-v1.6` §10.3).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LintResult {
    pub diagnostics: Vec<LintDiagnostic>,
    pub warning_count: usize,
    pub truncated: bool,
}

impl LintResult {
    /// Construct a [`LintResult`].
    pub fn new(diagnostics: Vec<LintDiagnostic>, warning_count: usize, truncated: bool) -> Self {
        Self {
            diagnostics,
            warning_count,
            truncated,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScalarHazard {
    BooleanTruthy,
    BooleanFalsity,
    SexagesimalInt,
    SexagesimalFloat,
    LegacyOctal,
    BinaryInt,
    SignedHexInt,
    UnderscoreNumber,
}

/// Classify an untagged plain scalar against §10.3 priority order:
/// 1. `Yaml11Boolean`
/// 2. `Yaml11Sexagesimal`
/// 3. `Yaml11Number`
pub(crate) fn classify_untagged_plain(raw: &str) -> Option<ScalarHazard> {
    match raw {
        "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" => {
            return Some(ScalarHazard::BooleanTruthy);
        }
        "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF" => {
            return Some(ScalarHazard::BooleanFalsity);
        }
        _ => {}
    }
    if let Some(is_float) = match_yaml11_sexagesimal(raw) {
        return Some(if is_float {
            ScalarHazard::SexagesimalFloat
        } else {
            ScalarHazard::SexagesimalInt
        });
    }
    if match_legacy_octal_no_underscore(raw) {
        return Some(ScalarHazard::LegacyOctal);
    }
    if match_binary_int(raw) {
        return Some(ScalarHazard::BinaryInt);
    }
    if match_signed_hex_int(raw) {
        return Some(ScalarHazard::SignedHexInt);
    }
    if match_underscore_number(raw) {
        return Some(ScalarHazard::UnderscoreNumber);
    }
    None
}

fn valid_sexagesimal_tail(mut parts: std::str::Split<'_, char>) -> bool {
    let mut count = 0usize;
    for seg in &mut parts {
        count += 1;
        let ok = match seg.as_bytes() {
            [b] => b.is_ascii_digit(),
            [b0, b1] => (b'0'..=b'5').contains(b0) && b1.is_ascii_digit(),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    count >= 1
}

/// Matches:
/// - Sexagesimal int: `\A[+-]?[1-9][0-9_]*(:[0-5]?[0-9])+\z` -> `Some(false)`
/// - Sexagesimal float: `\A[+-]?[0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*\z` -> `Some(true)`
fn match_yaml11_sexagesimal(raw: &str) -> Option<bool> {
    let rest = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    if let Some((before_dot, after_dot)) = rest.split_once('.') {
        if !after_dot.bytes().all(|b| b.is_ascii_digit() || b == b'_') {
            return None;
        }
        let mut parts = before_dot.split(':');
        let first = parts.next()?;
        if !first.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
            || !first.bytes().all(|b| b.is_ascii_digit() || b == b'_')
        {
            return None;
        }
        if valid_sexagesimal_tail(parts) {
            Some(true)
        } else {
            None
        }
    } else {
        let mut parts = rest.split(':');
        let first = parts.next()?;
        if !first
            .as_bytes()
            .first()
            .is_some_and(|b| (b'1'..=b'9').contains(b))
            || !first.bytes().all(|b| b.is_ascii_digit() || b == b'_')
        {
            return None;
        }
        if valid_sexagesimal_tail(parts) {
            Some(false)
        } else {
            None
        }
    }
}

/// `\A[+-]?0[0-7]*[1-7][0-7]+\z`
fn match_legacy_octal_no_underscore(raw: &str) -> bool {
    let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw).as_bytes();
    digits.len() >= 3
        && digits[0] == b'0'
        && digits.iter().all(|b| (b'0'..=b'7').contains(b))
        && digits[1..digits.len() - 1]
            .iter()
            .any(|b| (b'1'..=b'7').contains(b))
}

/// `\A[+-]?0b[0-1_]+\z` containing at least one binary digit `[01]`.
fn match_binary_int(raw: &str) -> bool {
    let rest = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    let Some(bin) = rest.strip_prefix("0b") else {
        return false;
    };
    !bin.is_empty()
        && bin.bytes().all(|b| matches!(b, b'0' | b'1' | b'_'))
        && bin.bytes().any(|b| matches!(b, b'0' | b'1'))
}

/// `\A[+-]0x[0-9a-fA-F_]+\z` containing at least one hex digit `[0-9a-fA-F]`.
fn match_signed_hex_int(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix(['+', '-']) else {
        return false;
    };
    let Some(hex) = rest.strip_prefix("0x") else {
        return false;
    };
    !hex.is_empty()
        && hex.bytes().all(|b| b.is_ascii_hexdigit() || b == b'_')
        && hex.bytes().any(|b| b.is_ascii_hexdigit())
}

/// Underscore-grouped YAML 1.1 integer or float literal (`raw` contains `'_'`).
fn match_underscore_number(raw: &str) -> bool {
    if !raw.contains('_') {
        return false;
    }
    // Unsigned hexadecimal integer with `_`: `\A0x[0-9a-fA-F_]+\z` with >= 1 hex digit.
    if let Some(hex) = raw.strip_prefix("0x") {
        return !hex.is_empty()
            && hex.bytes().all(|b| b.is_ascii_hexdigit() || b == b'_')
            && hex.bytes().any(|b| b.is_ascii_hexdigit());
    }
    let rest = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    // Legacy octal with `_`: `\A[+-]?0[0-7_]+\z` (leading '0' satisfies >= 1 octal digit).
    if rest.starts_with('0')
        && rest.len() >= 2
        && rest
            .bytes()
            .all(|b| (b'0'..=b'7').contains(&b) || b == b'_')
    {
        return true;
    }
    // Decimal integer with `_`: `\A[+-]?[1-9][0-9_]*\z`.
    if rest
        .as_bytes()
        .first()
        .is_some_and(|b| (b'1'..=b'9').contains(b))
        && rest.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    {
        return true;
    }
    // Decimal float with `_`:
    // `\A[+-]?([0-9][0-9_]*\.[0-9_]*|\.[0-9][0-9_]*)([eE][+-][0-9]+)?\z`.
    let (mant, exp) = match rest.find(['e', 'E']) {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    if let Some(e) = exp {
        let Some(edigits) = e.strip_prefix(['+', '-']) else {
            return false;
        };
        if edigits.is_empty() || !edigits.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    let Some((int_part, frac_part)) = mant.split_once('.') else {
        return false;
    };
    if int_part.is_empty() {
        frac_part
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_digit())
            && frac_part.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    } else {
        int_part
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_digit())
            && int_part.bytes().all(|b| b.is_ascii_digit() || b == b'_')
            && frac_part.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    }
}

struct JsonEscaped<'a>(&'a str);

impl fmt::Display for JsonEscaped<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("\"")?;
        let mut run = 0;
        for (i, c) in self.0.char_indices() {
            if i - run > MAX_MESSAGE {
                f.write_str(&self.0[run..i])?;
                run = i;
            }
            let esc = match c {
                '"' => "\\\"",
                '\\' => "\\\\",
                '\u{8}' => "\\b",
                '\u{c}' => "\\f",
                '\n' => "\\n",
                '\r' => "\\r",
                '\t' => "\\t",
                c if (c as u32) < 0x20 => "",
                _ => continue,
            };
            f.write_str(&self.0[run..i])?;
            if esc.is_empty() {
                write!(f, "\\u{:04x}", c as u32)?;
            } else {
                f.write_str(esc)?;
            }
            run = i + c.len_utf8();
        }
        f.write_str(&self.0[run..])?;
        f.write_str("\"")
    }
}

struct OctalAsBase10<'a> {
    raw: &'a str,
    resolved_val: &'a Value,
}

impl fmt::Display for OctalAsBase10<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Value::Int(i) = self.resolved_val {
            return write!(f, "{i}");
        }
        let neg = self.raw.starts_with('-');
        let digits = self
            .raw
            .strip_prefix(['+', '-'])
            .unwrap_or(self.raw)
            .trim_start_matches('0');
        if digits.is_empty() {
            f.write_str("0")
        } else {
            if neg {
                f.write_str("-")?;
            }
            for chunk in digits.as_bytes().chunks(MAX_FIELD) {
                let s = std::str::from_utf8(chunk).unwrap_or("");
                f.write_str(s)?;
            }
            Ok(())
        }
    }
}

#[derive(Default)]
pub(crate) struct LintCollector {
    pub(crate) result: LintResult,
}

impl LintCollector {
    fn admit_retention_and_path(
        &mut self,
        span: Span,
        path: &mut Vec<PathSegment>,
        opts: &Options,
        budget: &mut Budget,
    ) -> Result<bool, Error> {
        self.result.warning_count = self.result.warning_count.saturating_add(1);
        if self.result.diagnostics.len() >= opts.limits.max_warnings {
            self.result.truncated = true;
            return Ok(false);
        }
        // Step 1: Charge each PathSegment::Key in `path` before formatting or cloning.
        for seg in path.iter() {
            let PathSegment::Key(k) = seg else { continue };
            let (k_nodes, k_weight) = match measure_path_key(k, &opts.limits) {
                Ok(m) => m,
                Err(kind) => {
                    let configured = match kind {
                        LimitKind::MaxKeyBytes => opts.limits.max_key_bytes,
                        LimitKind::MaxDepth => opts.limits.max_depth,
                        _ => 0,
                    };
                    let mut e = limit_err(kind, configured, span.start);
                    e.path = std::mem::take(path);
                    return Err(e);
                }
            };
            if let Err((kind, configured)) = budget.charge(&opts.limits, k_nodes, k_weight, false) {
                let mut e = limit_err(kind, configured, span.start);
                e.path = std::mem::take(path);
                return Err(e);
            }
        }
        Ok(true)
    }

    pub(crate) fn record_merge_key(
        &mut self,
        span: Span,
        doc_index: usize,
        path: &mut Vec<PathSegment>,
        opts: &Options,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        if !self.admit_retention_and_path(span, path, opts, budget)? {
            return Ok(());
        }
        let yaml12_resolution = match opts.merge_keys {
            MergeKeys::Reject => {
                "rejected (MergeKeys::Reject default; literal string \"<<\" under MergeKeys::AsKey)"
            }
            MergeKeys::Apply => {
                "merge applied (MergeKeys::Apply opt-in; not part of YAML 1.2.2 Core)"
            }
            MergeKeys::AsKey => "string (\"<<\" under MergeKeys::AsKey)",
        };
        let yaml11_note = "YAML 1.1 merge key specification (https://yaml.org/type/merge.html)";
        self.commit_retained(
            LintCode::Yaml11MergeKey,
            span,
            doc_index,
            path,
            "<<",
            yaml12_resolution.to_owned(),
            yaml11_note.to_owned(),
            opts,
            budget,
        )
    }

    pub(crate) fn record_scalar(
        &mut self,
        hazard: ScalarHazard,
        raw: &str,
        resolved_val: &Value,
        span: Span,
        doc_index: usize,
        path: &mut Vec<PathSegment>,
        opts: &Options,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        if !self.admit_retention_and_path(span, path, opts, budget)? {
            return Ok(());
        }
        let (code, yaml12_resolution, yaml11_note) = match hazard {
            ScalarHazard::BooleanTruthy => (
                LintCode::Yaml11Boolean,
                bounded(format_args!("string ({})", JsonEscaped(raw)), MAX_FIELD),
                "YAML 1.1 boolean literal (true)".to_owned(),
            ),
            ScalarHazard::BooleanFalsity => (
                LintCode::Yaml11Boolean,
                bounded(format_args!("string ({})", JsonEscaped(raw)), MAX_FIELD),
                "YAML 1.1 boolean literal (false)".to_owned(),
            ),
            ScalarHazard::SexagesimalInt => (
                LintCode::Yaml11Sexagesimal,
                bounded(format_args!("string ({})", JsonEscaped(raw)), MAX_FIELD),
                "YAML 1.1 sexagesimal integer literal".to_owned(),
            ),
            ScalarHazard::SexagesimalFloat => (
                LintCode::Yaml11Sexagesimal,
                bounded(format_args!("string ({})", JsonEscaped(raw)), MAX_FIELD),
                "YAML 1.1 sexagesimal float literal".to_owned(),
            ),
            ScalarHazard::LegacyOctal => (
                LintCode::Yaml11Number,
                bounded(
                    format_args!("int ({})", OctalAsBase10 { raw, resolved_val }),
                    MAX_FIELD,
                ),
                "YAML 1.1 base-8 (octal) integer literal; use 0o prefix for octal in YAML 1.2 or quote as string".to_owned(),
            ),
            ScalarHazard::BinaryInt => (
                LintCode::Yaml11Number,
                bounded(format_args!("string ({})", JsonEscaped(raw)), MAX_FIELD),
                "YAML 1.1 base-2 (binary) integer literal".to_owned(),
            ),
            ScalarHazard::SignedHexInt => (
                LintCode::Yaml11Number,
                bounded(format_args!("string ({})", JsonEscaped(raw)), MAX_FIELD),
                "YAML 1.1 signed hexadecimal integer literal".to_owned(),
            ),
            ScalarHazard::UnderscoreNumber => (
                LintCode::Yaml11Number,
                bounded(format_args!("string ({})", JsonEscaped(raw)), MAX_FIELD),
                "YAML 1.1 underscore-grouped numeric literal".to_owned(),
            ),
        };
        self.commit_retained(
            code,
            span,
            doc_index,
            path,
            raw,
            yaml12_resolution,
            yaml11_note,
            opts,
            budget,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_retained(
        &mut self,
        code: LintCode,
        span: Span,
        doc_index: usize,
        path: &mut Vec<PathSegment>,
        raw_input: &str,
        yaml12_resolution: String,
        yaml11_note: String,
        opts: &Options,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        // Step 2: Format bounded diagnostic strings and charge the diagnostic itself before cloning path.
        let raw = bounded(raw_input, MAX_FIELD);
        let message = bounded(
            format_args!(
                "YAML 1.1 compatibility hazard ({}): unquoted {} resolves as {} in YAML 1.2 Core ({})",
                code.as_str(),
                JsonEscaped(raw_input),
                yaml12_resolution,
                yaml11_note
            ),
            MAX_MESSAGE,
        );
        let diag_weight = 64u64
            .saturating_add(8u64.saturating_mul(path.len() as u64))
            .saturating_add(raw.len() as u64)
            .saturating_add(yaml12_resolution.len() as u64)
            .saturating_add(yaml11_note.len() as u64)
            .saturating_add(message.len() as u64);
        if let Err((kind, configured)) = budget.charge(&opts.limits, 1, diag_weight, false) {
            let mut e = limit_err(kind, configured, span.start);
            e.path = std::mem::take(path);
            return Err(e);
        }

        #[cfg(test)]
        lint_admission_tests::path_clone_started();

        let cloned_path = clone_lint_path(path);
        self.result.diagnostics.push(LintDiagnostic {
            code,
            span,
            document_index: doc_index,
            path: cloned_path,
            raw,
            yaml12_resolution,
            yaml11_note,
            message,
        });
        Ok(())
    }
}

fn measure_path_key(k: &Value, l: &Limits) -> Result<(u64, u64), LimitKind> {
    let mut nodes = 1u64;
    let mut weight = NODE_OVERHEAD;
    let mut stack: Vec<(&Value, usize)> = vec![(k, 0)];
    while let Some((v, depth)) = stack.pop() {
        let (more_nodes, more_weight, children): (u64, u64, usize) = match v {
            Value::String(t) => (0, t.len() as u64, 0),
            Value::Tagged(t) => (1, tag_bytes(&t.tag) + NODE_OVERHEAD, 1),
            Value::Sequence(items) => (
                items.len() as u64,
                NODE_OVERHEAD.saturating_mul(items.len() as u64),
                items.len(),
            ),
            Value::Mapping(m) => (
                2u64.saturating_mul(m.len() as u64),
                (2 * NODE_OVERHEAD + ENTRY_OVERHEAD).saturating_mul(m.len() as u64),
                2 * m.len(),
            ),
            _ => (0, 0, 0),
        };
        nodes = nodes.saturating_add(more_nodes);
        weight = weight.saturating_add(more_weight);
        if weight > l.max_key_bytes as u64 {
            return Err(LimitKind::MaxKeyBytes);
        }
        if children > 0 && !matches!(v, Value::Tagged(_)) && depth + 1 > l.max_depth {
            return Err(LimitKind::MaxDepth);
        }
        match v {
            Value::Tagged(t) => stack.push((&t.value.value, depth)),
            Value::Sequence(items) => stack.extend(items.iter().map(|n| (&n.value, depth + 1))),
            Value::Mapping(m) => {
                for (mk, mv) in m.iter() {
                    stack.push((&mk.value, depth + 1));
                    stack.push((&mv.value, depth + 1));
                }
            }
            _ => {}
        }
    }
    Ok((nodes, weight))
}

fn clone_lint_path(path: &[PathSegment]) -> Vec<PathSegment> {
    path.iter()
        .map(|seg| match seg {
            PathSegment::Index(i) => PathSegment::Index(*i),
            PathSegment::Key(v) => {
                #[cfg(test)]
                record_test_key_clone();
                PathSegment::Key(Box::new(strip_value_locations(v)))
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn record_test_key_clone() {
    lint_admission_tests::key_clone_started();
}

/// Clone `Value` with all nested `Node` spans, `via`, and `tag_start` stripped (`NodeSpan::Synthetic`, `extra: None`).
pub(crate) fn strip_value_locations(root: &Value) -> Value {
    match root {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Bool(*b),
        Value::Int(i) => Value::Int(*i),
        Value::Float(f) => Value::Float(*f),
        Value::String(s) => Value::String(s.clone()),
        _ => {
            let synth_root = Node {
                value: match root {
                    Value::Sequence(items) => {
                        return Value::Sequence(items.iter().map(strip_node_locations).collect())
                    }
                    Value::Mapping(m) => {
                        let mut out = Mapping::new();
                        for (k, v) in &m.entries {
                            let _ =
                                out.insert_new(strip_node_locations(k), strip_node_locations(v));
                        }
                        return Value::Mapping(out);
                    }
                    Value::Tagged(t) => {
                        return Value::Tagged(Box::new(Tagged {
                            tag: t.tag.clone(),
                            value: strip_node_locations(&t.value),
                            inner_plain: t.inner_plain,
                        }))
                    }
                    _ => Value::Null,
                },
                span: NodeSpan::Synthetic,
                extra: None,
            };
            synth_root.into_value()
        }
    }
}

fn strip_node_locations(root: &Node) -> Node {
    enum Work<'a> {
        Node(&'a Node),
        FinishSeq(usize),
        FinishMap(usize),
        FinishTagged {
            tag: crate::value::Tag,
            inner_plain: bool,
        },
    }
    let mut work: Vec<Work<'_>> = vec![Work::Node(root)];
    let mut results: Vec<Node> = Vec::new();
    while let Some(step) = work.pop() {
        match step {
            Work::Node(n) => match &n.value {
                Value::Null => results.push(Node::synthetic(Value::Null)),
                Value::Bool(b) => results.push(Node::synthetic(Value::Bool(*b))),
                Value::Int(i) => results.push(Node::synthetic(Value::Int(*i))),
                Value::Float(f) => results.push(Node::synthetic(Value::Float(*f))),
                Value::String(s) => results.push(Node::synthetic(Value::String(s.clone()))),
                Value::Sequence(items) => {
                    work.push(Work::FinishSeq(items.len()));
                    for item in items.iter().rev() {
                        work.push(Work::Node(item));
                    }
                }
                Value::Mapping(m) => {
                    work.push(Work::FinishMap(m.entries.len()));
                    for (k, v) in m.entries.iter().rev() {
                        work.push(Work::Node(v));
                        work.push(Work::Node(k));
                    }
                }
                Value::Tagged(t) => {
                    work.push(Work::FinishTagged {
                        tag: t.tag.clone(),
                        inner_plain: t.inner_plain,
                    });
                    work.push(Work::Node(&t.value));
                }
            },
            Work::FinishSeq(len) => {
                let items = results.split_off(results.len() - len);
                results.push(Node::synthetic(Value::Sequence(items)));
            }
            Work::FinishMap(len) => {
                let tail = results.split_off(results.len() - 2 * len);
                let mut iter = tail.into_iter();
                let mut map = Mapping::new();
                while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
                    let _ = map.insert_new(k, v);
                }
                results.push(Node::synthetic(Value::Mapping(map)));
            }
            Work::FinishTagged { tag, inner_plain } => {
                let value = results.pop().expect("tagged inner node");
                results.push(Node::synthetic(Value::Tagged(Box::new(Tagged {
                    tag,
                    value,
                    inner_plain,
                }))));
            }
        }
    }
    results.pop().expect("stripped root node")
}

/// Inspect a YAML stream for unquoted YAML 1.1 compatibility hazards under default [`Options`]
/// (`spec-v1.6` §10.3).
pub fn lint_yaml11(input: &str) -> Result<LintResult, Error> {
    lint_yaml11_with(input, &Options::default())
}

/// Inspect a YAML stream for unquoted YAML 1.1 compatibility hazards with explicit [`Options`]
/// (`spec-v1.6` §10.3).
pub fn lint_yaml11_with(input: &str, opts: &Options) -> Result<LintResult, Error> {
    opts.validate()?;
    let (s, bom) = gate_str(input, opts)?;
    crate::build::Loader::new_lint(s, bom, opts).lint_stream()
}

/// Inspect raw bytes of a YAML stream for unquoted YAML 1.1 compatibility hazards (`spec-v1.6` §10.3).
pub fn lint_yaml11_bytes(input: &[u8], opts: &Options) -> Result<LintResult, Error> {
    opts.validate()?;
    let (s, bom) = gate_bytes(input, opts)?;
    crate::build::Loader::new_lint(s, bom, opts).lint_stream()
}

/// Read a YAML stream from `reader` and inspect it for unquoted YAML 1.1 compatibility hazards
/// under default [`Options`] (`spec-v1.6` §10.3).
pub fn lint_yaml11_reader<R: std::io::Read>(reader: R) -> Result<LintResult, Error> {
    lint_yaml11_reader_with(reader, &Options::default())
}

/// Read a YAML stream from `reader` and inspect it for unquoted YAML 1.1 compatibility hazards
/// with explicit [`Options`] (`spec-v1.6` §10.3).
pub fn lint_yaml11_reader_with<R: std::io::Read>(
    reader: R,
    opts: &Options,
) -> Result<LintResult, Error> {
    let bytes = read_bounded(reader, opts)?;
    lint_yaml11_bytes(&bytes, opts)
}

fn validate_loc(loc: &Location) -> bool {
    (loc.byte_offset as u64) <= MAX_WIRE_META_UINT
        && loc.line >= 1
        && (loc.line as u64) <= MAX_WIRE_META_UINT
        && loc.column >= 1
        && (loc.column as u64) <= MAX_WIRE_META_UINT
}

fn validate_span(span: &Span) -> bool {
    validate_loc(&span.start)
        && validate_loc(&span.end)
        && span.start.byte_offset <= span.end.byte_offset
        && (span.start.line < span.end.line
            || (span.start.line == span.end.line && span.start.column <= span.end.column))
}

/// Encode a [`LintResult`] to a canonical `yamlprims-lint/1` JSON line (`spec-v1.6` §11.7).
pub fn encode_lint(result: &LintResult, opts: &Options) -> Result<String, Error> {
    opts.validate()?;
    let invalid = |msg: &str| Error::not_source(ErrorKind::InvalidOptions, None, msg);
    if result.diagnostics.len() > opts.limits.max_warnings {
        return Err(invalid(
            "LintResult diagnostics length exceeds max_warnings",
        ));
    }
    if result.truncated {
        if result.warning_count <= result.diagnostics.len() {
            return Err(invalid(
                "truncated LintResult must have warning_count > diagnostics.len()",
            ));
        }
    } else if result.warning_count != result.diagnostics.len() {
        return Err(invalid(
            "non-truncated LintResult must have warning_count == diagnostics.len()",
        ));
    }
    if (result.warning_count as u64) > MAX_WIRE_META_UINT {
        return Err(invalid("LintResult warning_count exceeds 2^53 - 1"));
    }
    for d in &result.diagnostics {
        if (d.document_index as u64) > MAX_WIRE_META_UINT {
            return Err(invalid("LintDiagnostic document_index exceeds 2^53 - 1"));
        }
        if !validate_span(&d.span) {
            return Err(invalid("LintDiagnostic span has invalid coordinates"));
        }
        for seg in &d.path {
            if let PathSegment::Index(i) = seg {
                if (*i as u64) > MAX_WIRE_META_UINT {
                    return Err(invalid("LintDiagnostic path index exceeds 2^53 - 1"));
                }
            }
        }
        preflight_path(&d.path, &opts.limits)?;
    }

    let mut o = Out::new(opts.limits.max_output_bytes);
    o.push("{\"format\":\"yamlprims-lint\",\"version\":1,\"warning_count\":")?;
    o.num(result.warning_count)?;
    o.push(if result.truncated {
        ",\"truncated\":true,\"diagnostics\":["
    } else {
        ",\"truncated\":false,\"diagnostics\":["
    })?;
    for (i, d) in result.diagnostics.iter().enumerate() {
        if i > 0 {
            o.push(",")?;
        }
        o.push("{\"code\":")?;
        o.string(d.code.as_str())?;
        o.push(",\"span\":")?;
        o.span(&d.span)?;
        o.push(",\"document_index\":")?;
        o.num(d.document_index)?;
        o.push(",\"path\":[")?;
        write_path_segments(&mut o, &d.path)?;
        o.push("],\"raw\":")?;
        o.bounded_string(&d.raw, MAX_FIELD)?;
        o.push(",\"yaml12_resolution\":")?;
        o.bounded_string(&d.yaml12_resolution, MAX_FIELD)?;
        o.push(",\"yaml11_note\":")?;
        o.bounded_string(&d.yaml11_note, MAX_FIELD)?;
        o.push(",\"message\":")?;
        o.bounded_string(&d.message, MAX_MESSAGE)?;
        o.push("}")?;
    }
    o.push("]}")?;
    Ok(o.into_string())
}

#[cfg(test)]
mod lint_admission_tests {
    use super::*;
    use std::cell::Cell;

    std::thread_local! {
        static PATH_CLONES: Cell<Option<usize>> = const { Cell::new(None) };
        static KEY_CLONES: Cell<Option<usize>> = const { Cell::new(None) };
    }

    pub(super) fn path_clone_started() {
        PATH_CLONES.with(|clones| {
            if let Some(n) = clones.get() {
                clones.set(Some(n + 1));
            }
        });
    }

    pub(super) fn key_clone_started() {
        KEY_CLONES.with(|clones| {
            if let Some(n) = clones.get() {
                clones.set(Some(n + 1));
            }
        });
    }

    #[test]
    fn diagnostic_budget_rejects_before_cloning_path() {
        let yaml = "outer_key_with_bytes:\n  inner_key_with_bytes:\n    - yes\n    - no\n";
        let mut opts = Options::default();
        // Enough owned bytes for the document tree and first diagnostic, but not the second diagnostic's path clone.
        opts.limits.max_owned_bytes = 900;
        opts.limits.max_scalar_bytes = 256;
        opts.limits.max_key_bytes = 256;
        PATH_CLONES.with(|c| c.set(Some(0)));
        KEY_CLONES.with(|c| c.set(Some(0)));
        let err = lint_yaml11_with(yaml, &opts).unwrap_err();
        let clones = PATH_CLONES.with(|c| c.replace(None).unwrap());
        let key_clones = KEY_CLONES.with(|c| c.replace(None).unwrap());
        assert_eq!(
            err.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 900,
            }
        );
        // Only the first diagnostic cloned its path (2 keys); the second aborted during Step 1 key-charge and moved `self.path` into `err.path` with zero key clones.
        assert_eq!(clones, 1);
        assert_eq!(key_clones, 2);
        let expected_path = vec![
            PathSegment::Key(Box::new(Value::String("outer_key_with_bytes".into()))),
            PathSegment::Key(Box::new(Value::String("inner_key_with_bytes".into()))),
            PathSegment::Index(1),
        ];
        assert_eq!(err.path, expected_path);

        // Also verify Step 2 (commit_retained diagnostic weight charge) rejection moves `self.path` with zero key clones.
        let mut opts_step2 = Options::default();
        opts_step2.limits.max_owned_bytes = 980;
        opts_step2.limits.max_scalar_bytes = 256;
        opts_step2.limits.max_key_bytes = 256;
        PATH_CLONES.with(|c| c.set(Some(0)));
        KEY_CLONES.with(|c| c.set(Some(0)));
        let err_step2 = lint_yaml11_with(yaml, &opts_step2).unwrap_err();
        let step2_clones = PATH_CLONES.with(|c| c.replace(None).unwrap());
        let step2_key_clones = KEY_CLONES.with(|c| c.replace(None).unwrap());
        assert_eq!(
            err_step2.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 980,
            }
        );
        assert_eq!(step2_clones, 1);
        assert_eq!(step2_key_clones, 2);
        assert_eq!(err_step2.path, expected_path);

        // Composite mapping key on path: Step 1 key-charge rejection moves `self.path` with zero key clones.
        let yaml_comp = "? {k1: v1, k2: v2}\n: [yes, no]\n";
        let mut opts_comp = Options::default();
        opts_comp.limits.max_owned_bytes = 1500;
        opts_comp.limits.max_scalar_bytes = 256;
        opts_comp.limits.max_key_bytes = 512;
        PATH_CLONES.with(|c| c.set(Some(0)));
        KEY_CLONES.with(|c| c.set(Some(0)));
        let err_comp = lint_yaml11_with(yaml_comp, &opts_comp).unwrap_err();
        let comp_path_clones = PATH_CLONES.with(|c| c.replace(None).unwrap());
        let comp_key_clones = KEY_CLONES.with(|c| c.replace(None).unwrap());
        assert_eq!(
            err_comp.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxOwnedBytes,
                configured: 1500,
            }
        );
        // First diagnostic cloned 1 composite key; second diagnostic failed during Step 1 key-charge with zero additional key clones.
        assert_eq!(comp_path_clones, 1);
        assert_eq!(comp_key_clones, 1);
        assert_eq!(err_comp.path.len(), 2);
        assert!(
            matches!(&err_comp.path[0], PathSegment::Key(v) if matches!(**v, Value::Mapping(_)))
        );
        assert_eq!(err_comp.path[1], PathSegment::Index(1));
    }
}
