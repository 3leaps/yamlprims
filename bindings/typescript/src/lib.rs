// SPDX-License-Identifier: MIT OR Apache-2.0
// WebAssembly boundary for @3leaps/yamlprims.
//
// The module is single-threaded (wasm32-unknown-unknown) and enforces
// single-owner, non-reentrant execution per instance. Callers reserve the
// options buffer first (`options_reserve`), write the options JSON bytes, then
// reserve the input buffer (`input_reserve`), which validates the configured
// `Options`/`Limits` and checks `len` against `max_input_bytes` and the
// absolute WASM linear-memory ceiling (`MAX_WASM_INPUT_CEILING`) before
// allocating input memory.
//
// Any failed `options_reserve` or `input_reserve` immediately clears and
// invalidates `STATE.input`, `STATE.options`, and `STATE.output` so prior input
// or stale output can never be consumed by a subsequent `load_*` call.
//
// `unsafe_code` is denied across this crate and allowed only on the
// `#[no_mangle]` export attributes required by Rust's FFI rules; no function
// dereferences a caller pointer.

#![cfg_attr(target_arch = "wasm32", no_main)]

use std::cell::RefCell;
use std::collections::BTreeSet;
use yamlprims::{
    decode_wire, emit_all_with, encode, encode_error, encode_plain, load_all_bytes, load_bytes,
    load_ijson_bytes, CompatQuoting, EmitLimits, EmitOptions, Error, ErrorKind, Limits, MergeKeys,
    Options, Site, UnknownTags, WireEnvelope,
};

/// Wire ABI version exported by this module.
pub const ABI_VERSION: u32 = 1;

/// Maximum options payload accepted by `options_reserve` (64 KiB).
pub const MAX_OPTIONS_BYTES: usize = 64 * 1024;

/// Absolute linear-memory ceiling for a single input reservation (64 MiB).
pub const MAX_WASM_INPUT_CEILING: usize = 64 * 1024 * 1024;

/// Hard WebAssembly linear-memory maximum enforced via `-Clink-arg=--max-memory=134217728`
/// (128 MiB = 2048 pages of 64 KiB).
pub const MAX_WASM_MEMORY_BYTES: usize = 128 * 1024 * 1024;

/// Hard WebAssembly linear-memory page ceiling (`2048` 64-KiB pages = 128 MiB).
pub const MAX_WASM_MEMORY_PAGES: usize = 2048;

#[inline]
fn within_memory_ceiling(requested_len: usize) -> bool {
    if requested_len > MAX_WASM_MEMORY_BYTES {
        return false;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let cur_pages = core::arch::wasm32::memory_size(0);
        if cur_pages > MAX_WASM_MEMORY_PAGES {
            return false;
        }
    }
    true
}

#[derive(Default)]
struct State {
    options: Vec<u8>,
    input: Vec<u8>,
    output: Vec<u8>,
    options_valid: bool,
    input_valid: bool,
}

impl State {
    fn clear_all(&mut self) {
        self.options = Vec::new();
        self.input = Vec::new();
        self.output = Vec::new();
        self.options_valid = false;
        self.input_valid = false;
    }

    fn write_fallback_output_limit(&mut self, configured: u64) {
        self.output.clear();
        let msg = format!(
            "{{\"kind\":\"output_limit_exceeded\",\"configured\":{configured},\"message\":\"encoded output exceeds max_output_bytes ({configured})\"}}"
        );
        self.output.extend_from_slice(msg.as_bytes());
    }

