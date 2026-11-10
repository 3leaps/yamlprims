//! Owned value model (spec §3, §5.1).

use crate::error::{Error, ErrorKind};
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

/// A source position. `byte_offset` indexes the original input bytes, including any BOM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub byte_offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Location,
    pub end: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeSpan {
    Source(Span),
    Synthetic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Via {
    Alias { use_site: Span },
    Merge { directive: Span },
}

pub struct Node {
    pub(crate) value: Value,
    pub(crate) span: NodeSpan,
    /// Alias or merge origin and tag position. Most nodes have neither, so they are
    /// boxed together to keep `Node` small.
    pub(crate) extra: Option<Box<Extra>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Extra {
    pub(crate) via: Option<Via>,
    pub(crate) tag_start: Option<Location>,
}

impl Node {
    pub fn value(&self) -> &Value {
        &self.value
    }
    /// Move the inner [`Value`] out of this node without triggering recursive drop.
    pub fn into_value(mut self) -> Value {
        std::mem::replace(&mut self.value, Value::Null)
    }
    pub fn span(&self) -> &NodeSpan {
        &self.span
    }
    pub fn via(&self) -> Option<&Via> {
        self.extra.as_ref()?.via.as_ref()
    }
    pub fn tag_start(&self) -> Option<Location> {
        self.extra.as_ref()?.tag_start
    }
    /// A hand-built node with no source position.
    pub fn synthetic(value: Value) -> Node {
        Node {
            value,
            span: NodeSpan::Synthetic,
            extra: None,
        }
    }
    pub(crate) fn parsed(value: Value, span: Span, tag_start: Option<Location>) -> Node {
        let extra = tag_start.map(|t| {
            Box::new(Extra {
                via: None,
                tag_start: Some(t),
            })
        });
        Node {
            value,
            span: NodeSpan::Source(span),
            extra,
        }
    }
    pub(crate) fn set_via(&mut self, via: Option<Via>) {
        match (&mut self.extra, via) {
            (Some(e), via) => e.via = via,
            (None, None) => {}
            (None, via) => {
                self.extra = Some(Box::new(Extra {
                    via,
                    tag_start: None,
                }))
            }
        }
    }
    pub(crate) fn set_tag_start(&mut self, tag_start: Location) {
        self.extra.get_or_insert_with(Default::default).tag_start = Some(tag_start);
    }
    pub(crate) fn source_span(&self) -> Option<Span> {
        match self.span {
            NodeSpan::Source(s) => Some(s),
            NodeSpan::Synthetic => None,
        }
    }
    /// Where this node occurs in the source: the alias use site for an
    /// alias-expanded node, else its own span. Containing spans and error
    /// sites use this, so they never point back at the anchor definition.
    pub(crate) fn position_span(&self) -> Option<Span> {
        match self.via() {
            Some(Via::Alias { use_site }) => Some(*use_site),
            _ => self.source_span(),
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let mut stack: Vec<Value> = Vec::new();
        match &mut self.value {
            Value::Sequence(items) if !items.is_empty() => {
                for mut n in std::mem::take(items) {
                    let v = std::mem::replace(&mut n.value, Value::Null);
                    if matches!(v, Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)) {
                        stack.push(v);
                    }
                }
            }
            Value::Mapping(m) if !m.entries.is_empty() => {
                for (mut k, mut v) in std::mem::take(&mut m.entries) {
                    let kv = std::mem::replace(&mut k.value, Value::Null);
                    if matches!(
                        kv,
                        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)
                    ) {
                        stack.push(kv);
                    }
                    let vv = std::mem::replace(&mut v.value, Value::Null);
                    if matches!(
                        vv,
                        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)
                    ) {
                        stack.push(vv);
                    }
                }
            }
            Value::Tagged(_) => {
                if let Value::Tagged(mut t) = std::mem::replace(&mut self.value, Value::Null) {
                    let v = std::mem::replace(&mut t.value.value, Value::Null);
                    if matches!(v, Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)) {
                        stack.push(v);
                    }
                }
            }
            _ => return,
        }
        while let Some(val) = stack.pop() {
            match val {
                Value::Sequence(items) => {
                    for mut n in items {
                        let v = std::mem::replace(&mut n.value, Value::Null);
                        if matches!(v, Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)) {
                            stack.push(v);
                        }
                    }
                }
                Value::Mapping(mut m) => {
                    for (mut k, mut v) in std::mem::take(&mut m.entries) {
                        let kv = std::mem::replace(&mut k.value, Value::Null);
                        if matches!(
                            kv,
                            Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)
                        ) {
                            stack.push(kv);
                        }
                        let vv = std::mem::replace(&mut v.value, Value::Null);
                        if matches!(
                            vv,
                            Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)
                        ) {
                            stack.push(vv);
                        }
                    }
                }
                Value::Tagged(mut t) => {
                    let v = std::mem::replace(&mut t.value.value, Value::Null);
                    if matches!(v, Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_)) {
                        stack.push(v);
                    }
                }
                _ => {}
            }
        }
    }
}

