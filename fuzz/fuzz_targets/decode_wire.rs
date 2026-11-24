// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mirrors Go FuzzDecodeWire: anything decode_wire accepts re-encodes, and that
//! encoder output round-trips byte-identically (§11.1).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| yamlprims_fuzz::targets::fuzz_decode_wire(data));
