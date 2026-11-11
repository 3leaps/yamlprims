//! Wire format `yamlprims-value/1` (spec §11.1) and plain JSON (§11.3).

use crate::budget::{metrics, tag_bytes, ENTRY_OVERHEAD, NODE_OVERHEAD};
use crate::error::{
    Error, ErrorKind, LimitKind, PathSegment, Related, RelatedRole, Site, SyntaxKind, MAX_FIELD,
    MAX_MESSAGE,
};
use crate::options::{Limits, Options};
use crate::value::{
    Document, Integer, Location, Mapping, Node, NodeSpan, Span, Tag, Tagged, Value, Via,
};
use std::collections::HashSet;
use std::fmt::Write as _;

const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;
const MAX_WIRE_META_UINT: u64 = 9_007_199_254_740_991;

/// Decoded `yamlprims-value/1` wire result envelope (spec-v1.5 §10.1, §11.1).
#[derive(Clone, Debug)]
pub enum WireEnvelope {
    Documents(Vec<Document>),
    Error(Error),
}

/// A fixed buffer for one `\\uXXXX` escape.
struct Slice<'a>(&'a mut [u8; 6], usize);
impl std::fmt::Write for Slice<'_> {
    fn write_str(&mut self, t: &str) -> std::fmt::Result {
        let end = self.1 + t.len();
        self.0
            .get_mut(self.1..end)
            .ok_or(std::fmt::Error)?
            .copy_from_slice(t.as_bytes());
        self.1 = end;
        Ok(())
    }
}

pub(crate) struct Out {
    s: String,
    max: usize,
}

impl Out {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            s: String::new(),
            max,
        }
    }
    pub(crate) fn into_string(self) -> String {
        self.s
    }
    pub(crate) fn push(&mut self, t: &str) -> Result<(), Error> {
        if self.s.len() + t.len() > self.max {
            return self.push_over();
        }
        self.s.push_str(t);
        Ok(())
    }
    fn num(&mut self, n: usize) -> Result<(), Error> {
        self.push(&n.to_string())
    }
    pub(crate) fn string(&mut self, v: &str) -> Result<(), Error> {
        self.string_parts(v, "")
    }
    /// A JSON string of `v`, cut on a character boundary with `…` if it is longer than `max`
    /// bytes (descriptive text only; never structured identity).
    fn bounded_string(&mut self, v: &str, max: usize) -> Result<(), Error> {
        if v.len() <= max {
            return self.string(v);
        }
        let mut i = max.saturating_sub('…'.len_utf8());
        while !v.is_char_boundary(i) {
            i -= 1;
        }
        self.string_parts(&v[..i], "…")
    }
    /// Escape straight into the output, checking the budget as it goes: no escaped
    /// temporary copy is built first. Escaping never shrinks text, so an input that cannot
    /// fit fails before any work.
    fn string_parts(&mut self, a: &str, b: &str) -> Result<(), Error> {
        if self.s.len().saturating_add(a.len() + b.len() + 2) > self.max {
            return self.push_over();
        }
        self.push("\"")?;
        for v in [a, b] {
            let mut run = 0;
            for (i, c) in v.char_indices() {
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
                self.push(&v[run..i])?;
                if esc.is_empty() {
                    let mut u = [0u8; 6];
                    let _ = write!(Slice(&mut u, 0), "\\u{:04x}", c as u32);
                    self.push(std::str::from_utf8(&u).expect("ASCII escape"))?;
                } else {
                    self.push(esc)?;
                }
                run = i + c.len_utf8();
            }
            self.push(&v[run..])?;
        }
        self.push("\"")
    }
    fn push_over(&self) -> Result<(), Error> {
        Err(Error::not_source(
            ErrorKind::OutputLimitExceeded {
                configured: self.max as u64,
            },
            None,
            format_args!("encoded output exceeds max_output_bytes ({})", self.max),
        ))
    }
    fn loc(&mut self, l: &Location) -> Result<(), Error> {
        self.push(&format!("[{},{},{}]", l.byte_offset, l.line, l.column))
    }
    fn span(&mut self, s: &Span) -> Result<(), Error> {
        self.push(&format!(
            "[{},{},{},{},{},{}]",
            s.start.byte_offset,
            s.start.line,
            s.start.column,
            s.end.byte_offset,
            s.end.line,
            s.end.column
        ))
    }
}

pub(crate) fn float_bits(f: f64) -> String {
    let b = if f.is_nan() {
        CANONICAL_NAN
    } else {
        f.to_bits()
    };
    format!("{b:016x}")
}