#[non_exhaustive]
pub enum Value {
    Null,
    Bool(bool),
    Int(Integer),
    Float(f64),
    String(String),
    Sequence(Vec<Node>),
    Mapping(Mapping),
    Tagged(Box<Tagged>),
}

impl Value {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Sequence(_) => "sequence",
            Value::Mapping(_) => "mapping",
            Value::Tagged(_) => "tagged",
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_integer(&self) -> Option<&Integer> {
        match self {
            Value::Int(i) => Some(i),
            _ => None,
        }
    }
    pub fn as_mapping(&self) -> Option<&Mapping> {
        match self {
            Value::Mapping(m) => Some(m),
            _ => None,
        }
    }
    pub fn as_sequence(&self) -> Option<&[Node]> {
        match self {
            Value::Sequence(s) => Some(s),
            _ => None,
        }
    }
}

/// Exact integer in `i128::MIN..= u128::MAX`, one representation per number.
///
/// Stored as two `u64` halves and a sign so that `Integer` (and so `Value` and `Node`)
/// keeps 8-byte alignment; a `u128` field would force 16.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Integer {
    hi: u64,
    lo: u64,
    negative: bool,
}

#[derive(Clone, Copy, Debug)]
enum Repr {
    NonNegative(u128),
    Negative(i128), // always < 0
}

impl std::fmt::Debug for Integer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Integer").field(&self.repr()).finish()
    }
}

impl Integer {
    fn from_bits(bits: u128, negative: bool) -> Self {
        Integer {
            hi: (bits >> 64) as u64,
            lo: bits as u64,
            negative,
        }
    }
    fn repr(&self) -> Repr {
        let bits = (u128::from(self.hi) << 64) | u128::from(self.lo);
        if self.negative {
            Repr::Negative(bits as i128)
        } else {
            Repr::NonNegative(bits)
        }
    }
    pub fn from_u128(v: u128) -> Self {
        Integer::from_bits(v, false)
    }
    pub fn from_i128(v: i128) -> Self {
        Integer::from_bits(v as u128, v < 0)
    }
    pub fn to_u128(&self) -> Option<u128> {
        match self.repr() {
            Repr::NonNegative(v) => Some(v),
            Repr::Negative(_) => None,
        }
    }
    pub fn to_i128(&self) -> Option<i128> {
        match self.repr() {
            Repr::NonNegative(v) => i128::try_from(v).ok(),
            Repr::Negative(v) => Some(v),
        }
    }
    pub fn to_u64(&self) -> Option<u64> {
        self.to_u128().and_then(|v| u64::try_from(v).ok())
    }
    pub fn to_i64(&self) -> Option<i64> {
        self.to_i128().and_then(|v| i64::try_from(v).ok())
    }
    pub fn is_negative(&self) -> bool {
        self.negative
    }
    /// Exact conversion to f64, or None if the value would round.
    pub fn to_f64_exact(&self) -> Option<f64> {
        match self.repr() {
            Repr::NonNegative(v) => {
                let f = v as f64;
                // u128::MAX as f64 is exactly 2^128, outside the range.
                (f < u128::MAX as f64 && f as u128 == v).then_some(f)
            }
            Repr::Negative(v) => {
                let f = v as f64;
                (f as i128 == v).then_some(f)
            }
        }
    }
}

impl std::fmt::Display for Integer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.repr() {
            Repr::NonNegative(v) => write!(f, "{v}"),
            Repr::Negative(v) => write!(f, "{v}"),
        }
    }
}

pub(crate) const CORE_TAG_URIS: [&str; 7] = [
    "tag:yaml.org,2002:null",
    "tag:yaml.org,2002:bool",
    "tag:yaml.org,2002:int",
    "tag:yaml.org,2002:float",
    "tag:yaml.org,2002:str",
    "tag:yaml.org,2002:seq",
    "tag:yaml.org,2002:map",
];