    fn write_fallback_internal(&mut self, reason: &str) {
        self.output.clear();
        let mut s = String::from("{\"kind\":\"internal\",\"configured\":0,\"message\":\"");
        for c in reason.chars() {
            match c {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                '\n' => s.push_str("\\n"),
                '\r' => s.push_str("\\r"),
                '\t' => s.push_str("\\t"),
                c if (c as u32) < 0x20 => {}
                c => s.push(c),
            }
        }
        s.push_str("\"}");
        self.output.extend_from_slice(s.as_bytes());
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn abi_version() -> u32 {
    ABI_VERSION
}

/// Reserve `len` bytes for the options JSON buffer and return its linear-memory
/// offset (or `1` when `len == 0`).
///
/// Rejects `len > MAX_OPTIONS_BYTES` or allocation failure by clearing and
/// invalidating `STATE.options`, `STATE.input`, and `STATE.output` and returning `0`.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn options_reserve(len: usize) -> usize {
    STATE
        .try_with(|cell| {
            let Ok(mut s) = cell.try_borrow_mut() else {
                return 0;
            };
            s.clear_all();
            if len > MAX_OPTIONS_BYTES || !within_memory_ceiling(len) {
                return 0;
            }
            if len == 0 {
                s.options_valid = true;
                return 1;
            }
            if s.options.try_reserve_exact(len).is_err() {
                s.clear_all();
                return 0;
            }
            s.options.resize(len, 0);
            s.options_valid = true;
            s.options.as_mut_ptr() as usize
        })
        .unwrap_or(0)
}

/// Validate the reserved `Options`/`Limits` and reserve `len` bytes for the
/// YAML input buffer, returning its linear-memory offset (or `1` when `len == 0`).
///
/// Rejects unreserved/invalid options, `max_input_bytes == 0`, `len > max_input_bytes`,
/// `len > opts.limits.max_input_bytes`, `len > MAX_WASM_INPUT_CEILING`, or
/// allocation failure by clearing and invalidating `STATE.input`, `STATE.options`,
/// and `STATE.output` and returning `0`.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn input_reserve(len: usize, max_input_bytes: usize) -> usize {
    STATE
        .try_with(|cell| {
            let Ok(mut s) = cell.try_borrow_mut() else {
                return 0;
            };
            s.output = Vec::new();
            s.input = Vec::new();
            s.input_valid = false;

            if !s.options_valid
                || max_input_bytes == 0
                || len > MAX_WASM_INPUT_CEILING
                || !within_memory_ceiling(len)
            {
                s.clear_all();
                return 0;
            }
            let effective_max = if let Ok(opts) = parse_options(&s.options) {
                max_input_bytes.min(opts.limits.max_input_bytes)
            } else if parse_emit_options(&s.options).is_ok() {
                max_input_bytes.min(MAX_WASM_INPUT_CEILING)
            } else {
                s.clear_all();
                return 0;
            };
            if len > effective_max {
                s.clear_all();
                return 0;
            }
            if len == 0 {
                s.input_valid = true;
                return 1;
            }
            if s.input.try_reserve_exact(len).is_err() {
                s.clear_all();
                return 0;
            }
            s.input.resize(len, 0);
            s.input_valid = true;
            s.input.as_mut_ptr() as usize
        })
        .unwrap_or(0)
}

/// Load YAML from the reserved input buffer using the reserved options buffer
/// and encode the result as a `yamlprims-value/1` wire envelope in `STATE.output`.
///
/// `single_doc`: `1` for `load_bytes` (single document), `0` for `load_all_bytes`.
///
/// Returns:
/// - `0`: `STATE.output` contains the `yamlprims-value/1` `documents` envelope.
/// - `1`: `STATE.output` contains the `yamlprims-value/1` `error` envelope.
/// - `2`: static non-recursive fallback in `STATE.output` (`OutputLimitExceeded`
///   or protocol/internal failure).
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn load_wire(input_len: usize, options_len: usize, single_doc: u32) -> u32 {
    STATE
        .try_with(|cell| {
            let Ok(mut s) = cell.try_borrow_mut() else {
                return 2;
            };
            s.output.clear();
            let valid_reservation = s.options_valid
                && s.input_valid
                && options_len == s.options.len()
                && input_len == s.input.len()
                && (single_doc == 0 || single_doc == 1);
            s.options_valid = false;
            s.input_valid = false;
            if !valid_reservation {
                s.options = Vec::new();
                s.input = Vec::new();
                s.write_fallback_internal("invalid buffer reservation or single_doc flag");
                return 2;
            }

            let opts = match parse_options(&s.options) {
                Ok(o) => o,
                Err(e) => {
                    s.options = Vec::new();
                    s.input = Vec::new();
                    return finish_error(&mut s, &e, &Options::default());
                }
            };

            let load_res = if single_doc == 1 {
                load_bytes(&s.input, &opts).map(|doc| vec![doc])
            } else {
                load_all_bytes(&s.input, &opts)
            };
            s.options = Vec::new();
            s.input = Vec::new();

            match load_res {
                Ok(docs) => match encode(&docs, &opts) {
                    Ok(wire) => {
                        s.output.extend_from_slice(wire.as_bytes());
                        0
                    }
                    Err(e) => finish_encode_failure(&mut s, &e, &opts),
                },
                Err(e) => finish_error(&mut s, &e, &opts),
            }
        })
        .unwrap_or(2)
}