enum WireWriteFrame<'a> {
    Node(&'a Node),
    ValueOnly(&'a Value),
    FinishNode(Option<&'a Node>),
    Raw(&'static str),
    Seq {
        items: &'a [Node],
        idx: usize,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
    },
}

fn finish_node_locations(o: &mut Out, n: Option<&Node>, with_spans: bool) -> Result<(), Error> {
    let Some(n) = n else {
        return o.push(",\"span\":null,\"via\":null,\"tag_start\":null}");
    };
    o.push(",\"span\":")?;
    match (&n.span, with_spans) {
        (NodeSpan::Source(s), true) => o.span(s)?,
        _ => o.push("null")?,
    }
    o.push(",\"via\":")?;
    match (n.via(), with_spans) {
        (Some(Via::Alias { use_site }), true) => {
            o.push("{\"alias\":")?;
            o.span(use_site)?;
            o.push("}")?
        }
        (Some(Via::Merge { directive }), true) => {
            o.push("{\"merge\":")?;
            o.span(directive)?;
            o.push("}")?
        }
        _ => o.push("null")?,
    }
    o.push(",\"tag_start\":")?;
    match (n.tag_start(), with_spans) {
        (Some(l), true) => o.loc(&l)?,
        _ => o.push("null")?,
    }
    o.push("}")
}

fn run_wire_write_frames<'a>(
    o: &mut Out,
    mut stack: Vec<WireWriteFrame<'a>>,
    with_spans: bool,
) -> Result<(), Error> {
    while let Some(frame) = stack.pop() {
        match frame {
            WireWriteFrame::Raw(s) => o.push(s)?,
            WireWriteFrame::FinishNode(n) => finish_node_locations(o, n, with_spans)?,
            WireWriteFrame::Node(n) => {
                stack.push(WireWriteFrame::FinishNode(Some(n)));
                stack.push(WireWriteFrame::ValueOnly(&n.value));
            }
            WireWriteFrame::ValueOnly(v) => {
                o.push("{\"kind\":")?;
                o.string(v.kind_name())?;
                match v {
                    Value::Null => {}
                    Value::Bool(b) => o.push(if *b {
                        ",\"value\":true"
                    } else {
                        ",\"value\":false"
                    })?,
                    Value::Int(i) => {
                        o.push(",\"value\":")?;
                        o.string(&i.to_string())?;
                    }
                    Value::Float(f) => {
                        o.push(",\"bits\":")?;
                        o.string(&float_bits(*f))?;
                    }
                    Value::String(s) => {
                        o.push(",\"value\":")?;
                        o.string(s)?;
                    }
                    Value::Sequence(items) => {
                        o.push(",\"items\":[")?;
                        stack.push(WireWriteFrame::Seq { items, idx: 0 });
                    }
                    Value::Mapping(m) => {
                        o.push(",\"entries\":[")?;
                        stack.push(WireWriteFrame::Map {
                            entries: &m.entries,
                            idx: 0,
                        });
                    }
                    Value::Tagged(t) => {
                        o.push(",\"tag\":{\"uri\":")?;
                        o.string(&t.tag.uri)?;
                        o.push(",\"handle\":")?;
                        o.string(&t.tag.handle)?;
                        o.push(",\"suffix\":")?;
                        o.string(&t.tag.suffix)?;
                        o.push("},\"value\":")?;
                        stack.push(WireWriteFrame::Node(&t.value));
                    }
                }
            }
            WireWriteFrame::Seq { items, idx } => {
                if idx >= items.len() {
                    o.push("]")?;
                } else {
                    if idx > 0 {
                        o.push(",")?;
                    }
                    stack.push(WireWriteFrame::Seq {
                        items,
                        idx: idx + 1,
                    });
                    stack.push(WireWriteFrame::Node(&items[idx]));
                }
            }
            WireWriteFrame::Map { entries, idx } => {
                if idx >= entries.len() {
                    o.push("]")?;
                } else {
                    if idx > 0 {
                        o.push(",")?;
                    }
                    o.push("{\"key\":")?;
                    let (k, v) = &entries[idx];
                    stack.push(WireWriteFrame::Map {
                        entries,
                        idx: idx + 1,
                    });
                    stack.push(WireWriteFrame::Raw("}"));
                    stack.push(WireWriteFrame::Node(v));
                    stack.push(WireWriteFrame::Raw(",\"value\":"));
                    stack.push(WireWriteFrame::Node(k));
                }
            }
        }
    }
    Ok(())
}

fn node(o: &mut Out, n: &Node, with_spans: bool) -> Result<(), Error> {
    run_wire_write_frames(o, vec![WireWriteFrame::Node(n)], with_spans)
}

/// A node's `kind` and content members, without its closing location members.
fn value(o: &mut Out, v: &Value, with_spans: bool) -> Result<(), Error> {
    run_wire_write_frames(o, vec![WireWriteFrame::ValueOnly(v)], with_spans)
}

pub(crate) fn encode_documents(docs: &[Document], max: usize) -> Result<String, Error> {
    let mut o = Out {
        s: String::new(),
        max,
    };
    o.push("{\"format\":\"yamlprims-value\",\"version\":1,\"documents\":[")?;
    for (i, d) in docs.iter().enumerate() {
        if i > 0 {
            o.push(",")?;
        }
        o.push("{\"yaml_version\":")?;
        match d.yaml_version {
            Some((a, b)) => o.push(&format!("[{a},{b}]"))?,
            None => o.push("null")?,
        }
        o.push(if d.explicit_start {
            ",\"explicit_start\":true,\"root\":"
        } else {
            ",\"explicit_start\":false,\"root\":"
        })?;
        node(&mut o, &d.root, true)?;
        o.push("}")?;
    }
    o.push("]}")?;
    Ok(o.s)
}

/// Check an error against the current limits before encoding it (spec §§6, 7.1, 10 and 11.1). Its fields are public and mutable, so nothing is trusted from construction.
/// Structured identity (the path) is rejected when out of bounds, never cut: more segments
/// than `max_depth`, or a key whose nesting exceeds `max_depth` or whose §6 weight exceeds
/// `max_key_bytes`, is `LimitExceeded` with `Site::NotSource { last_known: None }`. A
/// `syntax_kind` that is not a snake_case code is an invalid kind/detail pair: `Internal`.
/// Descriptive text (message, type descriptions, field names) is truncated while writing.
/// The walk is iterative and stops at the first bound, so its work is bounded too.
fn preflight(e: &Error, l: &Limits) -> Result<(), Error> {
    let limit = |limit: LimitKind, configured: usize, what: std::fmt::Arguments<'_>| {
        Error::not_source(
            ErrorKind::LimitExceeded {
                limit,
                configured: configured as u64,
            },
            None,
            what,
        )
    };
    if e.path.len() > l.max_depth {
        return Err(limit(
            LimitKind::MaxDepth,
            l.max_depth,
            format_args!(
                "error path has {} segments, more than max_depth",
                e.path.len()
            ),
        ));
    }
    for seg in &e.path {
        let PathSegment::Key(k) = seg else { continue };
        // Each node is charged when pushed, so the stack never holds more than the key's
        // weight budget allows.
        let over = || {
            limit(
                LimitKind::MaxKeyBytes,
                l.max_key_bytes,
                format_args!("error path key exceeds max_key_bytes"),
            )
        };
        let mut weight = NODE_OVERHEAD;
        let mut stack: Vec<(&Value, usize)> = vec![(k, 0)];
        while let Some((v, depth)) = stack.pop() {
            let (more, children): (u64, usize) = match v {
                Value::String(t) => (t.len() as u64, 0),
                Value::Tagged(t) => (tag_bytes(&t.tag) + NODE_OVERHEAD, 1),
                Value::Sequence(items) => (
                    NODE_OVERHEAD.saturating_mul(items.len() as u64),
                    items.len(),
                ),
                Value::Mapping(m) => (
                    (2 * NODE_OVERHEAD + ENTRY_OVERHEAD).saturating_mul(m.len() as u64),
                    2 * m.len(),
                ),
                _ => (0, 0),
            };
            weight = weight.saturating_add(more);
            if weight > l.max_key_bytes as u64 {
                return Err(over());
            }
            if children > 0 && !matches!(v, Value::Tagged(_)) && depth + 1 > l.max_depth {
                return Err(limit(
                    LimitKind::MaxDepth,
                    l.max_depth,
                    format_args!("error path key nests deeper than max_depth"),
                ));
            }
            match v {
                // A tag wrapper does not add depth (as in `from_node`).
                Value::Tagged(t) => stack.push((&t.value.value, depth)),
                Value::Sequence(items) => stack.extend(items.iter().map(|n| (&n.value, depth + 1))),
                Value::Mapping(m) => {
                    for (k, v) in m.iter() {
                        stack.push((&k.value, depth + 1));
                        stack.push((&v.value, depth + 1));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub(crate) fn encode_error(e: &Error, l: &Limits) -> Result<String, Error> {
    preflight(e, l)?;
    let mut o = Out {
        s: String::new(),
        max: l.max_output_bytes,
    };
    o.push("{\"format\":\"yamlprims-value\",\"version\":1,\"error\":{\"kind\":")?;
    o.string(e.kind.name())?;
    o.push(",\"detail\":")?;
    match &e.kind {
        ErrorKind::InvalidUtf8 { invalid_len } => match invalid_len {
            Some(n) => o.push(&format!("{{\"invalid_len\":{n}}}"))?,
            None => o.push("{\"invalid_len\":null}")?,
        },
        ErrorKind::Syntax(k) => {
            o.push("{\"syntax_kind\":")?;
            o.string(k.as_str())?;
            o.push("}")?
        }
        ErrorKind::LimitExceeded { limit, configured } => {
            o.push("{\"limit\":")?;
            o.string(limit.field_name())?;
            o.push(&format!(",\"configured\":{configured}}}"))?
        }
        ErrorKind::Type { expected, found } => {
            o.push("{\"expected\":")?;
            o.bounded_string(expected, MAX_FIELD)?;
            o.push(",\"found\":")?;
            o.bounded_string(found, MAX_FIELD)?;
            o.push("}")?
        }
        ErrorKind::MissingField(f) | ErrorKind::UnknownField(f) => {
            o.push("{\"field\":")?;
            o.bounded_string(f, MAX_FIELD)?;
            o.push("}")?
        }
        ErrorKind::OutputLimitExceeded { configured } => {
            o.push(&format!("{{\"configured\":{configured}}}"))?
        }
        _ => o.push("{}")?,
    }
    o.push(",\"site\":")?;
    match &e.site {
        Site::Source(l) => {
            o.push("{\"source\":")?;
            o.loc(l)?;
            o.push("}")?
        }
        Site::NotSource { last_known } => {
            o.push("{\"not_source\":{\"last_known\":")?;
            match last_known {
                Some(l) => o.loc(l)?,
                None => o.push("null")?,
            }
            o.push("}}")?
        }
    }
    o.push(",\"related\":[")?;
    for (i, r) in e.related.iter().enumerate() {
        if i > 0 {
            o.push(",")?;
        }
        o.push("{\"role\":")?;
        o.string(r.role.name())?;
        o.push(",\"span\":")?;
        o.span(&r.span)?;
        o.push("}")?;
    }
    o.push("],\"path\":[")?;
    for (i, p) in e.path.iter().enumerate() {
        if i > 0 {
            o.push(",")?;
        }
        match p {
            PathSegment::Index(n) => {
                o.push("{\"index\":")?;
                o.num(*n)?;
                o.push("}")?
            }
            PathSegment::Key(k) => {
                o.push("{\"key\":")?;
                // A PathKey: the key's value with null locations, written without a copy.
                value(&mut o, k, false)?;
                o.push(",\"span\":null,\"via\":null,\"tag_start\":null}}")?
            }
        }
    }
    o.push("],\"message\":")?;
    o.bounded_string(&e.message, MAX_MESSAGE)?;
    o.push("}}")?;
    Ok(o.s)
}

/// Plain JSON text; fails instead of losing information (§11.3).
pub(crate) fn encode_plain(n: &Node, max: usize) -> Result<String, Error> {
    let mut o = Out {
        s: String::new(),
        max,
    };
    plain(&mut o, n)?;
    Ok(o.s)
}

fn loss(n: &Node, what: &str) -> Error {
    let loc = match n.span {
        NodeSpan::Source(s) => Some(s.start),
        NodeSpan::Synthetic => None,
    };
    Error::not_source(
        ErrorKind::Type {
            expected: "plain JSON value".into(),
            found: what.into(),
        },
        loc,
        format!("cannot write {what} as plain JSON without loss"),
    )
}

enum PlainWriteFrame<'a> {
    Node(&'a Node),
    Seq {
        items: &'a [Node],
        idx: usize,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
    },
}

fn plain<'a>(o: &mut Out, root: &'a Node) -> Result<(), Error> {
    let mut stack = vec![PlainWriteFrame::Node(root)];
    while let Some(frame) = stack.pop() {
        match frame {
            PlainWriteFrame::Node(n) => match &n.value {
                Value::Null => o.push("null")?,
                Value::Bool(b) => o.push(if *b { "true" } else { "false" })?,
                Value::Int(i) => o.push(&i.to_string())?,
                Value::Float(f) => {
                    if !f.is_finite() {
                        return Err(loss(n, "non-finite float"));
                    }
                    let mut s = format!("{f:?}");
                    if !s.contains(['.', 'e', 'E']) {
                        s.push_str(".0");
                    }
                    o.push(&s)?;
                }
                Value::String(s) => o.string(s)?,
                Value::Sequence(items) => {
                    o.push("[")?;
                    stack.push(PlainWriteFrame::Seq { items, idx: 0 });
                }
                Value::Mapping(m) => {
                    o.push("{")?;
                    stack.push(PlainWriteFrame::Map {
                        entries: &m.entries,
                        idx: 0,
                    });
                }
                Value::Tagged(_) => return Err(loss(n, "tagged value")),
            },
            PlainWriteFrame::Seq { items, idx } => {
                if idx >= items.len() {
                    o.push("]")?;
                } else {
                    if idx > 0 {
                        o.push(",")?;
                    }
                    stack.push(PlainWriteFrame::Seq {
                        items,
                        idx: idx + 1,
                    });
                    stack.push(PlainWriteFrame::Node(&items[idx]));
                }
            }
            PlainWriteFrame::Map { entries, idx } => {
                if idx >= entries.len() {
                    o.push("}")?;
                } else {
                    let (k, v) = &entries[idx];
                    let Value::String(ks) = &k.value else {
                        return Err(loss(k, "non-string key"));
                    };
                    if idx > 0 {
                        o.push(",")?;
                    }
                    o.string(ks)?;
                    o.push(":")?;
                    stack.push(PlainWriteFrame::Map {
                        entries,
                        idx: idx + 1,
                    });
                    stack.push(PlainWriteFrame::Node(v));
                }
            }
        }
    }
    Ok(())
}

/// Strictly validate and decode a `yamlprims-value/1` JSON wire envelope (spec-v1.5 §11.1).
pub fn decode_wire(wire_json: &str, opts: &Options) -> Result<WireEnvelope, Error> {
    opts.validate()?;
    if wire_json.len() > opts.limits.max_input_bytes {
        return Err(wire_limit_err(
            LimitKind::MaxInputBytes,
            opts.limits.max_input_bytes,
        ));
    }
    let mut parser = WireParser {
        src: wire_json,
        bytes: wire_json.as_bytes(),
        pos: 0,
        limits: &opts.limits,
        nodes_used: 0,
        owned_bytes: 0,
    };
    parser.parse_envelope()
}

fn wire_limit_err(limit: LimitKind, configured: usize) -> Error {
    Error::not_source(
        ErrorKind::LimitExceeded {
            limit,
            configured: configured as u64,
        },
        None,
        format_args!("wire payload exceeds {} ({configured})", limit.field_name()),
    )
}

fn wire_internal_err(msg: impl std::fmt::Display) -> Error {
    Error::not_source(ErrorKind::Internal, None, msg)
}

fn parse_canonical_wire_int(s: &str) -> Option<Integer> {
    if s.is_empty() {
        return None;
    }
    if s == "0" {
        return Some(Integer::from_u128(0));
    }
    if let Some(digits) = s.strip_prefix('-') {
        let first = *digits.as_bytes().first()?;
        if !first.is_ascii_digit() || first == b'0' || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mag: u128 = digits.parse().ok()?;
        let max_neg = 1u128 << 127;
        if mag > max_neg {
            return None;
        }
        if mag == max_neg {
            Some(Integer::from_i128(i128::MIN))
        } else {
            Some(Integer::from_i128(-(mag as i128)))
        }
    } else {
        let first = *s.as_bytes().first()?;
        if !first.is_ascii_digit() || first == b'0' || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mag: u128 = s.parse().ok()?;
        Some(Integer::from_u128(mag))
    }
}

fn parse_lower_hex16_float(s: &str) -> Option<f64> {
    if s.len() != 16 {
        return None;
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let bits = u64::from_str_radix(s, 16).ok()?;
    let f = f64::from_bits(bits);
    if f.is_nan() {
        Some(f64::from_bits(CANONICAL_NAN))
    } else {
        Some(f)
    }
}

fn parse_limit_kind(code: &str) -> Option<LimitKind> {
    match code {
        "max_input_bytes" => Some(LimitKind::MaxInputBytes),
        "max_depth" => Some(LimitKind::MaxDepth),
        "max_nodes" => Some(LimitKind::MaxNodes),
        "max_expanded_nodes" => Some(LimitKind::MaxExpandedNodes),
        "max_alias_events" => Some(LimitKind::MaxAliasEvents),
        "max_owned_bytes" => Some(LimitKind::MaxOwnedBytes),
        "max_scalar_bytes" => Some(LimitKind::MaxScalarBytes),
        "max_key_bytes" => Some(LimitKind::MaxKeyBytes),
        "max_mapping_entries" => Some(LimitKind::MaxMappingEntries),
        "max_documents" => Some(LimitKind::MaxDocuments),
        _ => None,
    }
}

fn parse_related_role(code: &str) -> Option<RelatedRole> {
    match code {
        "first_definition" => Some(RelatedRole::FirstDefinition),
        "alias_use" => Some(RelatedRole::AliasUse),
        "merge_directive" => Some(RelatedRole::MergeDirective),
        "anchor_definition" => Some(RelatedRole::AnchorDefinition),
        _ => None,
    }
}

struct WireParser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    limits: &'a Limits,
    nodes_used: usize,
    owned_bytes: u64,
}

impl<'a> WireParser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n')
        {
            self.pos += 1;
        }
    }

    fn peek_byte(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), Error> {
        self.skip_ws();
        if self.bytes.get(self.pos).copied() != Some(expected) {
            return Err(wire_internal_err(format_args!(
                "wire decode: expected {:?}",
                expected as char
            )));
        }
        self.pos += 1;
        Ok(())
    }

    fn consume_literal(&mut self, lit: &str) -> bool {
        self.skip_ws();
        if self.src[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn parse_bool(&mut self) -> Result<bool, Error> {
        if self.consume_literal("true") {
            Ok(true)
        } else if self.consume_literal("false") {
            Ok(false)
        } else {
            Err(wire_internal_err("wire decode: expected boolean"))
        }
    }

    fn parse_hex4(&mut self) -> Result<u16, Error> {
        let Some(slice) = self.src.get(self.pos..self.pos + 4) else {
            return Err(wire_internal_err("wire decode: short \\uXXXX escape"));
        };
        if !slice.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(wire_internal_err("wire decode: invalid \\uXXXX escape"));
        }
        let v = u16::from_str_radix(slice, 16)
            .map_err(|_| wire_internal_err("wire decode: invalid \\uXXXX escape"))?;
        self.pos += 4;
        Ok(v)
    }

    fn parse_string(&mut self) -> Result<String, Error> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b < 0x20 {
                return Err(wire_internal_err(
                    "wire decode: unescaped control character in string",
                ));
            }
            if b != b'\\' {
                let ch = self.src[self.pos..]
                    .chars()
                    .next()
                    .ok_or_else(|| wire_internal_err("wire decode: invalid UTF-8"))?;
                out.push(ch);
                self.pos += ch.len_utf8();
                continue;
            }
            self.pos += 1;
            let Some(&esc) = self.bytes.get(self.pos) else {
                return Err(wire_internal_err("wire decode: unexpected EOF in escape"));
            };
            self.pos += 1;
            match esc {
                b'"' | b'\\' | b'/' => out.push(esc as char),
                b'b' => out.push('\u{0008}'),
                b'f' => out.push('\u{000c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    let u = self.parse_hex4()?;
                    if (0xD800..=0xDBFF).contains(&u) {
                        if !self.src[self.pos..].starts_with("\\u") {
                            return Err(wire_internal_err(
                                "wire decode: unpaired high surrogate in string",
                            ));
                        }
                        self.pos += 2;
                        let low = self.parse_hex4()?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return Err(wire_internal_err(
                                "wire decode: invalid low surrogate in string",
                            ));
                        }
                        let cp = 0x10000 + (((u as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                        let ch = char::from_u32(cp)
                            .ok_or_else(|| wire_internal_err("wire decode: invalid code point"))?;
                        out.push(ch);
                    } else if (0xDC00..=0xDFFF).contains(&u) {
                        return Err(wire_internal_err(
                            "wire decode: unpaired low surrogate in string",
                        ));
                    } else {
                        let ch = char::from_u32(u as u32)
                            .ok_or_else(|| wire_internal_err("wire decode: invalid code point"))?;
                        out.push(ch);
                    }
                }
                _ => return Err(wire_internal_err("wire decode: invalid escape sequence")),
            }
        }
        Err(wire_internal_err("wire decode: unterminated string"))
    }

    fn parse_meta_u64(&mut self) -> Result<u64, Error> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let digits = &self.src[start..self.pos];
        if digits.is_empty() {
            return Err(wire_internal_err(
                "wire decode: expected unsigned metadata integer",
            ));
        }
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(wire_internal_err(
                "wire decode: non-canonical leading zero on metadata integer",
            ));
        }
        if self
            .bytes
            .get(self.pos)
            .is_some_and(|b| matches!(b, b'.' | b'e' | b'E'))
        {
            return Err(wire_internal_err(
                "wire decode: non-integer metadata number",
            ));
        }
        let v: u64 = digits
            .parse()
            .map_err(|_| wire_internal_err("wire decode: metadata integer overflow"))?;
        if v > MAX_WIRE_META_UINT {
            return Err(wire_internal_err(
                "wire decode: metadata integer exceeds 2^53 - 1",
            ));
        }
        Ok(v)
    }

    fn parse_meta_usize(&mut self) -> Result<usize, Error> {
        let v = self.parse_meta_u64()?;
        usize::try_from(v).map_err(|_| wire_internal_err("wire decode: metadata integer overflow"))
    }

    fn parse_loc(&mut self) -> Result<Location, Error> {
        self.expect_byte(b'[')?;
        let byte_offset = self.parse_meta_usize()?;
        self.expect_byte(b',')?;
        let line = self.parse_meta_usize()?;
        self.expect_byte(b',')?;
        let column = self.parse_meta_usize()?;
        self.expect_byte(b']')?;
        if line < 1 || column < 1 {
            return Err(wire_internal_err(
                "wire decode: location line and column must be >= 1",
            ));
        }
        Ok(Location {
            byte_offset,
            line,
            column,
        })
    }

    fn parse_span(&mut self) -> Result<Span, Error> {
        self.expect_byte(b'[')?;
        let sb = self.parse_meta_usize()?;
        self.expect_byte(b',')?;
        let sl = self.parse_meta_usize()?;
        self.expect_byte(b',')?;
        let sc = self.parse_meta_usize()?;
        self.expect_byte(b',')?;
        let eb = self.parse_meta_usize()?;
        self.expect_byte(b',')?;
        let el = self.parse_meta_usize()?;
        self.expect_byte(b',')?;
        let ec = self.parse_meta_usize()?;
        self.expect_byte(b']')?;
        if sl < 1 || sc < 1 || el < 1 || ec < 1 || sb > eb {
            return Err(wire_internal_err("wire decode: invalid span coordinates"));
        }
        Ok(Span {
            start: Location {
                byte_offset: sb,
                line: sl,
                column: sc,
            },
            end: Location {
                byte_offset: eb,
                line: el,
                column: ec,
            },
        })
    }

    fn charge_node(&mut self, extra_bytes: u64) -> Result<(), Error> {
        self.nodes_used = self
            .nodes_used
            .checked_add(1)
            .ok_or_else(|| wire_limit_err(LimitKind::MaxNodes, self.limits.max_nodes))?;
        if self.nodes_used > self.limits.max_nodes {
            return Err(wire_limit_err(LimitKind::MaxNodes, self.limits.max_nodes));
        }
        self.charge_owned_bytes(NODE_OVERHEAD.saturating_add(extra_bytes))
    }

    fn charge_owned_bytes(&mut self, bytes: u64) -> Result<(), Error> {
        self.owned_bytes = self
            .owned_bytes
            .checked_add(bytes)
            .ok_or_else(|| wire_limit_err(LimitKind::MaxOwnedBytes, self.limits.max_owned_bytes))?;
        if self.owned_bytes > self.limits.max_owned_bytes as u64 {
            return Err(wire_limit_err(
                LimitKind::MaxOwnedBytes,
                self.limits.max_owned_bytes,
            ));
        }
        Ok(())
    }

    fn parse_envelope(&mut self) -> Result<WireEnvelope, Error> {
        self.expect_byte(b'{')?;
        let mut seen = HashSet::with_capacity(3);
        let mut has_format = false;
        let mut has_version = false;
        let mut docs: Option<Vec<Document>> = None;
        let mut wire_err: Option<Error> = None;
        let mut first = true;

        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err("wire decode: unexpected EOF in envelope"));
            };
            if b == b'}' {
                self.pos += 1;
                break;
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            let key = self.parse_string()?;
            if !seen.insert(key.clone()) {
                return Err(wire_internal_err("wire decode: duplicate envelope member"));
            }
            self.expect_byte(b':')?;
            match key.as_str() {
                "format" => {
                    let f = self.parse_string()?;
                    if f != "yamlprims-value" {
                        return Err(wire_internal_err("wire decode: unsupported wire format"));
                    }
                    has_format = true;
                }
                "version" => {
                    let v = self.parse_meta_u64()?;
                    if v != 1 {
                        return Err(wire_internal_err("wire decode: unsupported wire version"));
                    }
                    has_version = true;
                }
                "documents" => {
                    if wire_err.is_some() {
                        return Err(wire_internal_err(
                            "wire decode: envelope has both documents and error",
                        ));
                    }
                    docs = Some(self.parse_documents()?);
                }
                "error" => {
                    if docs.is_some() {
                        return Err(wire_internal_err(
                            "wire decode: envelope has both documents and error",
                        ));
                    }
                    wire_err = Some(self.parse_wire_error()?);
                }
                _ => return Err(wire_internal_err("wire decode: unknown envelope member")),
            }
        }
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err(wire_internal_err(
                "wire decode: trailing content after envelope",
            ));
        }
        if !has_format || !has_version {
            return Err(wire_internal_err(
                "wire decode: missing format or version in envelope",
            ));
        }
        match (docs, wire_err) {
            (Some(d), None) => Ok(WireEnvelope::Documents(d)),
            (None, Some(e)) => Ok(WireEnvelope::Error(e)),
            _ => Err(wire_internal_err("wire decode: incomplete envelope")),
        }
    }

    fn parse_documents(&mut self) -> Result<Vec<Document>, Error> {
        self.expect_byte(b'[')?;
        let mut docs = Vec::new();
        let mut first = true;
        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err(
                    "wire decode: unexpected EOF in documents",
                ));
            };
            if b == b']' {
                self.pos += 1;
                return Ok(docs);
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            if docs.len() + 1 > self.limits.max_documents {
                return Err(wire_limit_err(
                    LimitKind::MaxDocuments,
                    self.limits.max_documents,
                ));
            }
            docs.push(self.parse_document()?);
        }
    }

    fn parse_document(&mut self) -> Result<Document, Error> {
        self.expect_byte(b'{')?;
        let mut seen = HashSet::with_capacity(3);
        let mut yaml_version: Option<(u32, u32)> = None;
        let mut explicit_start = false;
        let mut root: Option<Node> = None;
        let mut first = true;

        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err("wire decode: unexpected EOF in document"));
            };
            if b == b'}' {
                self.pos += 1;
                break;
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            let key = self.parse_string()?;
            if !seen.insert(key.clone()) {
                return Err(wire_internal_err("wire decode: duplicate document member"));
            }
            self.expect_byte(b':')?;
            match key.as_str() {
                "yaml_version" => {
                    if self.consume_literal("null") {
                        yaml_version = None;
                    } else {
                        self.expect_byte(b'[')?;
                        let maj = self.parse_meta_u64()?;
                        self.expect_byte(b',')?;
                        let min = self.parse_meta_u64()?;
                        self.expect_byte(b']')?;
                        let maj = u32::try_from(maj).map_err(|_| {
                            wire_internal_err("wire decode: yaml_version major exceeds u32")
                        })?;
                        let min = u32::try_from(min).map_err(|_| {
                            wire_internal_err("wire decode: yaml_version minor exceeds u32")
                        })?;
                        yaml_version = Some((maj, min));
                    }
                }
                "explicit_start" => {
                    explicit_start = self.parse_bool()?;
                }
                "root" => {
                    root = Some(self.parse_node(0, false, false)?);
                }
                _ => return Err(wire_internal_err("wire decode: unknown document member")),
            }
        }
        let Some(root) = root else {
            return Err(wire_internal_err("wire decode: missing document root"));
        };
        if !seen.contains("yaml_version") || !seen.contains("explicit_start") {
            return Err(wire_internal_err("wire decode: incomplete document"));
        }
        Ok(Document {
            root,
            yaml_version,
            explicit_start,
        })
    }

    fn parse_node(
        &mut self,
        root_depth: usize,
        root_is_path_key: bool,
        root_in_tagged: bool,
    ) -> Result<Node, Error> {
        enum PendingPayload {
            Bool(bool),
            StrOrInt { s: String, str_charged: bool },
            TaggedInner(Node),
            Float(f64),
            Sequence(Vec<Node>),
            Mapping(Mapping),
        }

        struct NodeFrame {
            depth: usize,
            is_path_key: bool,
            in_tagged: bool,
            seen: HashSet<String>,
            kind_str: Option<String>,
            payload: Option<PendingPayload>,
            tag_val: Option<Tag>,
            node_span: NodeSpan,
            via: Option<Via>,
            tag_start: Option<Location>,
            first: bool,
        }

        impl NodeFrame {
            fn new(depth: usize, is_path_key: bool, in_tagged: bool) -> Self {
                Self {
                    depth,
                    is_path_key,
                    in_tagged,
                    seen: HashSet::with_capacity(6),
                    kind_str: None,
                    payload: None,
                    tag_val: None,
                    node_span: NodeSpan::Synthetic,
                    via: None,
                    tag_start: None,
                    first: true,
                }
            }
        }

        enum ParseWork {
            Node(Box<NodeFrame>),
            ResumeTaggedValue(Box<NodeFrame>),
            SeqItems {
                parent: Box<NodeFrame>,
                items: Vec<Node>,
                first_item: bool,
            },
            SeqItemDone {
                parent: Box<NodeFrame>,
                items: Vec<Node>,
            },
            MapEntries {
                parent: Box<NodeFrame>,
                map: Mapping,
                first_entry: bool,
            },
            MapEntryObj {
                parent: Box<NodeFrame>,
                map: Mapping,
                seen: HashSet<String>,
                k_node: Option<Node>,
                v_node: Option<Node>,
                first_member: bool,
            },
            MapEntryMemberDone {
                parent: Box<NodeFrame>,
                map: Mapping,
                seen: HashSet<String>,
                k_node: Option<Node>,
                v_node: Option<Node>,
                is_key: bool,
            },
        }

        self.charge_node(0)?;
        self.expect_byte(b'{')?;
        let mut work: Vec<ParseWork> = vec![ParseWork::Node(Box::new(NodeFrame::new(
            root_depth,
            root_is_path_key,
            root_in_tagged,
        )))];
        let mut completed: Option<Node> = None;

        'outer: loop {
            if let Some(child) = completed.take() {
                match work.pop() {
                    None => return Ok(child),
                    Some(ParseWork::ResumeTaggedValue(mut parent)) => {
                        parent.payload = Some(PendingPayload::TaggedInner(child));
                        work.push(ParseWork::Node(parent));
                    }
                    Some(ParseWork::SeqItemDone { parent, mut items }) => {
                        items.push(child);
                        work.push(ParseWork::SeqItems {
                            parent,
                            items,
                            first_item: false,
                        });
                    }
                    Some(ParseWork::MapEntryMemberDone {
                        parent,
                        map,
                        seen,
                        mut k_node,
                        mut v_node,
                        is_key,
                    }) => {
                        if is_key {
                            k_node = Some(child);
                        } else {
                            v_node = Some(child);
                        }
                        work.push(ParseWork::MapEntryObj {
                            parent,
                            map,
                            seen,
                            k_node,
                            v_node,
                            first_member: false,
                        });
                    }
                    Some(_) => unreachable!("unexpected work frame awaiting completed node"),
                }
                continue;
            }

            let step = work.pop().expect("work stack non-empty");
            match step {
                ParseWork::SeqItems {
                    mut parent,
                    items,
                    first_item,
                } => {
                    let Some(ib) = self.peek_byte() else {
                        return Err(wire_internal_err(
                            "wire decode: unexpected EOF in sequence items",
                        ));
                    };
                    if ib == b']' {
                        self.pos += 1;
                        parent.payload = Some(PendingPayload::Sequence(items));
                        work.push(ParseWork::Node(parent));
                        continue;
                    }
                    if !first_item {
                        self.expect_byte(b',')?;
                    }
                    if parent.is_path_key && parent.depth + 1 > self.limits.max_depth {
                        return Err(wire_limit_err(LimitKind::MaxDepth, self.limits.max_depth));
                    }
                    let (child_depth, child_pk) = (parent.depth + 1, parent.is_path_key);
                    self.charge_node(0)?;
                    self.expect_byte(b'{')?;
                    work.push(ParseWork::SeqItemDone { parent, items });
                    work.push(ParseWork::Node(Box::new(NodeFrame::new(
                        child_depth,
                        child_pk,
                        false,
                    ))));
                }
                ParseWork::MapEntries {
                    mut parent,
                    map,
                    first_entry,
                } => {
                    let Some(eb) = self.peek_byte() else {
                        return Err(wire_internal_err(
                            "wire decode: unexpected EOF in mapping entries",
                        ));
                    };
                    if eb == b']' {
                        self.pos += 1;
                        parent.payload = Some(PendingPayload::Mapping(map));
                        work.push(ParseWork::Node(parent));
                        continue;
                    }
                    if !first_entry {
                        self.expect_byte(b',')?;
                    }
                    if parent.is_path_key && parent.depth + 1 > self.limits.max_depth {
                        return Err(wire_limit_err(LimitKind::MaxDepth, self.limits.max_depth));
                    }
                    self.expect_byte(b'{')?;
                    work.push(ParseWork::MapEntryObj {
                        parent,
                        map,
                        seen: HashSet::with_capacity(2),
                        k_node: None,
                        v_node: None,
                        first_member: true,
                    });
                }
                ParseWork::MapEntryObj {
                    parent,
                    mut map,
                    mut seen,
                    k_node,
                    v_node,
                    first_member,
                } => {
                    let Some(b) = self.peek_byte() else {
                        return Err(wire_internal_err(
                            "wire decode: unexpected EOF in mapping entry",
                        ));
                    };
                    if b == b'}' {
                        self.pos += 1;
                        let (Some(kn), Some(vn)) = (k_node, v_node) else {
                            return Err(wire_internal_err("wire decode: incomplete mapping entry"));
                        };
                        let (_, kw, _) = metrics(&kn);
                        if kw > self.limits.max_key_bytes as u64 {
                            return Err(wire_limit_err(
                                LimitKind::MaxKeyBytes,
                                self.limits.max_key_bytes,
                            ));
                        }
                        self.charge_owned_bytes(ENTRY_OVERHEAD)?;
                        if map.len() >= self.limits.max_mapping_entries {
                            return Err(wire_limit_err(
                                LimitKind::MaxMappingEntries,
                                self.limits.max_mapping_entries,
                            ));
                        }
                        map.insert_new(kn, vn).map_err(|_| {
                            wire_internal_err("wire decode: invalid or duplicate mapping key")
                        })?;
                        work.push(ParseWork::MapEntries {
                            parent,
                            map,
                            first_entry: false,
                        });
                        continue;
                    }
                    if !first_member {
                        self.expect_byte(b',')?;
                    }
                    let k = self.parse_string()?;
                    if !seen.insert(k.clone()) {
                        return Err(wire_internal_err(
                            "wire decode: duplicate mapping entry member",
                        ));
                    }
                    self.expect_byte(b':')?;
                    let is_key = match k.as_str() {
                        "key" => true,
                        "value" => false,
                        _ => {
                            return Err(wire_internal_err(
                                "wire decode: unknown mapping entry member",
                            ))
                        }
                    };
                    let (child_depth, child_pk) = (parent.depth + 1, parent.is_path_key);
                    self.charge_node(0)?;
                    self.expect_byte(b'{')?;
                    work.push(ParseWork::MapEntryMemberDone {
                        parent,
                        map,
                        seen,
                        k_node,
                        v_node,
                        is_key,
                    });
                    work.push(ParseWork::Node(Box::new(NodeFrame::new(
                        child_depth,
                        child_pk,
                        false,
                    ))));
                }
                ParseWork::Node(mut nf) => {
                    loop {
                        let Some(b) = self.peek_byte() else {
                            return Err(wire_internal_err("wire decode: unexpected EOF in node"));
                        };
                        if b == b'}' {
                            self.pos += 1;
                            break;
                        }
                        if !nf.first {
                            self.expect_byte(b',')?;
                        }
                        nf.first = false;
                        let key = self.parse_string()?;
                        if !nf.seen.insert(key.clone()) {
                            return Err(wire_internal_err("wire decode: duplicate node member"));
                        }
                        self.expect_byte(b':')?;
                        match key.as_str() {
                            "kind" => {
                                let k = self.parse_string()?;
                                match k.as_str() {
                                    "null" | "bool" | "int" | "float" | "string" | "sequence"
                                    | "mapping" => {}
                                    "tagged" => {
                                        if nf.in_tagged {
                                            return Err(wire_internal_err(
                                                "wire decode: nested tagged wrapper in node",
                                            ));
                                        }
                                    }
                                    _ => {
                                        return Err(wire_internal_err(
                                            "wire decode: unknown node kind",
                                        ))
                                    }
                                }
                                nf.kind_str = Some(k);
                            }
                            "value" => {
                                if nf.payload.is_some() {
                                    return Err(wire_internal_err(
                                        "wire decode: conflicting node payload members",
                                    ));
                                }
                                match self.peek_byte() {
                                    Some(b't' | b'f') => {
                                        if nf.kind_str.as_deref().is_some_and(|k| k != "bool") {
                                            return Err(wire_internal_err(
                                                "wire decode: unexpected bool value for kind",
                                            ));
                                        }
                                        nf.payload = Some(PendingPayload::Bool(self.parse_bool()?));
                                    }
                                    Some(b'"') => {
                                        if nf
                                            .kind_str
                                            .as_deref()
                                            .is_some_and(|k| k != "int" && k != "string")
                                        {
                                            return Err(wire_internal_err(
                                                "wire decode: unexpected string value for kind",
                                            ));
                                        }
                                        let s = self.parse_string()?;
                                        let mut str_charged = false;
                                        if nf.kind_str.as_deref() == Some("string") || s.len() > 64
                                        {
                                            if s.len() > self.limits.max_scalar_bytes {
                                                return Err(wire_limit_err(
                                                    LimitKind::MaxScalarBytes,
                                                    self.limits.max_scalar_bytes,
                                                ));
                                            }
                                            self.charge_owned_bytes(s.len() as u64)?;
                                            str_charged = true;
                                        }
                                        nf.payload =
                                            Some(PendingPayload::StrOrInt { s, str_charged });
                                    }
                                    Some(b'{') => {
                                        if nf.in_tagged {
                                            return Err(wire_internal_err(
                                                "wire decode: nested tagged wrapper in node",
                                            ));
                                        }
                                        if nf.kind_str.as_deref().is_some_and(|k| k != "tagged") {
                                            return Err(wire_internal_err(
                                                "wire decode: unexpected object value for kind",
                                            ));
                                        }
                                        let (child_depth, child_pk) = (nf.depth, nf.is_path_key);
                                        self.charge_node(0)?;
                                        self.expect_byte(b'{')?;
                                        work.push(ParseWork::ResumeTaggedValue(nf));
                                        work.push(ParseWork::Node(Box::new(NodeFrame::new(
                                            child_depth,
                                            child_pk,
                                            true,
                                        ))));
                                        continue 'outer;
                                    }
                                    _ => {
                                        return Err(wire_internal_err(
                                            "wire decode: invalid value token in node",
                                        ))
                                    }
                                }
                            }
                            "bits" => {
                                if nf.payload.is_some()
                                    || nf.kind_str.as_deref().is_some_and(|k| k != "float")
                                {
                                    return Err(wire_internal_err(
                                        "wire decode: unexpected bits member for kind",
                                    ));
                                }
                                let s = self.parse_string()?;
                                let fv = parse_lower_hex16_float(&s).ok_or_else(|| {
                                    wire_internal_err("wire decode: invalid float bits")
                                })?;
                                nf.payload = Some(PendingPayload::Float(fv));
                            }
                            "items" => {
                                if nf.payload.is_some()
                                    || nf.kind_str.as_deref().is_some_and(|k| k != "sequence")
                                {
                                    return Err(wire_internal_err(
                                        "wire decode: unexpected items member for kind",
                                    ));
                                }
                                if !nf.is_path_key && nf.depth + 1 > self.limits.max_depth {
                                    return Err(wire_limit_err(
                                        LimitKind::MaxDepth,
                                        self.limits.max_depth,
                                    ));
                                }
                                self.expect_byte(b'[')?;
                                work.push(ParseWork::SeqItems {
                                    parent: nf,
                                    items: Vec::new(),
                                    first_item: true,
                                });
                                continue 'outer;
                            }
                            "entries" => {
                                if nf.payload.is_some()
                                    || nf.kind_str.as_deref().is_some_and(|k| k != "mapping")
                                {
                                    return Err(wire_internal_err(
                                        "wire decode: unexpected entries member for kind",
                                    ));
                                }
                                if !nf.is_path_key && nf.depth + 1 > self.limits.max_depth {
                                    return Err(wire_limit_err(
                                        LimitKind::MaxDepth,
                                        self.limits.max_depth,
                                    ));
                                }
                                self.expect_byte(b'[')?;
                                work.push(ParseWork::MapEntries {
                                    parent: nf,
                                    map: Mapping::new(),
                                    first_entry: true,
                                });
                                continue 'outer;
                            }
                            "tag" => {
                                if nf.kind_str.as_deref().is_some_and(|k| k != "tagged") {
                                    return Err(wire_internal_err(
                                        "wire decode: unexpected tag member for kind",
                                    ));
                                }
                                let t = self.parse_tag_object()?;
                                self.charge_owned_bytes(tag_bytes(&t))?;
                                nf.tag_val = Some(t);
                            }
                            "span" => {
                                if self.consume_literal("null") {
                                    nf.node_span = NodeSpan::Synthetic;
                                } else if nf.is_path_key {
                                    return Err(wire_internal_err(
                                        "wire decode: path key span must be null",
                                    ));
                                } else {
                                    nf.node_span = NodeSpan::Source(self.parse_span()?);
                                }
                            }
                            "via" => {
                                if self.consume_literal("null") {
                                    nf.via = None;
                                } else if nf.is_path_key {
                                    return Err(wire_internal_err(
                                        "wire decode: path key via must be null",
                                    ));
                                } else {
                                    nf.via = Some(self.parse_via_object()?);
                                }
                            }
                            "tag_start" => {
                                if self.consume_literal("null") {
                                    nf.tag_start = None;
                                } else if nf.is_path_key {
                                    return Err(wire_internal_err(
                                        "wire decode: path key tag_start must be null",
                                    ));
                                } else {
                                    nf.tag_start = Some(self.parse_loc()?);
                                }
                            }
                            _ => return Err(wire_internal_err("wire decode: unknown node member")),
                        }
                    }
                    if !nf.seen.contains("kind")
                        || !nf.seen.contains("span")
                        || !nf.seen.contains("via")
                        || !nf.seen.contains("tag_start")
                    {
                        return Err(wire_internal_err(
                            "wire decode: missing node location fields",
                        ));
                    }
                    if nf.tag_val.is_some() && nf.kind_str.as_deref() != Some("tagged") {
                        return Err(wire_internal_err(
                            "wire decode: unexpected tag member for kind",
                        ));
                    }
                    let value = match (nf.kind_str.as_deref(), nf.payload) {
                        (Some("null"), None) => Value::Null,
                        (Some("bool"), Some(PendingPayload::Bool(b))) => Value::Bool(b),
                        (Some("int"), Some(PendingPayload::StrOrInt { s, str_charged })) => {
                            if str_charged {
                                return Err(wire_internal_err(
                                    "wire decode: non-canonical integer string",
                                ));
                            }
                            let iv = parse_canonical_wire_int(&s).ok_or_else(|| {
                                wire_internal_err("wire decode: non-canonical integer string")
                            })?;
                            Value::Int(iv)
                        }
                        (Some("string"), Some(PendingPayload::StrOrInt { s, str_charged })) => {
                            if !str_charged {
                                if s.len() > self.limits.max_scalar_bytes {
                                    return Err(wire_limit_err(
                                        LimitKind::MaxScalarBytes,
                                        self.limits.max_scalar_bytes,
                                    ));
                                }
                                self.charge_owned_bytes(s.len() as u64)?;
                            }
                            Value::String(s)
                        }
                        (Some("float"), Some(PendingPayload::Float(fv))) => Value::Float(fv),
                        (Some("sequence"), Some(PendingPayload::Sequence(items))) => {
                            Value::Sequence(items)
                        }
                        (Some("mapping"), Some(PendingPayload::Mapping(m))) => Value::Mapping(m),
                        (Some("tagged"), Some(PendingPayload::TaggedInner(inner))) => {
                            let Some(tag) = nf.tag_val else {
                                return Err(wire_internal_err(
                                    "wire decode: missing tag for tagged node",
                                ));
                            };
                            let valid_inner = if tag.uri == "tag:yaml.org,2002:merge" {
                                matches!(inner.value(), Value::String(_))
                            } else {
                                matches!(
                                    inner.value(),
                                    Value::String(_) | Value::Sequence(_) | Value::Mapping(_)
                                )
                            };
                            if !valid_inner {
                                return Err(wire_internal_err(
                                    "wire decode: invalid inner value kind for tagged node",
                                ));
                            }
                            Value::Tagged(Box::new(Tagged {
                                tag,
                                value: inner,
                                inner_plain: false,
                            }))
                        }
                        _ => return Err(wire_internal_err("wire decode: incomplete node")),
                    };
                    let mut node = Node {
                        value,
                        span: nf.node_span,
                        extra: None,
                    };
                    node.set_via(nf.via);
                    if let Some(ts) = nf.tag_start {
                        node.set_tag_start(ts);
                    }
                    completed = Some(node);
                }
                _ => unreachable!("continuation frame popped without completed child"),
            }
        }
    }

    fn parse_tag_object(&mut self) -> Result<Tag, Error> {
        self.expect_byte(b'{')?;
        let mut seen = HashSet::with_capacity(3);
        let mut uri = String::new();
        let mut handle = String::new();
        let mut suffix = String::new();
        let mut first = true;
        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err("wire decode: unexpected EOF in tag"));
            };
            if b == b'}' {
                self.pos += 1;
                break;
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            let k = self.parse_string()?;
            if !seen.insert(k.clone()) {
                return Err(wire_internal_err("wire decode: duplicate tag member"));
            }
            self.expect_byte(b':')?;
            let v = self.parse_string()?;
            match k.as_str() {
                "uri" => uri = v,
                "handle" => handle = v,
                "suffix" => suffix = v,
                _ => return Err(wire_internal_err("wire decode: unknown tag member")),
            }
        }
        if !seen.contains("uri")
            || !seen.contains("handle")
            || !seen.contains("suffix")
            || !crate::value::is_valid_non_core_tag_uri(&uri)
            || !crate::value::is_valid_tag_spelling(&uri, &handle, &suffix)
        {
            return Err(wire_internal_err(
                "wire decode: invalid or empty tag object",
            ));
        }
        Ok(Tag {
            uri,
            handle,
            suffix,
        })
    }

    fn parse_via_object(&mut self) -> Result<Via, Error> {
        self.expect_byte(b'{')?;
        let k = self.parse_string()?;
        self.expect_byte(b':')?;
        let sp = self.parse_span()?;
        self.expect_byte(b'}')?;
        match k.as_str() {
            "alias" => Ok(Via::Alias { use_site: sp }),
            "merge" => Ok(Via::Merge { directive: sp }),
            _ => Err(wire_internal_err("wire decode: unknown via member")),
        }
    }

    fn parse_wire_error(&mut self) -> Result<Error, Error> {
        self.expect_byte(b'{')?;
        let mut seen = HashSet::with_capacity(6);
        let mut kind_name: Option<String> = None;
        let mut raw_detail: Option<RawErrorDetail> = None;
        let mut site: Option<Site> = None;
        let mut related: Option<Vec<Related>> = None;
        let mut path: Option<Vec<PathSegment>> = None;
        let mut message: Option<String> = None;
        let mut first = true;

        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err("wire decode: unexpected EOF in error"));
            };
            if b == b'}' {
                self.pos += 1;
                break;
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            let k = self.parse_string()?;
            if !seen.insert(k.clone()) {
                return Err(wire_internal_err("wire decode: duplicate error member"));
            }
            self.expect_byte(b':')?;
            match k.as_str() {
                "kind" => {
                    kind_name = Some(self.parse_string()?);
                }
                "detail" => {
                    raw_detail = Some(self.parse_raw_error_detail()?);
                }
                "site" => {
                    site = Some(self.parse_error_site()?);
                }
                "related" => {
                    related = Some(self.parse_error_related()?);
                }
                "path" => {
                    path = Some(self.parse_error_path()?);
                }
                "message" => {
                    message = Some(self.parse_string()?);
                }
                _ => return Err(wire_internal_err("wire decode: unknown error member")),
            }
        }
        let (Some(kn), Some(detail), Some(site), Some(related), Some(path), Some(message)) =
            (kind_name, raw_detail, site, related, path, message)
        else {
            return Err(wire_internal_err("wire decode: incomplete error object"));
        };
        let kind = detail.into_error_kind(&kn)?;
        let mut err = Error::new(kind, site, message);
        err.related = related;
        err.path = path;
        Ok(err)
    }

    fn parse_raw_error_detail(&mut self) -> Result<RawErrorDetail, Error> {
        self.expect_byte(b'{')?;
        let mut detail = RawErrorDetail::default();
        let mut first = true;
        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err(
                    "wire decode: unexpected EOF in error detail",
                ));
            };
            if b == b'}' {
                self.pos += 1;
                break;
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            let k = self.parse_string()?;
            if !detail.seen.insert(k.clone()) {
                return Err(wire_internal_err(
                    "wire decode: duplicate error detail member",
                ));
            }
            self.expect_byte(b':')?;
            match k.as_str() {
                "invalid_len" => {
                    let invalid_len = if self.consume_literal("null") {
                        None
                    } else {
                        Some(self.parse_meta_usize()?)
                    };
                    detail.invalid_len = Some(invalid_len);
                }
                "syntax_kind" => {
                    detail.syntax_kind = Some(self.parse_string()?);
                }
                "limit" => {
                    let ls = self.parse_string()?;
                    detail.limit = Some(
                        parse_limit_kind(&ls)
                            .ok_or_else(|| wire_internal_err("wire decode: unknown limit kind"))?,
                    );
                }
                "configured" => {
                    let c = self.parse_meta_u64()?;
                    if c == 0 {
                        return Err(wire_internal_err(
                            "wire decode: configured limit must be >= 1",
                        ));
                    }
                    detail.configured = Some(c);
                }
                "expected" => {
                    detail.expected = Some(self.parse_string()?);
                }
                "found" => {
                    detail.found = Some(self.parse_string()?);
                }
                "field" => {
                    detail.field = Some(self.parse_string()?);
                }
                _ => {
                    return Err(wire_internal_err(
                        "wire decode: unknown error detail member",
                    ))
                }
            }
        }
        Ok(detail)
    }

    fn parse_error_site(&mut self) -> Result<Site, Error> {
        self.expect_byte(b'{')?;
        let k = self.parse_string()?;
        self.expect_byte(b':')?;
        let site = match k.as_str() {
            "source" => Site::Source(self.parse_loc()?),
            "not_source" => {
                self.expect_byte(b'{')?;
                let lk = self.parse_string()?;
                if lk != "last_known" {
                    return Err(wire_internal_err(
                        "wire decode: expected last_known in not_source",
                    ));
                }
                self.expect_byte(b':')?;
                let last_known = if self.consume_literal("null") {
                    None
                } else {
                    Some(self.parse_loc()?)
                };
                self.expect_byte(b'}')?;
                Site::NotSource { last_known }
            }
            _ => return Err(wire_internal_err("wire decode: unknown site member")),
        };
        self.expect_byte(b'}')?;
        Ok(site)
    }

    fn parse_error_related(&mut self) -> Result<Vec<Related>, Error> {
        self.expect_byte(b'[')?;
        let mut rel = Vec::new();
        let mut first = true;
        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err("wire decode: unexpected EOF in related"));
            };
            if b == b']' {
                self.pos += 1;
                return Ok(rel);
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            self.expect_byte(b'{')?;
            let mut seen = HashSet::with_capacity(2);
            let mut role: Option<RelatedRole> = None;
            let mut span: Option<Span> = None;
            let mut f_rel = true;
            loop {
                let Some(rb) = self.peek_byte() else {
                    return Err(wire_internal_err(
                        "wire decode: unexpected EOF in related item",
                    ));
                };
                if rb == b'}' {
                    self.pos += 1;
                    break;
                }
                if !f_rel {
                    self.expect_byte(b',')?;
                }
                f_rel = false;
                let k = self.parse_string()?;
                if !seen.insert(k.clone()) {
                    return Err(wire_internal_err("wire decode: duplicate related member"));
                }
                self.expect_byte(b':')?;
                match k.as_str() {
                    "role" => {
                        let rs = self.parse_string()?;
                        role = Some(parse_related_role(&rs).ok_or_else(|| {
                            wire_internal_err("wire decode: unknown related role")
                        })?);
                    }
                    "span" => span = Some(self.parse_span()?),
                    _ => return Err(wire_internal_err("wire decode: unknown related member")),
                }
            }
            let (Some(role), Some(span)) = (role, span) else {
                return Err(wire_internal_err("wire decode: incomplete related item"));
            };
            rel.push(Related { role, span });
        }
    }

    fn parse_error_path(&mut self) -> Result<Vec<PathSegment>, Error> {
        self.expect_byte(b'[')?;
        let mut path = Vec::new();
        let mut first = true;
        loop {
            let Some(b) = self.peek_byte() else {
                return Err(wire_internal_err("wire decode: unexpected EOF in path"));
            };
            if b == b']' {
                self.pos += 1;
                return Ok(path);
            }
            if !first {
                self.expect_byte(b',')?;
            }
            first = false;
            if path.len() + 1 > self.limits.max_depth {
                return Err(wire_limit_err(LimitKind::MaxDepth, self.limits.max_depth));
            }
            self.expect_byte(b'{')?;
            let k = self.parse_string()?;
            self.expect_byte(b':')?;
            match k.as_str() {
                "index" => {
                    let idx = self.parse_meta_usize()?;
                    path.push(PathSegment::Index(idx));
                }
                "key" => {
                    let n = self.parse_node(0, true, false)?;
                    let (_, kw, _) = metrics(&n);
                    if kw > self.limits.max_key_bytes as u64 {
                        return Err(wire_limit_err(
                            LimitKind::MaxKeyBytes,
                            self.limits.max_key_bytes,
                        ));
                    }
                    path.push(PathSegment::Key(Box::new(n.into_value())));
                }
                _ => {
                    return Err(wire_internal_err(
                        "wire decode: unknown path segment member",
                    ))
                }
            }
            self.expect_byte(b'}')?;
        }
    }
}