fn has_valid_uri_scheme(uri: &str) -> bool {
    let Some((scheme, _rest)) = uri.split_once(':') else {
        return false;
    };
    if scheme.is_empty() {
        return false;
    }
    let mut bytes = scheme.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

pub(crate) fn is_valid_non_core_tag_uri(uri: &str) -> bool {
    if uri.is_empty() || CORE_TAG_URIS.contains(&uri) {
        return false;
    }
    if uri.starts_with('!') {
        uri.len() >= 2
    } else {
        has_valid_uri_scheme(uri)
    }
}

pub(crate) fn is_valid_tag_spelling(uri: &str, handle: &str, suffix: &str) -> bool {
    match handle {
        "" => uri == suffix,
        "!" | "!!" => {
            !suffix.is_empty()
                && uri
                    .strip_suffix(suffix)
                    .is_some_and(|prefix| !prefix.is_empty())
        }
        h if h.len() >= 3 && h.starts_with('!') && h.ends_with('!') => {
            let word = &h.as_bytes()[1..h.len() - 1];
            word.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-')
                && !suffix.is_empty()
                && uri
                    .strip_suffix(suffix)
                    .is_some_and(|prefix| !prefix.is_empty())
        }
        _ => false,
    }
}

#[derive(Clone, Debug, Eq)]
pub struct Tag {
    pub(crate) uri: String,
    pub(crate) handle: String,
    pub(crate) suffix: String,
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        self.uri == other.uri
    }
}

impl Tag {
    pub fn uri(&self) -> &str {
        &self.uri
    }
    pub fn handle(&self) -> &str {
        &self.handle
    }
    pub fn suffix(&self) -> &str {
        &self.suffix
    }
    pub fn new(uri: impl Into<String>) -> Tag {
        let uri = uri.into();
        Tag {
            handle: String::new(),
            suffix: uri.clone(),
            uri,
        }
    }
    /// Construct a validated non-core [`Tag`] with empty handle and `suffix == uri` (spec §3).
    pub fn try_new(uri: impl Into<String>) -> Result<Tag, Error> {
        let uri = uri.into();
        if !is_valid_non_core_tag_uri(&uri) {
            return Err(Error::not_source(
                ErrorKind::InvalidOptions,
                None,
                "tag URI must be a non-empty local (!name) or RFC 3986 global non-core URI",
            ));
        }
        Ok(Tag {
            handle: String::new(),
            suffix: uri.clone(),
            uri,
        })
    }
    /// Construct a validated non-core [`Tag`] with explicit handle and suffix spelling (spec §3).
    pub fn with_spelling(
        uri: impl Into<String>,
        handle: impl Into<String>,
        suffix: impl Into<String>,
    ) -> Result<Tag, Error> {
        let uri = uri.into();
        let handle = handle.into();
        let suffix = suffix.into();
        if !is_valid_non_core_tag_uri(&uri) {
            return Err(Error::not_source(
                ErrorKind::InvalidOptions,
                None,
                "tag URI must be a non-empty local (!name) or RFC 3986 global non-core URI",
            ));
        }
        if !is_valid_tag_spelling(&uri, &handle, &suffix) {
            return Err(Error::not_source(
                ErrorKind::InvalidOptions,
                None,
                "tag handle and suffix are inconsistent with tag URI",
            ));
        }
        Ok(Tag {
            uri,
            handle,
            suffix,
        })
    }
}

pub struct Tagged {
    pub(crate) tag: Tag,
    pub(crate) value: Node,
    /// The inner scalar was written plain (used only for enum variant payloads).
    pub(crate) inner_plain: bool,
}

