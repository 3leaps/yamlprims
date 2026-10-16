//! Input gate, event pump and builder (spec §2, §5, §6, §7.2).

use crate::error::{Error, ErrorKind, LimitKind, PathSegment, RelatedRole};
use crate::options::{MergeKeys, Options, UnknownTags};
use crate::resolve::{classify, resolve_scalar, Resolved, TagClass};
use crate::value::{
    contains_nan, Document, InsertError, Location, Mapping, Node, NodeSpan, Span, Tagged, Value,
    Via,
};
use granit_parser::{Event, Marker, Parser, ScanError, Span as GSpan, StructureStyle};
use std::collections::{HashMap, HashSet};

pub(crate) const NODE_OVERHEAD: u64 = 64;
pub(crate) const ENTRY_OVERHEAD: u64 = 24;

/// Gate raw bytes: size, BOM, UTF-8 (§6, §7.2).
pub(crate) fn gate_bytes<'a>(input: &'a [u8], opts: &Options) -> Result<(&'a str, usize), Error> {
    if input.len() > opts.limits.max_input_bytes {
        return Err(limit_err(
            LimitKind::MaxInputBytes,
            opts.limits.max_input_bytes,
            loc0(),
        ));
    }
    let bom = if input.starts_with(b"\xEF\xBB\xBF") {
        3
    } else {
        0
    };
    match std::str::from_utf8(&input[bom..]) {
        Ok(s) => Ok((s, bom)),
        Err(e) => {
            let valid = &input[bom..bom + e.valid_up_to()];
            // Valid prefix by construction.
            let prefix = std::str::from_utf8(valid).map_err(|_| internal("utf8 prefix"))?;
            let loc = scan_location(prefix, bom);
            Err(Error::at(
                ErrorKind::InvalidUtf8 {
                    invalid_len: e.error_len(),
                },
                loc,
                "invalid UTF-8",
            ))
        }
    }
}

pub(crate) fn gate_str<'a>(input: &'a str, opts: &Options) -> Result<(&'a str, usize), Error> {
    if input.len() > opts.limits.max_input_bytes {
        return Err(limit_err(
            LimitKind::MaxInputBytes,
            opts.limits.max_input_bytes,
            loc0(),
        ));
    }
    Ok(match input.strip_prefix('\u{feff}') {
        Some(rest) => (rest, 3),
        None => (input, 0),
    })
}

fn loc0() -> Location {
    Location {
        byte_offset: 0,
        line: 1,
        column: 1,
    }
}

/// Reference location of the end of `prefix` (LF, CRLF and lone CR are breaks; columns in chars).
/// Move an empty (zero-width) scalar document root, and a tagged root's empty inner node,
/// to `at`. Collections (an empty-key mapping can be zero-width), roots with content, and
/// alias copies are left alone.
fn place_empty_root(root: &mut Node, at: Location) {
    let NodeSpan::Source(old) = root.span else {
        return;
    };
    let collection = |v: &Value| matches!(v, Value::Sequence(_) | Value::Mapping(_));
    let inner_collection = matches!(&root.value, Value::Tagged(t) if collection(&t.value.value));
    if old.start != old.end || root.via().is_some() || collection(&root.value) || inner_collection {
        return;
    }
    root.span = NodeSpan::Source(Span { start: at, end: at });
    if let Value::Tagged(t) = &mut root.value {
        if t.value.span == NodeSpan::Source(old) {
            t.value.span = NodeSpan::Source(Span { start: at, end: at });
        }
    }
}

pub(crate) fn scan_location(prefix: &str, bom: usize) -> Location {
    let (mut line, mut col) = (1usize, 1usize);
    let mut it = prefix.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\n' => {
                line += 1;
                col = 1
            }
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                line += 1;
                col = 1
            }
            _ => col += 1,
        }
    }
    Location {
        byte_offset: prefix.len() + bom,
        line,
        column: col,
    }
}

fn internal(what: &str) -> Error {
    Error::not_source(ErrorKind::Internal, None, format!("internal error: {what}"))
}

fn limit_err(limit: LimitKind, configured: usize, at: Location) -> Error {
    Error::at(
        ErrorKind::LimitExceeded {
            limit,
            configured: configured as u64,
        },
        at,
        format!("{} ({configured}) exceeded", limit.field_name()),
    )
}