/// Load a single YAML document from the reserved input buffer and encode its
/// root as plain JSON text (`spec-v1.4 §11.3`) in `STATE.output`.
///
/// Returns:
/// - `0`: `STATE.output` contains the plain JSON text.
/// - `1`: `STATE.output` contains the `yamlprims-value/1` `error` envelope.
/// - `2`: static non-recursive fallback in `STATE.output` (`OutputLimitExceeded`
///   or protocol/internal failure).
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn load_plain(input_len: usize, options_len: usize) -> u32 {
    STATE
        .try_with(|cell| {
            let Ok(mut s) = cell.try_borrow_mut() else {
                return 2;
            };
            s.output.clear();
            let valid_reservation = s.options_valid
                && s.input_valid
                && options_len == s.options.len()
                && input_len == s.input.len();
            s.options_valid = false;
            s.input_valid = false;
            if !valid_reservation {
                s.options = Vec::new();
                s.input = Vec::new();
                s.write_fallback_internal("invalid buffer reservation");
                return 2;
            }

            let opts = match parse_options(&s.options) {
                Ok(o) => o,
                Err(e) => {
                    s.options = Vec::new();
                    s.input = Vec::new();
                    return finish_error(&mut s, &e, &Options::default());
                }
            };

            let load_res = load_bytes(&s.input, &opts);
            s.options = Vec::new();
            s.input = Vec::new();

            match load_res {
                Ok(doc) => match encode_plain(doc.root(), &opts) {
                    Ok(json) => {
                        s.output.extend_from_slice(json.as_bytes());
                        0
                    }
                    Err(e) => finish_encode_failure(&mut s, &e, &opts),
                },
                Err(e) => finish_error(&mut s, &e, &opts),
            }
        })
        .unwrap_or(2)
}

/// Load a single YAML document from the reserved input buffer and project it
/// to compact RFC 7493 I-JSON text (`spec-v1.5 §11.6`) in `STATE.output`.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn load_ijson(input_len: usize, options_len: usize) -> u32 {
    STATE
        .try_with(|cell| {
            let Ok(mut s) = cell.try_borrow_mut() else {
                return 2;
            };
            s.output.clear();
            let valid_reservation = s.options_valid
                && s.input_valid
                && options_len == s.options.len()
                && input_len == s.input.len();
            s.options_valid = false;
            s.input_valid = false;
            if !valid_reservation {
                s.options = Vec::new();
                s.input = Vec::new();
                s.write_fallback_internal("invalid buffer reservation");
                return 2;
            }

            let opts = match parse_ijson_options(&s.options) {
                Ok(o) => o,
                Err(e) => {
                    s.options = Vec::new();
                    s.input = Vec::new();
                    return finish_error(&mut s, &e, &Options::default());
                }
            };

            let res = load_ijson_bytes(&s.input, &opts);
            s.options = Vec::new();
            s.input = Vec::new();

            match res {
                Ok(json) => {
                    s.output.extend_from_slice(json.as_bytes());
                    0
                }
                Err(e) => {
                    let mut err_opts = opts.clone();
                    err_opts.limits.max_output_bytes = err_opts
                        .limits
                        .max_output_bytes
                        .max(Limits::default().max_output_bytes);
                    finish_encode_failure(&mut s, &e, &err_opts)
                }
            }
        })
        .unwrap_or(2)
}