impl Tagged {
    /// Construct a validated [`Tagged`] node enforcing §3 and §4.4 inner-kind rules.
    pub fn new(tag: Tag, value: Node) -> Result<Tagged, Error> {
        if !is_valid_non_core_tag_uri(&tag.uri)
            || !is_valid_tag_spelling(&tag.uri, &tag.handle, &tag.suffix)
        {
            return Err(Error::not_source(
                ErrorKind::InvalidOptions,
                None,
                "invalid or core tag on Tagged node",
            ));
        }
        let valid_inner = if tag.uri == "tag:yaml.org,2002:merge" {
            matches!(value.value(), Value::String(_))
        } else {
            matches!(
                value.value(),
                Value::String(_) | Value::Sequence(_) | Value::Mapping(_)
            )
        };
        if !valid_inner {
            return Err(Error::not_source(
                ErrorKind::TagKindMismatch,
                None,
                "tagged value inner kind is not permitted for this tag",
            ));
        }
        Ok(Tagged {
            tag,
            value,
            inner_plain: false,
        })
    }
    pub fn tag(&self) -> &Tag {
        &self.tag
    }
    pub fn value(&self) -> &Node {
        &self.value
    }
    pub fn into_parts(self) -> (Tag, Node) {
        (self.tag, self.value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub(crate) root: Node,
    pub(crate) yaml_version: Option<(u32, u32)>,
    pub(crate) explicit_start: bool,
}

impl Document {
    /// Construct a document with `yaml_version = None` and `explicit_start = false` (spec §3).
    pub fn new(root: Node) -> Document {
        Document {
            root,
            yaml_version: None,
            explicit_start: false,
        }
    }
    /// Construct a document with explicit version and start-marker metadata (spec §3).
    pub fn with_metadata(
        root: Node,
        yaml_version: Option<(u32, u32)>,
        explicit_start: bool,
    ) -> Result<Document, Error> {
        if let Some((major, minor)) = yaml_version {
            if major != 1 || minor > 999_999_999 {
                return Err(Error::not_source(
                    ErrorKind::UnsupportedYamlVersion,
                    None,
                    "unsupported YAML version",
                ));
            }
        }
        Ok(Document {
            root,
            yaml_version,
            explicit_start,
        })
    }
    pub fn root(&self) -> &Node {
        &self.root
    }
    pub fn into_root(self) -> Node {
        self.root
    }
    pub fn yaml_version(&self) -> Option<(u32, u32)> {
        self.yaml_version
    }
    pub fn explicit_start(&self) -> bool {
        self.explicit_start
    }
}

// ---- key equality and hashing (§5.1) -------------------------------------

/// Iterative key equality: ignores spans, via, tag_start and tag spelling.
pub fn key_eq(a: &Value, b: &Value) -> bool {
    let mut stack = vec![(a, b)];
    while let Some((mut left, mut right)) = stack.pop() {
        loop {
            match (left, right) {
                (Value::Tagged(x), Value::Tagged(y)) => {
                    if x.tag.uri != y.tag.uri {
                        return false;
                    }
                    left = &x.value.value;
                    right = &y.value.value;
                }
                _ => break,
            }
        }
        match (left, right) {
            (Value::Null, Value::Null) => {}
            (Value::Bool(x), Value::Bool(y)) => {
                if x != y {
                    return false;
                }
            }
            (Value::Int(x), Value::Int(y)) => {
                if x != y {
                    return false;
                }
            }
            (Value::Float(x), Value::Float(y)) => {
                if norm_bits(*x) != norm_bits(*y) {
                    return false;
                }
            }
            (Value::String(x), Value::String(y)) => {
                if x != y {
                    return false;
                }
            }
            (Value::Sequence(x), Value::Sequence(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                for (p, q) in x.iter().zip(y).rev() {
                    stack.push((&p.value, &q.value));
                }
            }
            (Value::Mapping(x), Value::Mapping(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                if x.is_empty() {
                    continue;
                }
                let Some(y_idx) = y.index.as_deref() else {
                    return false;
                };
                for (k, v) in x.entries.iter().rev() {
                    let h = key_hash(&y_idx.state, &k.value);
                    let Some(&head) = y_idx.heads.get(&h) else {
                        return false;
                    };
                    if y_idx.prev[head] == NO_ENTRY {
                        stack.push((&v.value, &y.entries[head].1.value));
                        stack.push((&k.value, &y.entries[head].0.value));
                    } else {
                        let Some(w) = y.get_value(&k.value) else {
                            return false;
                        };
                        stack.push((&v.value, &w.value));
                    }
                }
            }
            _ => return false,
        }
    }
    true
}

fn norm_bits(f: f64) -> u64 {
    if f == 0.0 {
        0
    } else {
        f.to_bits()
    }
}

pub(crate) fn contains_nan(root: &Value) -> bool {
    let mut stack = vec![root];
    while let Some(v) = stack.pop() {
        match v {
            Value::Float(f) if f.is_nan() => return true,
            Value::Sequence(s) => {
                for n in s {
                    stack.push(&n.value);
                }
            }
            Value::Mapping(m) => {
                for (k, v) in &m.entries {
                    stack.push(&k.value);
                    stack.push(&v.value);
                }
            }
            Value::Tagged(t) => stack.push(&t.value.value),
            _ => {}
        }
    }
    false
}

fn key_hash<S: BuildHasher>(state: &S, root: &Value) -> u64 {
    enum Frame<'a, H> {
        Val(&'a Value),
        MapIter {
            entries: std::slice::Iter<'a, (Node, Node)>,
            acc: u64,
            saved: H,
        },
        MapEntryDone {
            entries: std::slice::Iter<'a, (Node, Node)>,
            acc: u64,
            saved: H,
        },
    }
    let mut cur = state.build_hasher();
    let mut stack: Vec<Frame<'_, S::Hasher>> = vec![Frame::Val(root)];
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Val(v) => match v {
                Value::Null => 0u8.hash(&mut cur),
                Value::Bool(b) => {
                    1u8.hash(&mut cur);
                    b.hash(&mut cur);
                }
                Value::Int(i) => {
                    2u8.hash(&mut cur);
                    i.hash(&mut cur);
                }
                Value::Float(f) => {
                    3u8.hash(&mut cur);
                    norm_bits(*f).hash(&mut cur);
                }
                Value::String(s) => {
                    4u8.hash(&mut cur);
                    s.hash(&mut cur);
                }
                Value::Sequence(s) => {
                    5u8.hash(&mut cur);
                    s.len().hash(&mut cur);
                    for n in s.iter().rev() {
                        stack.push(Frame::Val(&n.value));
                    }
                }
                Value::Mapping(m) => {
                    6u8.hash(&mut cur);
                    m.len().hash(&mut cur);
                    let saved = std::mem::replace(&mut cur, state.build_hasher());
                    stack.push(Frame::MapIter {
                        entries: m.entries.iter(),
                        acc: 0,
                        saved,
                    });
                }
                Value::Tagged(t) => {
                    7u8.hash(&mut cur);
                    t.tag.uri.hash(&mut cur);
                    stack.push(Frame::Val(&t.value.value));
                }
            },
            Frame::MapIter {
                mut entries,
                acc,
                saved,
            } => {
                if let Some((k, v)) = entries.next() {
                    cur = state.build_hasher();
                    stack.push(Frame::MapEntryDone {
                        entries,
                        acc,
                        saved,
                    });
                    stack.push(Frame::Val(&v.value));
                    stack.push(Frame::Val(&k.value));
                } else {
                    cur = saved;
                    acc.hash(&mut cur);
                }
            }
            Frame::MapEntryDone {
                entries,
                acc,
                saved,
            } => {
                let acc = acc.wrapping_add(cur.finish());
                stack.push(Frame::MapIter {
                    entries,
                    acc,
                    saved,
                });
            }
        }
    }
    cur.finish()
}

/// Ordered mapping with an index that stays valid: keys cannot be mutated in place.
#[derive(Default)]
pub struct Mapping {
    pub(crate) entries: Vec<(Node, Node)>,
    /// Built on the first insert, so an empty mapping does not allocate.
    index: Option<Box<Index>>,
}

impl Clone for Node {
    fn clone(&self) -> Self {
        clone_node_iterative(self)
    }
}

impl Clone for Value {
    fn clone(&self) -> Self {
        match self {
            Value::Null => Value::Null,
            Value::Bool(b) => Value::Bool(*b),
            Value::Int(i) => Value::Int(*i),
            Value::Float(f) => Value::Float(*f),
            Value::String(s) => Value::String(s.clone()),
            Value::Sequence(items) => {
                Value::Sequence(items.iter().map(clone_node_iterative).collect())
            }
            Value::Mapping(m) => Value::Mapping(m.clone()),
            Value::Tagged(t) => Value::Tagged(Box::new((**t).clone())),
        }
    }
}

impl Clone for Mapping {
    fn clone(&self) -> Self {
        let entries = self
            .entries
            .iter()
            .map(|(k, v)| (clone_node_iterative(k), clone_node_iterative(v)))
            .collect();
        Mapping {
            entries,
            index: self.index.clone(),
        }
    }
}

impl Clone for Tagged {
    fn clone(&self) -> Self {
        Tagged {
            tag: self.tag.clone(),
            value: clone_node_iterative(&self.value),
            inner_plain: self.inner_plain,
        }
    }
}

fn clone_node_iterative(root: &Node) -> Node {
    enum Work<'a> {
        Node(&'a Node),
        FinishSeq {
            span: NodeSpan,
            extra: Option<Box<Extra>>,
            len: usize,
        },
        FinishMap {
            span: NodeSpan,
            extra: Option<Box<Extra>>,
            len: usize,
            index: Option<Box<Index>>,
        },
        FinishTagged {
            span: NodeSpan,
            extra: Option<Box<Extra>>,
            tag: Tag,
            inner_plain: bool,
        },
    }

    let mut work: Vec<Work<'_>> = vec![Work::Node(root)];
    let mut results: Vec<Node> = Vec::new();
    while let Some(step) = work.pop() {
        match step {
            Work::Node(n) => match &n.value {
                Value::Null => results.push(Node {
                    value: Value::Null,
                    span: n.span.clone(),
                    extra: n.extra.clone(),
                }),
                Value::Bool(b) => results.push(Node {
                    value: Value::Bool(*b),
                    span: n.span.clone(),
                    extra: n.extra.clone(),
                }),
                Value::Int(i) => results.push(Node {
                    value: Value::Int(*i),
                    span: n.span.clone(),
                    extra: n.extra.clone(),
                }),
                Value::Float(f) => results.push(Node {
                    value: Value::Float(*f),
                    span: n.span.clone(),
                    extra: n.extra.clone(),
                }),
                Value::String(s) => results.push(Node {
                    value: Value::String(s.clone()),
                    span: n.span.clone(),
                    extra: n.extra.clone(),
                }),
                Value::Sequence(items) => {
                    work.push(Work::FinishSeq {
                        span: n.span.clone(),
                        extra: n.extra.clone(),
                        len: items.len(),
                    });
                    for item in items.iter().rev() {
                        work.push(Work::Node(item));
                    }
                }
                Value::Mapping(m) => {
                    work.push(Work::FinishMap {
                        span: n.span.clone(),
                        extra: n.extra.clone(),
                        len: m.entries.len(),
                        index: m.index.clone(),
                    });
                    for (k, v) in m.entries.iter().rev() {
                        work.push(Work::Node(v));
                        work.push(Work::Node(k));
                    }
                }
                Value::Tagged(t) => {
                    work.push(Work::FinishTagged {
                        span: n.span.clone(),
                        extra: n.extra.clone(),
                        tag: t.tag.clone(),
                        inner_plain: t.inner_plain,
                    });
                    work.push(Work::Node(&t.value));
                }
            },
            Work::FinishSeq { span, extra, len } => {
                let items = results.split_off(results.len() - len);
                results.push(Node {
                    value: Value::Sequence(items),
                    span,
                    extra,
                });
            }
            Work::FinishMap {
                span,
                extra,
                len,
                index,
            } => {
                let tail = results.split_off(results.len() - 2 * len);
                let mut iter = tail.into_iter();
                let mut entries = Vec::with_capacity(len);
                while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
                    entries.push((k, v));
                }
                results.push(Node {
                    value: Value::Mapping(Mapping { entries, index }),
                    span,
                    extra,
                });
            }
            Work::FinishTagged {
                span,
                extra,
                tag,
                inner_plain,
            } => {
                let value = results.pop().expect("tagged inner node");
                results.push(Node {
                    value: Value::Tagged(Box::new(Tagged {
                        tag,
                        value,
                        inner_plain,
                    })),
                    span,
                    extra,
                });
            }
        }
    }
    results.pop().expect("cloned root node")
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        let mut stack = vec![(self, other)];
        while let Some((a, b)) = stack.pop() {
            if a.span != b.span || a.extra != b.extra {
                return false;
            }
            if !push_value_eq(&a.value, &b.value, &mut stack) {
                return false;
            }
        }
        true
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let mut stack = Vec::new();
        if !push_value_eq(self, other, &mut stack) {
            return false;
        }
        while let Some((a, b)) = stack.pop() {
            if a.span != b.span || a.extra != b.extra {
                return false;
            }
            if !push_value_eq(&a.value, &b.value, &mut stack) {
                return false;
            }
        }
        true
    }
}

