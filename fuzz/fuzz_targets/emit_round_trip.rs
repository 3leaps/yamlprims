// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mirrors Go FuzzEmitRoundTrip: the §10.2 self-round-trip invariant.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| yamlprims_fuzz::targets::fuzz_emit_round_trip(data));