/// Decode a `yamlprims-value/1` wire envelope from the reserved input buffer
/// and emit YAML 1.2.2 Core text (`spec-v1.5 §10.2`, `§11.5`) in `STATE.output`.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn emit_wire(input_len: usize, options_len: usize) -> u32 {
    STATE
        .try_with(|cell| {
            let Ok(mut s) = cell.try_borrow_mut() else {
                return 2;
            };
            s.output.clear();
            let valid_reservation = s.options_valid
                && s.input_valid
                && options_len == s.options.len()
                && input_len == s.input.len();
            s.options_valid = false;
            s.input_valid = false;
            if !valid_reservation {
                s.options = Vec::new();
                s.input = Vec::new();
                s.write_fallback_internal("invalid buffer reservation");
                return 2;
            }

            let eopts = match parse_emit_options(&s.options) {
                Ok(o) => o,
                Err(e) => {
                    s.options = Vec::new();
                    s.input = Vec::new();
                    return finish_error(&mut s, &e, &Options::default());
                }
            };

            let wire_cap = 65_536usize
                .saturating_add(eopts.limits.max_nodes.saturating_mul(512))
                .saturating_add(eopts.limits.max_output_bytes.saturating_mul(18))
                .min(MAX_WASM_INPUT_CEILING);
            let owned_cap = 65_536usize
                .saturating_add(eopts.limits.max_nodes.saturating_mul(128))
                .saturating_add(eopts.limits.max_output_bytes.saturating_mul(4))
                .min(MAX_WASM_INPUT_CEILING);
            let mut bridge_opts = Options::default();
            bridge_opts.limits.max_depth = eopts.limits.max_depth;
            bridge_opts.limits.max_nodes = eopts.limits.max_nodes;
            bridge_opts.limits.max_expanded_nodes = eopts.limits.max_nodes;
            bridge_opts.limits.max_documents = eopts.limits.max_documents;
            bridge_opts.limits.max_mapping_entries = eopts.limits.max_mapping_entries;
            bridge_opts.limits.max_scalar_bytes = eopts.limits.max_scalar_bytes.min(owned_cap);
            bridge_opts.limits.max_key_bytes = eopts.limits.max_key_bytes.min(owned_cap);
            bridge_opts.limits.max_input_bytes = wire_cap;
            bridge_opts.limits.max_owned_bytes = owned_cap;
            bridge_opts.limits.max_output_bytes =
                eopts.limits.max_output_bytes.min(MAX_WASM_INPUT_CEILING);

            let input_bytes = std::mem::take(&mut s.input);
            s.options = Vec::new();

            let Ok(wire_str) = std::str::from_utf8(&input_bytes) else {
                let err = Error::new(
                    ErrorKind::InvalidUtf8 { invalid_len: None },
                    Site::NotSource { last_known: None },
                    "invalid UTF-8 in wire envelope",
                );
                return finish_error(&mut s, &err, &Options::default());
            };

            match decode_wire(wire_str, &bridge_opts) {
                Ok(WireEnvelope::Documents(docs)) => match emit_all_with(&docs, &eopts) {
                    Ok(yaml_out) => {
                        s.output.extend_from_slice(yaml_out.as_bytes());
                        0
                    }
                    Err(e) => finish_encode_failure(&mut s, &e, &Options::default()),
                },
                Ok(WireEnvelope::Error(e)) => finish_error(&mut s, &e, &Options::default()),
                Err(e) => finish_error(&mut s, &e, &Options::default()),
            }
        })
        .unwrap_or(2)
}

fn finish_encode_failure(s: &mut State, err: &Error, opts: &Options) -> u32 {
    match err.kind {
        ErrorKind::OutputLimitExceeded { configured } => {
            // Never recursively call encode_error on OutputLimitExceeded.
            s.write_fallback_output_limit(configured);
            2
        }
        _ => finish_error(s, err, opts),
    }
}

fn finish_error(s: &mut State, err: &Error, opts: &Options) -> u32 {
    if let ErrorKind::OutputLimitExceeded { configured } = err.kind {
        s.write_fallback_output_limit(configured);
        return 2;
    }
    match encode_error(err, opts) {
        Ok(wire) => {
            s.output.extend_from_slice(wire.as_bytes());
            1
        }
        Err(enc_err) => {
            match enc_err.kind {
                ErrorKind::OutputLimitExceeded { configured } => {
                    s.write_fallback_output_limit(configured);
                }
                _ => {
                    s.write_fallback_internal(&enc_err.message);
                }
            }
            2
        }
    }
}

#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn output_ptr() -> *const u8 {
    STATE
        .try_with(|cell| {
            cell.try_borrow()
                .map(|s| s.output.as_ptr())
                .unwrap_or(std::ptr::null())
        })
        .unwrap_or(std::ptr::null())
}

