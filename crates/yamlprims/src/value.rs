//! Owned value model (spec §3, §5.1).

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

#[derive(Clone, Debug)]
pub struct Node {
    pub(crate) value: Value,
    pub(crate) span: NodeSpan,
    /// Alias or merge origin and tag position. Most nodes have neither, so they are
    /// boxed together to keep `Node` small.
    pub(crate) extra: Option<Box<Extra>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Extra {
    pub(crate) via: Option<Via>,
    pub(crate) tag_start: Option<Location>,
}

impl Node {
    pub fn value(&self) -> &Value {
        &self.value
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

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub(crate) uri: String,
    pub(crate) handle: String,
    pub(crate) suffix: String,
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
}

#[derive(Clone, Debug)]
pub struct Tagged {
    pub(crate) tag: Tag,
    pub(crate) value: Node,
    /// The inner scalar was written plain (used only for enum variant payloads).
    pub(crate) inner_plain: bool,
}

impl Tagged {
    pub fn tag(&self) -> &Tag {
        &self.tag
    }
    pub fn value(&self) -> &Node {
        &self.value
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) root: Node,
    pub(crate) yaml_version: Option<(u32, u32)>,
    pub(crate) explicit_start: bool,
}

impl Document {
    pub fn root(&self) -> &Node {
        &self.root
    }
    pub fn yaml_version(&self) -> Option<(u32, u32)> {
        self.yaml_version
    }
    pub fn explicit_start(&self) -> bool {
        self.explicit_start
    }
}

// ---- key equality and hashing (§5.1) -------------------------------------

/// Recursive key equality: ignores spans, via, tag_start and tag spelling.
pub fn key_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => norm_bits(*x) == norm_bits(*y),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Sequence(x), Value::Sequence(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| key_eq(&p.value, &q.value))
        }
        (Value::Mapping(x), Value::Mapping(y)) => {
            x.len() == y.len()
                && x.entries.iter().all(|(k, v)| {
                    y.get_value(&k.value)
                        .is_some_and(|w| key_eq(&v.value, &w.value))
                })
        }
        (Value::Tagged(x), Value::Tagged(y)) => {
            x.tag.uri == y.tag.uri && key_eq(&x.value.value, &y.value.value)
        }
        _ => false,
    }
}

fn norm_bits(f: f64) -> u64 {
    if f == 0.0 {
        0
    } else {
        f.to_bits()
    }
}

pub(crate) fn contains_nan(v: &Value) -> bool {
    match v {
        Value::Float(f) => f.is_nan(),
        Value::Sequence(s) => s.iter().any(|n| contains_nan(&n.value)),
        Value::Mapping(m) => m
            .entries
            .iter()
            .any(|(k, v)| contains_nan(&k.value) || contains_nan(&v.value)),
        Value::Tagged(t) => contains_nan(&t.value.value),
        _ => false,
    }
}

fn key_hash<S: BuildHasher>(state: &S, v: &Value) -> u64 {
    let mut h = state.build_hasher();
    hash_into(state, v, &mut h);
    h.finish()
}

fn hash_into<S: BuildHasher, H: Hasher>(state: &S, v: &Value, h: &mut H) {
    match v {
        Value::Null => 0u8.hash(h),
        Value::Bool(b) => {
            1u8.hash(h);
            b.hash(h)
        }
        Value::Int(i) => {
            2u8.hash(h);
            i.hash(h)
        }
        Value::Float(f) => {
            3u8.hash(h);
            norm_bits(*f).hash(h)
        }
        Value::String(s) => {
            4u8.hash(h);
            s.hash(h)
        }
        Value::Sequence(s) => {
            5u8.hash(h);
            s.len().hash(h);
            for n in s {
                hash_into(state, &n.value, h);
            }
        }
        Value::Mapping(m) => {
            // Order-independent: sum of per-entry hashes.
            6u8.hash(h);
            m.len().hash(h);
            let mut acc = 0u64;
            for (k, v) in &m.entries {
                let mut eh = state.build_hasher();
                hash_into(state, &k.value, &mut eh);
                hash_into(state, &v.value, &mut eh);
                acc = acc.wrapping_add(eh.finish());
            }
            acc.hash(h)
        }
        Value::Tagged(t) => {
            7u8.hash(h);
            t.tag.uri.hash(h);
            hash_into(state, &t.value.value, h)
        }
    }
}

/// Ordered mapping with an index that stays valid: keys cannot be mutated in place.
#[derive(Clone, Debug, Default)]
pub struct Mapping {
    pub(crate) entries: Vec<(Node, Node)>,
    /// Built on the first insert, so an empty mapping does not allocate.
    index: Option<Box<Index>>,
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
