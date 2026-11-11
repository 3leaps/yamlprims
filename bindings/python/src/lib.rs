// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `yamlprims._yamlprims` native extension over `crates/yamlprims`.
//!
//! Exposes `load_wire` and `load_plain` plus default limit constants and
//! `VERSION` with `#![forbid(unsafe_code)]`.

use pyo3::prelude::*;
use yamlprims::{
    decode_wire, emit_all_with, encode, encode_error, encode_plain, load_all_bytes, load_bytes,
    load_ijson_bytes, CompatQuoting, EmitLimits, EmitOptions, Error, ErrorKind, Limits, MergeKeys,
    Options, Site, UnknownTags, WireEnvelope,
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

fn run_with_panic_guard<F: FnOnce() -> (u32, String)>(f: F) -> (u32, String) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(res) => res,
        Err(_) => (3, "native panic in yamlprims".to_owned()),
    }
}

#[pyfunction]
fn load_wire(
    py: Python<'_>,
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
    py.detach(|| {
        run_with_panic_guard(|| {
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
        })
    })
}

#[pyfunction]
fn load_plain(
    py: Python<'_>,
    input: &[u8],
    merge_keys: &str,
    unknown_tags: &str,
    limits: Vec<usize>,
) -> (u32, String) {
    let opts = match build_options(merge_keys, unknown_tags, &limits) {
        Ok(o) => o,
        Err(e) => return finish_error(&e, &Options::default()),
    };
    py.detach(|| {
        run_with_panic_guard(|| match load_bytes(input, &opts) {
            Ok(doc) => match encode_plain(doc.root(), &opts) {
                Ok(json) => (0, json),
                Err(e) => finish_error(&e, &opts),
            },
            Err(e) => finish_error(&e, &opts),
        })
    })
}

#[pyfunction]
fn load_ijson(
    py: Python<'_>,
    input: &[u8],
    merge_keys: &str,
    unknown_tags: &str,
    limits: Vec<usize>,
) -> (u32, String) {
    let opts = match build_options(merge_keys, unknown_tags, &limits) {
        Ok(o) => o,
        Err(e) => return finish_error(&e, &Options::default()),
    };
    let mut err_opts = opts.clone();
    err_opts.limits.max_output_bytes = err_opts
        .limits
        .max_output_bytes
        .max(Limits::default().max_output_bytes);
    py.detach(|| {
        run_with_panic_guard(|| match load_ijson_bytes(input, &opts) {
            Ok(json) => (0, json),
            Err(e) => finish_error(&e, &err_opts),
        })
    })
}

fn build_emit_options(compat_quoting: &str, emit_limits: &[usize]) -> Result<EmitOptions, Error> {
    if emit_limits.len() != 8 {
        return Err(Error::new(
            ErrorKind::InvalidOptions,
            Site::NotSource { last_known: None },
            "emit_limits tuple must have 8 elements",
        ));
    }
    let cq = match compat_quoting {
        "yaml11_safe" | "yaml11-safe" => CompatQuoting::Yaml11Safe,
        "core_only" | "core-only" => CompatQuoting::CoreOnly,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidOptions,
                Site::NotSource { last_known: None },
                "unknown compat_quoting option",
            ));
        }
    };
    let mut eopts = EmitOptions::default();
    eopts.compat_quoting = cq;
    eopts.limits.max_depth = emit_limits[0];
    eopts.limits.max_nodes = emit_limits[1];
    eopts.limits.max_scalar_bytes = emit_limits[2];
    eopts.limits.max_key_bytes = emit_limits[3];
    eopts.limits.max_mapping_entries = emit_limits[4];
    eopts.limits.max_tag_bytes = emit_limits[5];
    eopts.limits.max_documents = emit_limits[6];
    eopts.limits.max_output_bytes = emit_limits[7];
    eopts.validate()?;
    Ok(eopts)
}

#[pyfunction]
fn emit_wire(
    py: Python<'_>,
    wire: &str,
    compat_quoting: &str,
    emit_limits: Vec<usize>,
) -> (u32, String) {
    let eopts = match build_emit_options(compat_quoting, &emit_limits) {
        Ok(o) => o,
        Err(e) => return finish_error(&e, &Options::default()),
    };
    let mut bridge_opts = Options::default();
    bridge_opts.limits.max_depth = eopts.limits.max_depth;
    bridge_opts.limits.max_nodes = eopts.limits.max_nodes;
    bridge_opts.limits.max_documents = eopts.limits.max_documents;
    bridge_opts.limits.max_mapping_entries = eopts.limits.max_mapping_entries;
    bridge_opts.limits.max_scalar_bytes = eopts.limits.max_scalar_bytes;
    bridge_opts.limits.max_key_bytes = eopts.limits.max_key_bytes;
    let wire_cap = 65_536usize.saturating_add(
        eopts
            .limits
            .max_nodes
            .saturating_mul(512)
            .saturating_add(eopts.limits.max_output_bytes.saturating_mul(18)),
    );
    let owned_cap = 65_536usize.saturating_add(
        eopts
            .limits
            .max_nodes
            .saturating_mul(128)
            .saturating_add(eopts.limits.max_output_bytes.saturating_mul(4)),
    );
    bridge_opts.limits.max_input_bytes = wire_cap;
    bridge_opts.limits.max_owned_bytes = owned_cap;
    bridge_opts.limits.max_output_bytes = wire_cap;

    let mut err_opts = Options::default();
    err_opts.limits.max_depth = err_opts.limits.max_depth.max(eopts.limits.max_depth);
    err_opts.limits.max_key_bytes = err_opts
        .limits
        .max_key_bytes
        .max(eopts.limits.max_key_bytes);

    py.detach(|| {
        run_with_panic_guard(|| match decode_wire(wire, &bridge_opts) {
            Ok(WireEnvelope::Documents(docs)) => match emit_all_with(&docs, &eopts) {
                Ok(yaml) => (0, yaml),
                Err(e) => finish_error(&e, &err_opts),
            },
            Ok(WireEnvelope::Error(e)) => finish_error(&e, &err_opts),
            Err(e) => finish_error(&e, &err_opts),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::run_with_panic_guard;

    #[test]
    fn panic_guard_maps_unwind_to_status_3() {
        let res = run_with_panic_guard(|| -> (u32, String) {
            panic!("simulated native panic");
        });
        assert_eq!(res, (3, "native panic in yamlprims".to_owned()));
    }
}

#[pymodule]
fn _yamlprims(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let def = Limits::default();
    let edef = EmitLimits::default();
    let ijson_def = Options::ijson_default();
    m.add_function(wrap_pyfunction!(load_wire, m)?)?;
    m.add_function(wrap_pyfunction!(load_plain, m)?)?;
    m.add_function(wrap_pyfunction!(load_ijson, m)?)?;
    m.add_function(wrap_pyfunction!(emit_wire, m)?)?;
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
    m.add(
        "DEFAULT_IJSON_MAX_OUTPUT_BYTES",
        ijson_def.limits.max_output_bytes,
    )?;
    m.add("DEFAULT_EMIT_MAX_TAG_BYTES", edef.max_tag_bytes)?;
    m.add("DEFAULT_EMIT_MAX_OUTPUT_BYTES", edef.max_output_bytes)?;
    m.add("MAX_LIMIT", usize::MAX)?;
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
