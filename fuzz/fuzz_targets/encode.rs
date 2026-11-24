// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mirrors Go FuzzEncode: wire and plain output is JSON, and wire output
//! round-trips (§11.1).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| yamlprims_fuzz::targets::fuzz_encode(data));
