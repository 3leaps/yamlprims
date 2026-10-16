//! Owned YAML values, strict parsing, typed conversion and bounded wire encoding.
//! The API is under development; see the repository specification and design review.

#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod de;
mod error;
mod load;
mod options;
mod protocol;
mod resolve;
mod value;
mod wire;

pub use error::{
    Error, ErrorDetail, ErrorKind, LimitKind, PathSegment, Related, RelatedRole, Site,
};
pub use options::{Limits, MergeKeys, Options, UnknownTags};
pub use protocol::{Spanned, TaggedValue};
pub use value::{
    key_eq, Document, InsertError, Integer, Location, Mapping, Node, NodeSpan, Span, Tag, Tagged,
    Value, Via,
};

use serde::de::DeserializeOwned;

/// Load exactly one document with default options.
pub fn load(input: &str) -> Result<Document, Error> {
    load_with(input, &Options::default())
}

/// Load exactly one document. A second document is `MultipleDocuments`.
pub fn load_with(input: &str, opts: &Options) -> Result<Document, Error> {
    opts.validate()?;
    let (s, bom) = load::gate_str(input, opts)?;
    one(load::Loader::new(s, bom, opts).load_all(true)?)
}

/// Load exactly one document from raw bytes (BOM and UTF-8 checked).
pub fn load_bytes(input: &[u8], opts: &Options) -> Result<Document, Error> {
    opts.validate()?;
    let (s, bom) = load::gate_bytes(input, opts)?;
    one(load::Loader::new(s, bom, opts).load_all(true)?)
}

fn one(mut docs: Vec<Document>) -> Result<Document, Error> {
    docs.pop()
        .ok_or_else(|| Error::not_source(ErrorKind::Internal, None, "no document"))
}

/// Load every document; all-or-nothing.
pub fn load_all(input: &str) -> Result<Vec<Document>, Error> {
    load_all_with(input, &Options::default())
}

pub fn load_all_with(input: &str, opts: &Options) -> Result<Vec<Document>, Error> {
    opts.validate()?;
    let (s, bom) = load::gate_str(input, opts)?;
    load::Loader::new(s, bom, opts).load_all(false)
}

pub fn load_all_bytes(input: &[u8], opts: &Options) -> Result<Vec<Document>, Error> {
    opts.validate()?;
    let (s, bom) = load::gate_bytes(input, opts)?;
    load::Loader::new(s, bom, opts).load_all(false)
}

/// Typed deserialization of one document with default options.
pub fn from_str<T: DeserializeOwned>(input: &str) -> Result<T, Error> {
    from_str_with(input, &Options::default())
}

pub fn from_str_with<T: DeserializeOwned>(input: &str, opts: &Options) -> Result<T, Error> {
    let doc = load_with(input, opts)?;
    T::deserialize(de::NodeDe::root(&doc.root, opts))
}

/// Typed deserialization of any node, including hand-built trees.
///
/// `from_node` does not reparse source: it does not check that the tree's source was valid
/// YAML. It validates the supplied options and tree budgets before conversion.
pub fn from_node<T: DeserializeOwned>(node: &Node, opts: &Options) -> Result<T, Error> {
    opts.validate()?;
    let mut budget = opts.limits.max_nodes;
    count_nodes(node, &mut budget, 0, opts.limits.max_depth, None, None).map_err(
        |(limit, site, origin)| {
            let configured = match limit {
                LimitKind::MaxDepth => opts.limits.max_depth,
                _ => opts.limits.max_nodes,
            } as u64;
            let e = Error::new(
                ErrorKind::LimitExceeded { limit, configured },
                site,
                "tree exceeds limits",
            );
            match origin {
                Some((role, span)) => e.with_related(role, span),
                None => e,
            }
        },
    )?;
    T::deserialize(de::NodeDe::root(node, opts))
}

type Origin = Option<(RelatedRole, Span)>;

/// Budget walk for `from_node`. The error carries the offending node's site (its source
/// location for parsed nodes, `NotSource` with the nearest source ancestor otherwise, §8)
/// and the nearest alias use or merge directive at or above it, as `related` (§7.2).
fn count_nodes(
    mut n: &Node,
    budget: &mut usize,
    depth: usize,
    max_depth: usize,
    mut last_known: Option<Location>,
    mut origin: Origin,
) -> Result<(), (LimitKind, Site, Origin)> {
    let site = |n: &Node, last_known: Option<Location>| match n.span() {
        NodeSpan::Source(s) => Site::Source(s.start),
        NodeSpan::Synthetic => Site::NotSource { last_known },
    };
    loop {
        if let NodeSpan::Source(s) = n.span() {
            last_known = Some(s.start);
        }
        origin = match n.via() {
            Some(Via::Alias { use_site }) => Some((RelatedRole::AliasUse, *use_site)),
            Some(Via::Merge { directive }) => Some((RelatedRole::MergeDirective, *directive)),
            None => origin,
        };
        *budget = budget
            .checked_sub(1)
            .ok_or_else(|| (LimitKind::MaxNodes, site(n, last_known), origin))?;
        // Tag wrappers do not add depth; walk them in a loop so a chain cannot recurse.
        match &n.value {
            Value::Tagged(t) => n = &t.value,
            _ => break,
        }
    }
    let children: Vec<&Node> = match &n.value {
        Value::Sequence(s) => s.iter().collect(),
        Value::Mapping(m) => m.iter().flat_map(|(k, v)| [k, v]).collect(),
        _ => return Ok(()),
    };
    if depth + 1 > max_depth {
        return Err((LimitKind::MaxDepth, site(n, last_known), origin));
    }
    for c in children {
        count_nodes(c, budget, depth + 1, max_depth, last_known, origin)?;
    }
    Ok(())
}

/// Encode documents in the wire format, bounded by `max_output_bytes`.
pub fn encode(docs: &[Document], opts: &Options) -> Result<String, Error> {
    opts.validate()?;
    wire::encode_documents(docs, opts.limits.max_output_bytes)
}

/// Encode an error result envelope.
pub fn encode_error(err: &Error, opts: &Options) -> Result<String, Error> {
    opts.validate()?;
    wire::encode_error(err, &opts.limits)
}

/// Plain JSON text, failing instead of losing information.
pub fn encode_plain(node: &Node, opts: &Options) -> Result<String, Error> {
    opts.validate()?;
    wire::encode_plain(node, opts.limits.max_output_bytes).map_err(|mut e| {
        if let Site::NotSource {
            last_known: Some(l),
        } = e.site
        {
            e.site = Site::Source(l);
        }
        e
    })
}
