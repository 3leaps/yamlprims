// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mirrors Go FuzzLintYAML11: §7.2 coordinates and the §11.7 encode_lint
//! preflight accept what the linter produced.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| yamlprims_fuzz::targets::fuzz_lint_yaml11(data));