impl PartialEq for Mapping {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl PartialEq for Tagged {
    fn eq(&self, other: &Self) -> bool {
        self.tag == other.tag && self.inner_plain == other.inner_plain && self.value == other.value
    }
}

fn push_value_eq<'a>(a: &'a Value, b: &'a Value, stack: &mut Vec<(&'a Node, &'a Node)>) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Sequence(x), Value::Sequence(y)) => {
            if x.len() != y.len() {
                return false;
            }
            for (p, q) in x.iter().zip(y).rev() {
                stack.push((p, q));
            }
            true
        }
        (Value::Mapping(x), Value::Mapping(y)) => {
            if x.entries.len() != y.entries.len() {
                return false;
            }
            for ((k1, v1), (k2, v2)) in x.entries.iter().zip(&y.entries).rev() {
                stack.push((v1, v2));
                stack.push((k1, k2));
            }
            true
        }
        (Value::Tagged(x), Value::Tagged(y)) => {
            if x.tag != y.tag || x.inner_plain != y.inner_plain {
                return false;
            }
            stack.push((&x.value, &y.value));
            true
        }
        _ => false,
    }
}

std::thread_local! {
    static DEBUG_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct DebugDepthGuard;

impl DebugDepthGuard {
    fn try_enter() -> Option<Self> {
        DEBUG_DEPTH.with(|d| {
            let cur = d.get();
            if cur >= 255 {
                None
            } else {
                d.set(cur + 1);
                Some(DebugDepthGuard)
            }
        })
    }
}

impl Drop for DebugDepthGuard {
    fn drop(&mut self) {
        DEBUG_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("value", &self.value)
            .field("span", &self.span)
            .field("extra", &self.extra)
            .finish()
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => f.write_str("Null"),
            Value::Bool(b) => f.debug_tuple("Bool").field(b).finish(),
            Value::Int(i) => f.debug_tuple("Int").field(i).finish(),
            Value::Float(fl) => f.debug_tuple("Float").field(fl).finish(),
            Value::String(s) => f.debug_tuple("String").field(s).finish(),
            Value::Sequence(items) => {
                if let Some(_g) = DebugDepthGuard::try_enter() {
                    f.debug_tuple("Sequence").field(items).finish()
                } else {
                    f.write_str("Sequence(...)")
                }
            }
            Value::Mapping(m) => {
                if let Some(_g) = DebugDepthGuard::try_enter() {
                    f.debug_tuple("Mapping").field(m).finish()
                } else {
                    f.write_str("Mapping(...)")
                }
            }
            Value::Tagged(t) => {
                if let Some(_g) = DebugDepthGuard::try_enter() {
                    f.debug_tuple("Tagged").field(t).finish()
                } else {
                    f.write_str("Tagged(...)")
                }
            }
        }
    }
}