#[derive(Default)]
struct RawErrorDetail {
    seen: HashSet<String>,
    invalid_len: Option<Option<usize>>,
    syntax_kind: Option<String>,
    limit: Option<LimitKind>,
    configured: Option<u64>,
    expected: Option<String>,
    found: Option<String>,
    field: Option<String>,
}

impl RawErrorDetail {
    fn into_error_kind(self, kind_name: &str) -> Result<ErrorKind, Error> {
        match kind_name {
            "invalid_utf8" => {
                if self.seen.len() != 1 {
                    return Err(wire_internal_err(
                        "wire decode: unexpected detail members for invalid_utf8",
                    ));
                }
                let Some(invalid_len) = self.invalid_len else {
                    return Err(wire_internal_err(
                        "wire decode: expected invalid_len in invalid_utf8 detail",
                    ));
                };
                Ok(ErrorKind::InvalidUtf8 { invalid_len })
            }
            "syntax" => {
                if self.seen.len() != 1 {
                    return Err(wire_internal_err(
                        "wire decode: unexpected detail members for syntax",
                    ));
                }
                let Some(code) = self.syntax_kind else {
                    return Err(wire_internal_err(
                        "wire decode: expected syntax_kind in syntax detail",
                    ));
                };
                let sk = SyntaxKind::from_code(&code).ok_or_else(|| {
                    wire_internal_err("wire decode: unknown syntax_kind in syntax detail")
                })?;
                Ok(ErrorKind::Syntax(sk))
            }
            "limit_exceeded" => {
                if self.seen.len() != 2 {
                    return Err(wire_internal_err(
                        "wire decode: unexpected detail members for limit_exceeded",
                    ));
                }
                let (Some(limit), Some(configured)) = (self.limit, self.configured) else {
                    return Err(wire_internal_err(
                        "wire decode: incomplete limit_exceeded detail",
                    ));
                };
                Ok(ErrorKind::LimitExceeded { limit, configured })
            }
            "type" => {
                if self.seen.len() != 2 {
                    return Err(wire_internal_err(
                        "wire decode: unexpected detail members for type",
                    ));
                }
                let (Some(expected), Some(found)) = (self.expected, self.found) else {
                    return Err(wire_internal_err("wire decode: incomplete type detail"));
                };
                Ok(ErrorKind::Type { expected, found })
            }
            "missing_field" | "unknown_field" => {
                if self.seen.len() != 1 {
                    return Err(wire_internal_err(
                        "wire decode: unexpected detail members for field error",
                    ));
                }
                let Some(field) = self.field else {
                    return Err(wire_internal_err("wire decode: expected field in detail"));
                };
                if kind_name == "missing_field" {
                    Ok(ErrorKind::MissingField(field))
                } else {
                    Ok(ErrorKind::UnknownField(field))
                }
            }
            "output_limit_exceeded" => {
                if self.seen.len() != 1 {
                    return Err(wire_internal_err(
                        "wire decode: unexpected detail members for output_limit_exceeded",
                    ));
                }
                let Some(configured) = self.configured else {
                    return Err(wire_internal_err(
                        "wire decode: expected configured in output_limit_exceeded detail",
                    ));
                };
                Ok(ErrorKind::OutputLimitExceeded { configured })
            }
            "unsupported_yaml_version"
            | "invalid_options"
            | "duplicate_key"
            | "invalid_key"
            | "unknown_anchor"
            | "recursive_alias"
            | "merge_rejected"
            | "merge_source"
            | "unknown_tag"
            | "tag_kind_mismatch"
            | "tag_value_mismatch"
            | "integer_out_of_range"
            | "float_out_of_range"
            | "multiple_documents"
            | "custom"
            | "internal" => {
                if !self.seen.is_empty() {
                    return Err(wire_internal_err(
                        "wire decode: expected empty detail object",
                    ));
                }
                Ok(match kind_name {
                    "unsupported_yaml_version" => ErrorKind::UnsupportedYamlVersion,
                    "invalid_options" => ErrorKind::InvalidOptions,
                    "duplicate_key" => ErrorKind::DuplicateKey,
                    "invalid_key" => ErrorKind::InvalidKey,
                    "unknown_anchor" => ErrorKind::UnknownAnchor,
                    "recursive_alias" => ErrorKind::RecursiveAlias,
                    "merge_rejected" => ErrorKind::MergeRejected,
                    "merge_source" => ErrorKind::MergeSource,
                    "unknown_tag" => ErrorKind::UnknownTag,
                    "tag_kind_mismatch" => ErrorKind::TagKindMismatch,
                    "tag_value_mismatch" => ErrorKind::TagValueMismatch,
                    "integer_out_of_range" => ErrorKind::IntegerOutOfRange,
                    "float_out_of_range" => ErrorKind::FloatOutOfRange,
                    "multiple_documents" => ErrorKind::MultipleDocuments,
                    "custom" => ErrorKind::Custom,
                    "internal" => ErrorKind::Internal,
                    _ => unreachable!(),
                })
            }
            _ => Err(wire_internal_err("wire decode: unknown error kind")),
        }
    }
}