#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn output_len() -> usize {
    STATE
        .try_with(|cell| cell.try_borrow().map(|s| s.output.len()).unwrap_or(0))
        .unwrap_or(0)
}

/// Drop the input, options, and output allocations and invalidate all reservations.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn release() {
    let _ = STATE.try_with(|cell| {
        if let Ok(mut s) = cell.try_borrow_mut() {
            s.clear_all();
        }
    });
}

// ---- Strict options JSON parser ------------------------------------------

fn invalid_opts(msg: &str) -> Error {
    Error::new(
        ErrorKind::InvalidOptions,
        Site::NotSource { last_known: None },
        msg,
    )
}

fn parse_options(bytes: &[u8]) -> Result<Options, Error> {
    if bytes.is_empty() {
        let opts = Options::default();
        opts.validate()?;
        return Ok(opts);
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid_opts("options must be valid UTF-8"))?;
    let mut p = OptParser::new(text.as_bytes());
    let opts = p.parse_root(Options::default())?;
    opts.validate()?;
    Ok(opts)
}

fn parse_ijson_options(bytes: &[u8]) -> Result<Options, Error> {
    if bytes.is_empty() {
        let opts = Options::ijson_default();
        opts.validate()?;
        return Ok(opts);
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid_opts("options must be valid UTF-8"))?;
    let mut p = OptParser::new(text.as_bytes());
    let opts = p.parse_root(Options::ijson_default())?;
    opts.validate()?;
    Ok(opts)
}

fn parse_emit_options(bytes: &[u8]) -> Result<EmitOptions, Error> {
    if bytes.is_empty() {
        let eopts = EmitOptions::default();
        eopts.validate()?;
        return Ok(eopts);
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid_opts("options must be valid UTF-8"))?;
    let mut p = OptParser::new(text.as_bytes());
    let eopts = p.parse_emit_root()?;
    eopts.validate()?;
    Ok(eopts)
}

