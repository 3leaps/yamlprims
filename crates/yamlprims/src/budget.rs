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

/// Final node count, §6 weight and collection height (used for merge copies).
pub(crate) fn metrics(root: &Node) -> (u64, u64, usize) {
    enum Step<'a> {
        Enter(&'a Node),
        FinishTagged(&'a crate::value::Tag),
        FinishSeq(usize),
        FinishMap(usize),
    }
    let mut stack = vec![Step::Enter(root)];
    let mut results: Vec<(u64, u64, usize)> = Vec::new();
    while let Some(step) = stack.pop() {
        match step {
            Step::Enter(n) => match &n.value {
                Value::String(s) => results.push((1, NODE_OVERHEAD + s.len() as u64, 0)),
                Value::Tagged(t) => {
                    stack.push(Step::FinishTagged(&t.tag));
                    stack.push(Step::Enter(&t.value));
                }
                Value::Sequence(items) => {
                    stack.push(Step::FinishSeq(items.len()));
                    for it in items.iter().rev() {
                        stack.push(Step::Enter(it));
                    }
                }
                Value::Mapping(m) => {
                    stack.push(Step::FinishMap(m.len()));
                    for (k, v) in m.entries.iter().rev() {
                        stack.push(Step::Enter(v));
                        stack.push(Step::Enter(k));
                    }
                }
                _ => results.push((1, NODE_OVERHEAD, 0)),
            },
            Step::FinishTagged(tag) => {
                let (c, w, h) = results.pop().expect("tagged child");
                results.push((c + 1, w + NODE_OVERHEAD + tag_bytes(tag), h));
            }
            Step::FinishSeq(len) => {
                let mut c = 1u64;
                let mut w = NODE_OVERHEAD;
                let mut h = 1usize;
                for _ in 0..len {
                    let (ic, iw, ih) = results.pop().expect("seq item");
                    c += ic;
                    w += iw;
                    h = h.max(1 + ih);
                }
                results.push((c, w, h));
            }
            Step::FinishMap(len) => {
                let mut c = 1u64;
                let mut w = NODE_OVERHEAD;
                let mut h = 1usize;
                for _ in 0..len {
                    let (vc, vw, vh) = results.pop().expect("map val");
                    let (kc, kw, kh) = results.pop().expect("map key");
                    c += kc + vc;
                    w += kw + vw + ENTRY_OVERHEAD;
                    h = h.max(1 + kh.max(vh));
                }
                results.push((c, w, h));
            }
        }
    }
    results.pop().expect("root metrics")
}