/// granit error kind name in snake_case.
fn syntax_name(e: &ScanError) -> String {
    let dbg = format!("{:?}", e.kind());
    let name: String = dbg
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

struct Built {
    node: Node,
    count: u64,
    weight: u64,
    /// Collection levels in this subtree (scalars 0).
    height: usize,
    /// Scalar key written as a merge directive (§5.3 recognition).
    merge_directive: bool,
}

/// What started at a node position.
enum Started {
    Leaf(Built),
    Open(Box<Open>),
}

/// A collection whose children are still being read: the builder's per-level state.
struct Open {
    kind: OpenKind,
    anchor: usize,
    class: TagClass,
    flow: bool,
    span: Span,
    tag_start: Option<Location>,
    count: u64,
    weight: u64,
    /// Collection levels below this one so far.
    height: usize,
    /// Position span of the last child (the block end is trimmed back to it).
    last_start: Location,
    last_end: Location,
    /// Where the next empty child may be placed from (§7.2 floors).
    floor: Location,
}

enum OpenKind {
    Seq(Vec<Node>),
    Map {
        map: Mapping,
        /// (slot index in explicit entries, directive key span, source value)
        directive: Option<(usize, Span, Node)>,
        /// A key read, waiting for its value.
        pending: Option<Box<PendingKey>>,
    },
}

struct PendingKey {
    node: Node,
    count: u64,
    weight: u64,
    height: usize,
    key_end: Location,
    kspan: Span,
    key_use: Option<Span>,
    is_directive: bool,
}

struct Anchored {
    node: Node,
    count: u64,
    weight: u64,
    height: usize,
}

pub(crate) struct Loader<'a> {
    input: &'a str,
    bom: usize,
    opts: &'a Options,
    parser: Parser<'a, granit_parser::StrInput<'a>>,
    peeked: Option<(Event<'a>, GSpan)>,
    nodes: u64,
    expanded: u64,
    alias_events: u64,
    owned: u64,
    anchors: HashMap<usize, Anchored>,
    open_anchors: HashSet<usize>,
    path: Vec<PathSegment>,
}

impl<'a> Loader<'a> {
    pub(crate) fn new(input: &'a str, bom: usize, opts: &'a Options) -> Loader<'a> {
        let mut g = granit_parser::Options::default();
        g.strict_indentation = opts.strict_indentation();
        g.flow_nesting_limit = opts.limits.max_depth;
        g.block_nesting_limit = opts.limits.max_depth;
        Loader {
            input,
            bom,
            opts,
            parser: Parser::new_from_str_with_options(input, g),
            peeked: None,
            nodes: 0,
            expanded: 0,
            alias_events: 0,
            owned: 0,
            anchors: HashMap::new(),
            open_anchors: HashSet::new(),
            path: Vec::new(),
        }
    }

    fn loc(&self, m: &Marker) -> Result<Location, Error> {
        let b = m
            .byte_offset()
            .ok_or_else(|| internal("marker without byte offset"))?;
        Ok(Location {
            byte_offset: b + self.bom,
            line: m.line(),
            column: m.col() + 1,
        })
    }

    /// §7.2: a block collection ends at the exclusive end of its last content
    /// line. A last child that is a block scalar ends after its line break(s);
    /// step back over them, but not before the child's start.
    fn trim_breaks(&self, end: Location, floor: Location) -> Location {
        let bytes = self.input.as_bytes();
        let floor = floor.byte_offset - self.bom;
        let (mut j, mut line) = (end.byte_offset - self.bom, end.line);
        while j > floor && matches!(bytes[j - 1], b'\n' | b'\r') {
            j -= if bytes[j - 1] == b'\n' && j - 1 > floor && bytes[j - 2] == b'\r' {
                2
            } else {
                1
            };
            line -= 1;
        }
        if j == end.byte_offset - self.bom {
            return end;
        }
        let line_start = self.input[..j].rfind(['\n', '\r']).map_or(0, |i| i + 1);
        let column = self.input[line_start..j].chars().count() + 1;
        Location {
            byte_offset: j + self.bom,
            line,
            column,
        }
    }

    /// The start of `at`'s line when only spaces (indentation) precede `at` on it and that
    /// line start is after `floor`; otherwise `at`.
    fn skip_indentation(&self, at: Location, floor: Location) -> Location {
        let bytes = self.input.as_bytes();
        let (j, floor) = (at.byte_offset - self.bom, floor.byte_offset - self.bom);
        let mut k = j;
        while k > floor && bytes[k - 1] == b' ' {
            k -= 1;
        }
        if k == j || k == floor || !matches!(bytes[k - 1], b'\n' | b'\r') {
            return at;
        }
        Location {
            byte_offset: k + self.bom,
            line: at.line,
            column: 1,
        }
    }

    /// a block scalar ends at the exclusive end of its last source line that has a
    /// non-whitespace byte, without that line's break. Kept blank lines, trailing breaks and
    /// the next line's indentation are outside the span. A block scalar with no such line
    /// (`|+` followed only by blank lines) ends at the end of the line its start is on.
    fn block_scalar_end(&self, span: Span) -> Location {
        let bytes = self.input.as_bytes();
        let (s, e) = (
            span.start.byte_offset - self.bom,
            span.end.byte_offset - self.bom,
        );
        let mut k = e;
        while k > s && matches!(bytes[k - 1], b' ' | b'\t' | b'\n' | b'\r') {
            k -= 1;
        }
        while k < e && !matches!(bytes[k], b'\n' | b'\r') {
            k += 1;
        }
        if k == e {
            return span.end;
        }
        // Only ASCII whitespace and line breaks were stepped over, so `k` is a char boundary.
        let (mut line, mut column) = (span.start.line, span.start.column);
        let mut it = self.input[s..k].chars().peekable();
        while let Some(c) = it.next() {
            match c {
                '\n' => (line, column) = (line + 1, 1),
                '\r' => {
                    if it.peek() == Some(&'\n') {
                        it.next();
                    }
                    (line, column) = (line + 1, 1)
                }
                _ => column += 1,
            }
        }
        Location {
            byte_offset: k + self.bom,
            line,
            column,
        }
    }

    /// an empty node whose granit marker directly follows line breaks (a bare `?`,
    /// or `? a` with no value, on the last line of a block) sits before those breaks, at the
    /// end of the line it is omitted on, but not before `floor` (the preceding sibling or
    /// key, or the collection start). Alias copies keep their definition spans.
    /// a marker preceded only by its line's indentation (a nested block collection's
    /// next entry) is treated the same: the indentation is stepped over first.
    fn settle_empty(&self, n: &mut Node, floor: Location) {
        let NodeSpan::Source(s) = n.span else { return };
        if s.start != s.end || n.via().is_some() || s.start.byte_offset <= floor.byte_offset {
            return;
        }
        // A zero-width collection (a block mapping of empty entries) keeps its place: its
        // children share its point, and moving it alone would break containment.
        let collection = |v: &Value| matches!(v, Value::Mapping(_) | Value::Sequence(_));
        if collection(&n.value)
            || matches!(&n.value, Value::Tagged(t) if collection(&t.value.value))
        {
            return;
        }
        let at = self.trim_breaks(self.skip_indentation(s.start, floor), floor);
        if at == s.start {
            return;
        }
        n.span = NodeSpan::Source(Span { start: at, end: at });
        if let Value::Tagged(t) = &mut n.value {
            if t.value.span == NodeSpan::Source(s) {
                t.value.span = NodeSpan::Source(Span { start: at, end: at });
            }
        }
    }

    fn span(&self, s: &GSpan) -> Result<Span, Error> {
        Ok(Span {
            start: self.loc(&s.start)?,
            end: self.loc(&s.end)?,
        })
    }

    fn scan_err(&self, e: ScanError) -> Error {
        use granit_parser::ErrorKind as G;
        let loc = match self.loc(e.marker()) {
            Ok(l) => l,
            Err(err) => return err,
        };
        let detail = e.info();
        let mut err = match e.kind() {
            G::UnknownAnchor { .. } => Error::at(ErrorKind::UnknownAnchor, loc, "unknown anchor"),
            G::UnsupportedYamlMajorVersion { .. } => Error::at(
                ErrorKind::UnsupportedYamlVersion,
                loc,
                "unsupported YAML version",
            ),
            G::RecursionLimitExceeded { .. } => {
                limit_err(LimitKind::MaxDepth, self.opts.limits.max_depth, loc)
            }
            _ => Error::at(
                ErrorKind::Syntax(syntax_name(&e)),
                loc,
                format!("syntax error: {detail}"),
            ),
        };
        err.engine_detail = Some(detail);
        err
    }

    fn next(&mut self) -> Result<(Event<'a>, GSpan), Error> {
        if let Some(p) = self.peeked.take() {
            return Ok(p);
        }
        match self.parser.next() {
            Some(Ok(ev)) => Ok(ev),
            Some(Err(e)) => Err(self.scan_err(e)),
            None => Err(internal("event stream ended early")),
        }
    }

    fn peek(&mut self) -> Result<&Event<'a>, Error> {
        if self.peeked.is_none() {
            let ev = self.next()?;
            self.peeked = Some(ev);
        }
        Ok(&self.peeked.as_ref().expect("just set").0)
    }

    /// End of input, from the stream-end marker when it is there, else by scanning.
    fn eof(&self, span: &GSpan) -> Location {
        let end = self
            .loc(&span.end)
            .unwrap_or_else(|_| scan_location(self.input, self.bom));
        if end.byte_offset < self.input.len() + self.bom {
            scan_location(self.input, self.bom)
        } else {
            end
        }
    }

    fn err(&self, kind: ErrorKind, at: Location, msg: impl std::fmt::Display) -> Error {
        let mut e = Error::at(kind, at, msg);
        e.path = self.path.clone();
        e
    }

    fn charge(
        &mut self,
        count: u64,
        weight: u64,
        at: Location,
        expanded: bool,
    ) -> Result<(), Error> {
        let l = &self.opts.limits;
        let nodes = self.nodes.checked_add(count);
        if nodes.is_none_or(|n| n > l.max_nodes as u64) {
            return Err(self.err_limit(LimitKind::MaxNodes, l.max_nodes, at));
        }
        if expanded {
            let e = self.expanded.checked_add(count);
            if e.is_none_or(|n| n > l.max_expanded_nodes as u64) {
                return Err(self.err_limit(LimitKind::MaxExpandedNodes, l.max_expanded_nodes, at));
            }
            self.expanded = e.unwrap_or(u64::MAX);
        }
        let owned = self.owned.checked_add(weight);
        if owned.is_none_or(|n| n > l.max_owned_bytes as u64) {
            return Err(self.err_limit(LimitKind::MaxOwnedBytes, l.max_owned_bytes, at));
        }
        self.nodes = nodes.unwrap_or(u64::MAX);
        self.owned = owned.unwrap_or(u64::MAX);
        Ok(())
    }

    fn err_limit(&self, k: LimitKind, configured: usize, at: Location) -> Error {
        let mut e = limit_err(k, configured, at);
        e.path = self.path.clone();
        e
    }

    /// Syntax pre-pass (spec §7.1): a stream that is not well-formed YAML reports its
    /// first syntax error, ahead of any resolution or structure error.
    fn syntax_pass(&self) -> Result<(), Error> {
        let mut g = granit_parser::Options::default();
        g.strict_indentation = self.opts.strict_indentation();
        g.flow_nesting_limit = self.opts.limits.max_depth;
        g.block_nesting_limit = self.opts.limits.max_depth;
        for ev in Parser::new_from_str_with_options(self.input, g) {
            if let Err(e) = ev {
                return Err(self.scan_err(e));
            }
        }
        Ok(())
    }

    /// Load every document in the stream.
    /// `single` selects the one-document API: a second document is `MultipleDocuments`.
    /// Otherwise the configured `max_documents` applies (`LimitExceeded`), whatever its value.
    pub(crate) fn load_all(mut self, single: bool) -> Result<Vec<Document>, Error> {
        let max_docs = if single {
            1
        } else {
            self.opts.limits.max_documents
        };
        self.syntax_pass()?;
        match self.next()? {
            (Event::StreamStart, _) => {}
            _ => return Err(internal("missing stream start")),
        }
        let mut docs = Vec::new();
        loop {
            match self.next()? {
                (Event::StreamEnd, span) => {
                    if docs.is_empty() && single {
                        // `load` of empty or comment-only input: Null with an empty span at
                        // end of input. `load_all` returns no documents.
                        let end = self.eof(&span);
                        docs.push(Document {
                            root: Node {
                                value: Value::Null,
                                span: NodeSpan::Source(Span { start: end, end }),
                                extra: None,
                            },
                            yaml_version: None,
                            explicit_start: false,
                        });
                    }
                    docs.shrink_to_fit();
                    return Ok(docs);
                }
                (Event::DocumentStart(explicit, version), span) => {
                    let at = self.loc(&span.start)?;
                    if docs.len() >= max_docs {
                        if single {
                            return Err(Error::at(
                                ErrorKind::MultipleDocuments,
                                at,
                                "more than one document",
                            ));
                        }
                        return Err(limit_err(LimitKind::MaxDocuments, max_docs, at));
                    }
                    if let Some(v) = version {
                        if v.major != 1 {
                            return Err(Error::at(
                                ErrorKind::UnsupportedYamlVersion,
                                at,
                                "unsupported YAML version",
                            ));
                        }
                    }
                    self.anchors.clear();
                    self.open_anchors.clear();
                    let mut root = self.node(0)?.node;
                    let end_marker = match self.next()? {
                        (Event::DocumentEnd, span) if span.start.index() < span.end.index() => {
                            Some(self.loc(&span.start)?)
                        }
                        (Event::DocumentEnd, _) => None,
                        _ => return Err(internal("missing document end")),
                    };
                    if explicit {
                        // §5.5: an explicit empty document's root sits at the boundary ending
                        // the document: the first byte of `...`, the next `---`, or EOF.
                        let boundary = match end_marker {
                            Some(at) => at,
                            None => {
                                self.peek()?;
                                let (ev, span) = self.peeked.as_ref().expect("just peeked");
                                match ev {
                                    Event::StreamEnd => self.eof(span),
                                    _ => self.loc(&span.start)?,
                                }
                            }
                        };
                        place_empty_root(&mut root, boundary);
                    }
                    docs.push(Document {
                        root,
                        yaml_version: version.map(|v| (v.major, v.minor)),
                        explicit_start: explicit,
                    });
                }
                _ => return Err(internal("unexpected event between documents")),
            }
        }
    }

    /// Build one node. `base` = enclosing collection levels.
    ///
    /// Iterative : open collections live on a heap stack, one entry
    /// per nesting level, so the call stack does not grow with input depth. Each event is
    /// handled exactly as the recursive builder did, in the same order.
    fn node(&mut self, base: usize) -> Result<Built, Error> {
        let mut stack: Vec<Open> = Vec::new();
        let mut built: Option<Built> = None;
        loop {
            if let Some(b) = built.take() {
                let Some(top) = stack.last_mut() else {
                    return Ok(b);
                };
                self.accept(top, b)?;
            }
            // The innermost collection either ends here or wants its next child.
            if let Some(top) = stack.last() {
                let ends = match &top.kind {
                    OpenKind::Seq(_) => matches!(self.peek()?, Event::SequenceEnd),
                    OpenKind::Map { pending: None, .. } => {
                        matches!(self.peek()?, Event::MappingEnd)
                    }
                    // A key always has a value event, possibly an empty scalar.
                    OpenKind::Map {
                        pending: Some(_), ..
                    } => false,
                };
                if ends {
                    let o = stack.pop().expect("innermost collection");
                    built = Some(self.close(o)?);
                    continue;
                }
                if let OpenKind::Seq(items) = &top.kind {
                    self.path.push(PathSegment::Index(items.len()));
                }
            }
            match self.start(base + stack.len())? {
                Started::Leaf(b) => built = Some(b),
                Started::Open(o) => stack.push(*o),
            }
        }
    }

    /// Read the event that starts a node at `depth`: a finished scalar or alias copy, or a
    /// collection opened (guarded and charged) and awaiting its children.
    fn start(&mut self, depth: usize) -> Result<Started, Error> {
        let (ev, gspan) = self.next()?;
        let mut span = self.span(&gspan)?;
        if let Event::Scalar(
            _,
            granit_parser::ScalarStyle::Literal | granit_parser::ScalarStyle::Folded,
            ..,
        ) = ev
        {
            span.end = self.block_scalar_end(span);
        }
        let tag_start = match gspan.tag_start() {
            Some(m) => Some(self.loc(&m)?),
            None => None,
        };
        let seq = matches!(ev, Event::SequenceStart(..));
        match ev {
            Event::Scalar(text, style, anchor, tag) => {
                if text.len() > self.opts.limits.max_scalar_bytes {
                    return Err(self.err_limit(
                        LimitKind::MaxScalarBytes,
                        self.opts.limits.max_scalar_bytes,
                        span.start,
                    ));
                }
                let class = classify(tag.as_deref());
                let tag_loc = tag_start.unwrap_or(span.start);
                if let TagClass::Other(_) = class {
                    if self.opts.unknown_tags == UnknownTags::Reject {
                        return Err(self.err(ErrorKind::UnknownTag, tag_loc, "unknown tag"));
                    }
                }
                let merge_directive = match class {
                    TagClass::None => {
                        matches!(style, granit_parser::ScalarStyle::Plain) && text == "<<"
                    }
                    // Only meaningful in key position; the caller decides.
                    TagClass::Merge => text == "<<",
                    _ => false,
                };
                let merge_tagged_other = matches!(class, TagClass::Merge) && text != "<<";
                let resolved = resolve_scalar(&text, style, class).map_err(|k| {
                    let msg = format!("cannot resolve scalar: {}", k.name());
                    self.err(k, tag_loc, msg)
                })?;
                let (value, tag_bytes, nodes) = match resolved {
                    Resolved::Plain(v) => (v, 0, 1),
                    Resolved::Tagged(t, inner) => {
                        let tb = tag_bytes(&t);
                        let inner = Node::parsed(inner, span, None);
                        let inner_plain = matches!(style, granit_parser::ScalarStyle::Plain);
                        (
                            Value::Tagged(Box::new(Tagged {
                                tag: t,
                                value: inner,
                                inner_plain,
                            })),
                            tb + NODE_OVERHEAD,
                            2,
                        )
                    }
                };
                let weight = NODE_OVERHEAD + text.len() as u64 + tag_bytes;
                self.charge(nodes, weight, span.start, false)?;
                let node = Node::parsed(value, span, tag_start);
                let mut b = Built {
                    node,
                    count: nodes,
                    weight,
                    height: 0,
                    merge_directive,
                };
                if merge_tagged_other {
                    // Recorded so a key-position caller can raise TagValueMismatch.
                    b.merge_directive = false;
                    b.node.set_tag_start(tag_loc);
                }
                self.register(anchor, &b)?;
                Ok(Started::Leaf(b))
            }
            Event::Alias(id) => {
                self.alias_events += 1;
                if self.alias_events > self.opts.limits.max_alias_events as u64 {
                    return Err(self.err_limit(
                        LimitKind::MaxAliasEvents,
                        self.opts.limits.max_alias_events,
                        span.start,
                    ));
                }
                if self.open_anchors.contains(&id) {
                    return Err(self.err(
                        ErrorKind::RecursiveAlias,
                        span.start,
                        "alias refers to an enclosing node",
                    ));
                }
                let Some(a) = self.anchors.get(&id) else {
                    return Err(self.err(ErrorKind::UnknownAnchor, span.start, "unknown anchor"));
                };
                let (count, weight, height) = (a.count, a.weight, a.height);
                if depth + height > self.opts.limits.max_depth {
                    return Err(self
                        .err_limit(LimitKind::MaxDepth, self.opts.limits.max_depth, span.start)
                        .with_related(
                            RelatedRole::AnchorDefinition,
                            a.node.source_span().unwrap_or(span),
                        ));
                }
                // Checked before the copy (§5.4).
                self.charge(count, weight, span.start, true)?;
                let a = self
                    .anchors
                    .get(&id)
                    .ok_or_else(|| internal("anchor vanished"))?;
                let mut node = a.node.clone();
                node.set_via(Some(Via::Alias { use_site: span }));
                Ok(Started::Leaf(Built {
                    node,
                    count,
                    weight,
                    height,
                    merge_directive: false,
                }))
            }
            Event::SequenceStart(style, anchor, tag) | Event::MappingStart(style, anchor, tag) => {
                self.collection_guard(depth, span.start)?;
                if anchor != 0 {
                    self.open_anchors.insert(anchor);
                }
                let class = classify(tag.as_deref());
                let kind = if seq { "seq" } else { "map" };
                let (wrap_count, wrap_weight) =
                    self.collection_tag(&class, kind, tag_start.unwrap_or(span.start))?;
                let (count, weight) = (1 + wrap_count, NODE_OVERHEAD + wrap_weight);
                self.charge(count, weight, span.start, false)?;
                Ok(Started::Open(Box::new(Open {
                    kind: if seq {
                        OpenKind::Seq(Vec::new())
                    } else {
                        OpenKind::Map {
                            map: Mapping::new(),
                            directive: None,
                            pending: None,
                        }
                    },
                    anchor,
                    class,
                    flow: matches!(style, StructureStyle::Flow),
                    span,
                    tag_start,
                    count,
                    weight,
                    height: 0,
                    last_start: span.start,
                    last_end: span.end,
                    floor: span.start,
                })))
            }
            _ => Err(internal("unexpected event in node position")),
        }
    }

    /// Hand a finished child to the innermost open collection.
    fn accept(&mut self, o: &mut Open, mut b: Built) -> Result<(), Error> {
        let span = o.span;
        match &mut o.kind {
            OpenKind::Seq(items) => {
                self.path.pop();
                self.settle_empty(&mut b.node, o.floor);
                if let Some(s) = b.node.position_span() {
                    (o.last_start, o.last_end) = (s.start, s.end);
                    o.floor = s.end;
                }
                o.count += b.count;
                o.weight += b.weight;
                o.height = o.height.max(b.height);
                items.push(b.node);
            }
            OpenKind::Map {
                pending: pending @ None,
                ..
            } => {
                let mut k = b;
                self.settle_empty(&mut k.node, o.floor);
                let key_end = k.node.position_span().map_or(o.floor, |s| s.end);
                let kspan = k.node.source_span().unwrap_or(span);
                let key_use = alias_use(&k.node);
                if k.weight > self.opts.limits.max_key_bytes as u64 {
                    return Err(with_alias_use(
                        self.err_limit(
                            LimitKind::MaxKeyBytes,
                            self.opts.limits.max_key_bytes,
                            kspan.start,
                        ),
                        key_use,
                    ));
                }
                if let (Value::Tagged(t), false) = (&k.node.value, k.merge_directive) {
                    if t.tag.uri == "tag:yaml.org,2002:merge" {
                        return Err(with_alias_use(
                            self.err(
                                ErrorKind::TagValueMismatch,
                                k.node.tag_start().unwrap_or(kspan.start),
                                "!!merge key must be <<",
                            ),
                            key_use,
                        ));
                    }
                }
                let is_directive = k.merge_directive;
                if is_directive && self.opts.merge_keys == MergeKeys::Reject {
                    return Err(with_alias_use(
                        self.err(
                            ErrorKind::MergeRejected,
                            kspan.start,
                            "merge keys are rejected (merge_keys = Reject)",
                        ),
                        key_use,
                    ));
                }
                let key_node = if is_directive {
                    Node {
                        value: Value::String("<<".into()),
                        ..k.node
                    }
                } else {
                    k.node
                };
                self.path
                    .push(PathSegment::Key(Box::new(key_node.value.clone())));
                *pending = Some(Box::new(PendingKey {
                    node: key_node,
                    count: k.count,
                    weight: k.weight,
                    height: k.height,
                    key_end,
                    kspan,
                    key_use,
                    is_directive,
                }));
            }
            OpenKind::Map {
                map,
                directive,
                pending,
            } => {
                let k = *pending.take().expect("pending key");
                let mut v = b;
                self.path.pop();
                self.settle_empty(&mut v.node, k.key_end);
                if let Some(s) = v.node.position_span() {
                    (o.last_start, o.last_end) = (s.start, s.end);
                    o.floor = s.end;
                }
                o.count += k.count + v.count;
                o.weight += k.weight + v.weight + ENTRY_OVERHEAD;
                o.height = o.height.max(k.height).max(v.height);
                let (kspan, key_use) = (k.kspan, k.key_use);
                self.charge(0, ENTRY_OVERHEAD, kspan.start, false)?;
                if k.is_directive && self.opts.merge_keys == MergeKeys::Apply {
                    if let Some((_, first, _)) = directive {
                        return Err(with_alias_use(
                            self.err(
                                ErrorKind::DuplicateKey,
                                kspan.start,
                                "second merge directive in one mapping",
                            )
                            .with_related(RelatedRole::FirstDefinition, *first),
                            key_use,
                        ));
                    }
                    *directive = Some((map.len(), kspan, v.node));
                    return Ok(());
                }
                if contains_nan(&k.node.value) {
                    return Err(with_alias_use(
                        self.err(
                            ErrorKind::InvalidKey,
                            kspan.start,
                            "NaN inside a mapping key",
                        ),
                        key_use,
                    ));
                }
                if map.len() >= self.opts.limits.max_mapping_entries {
                    return Err(with_alias_use(
                        self.err_limit(
                            LimitKind::MaxMappingEntries,
                            self.opts.limits.max_mapping_entries,
                            kspan.start,
                        ),
                        key_use,
                    ));
                }
                if let Err(d) = map.insert_new(k.node, v.node) {
                    let InsertError::Duplicate { existing } = d else {
                        return Err(with_alias_use(
                            self.err(
                                ErrorKind::InvalidKey,
                                kspan.start,
                                "NaN inside a mapping key",
                            ),
                            key_use,
                        ));
                    };
                    let first = map.entry_at(existing).0.source_span();
                    let mut e = with_alias_use(
                        self.err(
                            ErrorKind::DuplicateKey,
                            kspan.start,
                            "duplicate mapping key",
                        ),
                        key_use,
                    );
                    if let Some(f) = first {
                        e = e.with_related(RelatedRole::FirstDefinition, f);
                    }
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Consume a collection's end event and finish it: span end, merge, tag wrapper, anchor.
    fn close(&mut self, o: Open) -> Result<Built, Error> {
        let Open {
            kind,
            anchor,
            class,
            flow,
            span,
            tag_start,
            count,
            weight,
            height,
            last_start,
            last_end,
            ..
        } = o;
        let (_, end_span) = self.next()?;
        let end = if flow {
            self.loc(&end_span.end)?
        } else {
            self.trim_breaks(last_end, last_start)
        };
        let full = Span {
            start: span.start,
            end,
        };
        let value = match kind {
            OpenKind::Seq(mut items) => {
                items.shrink_to_fit();
                Value::Sequence(items)
            }
            OpenKind::Map {
                mut map, directive, ..
            } => {
                if let Some((slot, dspan, source)) = directive {
                    map = self.apply_merge(map, slot, dspan, source)?;
                }
                map.shrink();
                Value::Mapping(map)
            }
        };
        let value = wrap_collection(value, class, full);
        let b = Built {
            node: Node::parsed(value, full, tag_start),
            count,
            weight,
            height: height + 1,
            merge_directive: false,
        };
        if anchor != 0 {
            self.open_anchors.remove(&anchor);
        }
        self.register(anchor, &b)?;
        Ok(b)
    }

    fn collection_guard(&self, depth: usize, at: Location) -> Result<(), Error> {
        if depth + 1 > self.opts.limits.max_depth {
            return Err(self.err_limit(LimitKind::MaxDepth, self.opts.limits.max_depth, at));
        }
        Ok(())
    }

    /// Anchor storage is a copy outside the tree; its weight is charged (monotonic, §6).
    fn register(&mut self, anchor: usize, b: &Built) -> Result<(), Error> {
        if anchor != 0 {
            let at = b.node.source_span().map(|s| s.start).unwrap_or(Location {
                byte_offset: 0,
                line: 1,
                column: 1,
            });
            self.charge(0, b.weight, at, false)?;
            self.anchors.insert(
                anchor,
                Anchored {
                    node: b.node.clone(),
                    count: b.count,
                    weight: b.weight,
                    height: b.height,
                },
            );
        }
        Ok(())
    }

    /// Collection tag policy, decided at the start event (§4.4, event order). A retained
    /// tag adds a wrapper node: one node of overhead plus its tag bytes (§6), charged here,
    /// before the children are built.
    fn collection_tag(
        &self,
        class: &TagClass,
        kind: &str,
        at: Location,
    ) -> Result<(u64, u64), Error> {
        match class {
            TagClass::None | TagClass::NonSpecific => Ok((0, 0)),
            TagClass::Core(c) if *c == kind => Ok((0, 0)),
            TagClass::Core(_) => Err(self.err(
                ErrorKind::TagKindMismatch,
                at,
                "core tag does not match node kind",
            )),
            TagClass::Merge => {
                Err(self.err(ErrorKind::TagKindMismatch, at, "!!merge on a collection"))
            }
            TagClass::Other(tag) => {
                if self.opts.unknown_tags == UnknownTags::Reject {
                    return Err(self.err(ErrorKind::UnknownTag, at, "unknown tag"));
                }
                Ok((1, NODE_OVERHEAD + tag_bytes(tag)))
            }
        }
    }

    /// §5.3 Apply: explicit keys win wherever they appear; earlier sources win; the
    /// directive slot is replaced by unshadowed merged-in entries in source order.
    fn apply_merge(
        &mut self,
        explicit: Mapping,
        slot: usize,
        dspan: Span,
        source: Node,
    ) -> Result<Mapping, Error> {
        let src_span = source.source_span().unwrap_or(dspan);
        let src_use = alias_use(&source);
        let sources: Vec<Mapping> = match source.value {
            Value::Mapping(m) => vec![m],
            Value::Sequence(items) => {
                let mut v = Vec::new();
                for it in items {
                    match it.value {
                        Value::Mapping(m) => v.push(m),
                        _ => {
                            let at = it.source_span().map(|s| s.start).unwrap_or(src_span.start);
                            let e = self.err(
                                ErrorKind::MergeSource,
                                at,
                                "merge source must be a mapping",
                            );
                            return Err(with_alias_use(with_alias_use(e, alias_use(&it)), src_use)
                                .with_related(RelatedRole::MergeDirective, dspan));
                        }
                    }
                }
                v
            }
            _ => {
                let e = self.err(
                    ErrorKind::MergeSource,
                    src_span.start,
                    "merge source must be a mapping or a sequence of mappings",
                );
                return Err(
                    with_alias_use(e, src_use).with_related(RelatedRole::MergeDirective, dspan)
                );
            }
        };
        // §5.3 rule 6: each merged-in entry is a copy, charged to the expansion budgets
        // before it joins the result. Entries move; nothing is cloned.
        let mut merged = Mapping::new();
        for m in sources {
            for (k, v) in m.entries {
                if explicit.get_value(&k.value).is_some() || merged.get_value(&k.value).is_some() {
                    continue;
                }
                let ((kc, kw), (vc, vw)) = (metrics(&k), metrics(&v));
                let e = self.charge(kc + vc, kw + vw + ENTRY_OVERHEAD, dspan.start, true);
                e.map_err(|e| e.with_related(RelatedRole::MergeDirective, dspan))?;
                let via = Some(Via::Merge { directive: dspan });
                let (mut k, mut v) = (k, v);
                k.set_via(via.clone());
                v.set_via(via);
                merged.insert_new(k, v).map_err(|_| internal("merge dup"))?;
            }
        }
        let merged = merged.entries;
        let mut out = Mapping::new();
        let mut explicit_entries = explicit.entries.into_iter();
        for _ in 0..slot {
            let (k, v) = explicit_entries
                .next()
                .ok_or_else(|| internal("merge slot"))?;
            out.insert_new(k, v)
                .map_err(|_| internal("merge explicit dup"))?;
        }
        for (k, v) in merged {
            out.insert_new(k, v).map_err(|_| internal("merge dup"))?;
        }
        for (k, v) in explicit_entries {
            out.insert_new(k, v)
                .map_err(|_| internal("merge explicit dup"))?;
        }
        if out.len() > self.opts.limits.max_mapping_entries {
            return Err(self.err_limit(
                LimitKind::MaxMappingEntries,
                self.opts.limits.max_mapping_entries,
                dspan.start,
            ));
        }
        Ok(out)
    }
}

pub(crate) fn tag_bytes(t: &crate::value::Tag) -> u64 {
    (t.uri.len() + t.handle.len() + t.suffix.len()) as u64
}

/// Wraps a collection whose tag `collection_tag` accepted and charged.
fn wrap_collection(v: Value, class: TagClass, span: Span) -> Value {
    match class {
        TagClass::Other(tag) => {
            let inner = Node::parsed(v, span, None);
            Value::Tagged(Box::new(Tagged {
                tag,
                value: inner,
                inner_plain: false,
            }))
        }
        _ => v,
    }
}

/// Node count and §6 weight of a built subtree (used to charge merge copies).
fn metrics(n: &Node) -> (u64, u64) {
    match &n.value {
        Value::String(s) => (1, NODE_OVERHEAD + s.len() as u64),
        Value::Tagged(t) => {
            let (c, w) = metrics(&t.value);
            (c + 1, w + NODE_OVERHEAD + tag_bytes(&t.tag))
        }
        Value::Sequence(items) => items.iter().fold((1, NODE_OVERHEAD), |(c, w), i| {
            let (ic, iw) = metrics(i);
            (c + ic, w + iw)
        }),
        Value::Mapping(m) => m.iter().fold((1, NODE_OVERHEAD), |(c, w), (k, v)| {
            let (kc, kw) = metrics(k);
            let (vc, vw) = metrics(v);
            (c + kc + vc, w + kw + vw + ENTRY_OVERHEAD)
        }),
        _ => (1, NODE_OVERHEAD),
    }
}

/// The use site of an alias-expanded node (§7.2: site is the definition, related is the use).
fn alias_use(n: &Node) -> Option<Span> {
    match n.via() {
        Some(Via::Alias { use_site }) => Some(*use_site),
        _ => None,
    }
}

fn with_alias_use(e: Error, use_site: Option<Span>) -> Error {
    match use_site {
        Some(u) => e.with_related(RelatedRole::AliasUse, u),
        None => e,
    }
}
