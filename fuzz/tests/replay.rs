// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replay without mutation, on stable Rust: every fuzz target runs once over
//! the committed seeds (conformance vectors and wire fixtures) and any files
//! under `fuzz/regressions/<target>/`. Failures name the target and the
//! failure kind only.

use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use yamlprims_fuzz::{targets, QUALITY_PREFIX};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn hex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "odd-length input_hex");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("input_hex"))
        .collect()
}

/// (id, bytes) for every conformance vector with an input.
fn vectors() -> Vec<(String, Vec<u8>)> {
    let text = fs::read_to_string(repo().join("conformance/vectors/cases.jsonl")).expect("vectors");
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let rec: serde_json::Value = serde_json::from_str(line).expect("vector JSON");
        let id = rec["id"].as_str().unwrap_or("?").to_string();
        if let Some(s) = rec["input"].as_str() {
            out.push((id, s.as_bytes().to_vec()));
        } else if let Some(h) = rec["input_hex"].as_str() {
            out.push((id, hex(h)));
        }
    }
    assert!(!out.is_empty(), "no conformance inputs");
    out
}

fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    paths
        .into_iter()
        .filter(|p| p.is_file())
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read(&p).expect("read seed"))
        })
        .collect()
}

/// Each conformance input once per options selector byte.
fn selected() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for (id, data) in vectors() {
        for sel in 0u8..4 {
            let mut d = vec![sel];
            d.extend_from_slice(&data);
            out.push((format!("{id}#options{sel}"), d));
        }
    }
    out
}

/// Runs every seed through the target. A failure reports the target and the
/// failure kind only: a changed parser can fail on a public seed, and the
/// panic text may describe the input.
fn replay(target: &str, f: fn(&[u8]), mut seeds: Vec<(String, Vec<u8>)>) {
    seeds.extend(files(&repo().join("fuzz/regressions").join(target)));
    let total = seeds.len();
    // The panic hook is process-wide: one replay at a time.
    static HOOK: Mutex<()> = Mutex::new(());
    let _guard = HOOK.lock().unwrap_or_else(|e| e.into_inner());
    let (property, quality) = std::thread::scope(|s| {
        s.spawn(|| {
            let prev = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let (mut property, mut quality) = (0usize, 0usize);
            for (_, data) in &seeds {
                if let Err(payload) = catch_unwind(AssertUnwindSafe(|| f(data))) {
                    let text = payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("");
                    if text.starts_with(QUALITY_PREFIX) {
                        quality += 1;
                    } else {
                        property += 1;
                    }
                }
            }
            std::panic::set_hook(prev);
            (property, quality)
        })
        .join()
        .expect("replay thread")
    });
    assert!(
        property == 0 && quality == 0,
        "{target}: {property} property and {quality} implementation-quality failures in {total} seeds"
    );
}

#[test]
fn load() {
    replay("load", targets::fuzz_load, selected());
}

#[test]
fn emit_round_trip() {
    replay("emit_round_trip", targets::fuzz_emit_round_trip, selected());
}

#[test]
fn encode() {
    replay("encode", targets::fuzz_encode, selected());
}

#[test]
fn lint_yaml11() {
    replay("lint_yaml11", targets::fuzz_lint_yaml11, vectors());
}

#[test]
fn decode_wire() {
    let wire = files(&repo().join("crates/yamlprims/tests/fixtures/wire"))
        .into_iter()
        .filter(|(n, _)| n.ends_with(".json"))
        .collect();
    replay("decode_wire", targets::fuzz_decode_wire, wire);
}
