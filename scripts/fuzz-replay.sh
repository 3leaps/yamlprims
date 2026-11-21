#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Replay every fuzz target over the committed seeds, with no mutation, on
# stable toolchains:
#   - Go: the Fuzz* functions in yaml/fuzz_test.go run their seed corpus
#     (conformance vectors and wire fixtures) as ordinary tests;
#   - Rust: fuzz/tests/replay.rs feeds the same seeds, plus any files under
#     fuzz/regressions/<target>/, to the functions the libFuzzer targets call.
#
# Output names the target and the failure kind only. A changed parser can fail
# on a public seed, and test failure text may describe the input, so the full
# logs stay in a temporary directory outside the working tree.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

logs=$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-fuzz-replay.XXXXXX")
failures=0

kind() { # log
  if grep -q '^panic:\|panicked at' "$1"; then echo panic; else echo property; fi
}

if GOWORK=off CGO_ENABLED=0 go test -count=1 -run '^Fuzz' ./yaml/ >"$logs/go.log" 2>&1; then
  echo "ok   go: $(grep -c '^func Fuzz' yaml/fuzz_test.go) targets"
else
  k=$(kind "$logs/go.log")
  names=$(sed -n 's/^--- FAIL: \(Fuzz[A-Za-z0-9]*\).*/\1/p' "$logs/go.log" | sort -u)
  for n in ${names:-unknown}; do
    echo "FAIL go/$n: $k"
    failures=$((failures + 1))
  done
fi

export CARGO_TARGET_DIR="$root/target/fuzz-check"
if cargo test --locked --manifest-path fuzz/Cargo.toml --test replay >"$logs/rust.log" 2>&1; then
  echo "ok   rust: $(grep -c '^#\[test\]' fuzz/tests/replay.rs) targets"
else
  names=$(sed -n 's/^test \([a-z0-9_]*\) \.\.\. FAILED$/\1/p' "$logs/rust.log" | sort -u)
  for n in ${names:-build}; do
    echo "FAIL rust/$n: property"
    failures=$((failures + 1))
  done
fi

if [ "$failures" -gt 0 ]; then
  echo "fuzz-replay: FAILED ($failures; logs in $logs)"
  exit 1
fi
echo "fuzz-replay: PASS"
