#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Replay every fuzz target over the committed seeds, with no mutation, on
# stable toolchains:
#   - Go: the Fuzz* functions in yaml/fuzz_test.go run their seed corpus
#     (conformance vectors and wire fixtures) as ordinary tests;
#   - Rust: fuzz/tests/replay.rs feeds the same seeds, plus any files under
#     fuzz/regressions/<target>/, to the functions the libFuzzer targets call.
#
# Output names the target and the failure kind only (property, quality,
# hang, memory or panic; quality is the implementation-quality check). A changed parser can fail
# on a public seed, and test failure text may describe the input, so the full
# logs stay in a temporary directory outside the working tree.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

logs=$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-fuzz-replay.XXXXXX")
failures=0

# Per failing Go target, the failure kinds (target kind), from the go test log.
go_failures() { # log
  awk '
    /^--- FAIL: Fuzz/ { t = $3; sub(/\(.*/, "", t); seen[t] = 1; next }
    t == "" { next }
    /fuzz-guard: hang/ { k[t, "hang"] = 1; next }
    /fuzz-guard: memory/ { k[t, "memory"] = 1; next }
    /implementation-quality:/ { k[t, "quality"] = 1; next }
    /fuzz-guard: panic|^panic:/ { k[t, "panic"] = 1; next }
    /^        [a-z_]+\.go:[0-9]+:/ { k[t, "property"] = 1 }
    END {
      for (t in seen) {
        n = 0
        split("panic hang memory quality property", order, " ")
        for (i = 1; i <= 5; i++) if ((t, order[i]) in k) { print t, order[i]; n++ }
        if (n == 0) print t, "property"
      }
    }' "$1"
}

if GOWORK=off CGO_ENABLED=0 go test -count=1 -run '^Fuzz' ./yaml/ >"$logs/go.log" 2>&1; then
  echo "ok   go: $(grep -c '^func Fuzz' yaml/fuzz_test.go) targets"
else
  lines=$(go_failures "$logs/go.log")
  [ -n "$lines" ] || lines="build-or-test-binary error"
  while read -r name kind; do
    echo "FAIL go/$name: $kind"
    failures=$((failures + 1))
  done <<<"$lines"
fi

export CARGO_TARGET_DIR="$root/target/fuzz-check"
if cargo test --locked --manifest-path fuzz/Cargo.toml --test replay >"$logs/rust.log" 2>&1; then
  echo "ok   rust: $(grep -c '^#\[test\]' fuzz/tests/replay.rs) targets"
else
  # The replay test reports "<target>: P property and Q implementation-quality failures".
  lines=$(sed -n 's/^\([a-z0-9_]*\): \([0-9]*\) property and \([0-9]*\) implementation-quality failures.*/\1 \2 \3/p' "$logs/rust.log")
  if [ -z "$lines" ]; then
    echo "FAIL rust: build error"
    failures=$((failures + 1))
  fi
  while read -r name p q; do
    [ -n "${name:-}" ] || continue
    if [ "$p" -gt 0 ]; then echo "FAIL rust/$name: property"; failures=$((failures + 1)); fi
    if [ "$q" -gt 0 ]; then echo "FAIL rust/$name: quality"; failures=$((failures + 1)); fi
  done <<<"$lines"
fi

if [ "$failures" -gt 0 ]; then
  echo "fuzz-replay: FAILED ($failures; logs in $logs)"
  exit 1
fi
echo "fuzz-replay: PASS"
