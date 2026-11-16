// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `yamlprims._yamlprims` native extension over `crates/yamlprims`.
//!
//! Exposes `load_wire` and `load_plain` plus default limit constants and
//! `VERSION` with `#![forbid(unsafe_code)]`.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyList, PyString};
use yamlprims::{
    decode_lint_wire, decode_wire, emit_all_with, encode, encode_error, encode_lint, encode_plain,
    lint_yaml11_bytes, load_all_bytes, load_bytes, load_ijson_bytes, CompatQuoting, EmitLimits,
    EmitOptions, Error, ErrorKind, Limits, MergeKeys, Node, NodeSpan, Options, RelatedRole, Site,
    UnknownTags, Value, Via, WireEnvelope,
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

#[pyfunction]
fn lint_wire(
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
    let mut bridge_opts = opts.clone();
    bridge_opts.limits.max_output_bytes = bridge_opts
        .limits
        .max_output_bytes
        .max(Limits::default().max_output_bytes);
    py.detach(|| {
        run_with_panic_guard(|| match lint_yaml11_bytes(input, &opts) {
            Ok(res) => match encode_lint(&res, &bridge_opts) {
                Ok(wire) => (0, wire),
                Err(e) => finish_error(&e, &bridge_opts),
            },
            Err(e) => finish_error(&e, &bridge_opts),
        })
    })
}

#[pyfunction]
fn encode_lint_wire(
    py: Python<'_>,
    wire: &str,
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

    let mut decode_opts = opts.clone();
    decode_opts.limits.max_input_bytes = decode_opts
        .limits
        .max_input_bytes
        .max(wire.len().saturating_add(1024));
    decode_opts.limits.max_nodes = decode_opts
        .limits
        .max_nodes
        .max(Limits::default().max_nodes);
    decode_opts.limits.max_owned_bytes = decode_opts
        .limits
        .max_owned_bytes
        .max(Limits::default().max_owned_bytes);
    decode_opts.limits.max_scalar_bytes = decode_opts
        .limits
        .max_scalar_bytes
        .max(Limits::default().max_scalar_bytes);

    py.detach(|| {
        run_with_panic_guard(|| match decode_lint_wire(wire, &decode_opts) {
            Ok(res) => match encode_lint(&res, &opts) {
                Ok(out) => (0, out),
                Err(e) => finish_error(&e, &err_opts),
            },
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

fn node_site(n: &Node) -> Site {
    match n.span() {
        NodeSpan::Source(s) => Site::Source(s.start),
        NodeSpan::Synthetic => Site::NotSource { last_known: None },
    }
}

fn attach_via(n: &Node, mut err: Error) -> Error {
    match n.via() {
        Some(Via::Alias { use_site }) => {
            err.related.push(yamlprims::Related {
                role: RelatedRole::AliasUse,
                span: *use_site,
            });
            err
        }
        Some(Via::Merge { directive }) => {
            err.related.push(yamlprims::Related {
                role: RelatedRole::MergeDirective,
                span: *directive,
            });
            err
        }
        None => err,
    }
}

enum PyBuildFrame<'a> {
    Eval(&'a Node),
    SeqStep {
        items: &'a [Node],
        idx: usize,
        list: Py<PyList>,
    },
    MapStep {
        entries: Vec<(&'a Node, &'a Node)>,
        idx: usize,
        dict: Py<PyDict>,
        pending_key: Option<&'a str>,
    },
}

fn node_to_pyobject<'py>(py: Python<'py>, root: &Node) -> Result<Py<PyAny>, Error> {
    let mut work: Vec<PyBuildFrame<'_>> = vec![PyBuildFrame::Eval(root)];
    let mut results: Vec<Py<PyAny>> = Vec::new();

    while let Some(frame) = work.pop() {
        match frame {
            PyBuildFrame::SeqStep { items, idx, list } => {
                if idx > 0 {
                    let elem = results.pop().ok_or_else(|| {
                        Error::new(
                            ErrorKind::Internal,
                            Site::NotSource { last_known: None },
                            "missing sequence element on Python conversion stack",
                        )
                    })?;
                    list.bind(py).append(elem).map_err(|e| {
                        Error::new(
                            ErrorKind::Internal,
                            Site::NotSource { last_known: None },
                            format!("PyList::append failed: {e}"),
                        )
                    })?;
                }
                if idx >= items.len() {
                    results.push(list.into_any());
                } else {
                    work.push(PyBuildFrame::SeqStep {
                        items,
                        idx: idx + 1,
                        list,
                    });
                    work.push(PyBuildFrame::Eval(&items[idx]));
                }
            }
            PyBuildFrame::MapStep {
                entries,
                idx,
                dict,
                pending_key,
            } => {
                if let Some(k_str) = pending_key {
                    let val_obj = results.pop().ok_or_else(|| {
                        Error::new(
                            ErrorKind::Internal,
                            Site::NotSource { last_known: None },
                            "missing mapping value on Python conversion stack",
                        )
                    })?;
                    dict.bind(py).set_item(k_str, val_obj).map_err(|e| {
                        Error::new(
                            ErrorKind::Internal,
                            Site::NotSource { last_known: None },
                            format!("PyDict::set_item failed: {e}"),
                        )
                    })?;
                }
                if idx >= entries.len() {
                    results.push(dict.into_any());
                } else {
                    let (k_node, v_node) = entries[idx];
                    let Value::String(k_str) = k_node.value() else {
                        let found = k_node.value().kind_name().to_owned();
                        let err = Error::new(
                            ErrorKind::Type {
                                expected: "plain mapping string key".to_owned(),
                                found: found.clone(),
                            },
                            node_site(k_node),
                            format!("expected plain mapping string key, found {found}"),
                        );
                        return Err(attach_via(k_node, err));
                    };
                    work.push(PyBuildFrame::MapStep {
                        entries,
                        idx: idx + 1,
                        dict,
                        pending_key: Some(k_str.as_str()),
                    });
                    work.push(PyBuildFrame::Eval(v_node));
                }
            }
            PyBuildFrame::Eval(n) => match n.value() {
                Value::Null => {
                    results.push(py.None());
                }
                Value::Bool(b) => {
                    results.push(
                        PyInt::new(py, i64::from(*b))
                            .call_method0("__bool__")
                            .map_err(|e| {
                                Error::new(
                                    ErrorKind::Internal,
                                    Site::NotSource { last_known: None },
                                    format!("bool conversion failed: {e}"),
                                )
                            })?
                            .unbind(),
                    );
                }
                Value::Int(i) => {
                    let py_int = if let Some(iv) = i.to_i128() {
                        PyInt::new(py, iv).into_any().unbind()
                    } else if let Some(uv) = i.to_u128() {
                        PyInt::new(py, uv).into_any().unbind()
                    } else {
                        return Err(attach_via(
                            n,
                            Error::new(
                                ErrorKind::IntegerOutOfRange,
                                node_site(n),
                                "integer outside 128-bit range",
                            ),
                        ));
                    };
                    results.push(py_int);
                }
                Value::Float(f) => {
                    results.push(PyFloat::new(py, *f).into_any().unbind());
                }
                Value::String(s) => {
                    results.push(PyString::new(py, s).into_any().unbind());
                }
                Value::Tagged(_) => {
                    let err = Error::new(
                        ErrorKind::UnknownTag,
                        node_site(n),
                        "tagged values are rejected in plain host conversion",
                    );
                    return Err(attach_via(n, err));
                }
                Value::Sequence(items) => {
                    let list = PyList::empty(py).unbind();
                    work.push(PyBuildFrame::SeqStep {
                        items,
                        idx: 0,
                        list,
                    });
                }
                Value::Mapping(m) => {
                    let dict = PyDict::new(py).unbind();
                    work.push(PyBuildFrame::MapStep {
                        entries: m.iter().collect(),
                        idx: 0,
                        dict,
                        pending_key: None,
                    });
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::Internal,
                        node_site(n),
                        "unsupported Value variant",
                    ));
                }
            },
        }
    }

    results.pop().ok_or_else(|| {
        Error::new(
            ErrorKind::Internal,
            Site::NotSource { last_known: None },
            "empty Python conversion result stack",
        )
    })
}

#[pyfunction]
fn safe_load_native(
    py: Python<'_>,
    input: &[u8],
    single_doc: bool,
    merge_keys: &str,
    unknown_tags: &str,
    limits: Vec<usize>,
) -> (u32, Py<PyAny>, String) {
    let opts = match build_options(merge_keys, unknown_tags, &limits) {
        Ok(o) => o,
        Err(e) => {
            let (st, msg) = finish_error(&e, &Options::default());
            return (st, py.None(), msg);
        }
    };
    let mut err_opts = opts.clone();
    err_opts.limits.max_output_bytes = err_opts
        .limits
        .max_output_bytes
        .max(Limits::default().max_output_bytes);

    let parse_res = py.detach(|| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if single_doc {
                load_bytes(input, &opts).map(|doc| vec![doc])
            } else {
                load_all_bytes(input, &opts)
            }
        }))
    });

    let docs = match parse_res {
        Ok(Ok(docs)) => docs,
        Ok(Err(e)) => {
            let (st, msg) = finish_error(&e, &err_opts);
            return (st, py.None(), msg);
        }
        Err(_) => {
            return (3, py.None(), "native panic in yamlprims".to_owned());
        }
    };

    let conv_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Py<PyAny>, Error> {
            if single_doc {
                let doc = docs.into_iter().next().ok_or_else(|| {
                    Error::new(
                        ErrorKind::Internal,
                        Site::NotSource { last_known: None },
                        "expected single document from load_bytes",
                    )
                })?;
                node_to_pyobject(py, doc.root())
            } else {
                let list = PyList::empty(py);
                for doc in &docs {
                    let obj = node_to_pyobject(py, doc.root())?;
                    list.append(obj).map_err(|e| {
                        Error::new(
                            ErrorKind::Internal,
                            Site::NotSource { last_known: None },
                            format!("PyList::append failed: {e}"),
                        )
                    })?;
                }
                Ok(list.into_any().unbind())
            }
        },
    ));

    match conv_res {
        Ok(Ok(obj)) => (0, obj, String::new()),
        Ok(Err(e)) => {
            let (st, msg) = finish_error(&e, &err_opts);
            (st, py.None(), msg)
        }
        Err(_) => (3, py.None(), "native panic in yamlprims".to_owned()),
    }
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
    m.add_function(wrap_pyfunction!(lint_wire, m)?)?;
    m.add_function(wrap_pyfunction!(encode_lint_wire, m)?)?;
    m.add_function(wrap_pyfunction!(emit_wire, m)?)?;
    m.add_function(wrap_pyfunction!(safe_load_native, m)?)?;
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
