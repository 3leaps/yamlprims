//! Logical charges and copy admission; these counters are not allocator measurements.
use crate::error::{Error, ErrorKind, LimitKind};
use crate::options::Limits;
use crate::value::{Location, Node, Value};

pub(crate) const NODE_OVERHEAD: u64 = 64;
pub(crate) const ENTRY_OVERHEAD: u64 = 24;

#[derive(Default)]
pub(crate) struct Budget {
    nodes: u64,
    expanded: u64,
    owned: u64,
    alias_events: u64,
}

impl Budget {
    pub(crate) fn charge(
        &mut self,
        limits: &Limits,
        count: u64,
        weight: u64,
        expanded: bool,
    ) -> Result<(), (LimitKind, usize)> {
        let l = limits;
        let nodes = self.nodes.checked_add(count);
        if nodes.is_none_or(|n| n > l.max_nodes as u64) {
            return Err((LimitKind::MaxNodes, l.max_nodes));
        }
        if expanded {
            let e = self.expanded.checked_add(count);
            if e.is_none_or(|n| n > l.max_expanded_nodes as u64) {
                return Err((LimitKind::MaxExpandedNodes, l.max_expanded_nodes));
            }
            self.expanded = e.unwrap_or(u64::MAX);
        }
        let owned = self.owned.checked_add(weight);
        if owned.is_none_or(|n| n > l.max_owned_bytes as u64) {
            return Err((LimitKind::MaxOwnedBytes, l.max_owned_bytes));
        }
        self.nodes = nodes.unwrap_or(u64::MAX);
        self.owned = owned.unwrap_or(u64::MAX);
        Ok(())
    }
    pub(crate) fn alias(&mut self, limits: &Limits) -> Result<(), (LimitKind, usize)> {
        self.alias_events += 1;
        if self.alias_events > limits.max_alias_events as u64 {
            return Err((LimitKind::MaxAliasEvents, limits.max_alias_events));
        }
        Ok(())
    }
}

pub(crate) fn limit_err(limit: LimitKind, configured: usize, at: Location) -> Error {
    Error::at(
        ErrorKind::LimitExceeded {
            limit,
            configured: configured as u64,
        },
        at,
        format!("{} ({configured}) exceeded", limit.field_name()),
    )
}

pub(crate) fn tag_bytes(t: &crate::value::Tag) -> u64 {
    (t.uri.len() + t.handle.len() + t.suffix.len()) as u64
}

/// Node count and §6 weight of a built subtree (used to charge merge copies).
pub(crate) fn metrics(n: &Node) -> (u64, u64) {
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
