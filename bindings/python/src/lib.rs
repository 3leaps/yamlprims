// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `yamlprims._yamlprims` native extension over `crates/yamlprims`.
//!
//! Exposes `load_wire` and `load_plain` plus default limit constants and
//! `VERSION` with `#![forbid(unsafe_code)]`.

use pyo3::prelude::*;
use yamlprims::{
    encode, encode_error, encode_plain, load_all_bytes, load_bytes, Error, ErrorKind, Limits,
    MergeKeys, Options, Site, UnknownTags,
};

fn build_options(merge_keys: &str, unknown_tags: &str, limits: &[usize]) -> Result<Options, Error> {
    if limits.len() != 12 {
        return Err(Error::new(
            ErrorKind::InvalidOptions,
            Site::NotSource { last_known: None },
            "limits tuple must have 12 elements",
        ));
    }
    let mk = match merge_keys {
        "reject" => MergeKeys::Reject,
        "apply" => MergeKeys::Apply,
        "as_key" | "as-key" => MergeKeys::AsKey,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidOptions,
                Site::NotSource { last_known: None },
                "unknown merge_keys option",
            ));
        }
    };
    let ut = match unknown_tags {
        "preserve" => UnknownTags::Preserve,
        "reject" => UnknownTags::Reject,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidOptions,
                Site::NotSource { last_known: None },
                "unknown unknown_tags option",
            ));
        }
    };
    let mut opts = Options::default();
    opts.merge_keys = mk;
    opts.unknown_tags = ut;
    opts.limits.max_input_bytes = limits[0];
    opts.limits.max_depth = limits[1];
    opts.limits.max_nodes = limits[2];
    opts.limits.max_expanded_nodes = limits[3];
    opts.limits.max_alias_events = limits[4];
    opts.limits.max_owned_bytes = limits[5];
    opts.limits.max_scalar_bytes = limits[6];
    opts.limits.max_key_bytes = limits[7];
    opts.limits.max_mapping_entries = limits[8];
    opts.limits.max_documents = limits[9];
    opts.limits.max_output_bytes = limits[10];
    opts.limits.max_warnings = limits[11];
    opts.validate()?;
    Ok(opts)
}

fn finish_error(err: &Error, opts: &Options) -> (u32, String) {
    if let ErrorKind::OutputLimitExceeded { configured } = err.kind {
        return (2, configured.to_string());
    }
    match encode_error(err, opts) {
        Ok(wire) => (1, wire),
        Err(enc_err) => match enc_err.kind {
            ErrorKind::OutputLimitExceeded { configured } => (2, configured.to_string()),
            _ => (3, enc_err.message.clone()),
        },
    }
}

#[pyfunction]
fn load_wire(
    input: &[u8],
    single_doc: bool,
    merge_keys: &str,
    unknown_tags: &str,
    limits: Vec<usize>,
) -> (u32, String) {
    let opts = match build_options(merge_keys, unknown_tags, &limits) {
        Ok(o) => o,
        Err(e) => return finish_error(&e, &Options::default()),
    };
    let loaded = if single_doc {
        load_bytes(input, &opts).map(|doc| vec![doc])
    } else {
        load_all_bytes(input, &opts)
    };
    match loaded {
        Ok(docs) => match encode(&docs, &opts) {
            Ok(wire) => (0, wire),
            Err(e) => finish_error(&e, &opts),
        },
        Err(e) => finish_error(&e, &opts),
    }
}

#[pyfunction]
fn load_plain(
    input: &[u8],
    merge_keys: &str,
    unknown_tags: &str,
    limits: Vec<usize>,
) -> (u32, String) {
    let opts = match build_options(merge_keys, unknown_tags, &limits) {
        Ok(o) => o,
        Err(e) => return finish_error(&e, &Options::default()),
    };
    match load_bytes(input, &opts) {
        Ok(doc) => match encode_plain(doc.root(), &opts) {
            Ok(json) => (0, json),
            Err(e) => finish_error(&e, &opts),
        },
        Err(e) => finish_error(&e, &opts),
    }
}

#[pymodule]
fn _yamlprims(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let def = Limits::default();
    m.add_function(wrap_pyfunction!(load_wire, m)?)?;
    m.add_function(wrap_pyfunction!(load_plain, m)?)?;
    m.add("DEFAULT_MAX_INPUT_BYTES", def.max_input_bytes)?;
    m.add("DEFAULT_MAX_DEPTH", def.max_depth)?;
    m.add("DEFAULT_MAX_NODES", def.max_nodes)?;
    m.add("DEFAULT_MAX_EXPANDED_NODES", def.max_expanded_nodes)?;
    m.add("DEFAULT_MAX_ALIAS_EVENTS", def.max_alias_events)?;
    m.add("DEFAULT_MAX_OWNED_BYTES", def.max_owned_bytes)?;
    m.add("DEFAULT_MAX_SCALAR_BYTES", def.max_scalar_bytes)?;
    m.add("DEFAULT_MAX_KEY_BYTES", def.max_key_bytes)?;
    m.add("DEFAULT_MAX_MAPPING_ENTRIES", def.max_mapping_entries)?;
    m.add("DEFAULT_MAX_DOCUMENTS", def.max_documents)?;
    m.add("DEFAULT_MAX_OUTPUT_BYTES", def.max_output_bytes)?;
    m.add("DEFAULT_MAX_WARNINGS", def.max_warnings)?;
    m.add("MAX_LIMIT", usize::MAX)?;
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
