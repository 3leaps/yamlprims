#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Bounded mutating fuzz run for the Rust core (libFuzzer), then the Rust/Go
# differential over the corpora it produced. Local or private hosts only.
#
# Output rules: this script prints target names, execution counts, coverage
# and failure kinds only. It never prints an input, a dump of one, or a path
# inside the working tree. Logs, corpora, crash inputs and a campaign.json
# record go to $YAMLPRIMS_FUZZ_OUT (default: a fresh temporary directory),
# which must be outside the working tree. The script fails if a run leaves
# anything new under yaml/testdata/ or fuzz/.
#
# Settings (environment):
#   YAMLPRIMS_FUZZ_RUST_SECONDS   libFuzzer -max_total_time per target (default 60)
#   YAMLPRIMS_FUZZ_TIMEOUT        libFuzzer -timeout seconds per input (default 10)
#   YAMLPRIMS_FUZZ_RSS_MB         libFuzzer -rss_limit_mb and -malloc_limit_mb (default 2048)
#   YAMLPRIMS_FUZZ_MAX_LEN        libFuzzer -max_len bytes (default 65536)
#   YAMLPRIMS_FUZZ_TOOLCHAIN      rustup toolchain name; must have the commit hash
#                                 pinned in fuzz/toolchain.txt (default: the pinned channel)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

rust_seconds=${YAMLPRIMS_FUZZ_RUST_SECONDS:-60}
per_input_timeout=${YAMLPRIMS_FUZZ_TIMEOUT:-10}
rss_mb=${YAMLPRIMS_FUZZ_RSS_MB:-2048}
max_len=${YAMLPRIMS_FUZZ_MAX_LEN:-65536}

