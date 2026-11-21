#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Rust/Go differential over fuzz corpora.
#
# Usage: scripts/fuzz-differential.sh MODE:DIR [MODE:DIR ...]
#        scripts/fuzz-differential.sh --seeds
#   MODE is "selector" (the first byte picks the load options, as in the
#   load/emit/encode fuzz targets) or "raw" (default options).
#   Files may be raw bytes or Go fuzz corpus files ("go test fuzz v1").
#
# Every input runs through the crosscheck drivers (Rust structcorpus example
# and cmd/structcorpus-driver) as a multi-document and a single-document
# record. A byte difference between the twins in wire, plain, emit, I-JSON or
# lint output, or a twin whose wire round trip differs from its own wire,
# fails unless fuzz/known-differences.jsonl has an open or accepted entry for
# that exact input digest, operation and options (see fuzz/README.md).
#
# Output never contains input bytes: a failure prints an input SHA-256 and the
# operation or failure kind only. Full details go to $YAMLPRIMS_FUZZ_OUT
# (default: a fresh temporary directory), which must resolve outside the
# working tree.
#
# YAMLPRIMS_FUZZ_RUST_DRIVER / YAMLPRIMS_FUZZ_GO_DRIVER replace the built
# drivers (used by scripts/fuzz-differential.test.sh).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
# Every output and temporary location is checked by filesystem identity
# before anything is created, defaults included.
tmp_base=$(python3 -I "$root/scripts/fuzz_differential.py" resolve-out "$root" "${TMPDIR:-/tmp}") || {
  echo "fuzz-differential: TMPDIR must be outside the working tree" >&2
  exit 2
}
[ "$#" -gt 0 ] || { echo "usage: $0 MODE:DIR [MODE:DIR ...] | --seeds" >&2; exit 2; }

helper="$root/scripts/fuzz_differential.py"
if [ -n "${YAMLPRIMS_FUZZ_OUT:-}" ]; then
  out=$(python3 -I "$helper" resolve-out "$root" "$YAMLPRIMS_FUZZ_OUT") || {
    echo "fuzz-differential: YAMLPRIMS_FUZZ_OUT must be outside the working tree" >&2
    exit 2
  }
  mkdir -p "$out"
else
  out=$(mktemp -d "$tmp_base/yamlprims-fuzz-diff.XXXXXX")
fi

tmp=$(mktemp -d "$tmp_base/yamlprims-fuzz-diff-bin.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

if [ "$1" = --seeds ]; then
  # Differential over the committed seeds only.
  python3 -I fuzz/seeds.py "$tmp/seeds" >/dev/null
  set -- "selector:$tmp/seeds/load" "raw:$tmp/seeds/lint_yaml11"
fi

rust_driver=${YAMLPRIMS_FUZZ_RUST_DRIVER:-}
go_driver=${YAMLPRIMS_FUZZ_GO_DRIVER:-}
if [ -z "$rust_driver" ]; then
  cargo build --quiet --locked -p yamlprims --example structcorpus
  rust_driver="${CARGO_TARGET_DIR:-$root/target}/debug/examples/structcorpus"
fi
if [ -z "$go_driver" ]; then
  go_driver="$tmp/structcorpus-driver-go"
  GOWORK=off CGO_ENABLED=0 go build -o "$go_driver" ./cmd/structcorpus-driver
fi

python3 -I "$helper" run "$root/fuzz/known-differences.jsonl" "$rust_driver" "$go_driver" "$out" "$@"
