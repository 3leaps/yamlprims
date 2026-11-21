// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mirrors Go FuzzLoad: error kinds are not Internal, and the single- and
//! multi-document entrypoints agree (§5.5).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| yamlprims_fuzz::targets::fuzz_load(data));
