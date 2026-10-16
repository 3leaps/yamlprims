//! Options and limits (spec §6, §10).

use crate::error::{Error, ErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MergeKeys {
    #[default]
    Reject,
    Apply,
    AsKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UnknownTags {
    #[default]
    Preserve,
    Reject,
}

/// Proposed defaults; unmeasured (spec §6).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_expanded_nodes: usize,
    pub max_alias_events: usize,
    pub max_owned_bytes: usize,
    pub max_scalar_bytes: usize,
    pub max_key_bytes: usize,
    pub max_mapping_entries: usize,
    pub max_documents: usize,
    pub max_output_bytes: usize,
    pub max_warnings: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_input_bytes: 8 << 20,
            max_depth: 128,
            max_nodes: 250_000,
            max_expanded_nodes: 100_000,
            max_alias_events: 10_000,
            max_owned_bytes: 64 << 20,
            max_scalar_bytes: 1 << 20,
            max_key_bytes: 64 << 10,
            max_mapping_entries: 100_000,
            max_documents: 1_024,
            max_output_bytes: 64 << 20,
            max_warnings: 1_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Options {
    pub merge_keys: MergeKeys,
    pub unknown_tags: UnknownTags,
    pub limits: Limits,
}

impl Options {
    /// granit's `strict_indentation`, the same for every parsing pass.
    pub(crate) fn strict_indentation(&self) -> bool {
        true
    }

    pub fn validate(&self) -> Result<(), Error> {
        let l = &self.limits;
        let all = [
            l.max_input_bytes,
            l.max_depth,
            l.max_nodes,
            l.max_expanded_nodes,
            l.max_alias_events,
            l.max_owned_bytes,
            l.max_scalar_bytes,
            l.max_key_bytes,
            l.max_mapping_entries,
            l.max_documents,
            l.max_output_bytes,
            l.max_warnings,
        ];
        let bad = if all.contains(&0) {
            Some("every limit must be at least 1")
        } else if l.max_depth > 255 {
            Some("max_depth must be at most 255")
        } else if l.max_scalar_bytes > l.max_owned_bytes || l.max_key_bytes > l.max_owned_bytes {
            Some("max_scalar_bytes and max_key_bytes must not exceed max_owned_bytes")
        } else {
            None
        };
        match bad {
            Some(m) => Err(Error::not_source(ErrorKind::InvalidOptions, None, m)),
            None => Ok(()),
        }
    }
}
