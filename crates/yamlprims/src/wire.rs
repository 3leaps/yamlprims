//! Wire format `yamlprims-value/1` (spec §11.1) and plain JSON (§11.3).

use crate::budget::{tag_bytes, ENTRY_OVERHEAD, NODE_OVERHEAD};
use crate::error::{Error, ErrorKind, LimitKind, PathSegment, Site, MAX_FIELD, MAX_MESSAGE};
use crate::options::Limits;
use crate::value::{Document, Location, Node, NodeSpan, Span, Value, Via};
use std::fmt::Write as _;

const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;

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

struct Out {
    s: String,
    max: usize,
}

impl Out {
    fn push(&mut self, t: &str) -> Result<(), Error> {
        if self.s.len() + t.len() > self.max {
            return self.push_over();
        }
        self.s.push_str(t);
        Ok(())
    }
    fn num(&mut self, n: usize) -> Result<(), Error> {
        self.push(&n.to_string())
    }
    fn string(&mut self, v: &str) -> Result<(), Error> {
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

fn node(o: &mut Out, n: &Node, with_spans: bool) -> Result<(), Error> {
    value(o, &n.value, with_spans)?;
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

/// A node's `kind` and content members, without its closing location members.
fn value(o: &mut Out, v: &Value, with_spans: bool) -> Result<(), Error> {
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
            o.string(&i.to_string())?
        }
        Value::Float(f) => {
            o.push(",\"bits\":")?;
            o.string(&float_bits(*f))?
        }
        Value::String(s) => {
            o.push(",\"value\":")?;
            o.string(s)?
        }
        Value::Sequence(items) => {
            o.push(",\"items\":[")?;
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    o.push(",")?;
                }
                node(o, it, with_spans)?;
            }
            o.push("]")?
        }
        Value::Mapping(m) => {
            o.push(",\"entries\":[")?;
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 {
                    o.push(",")?;
                }
                o.push("{\"key\":")?;
                node(o, k, with_spans)?;
                o.push(",\"value\":")?;
                node(o, v, with_spans)?;
                o.push("}")?;
            }
            o.push("]")?
        }
        Value::Tagged(t) => {
            o.push(",\"tag\":{\"uri\":")?;
            o.string(&t.tag.uri)?;
            o.push(",\"handle\":")?;
            o.string(&t.tag.handle)?;
            o.push(",\"suffix\":")?;
            o.string(&t.tag.suffix)?;
            o.push("},\"value\":")?;
            node(o, &t.value, with_spans)?
        }
    }
    Ok(())
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
    if let ErrorKind::Syntax(k) = &e.kind {
        let code = !k.is_empty()
            && k.len() <= 64
            && k.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !code {
            return Err(Error::not_source(
                ErrorKind::Internal,
                None,
                "error not encodable: syntax_kind is not a snake_case code",
            ));
        }
    }
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
            o.string(k)?;
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

fn plain(o: &mut Out, n: &Node) -> Result<(), Error> {
    match &n.value {
        Value::Null => o.push("null"),
        Value::Bool(b) => o.push(if *b { "true" } else { "false" }),
        Value::Int(i) => o.push(&i.to_string()),
        Value::Float(f) => {
            if !f.is_finite() {
                return Err(loss(n, "non-finite float"));
            }
            let mut s = format!("{f:?}");
            if !s.contains(['.', 'e', 'E']) {
                s.push_str(".0");
            }
            o.push(&s)
        }
        Value::String(s) => o.string(s),
        Value::Sequence(items) => {
            o.push("[")?;
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    o.push(",")?;
                }
                plain(o, it)?;
            }
            o.push("]")
        }
        Value::Mapping(m) => {
            o.push("{")?;
            for (i, (k, v)) in m.iter().enumerate() {
                let Value::String(ks) = &k.value else {
                    return Err(loss(k, "non-string key"));
                };
                if i > 0 {
                    o.push(",")?;
                }
                o.string(ks)?;
                o.push(":")?;
                plain(o, v)?;
            }
            o.push("}")
        }
        Value::Tagged(_) => Err(loss(n, "tagged value")),
    }
}