impl std::fmt::Debug for Mapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mapping")
            .field("entries", &self.entries)
            .field("index", &self.index)
            .finish()
    }
}

impl std::fmt::Debug for Tagged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tagged")
            .field("tag", &self.tag)
            .field("value", &self.value)
            .field("inner_plain", &self.inner_plain)
            .finish()
    }
}

const NO_ENTRY: usize = usize::MAX;

/// Hash index over `entries`. Keys are hashed once with a per-mapping `RandomState`
/// (SipHash with random keys), and that hash is the table key. Entries whose keys share a
/// hash are chained through `prev`.
#[derive(Clone, Debug, Default)]
struct Index {
    state: RandomState,
    /// Key hash -> position of the last entry with that hash.
    heads: HashMap<u64, usize, BuildHasherDefault<PassThrough>>,
    /// Position -> earlier position with the same hash, or `NO_ENTRY`.
    prev: Vec<usize>,
}

/// The table key is already a keyed hash; use it as is.
#[derive(Default)]
struct PassThrough(u64);

impl Hasher for PassThrough {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(*b);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
}

impl Index {
    fn push(&mut self, h: u64, pos: usize) {
        let prev = self.heads.insert(h, pos).unwrap_or(NO_ENTRY);
        self.prev.push(prev);
    }
}