out=${YAMLPRIMS_FUZZ_OUT:-$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-fuzz.XXXXXX")}
case "$out" in /*) ;; *) out="$PWD/$out" ;; esac
case "$out/" in
  "$PWD/"* | "$(pwd -P)/"*) echo "fuzz-smoke: YAMLPRIMS_FUZZ_OUT must be outside the working tree" >&2; exit 2 ;;
esac
mkdir -p "$out/logs" "$out/crashers"
echo "fuzz-smoke: output in $out"


tree_state() { git status --porcelain=v1 --untracked-files=all --ignored -- yaml/testdata fuzz; }
tree_state >"$out/tree-before.txt"

failures=0
fail() { # target kind
  echo "FAIL $1: $2"
  failures=$((failures + 1))
}
# One row per run for the campaign record: target budget_s execs coverage outcome kind
runs="$out/runs.tsv"
: >"$runs"
row() { printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$@" >>"$runs"; }

# Classify a failed run from its log, without echoing the log.
classify() { # log
  local log=$1
  if grep -q 'libFuzzer: timeout' "$log"; then echo timeout
  elif grep -q 'out-of-memory\|malloc limit' "$log"; then echo memory-limit
  elif grep -q 'deadly signal\|panicked at' "$log"; then echo panic
  elif grep -q 'AddressSanitizer' "$log"; then echo sanitizer
  else echo error
  fi
}

pin_channel=$(awk '$1 == "channel" {print $2}' fuzz/toolchain.txt)
pin_hash=$(awk '$1 == "commit-hash" {print $2}' fuzz/toolchain.txt)
toolchain=${YAMLPRIMS_FUZZ_TOOLCHAIN:-$pin_channel}
have_hash=$(rustc "+$toolchain" -vV 2>/dev/null | awk '$1 == "commit-hash:" {print $2}')
if [ "$have_hash" != "$pin_hash" ]; then
  echo "fuzz-smoke: toolchain $toolchain is not the pinned nightly ($pin_channel, $pin_hash)" >&2
  exit 2
fi
host=$(rustc "+$toolchain" -vV | awk '$1 == "host:" {print $2}')
printf '%s\n' "engine=libfuzzer per_input_timeout_s=$per_input_timeout memory_limit_mb=$rss_mb max_len=$max_len" \
  "toolchain=$toolchain toolchain_commit=$pin_hash" >"$out/settings.txt"

export CARGO_TARGET_DIR="$root/target/fuzz"
if ! RUSTFLAGS="-Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4 \
-Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table \
-Cllvm-args=-sanitizer-coverage-trace-compares --cfg fuzzing -Cllvm-args=-simplifycfg-branch-fold-threshold=0 \
-Zsanitizer=address -Cdebug-assertions -Coverflow-checks" \
  cargo "+$toolchain" build --locked --release --manifest-path fuzz/Cargo.toml --features libfuzzer \
  --target "$host" --bins >"$out/logs/build.log" 2>&1; then
  echo "fuzz-smoke: fuzz build failed (log in $out/logs)" >&2
  exit 2
fi
python3 -I fuzz/seeds.py "$out/seeds" >/dev/null

for t in load decode_wire emit_round_trip lint_yaml11 encode; do
  log="$out/logs/$t.log"
  mkdir -p "$out/corpus/$t" "$out/crashers/$t"
  cp "$out/seeds/$t/"* "$out/corpus/$t/"
  dict=()
  [ "$t" != decode_wire ] && dict=(-dict="$root/fuzz/yaml.dict")
  # libFuzzer runs from the output directory so nothing lands in the tree.
  if (cd "$out" && "$CARGO_TARGET_DIR/$host/release/$t" "$out/corpus/$t" ${dict[@]+"${dict[@]}"} \
    -max_total_time="$rust_seconds" -timeout="$per_input_timeout" \
    -rss_limit_mb="$rss_mb" -malloc_limit_mb="$rss_mb" -max_len="$max_len" \
    -artifact_prefix="$out/crashers/$t/" -print_final_stats=1 >"$log" 2>&1); then
    outcome=pass kind=""
  else
    outcome=fail kind=$(classify "$log")
    fail "$t" "$kind"
  fi
  execs=$(sed -n 's/^stat::number_of_executed_units: *//p' "$log" | tail -1)
  cov=$(grep -o 'cov: [0-9]*' "$log" | tail -1 | cut -d' ' -f2 || true)
  [ "$outcome" = pass ] && echo "ok   $t: ${execs:-?} execs in ${rust_seconds}s, cov ${cov:-?}"
  row "$t" "$rust_seconds" "${execs:-}" "${cov:-}" "$outcome" "$kind"
done

# Differential over the YAML corpora the run produced.
if YAMLPRIMS_FUZZ_OUT="$out" bash scripts/fuzz-differential.sh "selector:$out/corpus/load" \
  "selector:$out/corpus/emit_round_trip" "selector:$out/corpus/encode" "raw:$out/corpus/lint_yaml11" \
  >"$out/logs/differential.log" 2>&1; then
  echo "ok   differential: $(tail -1 "$out/logs/differential.log" | sed 's/^fuzz-differential PASS: //')"
else
  n=$(grep -c '^DIFF ' "$out/logs/differential.log" || true)
  if [ "${n:-0}" -gt 0 ]; then
    fail differential "$n unlisted differences (listed by input SHA-256 in $out/logs)"
  else
    fail differential "error (log in $out/logs)"
  fi
fi

tree_state >"$out/tree-after.txt"
if ! cmp -s "$out/tree-before.txt" "$out/tree-after.txt"; then
  fail tree "files were left under yaml/testdata/ or fuzz/"
fi

# Campaign record. Field names are generic so other subjects can use the same shape.
python3 -I - "$out" "$runs" "$failures" <<'PY'
import json, platform, subprocess, sys
out, runs, failures = sys.argv[1], sys.argv[2], int(sys.argv[3])
def cmd(*a):
    try:
        return subprocess.run(a, capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        return None
settings = {}
for kv in open(f"{out}/settings.txt", encoding="utf-8").read().split():
    k, _, v = kv.partition("=")
    settings[k] = v
rows = []
for line in open(runs, encoding="utf-8"):
    target, budget, execs, cov, outcome, kind = line.rstrip("\n").split("\t")
    rows.append({"target": target, "budget_seconds": int(budget),
                 "executions": int(execs) if execs else None,
                 "coverage": int(cov) if cov else None,
                 "outcome": outcome, "failure_kind": kind or None})
record = {
    "record_format": "fuzz-campaign/v0",
    "kind": "bounded-smoke",
    "subject": {"repository": cmd("git", "config", "--get", "remote.origin.url"),
                "revision": cmd("git", "rev-parse", "HEAD"),
                "worktree_dirty": bool(cmd("git", "status", "--porcelain"))},
    "host": {"os": platform.system(), "arch": platform.machine()},
    "settings": dict({"mutation": True}, **settings),
    "runs": rows,
    "failures": failures,
}
with open(f"{out}/campaign.json", "w", encoding="utf-8") as f:
    json.dump(record, f, indent=2)
PY

if [ "$failures" -gt 0 ]; then
  echo "fuzz-smoke: FAILED ($failures)"
  exit 1
fi
echo "fuzz-smoke: PASS"