struct OptParser<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> OptParser<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.b.len() && matches!(self.b[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), Error> {
        self.skip_ws();
        if self.pos < self.b.len() && self.b[self.pos] == expected {
            self.pos += 1;
            Ok(())
        } else {
            Err(invalid_opts("malformed options JSON"))
        }
    }

    fn parse_string(&mut self) -> Result<String, Error> {
        self.expect_byte(b'"')?;
        let start = self.pos;
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            if c == b'"' {
                let s = std::str::from_utf8(&self.b[start..self.pos])
                    .map_err(|_| invalid_opts("invalid UTF-8 in options string"))?
                    .to_owned();
                self.pos += 1;
                return Ok(s);
            }
            if c == b'\\' || c < 0x20 {
                return Err(invalid_opts("unsupported escape in options string"));
            }
            self.pos += 1;
        }
        Err(invalid_opts("unterminated string in options JSON"))
    }

    fn parse_usize(&mut self) -> Result<usize, Error> {
        self.skip_ws();
        let start = self.pos;
        if start >= self.b.len() || !self.b[start].is_ascii_digit() {
            return Err(invalid_opts("expected unsigned integer in options limits"));
        }
        while self.pos < self.b.len() && self.b[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let raw = &self.b[start..self.pos];
        if raw.len() > 1 && raw[0] == b'0' {
            return Err(invalid_opts("leading zero in options integer"));
        }
        let s = std::str::from_utf8(raw).map_err(|_| invalid_opts("invalid integer"))?;
        let val: u64 = s
            .parse()
            .map_err(|_| invalid_opts("options limit integer out of range"))?;
        if val > 9_007_199_254_740_991 {
            return Err(invalid_opts("options limit exceeds safe integer range"));
        }
        usize::try_from(val).map_err(|_| invalid_opts("options limit exceeds target usize range"))
    }

    fn parse_root(&mut self, mut opts: Options) -> Result<Options, Error> {
        self.expect_byte(b'{')?;
        self.skip_ws();
        if self.pos < self.b.len() && self.b[self.pos] == b'}' {
            self.pos += 1;
            self.skip_ws();
            if self.pos != self.b.len() {
                return Err(invalid_opts("trailing data after options JSON"));
            }
            return Ok(opts);
        }
        let mut seen = BTreeSet::new();
        loop {
            let key = self.parse_string()?;
            if !seen.insert(key.clone()) {
                return Err(invalid_opts("duplicate key in options JSON"));
            }
            self.expect_byte(b':')?;
            match key.as_str() {
                "merge_keys" => {
                    let v = self.parse_string()?;
                    opts.merge_keys = match v.as_str() {
                        "reject" => MergeKeys::Reject,
                        "apply" => MergeKeys::Apply,
                        "as_key" | "as-key" => MergeKeys::AsKey,
                        _ => return Err(invalid_opts("unknown merge_keys value")),
                    };
                }
                "unknown_tags" => {
                    let v = self.parse_string()?;
                    opts.unknown_tags = match v.as_str() {
                        "preserve" => UnknownTags::Preserve,
                        "reject" => UnknownTags::Reject,
                        _ => return Err(invalid_opts("unknown unknown_tags value")),
                    };
                }
                "limits" => {
                    self.parse_limits(&mut opts.limits)?;
                }
                _ => return Err(invalid_opts("unknown key in options JSON")),
            }
            self.skip_ws();
            if self.pos < self.b.len() && self.b[self.pos] == b',' {
                self.pos += 1;
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }
        self.skip_ws();
        if self.pos != self.b.len() {
            return Err(invalid_opts("trailing data after options JSON"));
        }
        Ok(opts)
    }

    fn parse_emit_root(&mut self) -> Result<EmitOptions, Error> {
        let mut eopts = EmitOptions::default();
        self.expect_byte(b'{')?;
        self.skip_ws();
        if self.pos < self.b.len() && self.b[self.pos] == b'}' {
            self.pos += 1;
            self.skip_ws();
            if self.pos != self.b.len() {
                return Err(invalid_opts("trailing data after options JSON"));
            }
            return Ok(eopts);
        }
        let mut seen = BTreeSet::new();
        loop {
            let key = self.parse_string()?;
            if !seen.insert(key.clone()) {
                return Err(invalid_opts("duplicate key in emit options JSON"));
            }
            self.expect_byte(b':')?;
            match key.as_str() {
                "compat_quoting" => {
                    let v = self.parse_string()?;
                    eopts.compat_quoting = match v.as_str() {
                        "yaml11_safe" | "yaml11-safe" => CompatQuoting::Yaml11Safe,
                        "core_only" | "core-only" => CompatQuoting::CoreOnly,
                        _ => return Err(invalid_opts("unknown compat_quoting value")),
                    };
                }
                "limits" => {
                    self.parse_emit_limits(&mut eopts.limits)?;
                }
                _ => return Err(invalid_opts("unknown key in emit options JSON")),
            }
            self.skip_ws();
            if self.pos < self.b.len() && self.b[self.pos] == b',' {
                self.pos += 1;
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }
        self.skip_ws();
        if self.pos != self.b.len() {
            return Err(invalid_opts("trailing data after emit options JSON"));
        }
        Ok(eopts)
    }

    fn parse_emit_limits(&mut self, limits: &mut EmitLimits) -> Result<(), Error> {
        self.expect_byte(b'{')?;
        self.skip_ws();
        if self.pos < self.b.len() && self.b[self.pos] == b'}' {
            self.pos += 1;
            return Ok(());
        }
        let mut seen = BTreeSet::new();
        loop {
            let key = self.parse_string()?;
            if !seen.insert(key.clone()) {
                return Err(invalid_opts("duplicate key in emit limits JSON"));
            }
            self.expect_byte(b':')?;
            let val = self.parse_usize()?;
            match key.as_str() {
                "max_depth" => limits.max_depth = val,
                "max_nodes" => limits.max_nodes = val,
                "max_scalar_bytes" => limits.max_scalar_bytes = val,
                "max_key_bytes" => limits.max_key_bytes = val,
                "max_mapping_entries" => limits.max_mapping_entries = val,
                "max_tag_bytes" => limits.max_tag_bytes = val,
                "max_documents" => limits.max_documents = val,
                "max_output_bytes" => limits.max_output_bytes = val,
                _ => return Err(invalid_opts("unknown key in emit limits JSON")),
            }
            self.skip_ws();
            if self.pos < self.b.len() && self.b[self.pos] == b',' {
                self.pos += 1;
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }
        Ok(())
    }

    fn parse_limits(&mut self, limits: &mut Limits) -> Result<(), Error> {
        self.expect_byte(b'{')?;
        self.skip_ws();
        if self.pos < self.b.len() && self.b[self.pos] == b'}' {
            self.pos += 1;
            return Ok(());
        }
        let mut seen = BTreeSet::new();
        loop {
            let key = self.parse_string()?;
            if !seen.insert(key.clone()) {
                return Err(invalid_opts("duplicate key in limits JSON"));
            }
            self.expect_byte(b':')?;
            let val = self.parse_usize()?;
            match key.as_str() {
                "max_input_bytes" => limits.max_input_bytes = val,
                "max_depth" => limits.max_depth = val,
                "max_nodes" => limits.max_nodes = val,
                "max_expanded_nodes" => limits.max_expanded_nodes = val,
                "max_alias_events" => limits.max_alias_events = val,
                "max_owned_bytes" => limits.max_owned_bytes = val,
                "max_scalar_bytes" => limits.max_scalar_bytes = val,
                "max_key_bytes" => limits.max_key_bytes = val,
                "max_mapping_entries" => limits.max_mapping_entries = val,
                "max_documents" => limits.max_documents = val,
                "max_output_bytes" => limits.max_output_bytes = val,
                "max_warnings" => limits.max_warnings = val,
                _ => return Err(invalid_opts("unknown key in limits JSON")),
            }
            self.skip_ws();
            if self.pos < self.b.len() && self.b[self.pos] == b',' {
                self.pos += 1;
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_and_negative_controls() {
        release();
        assert_eq!(abi_version(), 1);

        // Unreserved load_wire must fail with status 2.
        assert_eq!(load_wire(0, 0, 1), 2);

        // Oversized options_reserve must fail and invalidate state.
        assert_eq!(options_reserve(MAX_OPTIONS_BYTES + 1), 0);
        assert_eq!(input_reserve(1, 1024), 0);

        // Valid empty options + valid input.
        let opt_ptr = options_reserve(0);
        assert_ne!(opt_ptr, 0);
        let in_ptr = input_reserve(4, 1024);
        assert_ne!(in_ptr, 0);
        STATE.with(|c| c.borrow_mut().input.copy_from_slice(b"a: 1"));
        assert_eq!(load_wire(4, 0, 1), 0);
        assert!(output_len() > 0);

        // Post-load reuse without reserving again must be rejected.
        assert_eq!(load_wire(4, 0, 1), 2);

        // Partial reserve failure: reserve options, then fail input_reserve (len > max_input_bytes).
        assert_ne!(options_reserve(0), 0);
        assert_eq!(input_reserve(10, 5), 0);
        assert_eq!(load_wire(0, 0, 1), 2);

        // Absolute WASM input ceiling rejection.
        assert_ne!(options_reserve(0), 0);
        assert_eq!(
            input_reserve(MAX_WASM_INPUT_CEILING + 1, MAX_WASM_INPUT_CEILING + 10),
            0
        );

        // Invalid options JSON rejected during input_reserve before input allocation.
        let bad_opts = br#"{"limits":{"max_depth":0}}"#;
        assert_ne!(options_reserve(bad_opts.len()), 0);
        STATE.with(|c| c.borrow_mut().options.copy_from_slice(bad_opts));
        assert_eq!(input_reserve(4, 1024), 0);

        // Output budget exhaustion returns status 2 with static fallback (no recursive encode_error).
        let tiny_out_opts = br#"{"limits":{"max_output_bytes":16}}"#;
        assert_ne!(options_reserve(tiny_out_opts.len()), 0);
        STATE.with(|c| c.borrow_mut().options.copy_from_slice(tiny_out_opts));
        assert_ne!(input_reserve(4, 1024), 0);
        STATE.with(|c| c.borrow_mut().input.copy_from_slice(b"a: 1"));
        assert_eq!(load_wire(4, tiny_out_opts.len(), 1), 2);
        let fallback = STATE.with(|c| String::from_utf8(c.borrow().output.clone()).unwrap());
        assert!(fallback.contains("\"output_limit_exceeded\""), "{fallback}");
        assert!(fallback.contains("\"configured\":16"), "{fallback}");

        release();
        assert_eq!(output_len(), 0);
    }
}