/// Why `Mapping::insert_new` refused a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertError {
    /// A key equal to an existing one (§5.1); carries the existing entry position.
    Duplicate { existing: usize },
    /// The key contains NaN at any depth (§5.1), so it has no equality.
    InvalidKey,
}

impl Mapping {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&Node, &Node)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
    fn find(&self, key: &Value) -> Option<usize> {
        let idx = self.index.as_deref()?;
        let mut i = *idx.heads.get(&key_hash(&idx.state, key))?;
        while i != NO_ENTRY {
            if key_eq(&self.entries[i].0.value, key) {
                return Some(i);
            }
            i = idx.prev[i];
        }
        None
    }
    pub fn get_value(&self, key: &Value) -> Option<&Node> {
        self.find(key).map(|i| &self.entries[i].1)
    }
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.get_value(&Value::String(key.to_owned()))
    }
    pub fn get_mut(&mut self, key: &Value) -> Option<&mut Node> {
        let i = self.find(key)?;
        Some(&mut self.entries[i].1)
    }
    /// Insert a new key. Fails on a key containing NaN at any depth and on a key equal
    /// to an existing one, so a hand-built mapping keeps the same invariants as a parsed one.
    pub fn insert_new(&mut self, key: Node, value: Node) -> Result<(), InsertError> {
        if contains_nan(&key.value) {
            return Err(InsertError::InvalidKey);
        }
        if let Some(existing) = self.find(&key.value) {
            return Err(InsertError::Duplicate { existing });
        }
        let idx = self.index.get_or_insert_with(Default::default);
        idx.push(key_hash(&idx.state, &key.value), self.entries.len());
        self.entries.push((key, value));
        Ok(())
    }
    /// Replace the value of an existing key, keeping its position.
    pub fn replace_value(&mut self, key: &Value, value: Node) -> Option<Node> {
        let i = self.find(key)?;
        Some(std::mem::replace(&mut self.entries[i].1, value))
    }
    pub fn remove(&mut self, key: &Value) -> Option<(Node, Node)> {
        let i = self.find(key)?;
        let e = self.entries.remove(i);
        self.rebuild_index();
        Some(e)
    }
    pub(crate) fn rebuild_index(&mut self) {
        let Some(idx) = self.index.as_deref_mut() else {
            return;
        };
        idx.heads.clear();
        idx.prev.clear();
        for (i, (k, _)) in self.entries.iter().enumerate() {
            idx.push(key_hash(&idx.state, &k.value), i);
        }
    }
    /// Drop growth slack once a parsed mapping is complete.
    pub(crate) fn shrink(&mut self) {
        self.entries.shrink_to_fit();
        if let Some(idx) = self.index.as_deref_mut() {
            idx.heads.shrink_to_fit();
            idx.prev.shrink_to_fit();
        }
    }
    pub(crate) fn entry_at(&self, i: usize) -> &(Node, Node) {
        &self.entries[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Node {
        Node::synthetic(Value::String(v.into()))
    }

    /// Keys whose hashes collide are chained through `prev`. Real SipHash collisions cannot
    /// be produced in a test, so the chain is forced: every key's hash heads a chain over
    /// all entries, newest first.
    #[test]
    fn colliding_hashes_are_chained() {
        let mut m = Mapping::new();
        for k in ["a", "b", "c"] {
            m.insert_new(s(k), s(k)).unwrap();
        }
        let idx = m.index.as_deref_mut().unwrap();
        let hashes: Vec<u64> = m
            .entries
            .iter()
            .map(|(k, _)| key_hash(&idx.state, &k.value))
            .collect();
        idx.heads.clear();
        idx.prev = vec![NO_ENTRY, 0, 1];
        for h in hashes {
            idx.heads.insert(h, 2);
        }
        for (i, k) in ["a", "b", "c"].iter().enumerate() {
            assert_eq!(m.find(&Value::String((*k).into())), Some(i), "{k}");
        }
        assert_eq!(m.find(&Value::String("d".into())), None);
        assert_eq!(
            m.insert_new(s("a"), s("x")),
            Err(InsertError::Duplicate { existing: 0 })
        );
    }

    #[test]
    fn empty_mapping_has_no_index() {
        let m = Mapping::new();
        assert!(m.index.is_none());
        assert!(m.get("a").is_none());
    }

    #[test]
    fn integer_round_trips_and_one_representation() {
        for v in [i128::MIN, -1, 0, 1, i64::MAX as i128, i128::MAX] {
            assert_eq!(Integer::from_i128(v).to_i128(), Some(v));
        }
        for v in [0u128, u64::MAX as u128, u64::MAX as u128 + 1, u128::MAX] {
            assert_eq!(Integer::from_u128(v).to_u128(), Some(v));
        }
        assert_eq!(Integer::from_i128(5), Integer::from_u128(5));
        assert_eq!(Integer::from_i128(-1).to_u128(), None);
        assert_eq!(Integer::from_u128(u128::MAX).to_i128(), None);
        assert!(Integer::from_i128(-1).is_negative() && !Integer::from_u128(0).is_negative());
        assert_eq!(
            Integer::from_i128(i128::MIN).to_string(),
            i128::MIN.to_string()
        );
    }
}
