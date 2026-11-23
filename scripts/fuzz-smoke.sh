#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Bounded mutating fuzz run for the Rust core (libFuzzer), then the Rust/Go
# differential over the corpora it produced. Local or private hosts only.
#
# Output rules: this script prints target names, execution counts, coverage
# and failure kinds only. It never prints an input, a dump of one, or a path
# inside the working tree. Logs, corpora, crash inputs and a campaign.json
# record go to $YAMLPRIMS_FUZZ_OUT (default: a fresh temporary directory),
# which must resolve outside the working tree. The script fails if a run leaves
# anything new under yaml/testdata/ or fuzz/.
#
# Settings (environment):
#   YAMLPRIMS_FUZZ_RUST_SECONDS   libFuzzer -max_total_time per target (default 60)
#   YAMLPRIMS_FUZZ_TIMEOUT        libFuzzer -timeout seconds per input (default 10)
#   YAMLPRIMS_FUZZ_RSS_MB         libFuzzer -rss_limit_mb and -malloc_limit_mb (default 2048)
#   YAMLPRIMS_FUZZ_MAX_LEN        libFuzzer -max_len bytes (default 65536)
#   YAMLPRIMS_FUZZ_TOOLCHAIN      rustup toolchain name; must have the commit hash
#                                 pinned in fuzz/toolchain.txt (default: the pinned channel)
#   YAMLPRIMS_FUZZ_KEEP_GOING     1: libFuzzer fork mode (-fork=1) ignoring crashes,
#                                 timeouts and OOMs, so a target keeps fuzzing for its
#                                 whole budget after a failing input. Every saved
#                                 artifact still fails the run, counted by kind.
#                                 0 (default): a target stops at its first failure.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
# Every output and temporary location is checked by filesystem identity
# before anything is created, defaults included.
tmp_base=$(python3 -I "$root/scripts/fuzz_differential.py" resolve-out "$root" "${TMPDIR:-/tmp}") || {
  echo "fuzz-smoke: TMPDIR must be outside the working tree" >&2
  exit 2
}

rust_seconds=${YAMLPRIMS_FUZZ_RUST_SECONDS:-60}
per_input_timeout=${YAMLPRIMS_FUZZ_TIMEOUT:-10}
rss_mb=${YAMLPRIMS_FUZZ_RSS_MB:-2048}
max_len=${YAMLPRIMS_FUZZ_MAX_LEN:-65536}
keep_going=${YAMLPRIMS_FUZZ_KEEP_GOING:-0}
case "$keep_going" in
  0) keep_flags=() ;;
  1) keep_flags=(-fork=1 -ignore_crashes=1 -ignore_timeouts=1 -ignore_ooms=1) ;;
  *) echo "fuzz-smoke: YAMLPRIMS_FUZZ_KEEP_GOING must be 0 or 1" >&2; exit 2 ;;
esac

# The output root is created exclusively (or is an empty directory, or one
# this harness marked, so a campaign can reuse its corpora). Containment is
# checked by identity, and no symlink or hard-linked file may appear anywhere
# below it: guard runs before every step that creates or writes files there.
helper="$root/scripts/fuzz_differential.py"
out=${YAMLPRIMS_FUZZ_OUT:-$(mktemp -d "$tmp_base/yamlprims-fuzz.XXXXXX")}
out=$(python3 -I "$helper" claim-out "$root" "$out") || {
  echo "fuzz-smoke: YAMLPRIMS_FUZZ_OUT refused (see fuzz/README.md)" >&2
  exit 2
}
guard() { python3 -I "$helper" check-tree "$out" || { echo "fuzz-smoke: output directory changed under the run" >&2; exit 2; }; }
under() { python3 -I "$helper" mkdir-under "$out" "$@" || exit 2; }
# One campaign per output root at a time: the rest of this script runs as the
# child of a process that takes an exclusive flock on the root's lock file and
# passes the locked descriptor to this script and everything it starts. The
# lock stays held while any process of the run has it open; the lock file is
# never removed. Every run writes into its own exclusively
# created directory under runs/; only the corpora are shared.
if [ "${YAMLPRIMS_FUZZ_LOCKED_OUT:-}" != "$out" ]; then
  exec python3 -I "$helper" hold-lock "$out" -- \
    env YAMLPRIMS_FUZZ_LOCKED_OUT="$out" YAMLPRIMS_FUZZ_OUT="$out" bash "$root/scripts/fuzz-smoke.sh"
fi
# This process inherited the locked descriptor; confirm it is the root's lock
# file and holds the lock.
if python3 -I "$helper" lock-owned "$out" "${YAMLPRIMS_FUZZ_LOCK_FD:-none}" 2>/dev/null; then :; else
  echo "fuzz-smoke: the output directory lock is not held by this run" >&2
  exit 2
fi
seeds=""
cleanup() { if [ -n "$seeds" ]; then rm -rf "$seeds"; fi; }
trap cleanup EXIT
under runs
run="$out/runs/$(date -u +%Y%m%dT%H%M%SZ)-$$-$RANDOM"
mkdir "$run" "$run/logs" "$run/crashers"
# Files are written under a fresh name (mktemp creates it exclusively) and
# renamed into place, so an existing file or link at the destination is
# replaced, never written through or truncated.
fresh() { mktemp "$(dirname "$1")/.write.XXXXXX"; }
place() { mv -f "$1" "$2"; }
echo "fuzz-smoke: output in $out (this run: ${run#"$out"/})"


tree_state() { git status --porcelain=v1 --untracked-files=all --ignored -- yaml/testdata fuzz; }
guard
f=$(fresh "$run/tree-before.txt")
tree_state >"$f"
place "$f" "$run/tree-before.txt"

failures=0
fail() { # target kind
  echo "FAIL $1: $2"
  failures=$((failures + 1))
}
# One row per run for the campaign record: target budget_s execs coverage outcome kind
guard
runs=$(fresh "$run/runs.tsv")
row() { printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$@" >>"$runs"; }

# Classify a failed run from its log, without echoing the log.
classify() { # log
  local log=$1
  if grep -q 'libFuzzer: timeout' "$log"; then echo timeout
  elif grep -q 'implementation-quality:' "$log"; then echo quality
  elif grep -q 'out-of-memory\|malloc limit' "$log"; then echo memory-limit
  elif grep -q 'deadly signal\|panicked at' "$log"; then echo panic
  elif grep -q 'AddressSanitizer' "$log"; then echo sanitizer
  else echo error
  fi
}
# Keep-going runs: the saved artifacts by kind, e.g. "crash=3 timeout=1"; empty if none.
artifact_kinds() { # dir
  local d=$1 k n out=""
  for k in crash timeout oom leak slow-unit; do
    n=$(find "$d" -maxdepth 1 -type f -name "$k-*" | wc -l | tr -d ' ')
    [ "$n" -gt 0 ] && out="$out${out:+ }$k=$n"
  done
  echo "$out"
}

pin_channel=$(awk '$1 == "channel" {print $2}' fuzz/toolchain.txt)
pin_hash=$(awk '$1 == "commit-hash" {print $2}' fuzz/toolchain.txt)
toolchain=${YAMLPRIMS_FUZZ_TOOLCHAIN:-$pin_channel}
# Never let rustup download a missing toolchain as a side effect.
export RUSTUP_AUTO_INSTALL=0
have_hash=$(rustc "+$toolchain" -vV 2>/dev/null | awk '$1 == "commit-hash:" {print $2}')
if [ "$have_hash" != "$pin_hash" ]; then
  echo "fuzz-smoke: toolchain $toolchain is not the pinned nightly ($pin_channel, $pin_hash)" >&2
  exit 2
fi
host=$(rustc "+$toolchain" -vV | awk '$1 == "host:" {print $2}')
guard
f=$(fresh "$run/settings.txt")
printf '%s\n' "engine=libfuzzer per_input_timeout_s=$per_input_timeout memory_limit_mb=$rss_mb max_len=$max_len keep_going=$keep_going" \
  "toolchain=$toolchain toolchain_commit=$pin_hash" >"$f"
place "$f" "$run/settings.txt"

export CARGO_TARGET_DIR="$root/target/fuzz"
guard
f=$(fresh "$run/logs/build.log")
if ! RUSTFLAGS="-Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4 \
-Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table \
-Cllvm-args=-sanitizer-coverage-trace-compares --cfg fuzzing -Cllvm-args=-simplifycfg-branch-fold-threshold=0 \
-Zsanitizer=address -Cdebug-assertions -Coverflow-checks" \
  cargo "+$toolchain" build --locked --release --manifest-path fuzz/Cargo.toml --features libfuzzer \
  --target "$host" --bins >"$f" 2>&1; then
  place "$f" "$run/logs/build.log"
  echo "fuzz-smoke: fuzz build failed (log in $run/logs)" >&2
  exit 2
fi
place "$f" "$run/logs/build.log"
# Seeds go to a fresh directory of their own each run (a reused output root
# keeps its corpora; seeds are regenerated).
seeds=$(mktemp -d "$tmp_base/yamlprims-fuzz-seeds.XXXXXX")
python3 -I fuzz/seeds.py "$seeds" >/dev/null

for t in load decode_wire emit_round_trip lint_yaml11 encode; do
  log="$run/logs/$t.log"
  mkdir "$run/crashers/$t"
  guard
  # Seeds enter the shared corpus by exclusive creation only; a name already
  # there is skipped, never opened. libFuzzer itself adds a new unit as
  # corpus/<target>/<SHA-1 of its contents> with fopen(path, "wb"), which
  # follows links and truncates; the walk before and after each run, and the
  # trusted-ancestor assumption in fuzz/README.md, cover that path.
  python3 -I "$helper" import-new "$out" "corpus/$t" "$seeds/$t" >/dev/null || exit 2
  guard
  f=$(fresh "$log")
  dict=()
  [ "$t" != decode_wire ] && dict=(-dict="$root/fuzz/yaml.dict")
  # libFuzzer runs from this run's directory so nothing lands in the tree.
  if (cd "$run" && "$CARGO_TARGET_DIR/$host/release/$t" "$out/corpus/$t" ${dict[@]+"${dict[@]}"} \
    -max_total_time="$rust_seconds" -timeout="$per_input_timeout" \
    -rss_limit_mb="$rss_mb" -malloc_limit_mb="$rss_mb" -max_len="$max_len" \
    ${keep_flags[@]+"${keep_flags[@]}"} \
    -artifact_prefix="$run/crashers/$t/" -print_final_stats=1 >"$f" 2>&1); then
    place "$f" "$log"
    outcome=pass kind=""
  else
    place "$f" "$log"
    outcome=fail kind=$(classify "$log")
  fi
  if [ "$keep_going" = 1 ]; then
    found=$(artifact_kinds "$run/crashers/$t")
    if [ -n "$found" ]; then
      outcome=fail kind="$(classify "$log") ($found)"
    fi
  fi
  [ "$outcome" = fail ] && fail "$t" "$kind"
  guard
  execs=$(sed -n 's/^stat::number_of_executed_units: *//p' "$log" | tail -1)
  # Fork mode prints no final stats; take the last "#<execs>: cov:" status line.
  [ -n "$execs" ] || execs=$(sed -n 's/^#\([0-9]*\): cov:.*/\1/p' "$log" | tail -1)
  cov=$(grep -o 'cov: [0-9]*' "$log" | tail -1 | cut -d' ' -f2 || true)
  [ "$outcome" = pass ] && echo "ok   $t: ${execs:-?} execs in ${rust_seconds}s, cov ${cov:-?}"
  row "$t" "$rust_seconds" "${execs:-}" "${cov:-}" "$outcome" "$kind"
done

# Differential over the YAML corpora the run produced.
guard
f=$(fresh "$run/logs/differential.log")
if YAMLPRIMS_FUZZ_OUT="$run/differential" bash scripts/fuzz-differential.sh "selector:$out/corpus/load" \
  "selector:$out/corpus/emit_round_trip" "selector:$out/corpus/encode" "raw:$out/corpus/lint_yaml11" \
  >"$f" 2>&1; then
  place "$f" "$run/logs/differential.log"
  echo "ok   differential: $(tail -1 "$run/logs/differential.log" | sed 's/^fuzz-differential PASS: //')"
else
  place "$f" "$run/logs/differential.log"
  n=$(grep -c '^DIFF ' "$run/logs/differential.log" || true)
  if [ "${n:-0}" -gt 0 ]; then
    fail differential "$n unlisted differences (listed by input SHA-256 in $run/differential)"
  else
    fail differential "error (log in $run/logs)"
  fi
fi

guard
f=$(fresh "$run/tree-after.txt")
tree_state >"$f"
place "$f" "$run/tree-after.txt"
if ! cmp -s "$run/tree-before.txt" "$run/tree-after.txt"; then
  fail tree "files were left under yaml/testdata/ or fuzz/"
fi

guard
# Campaign record. Field names are generic so other subjects can use the same shape.
python3 -I - "$run" "$runs" "$failures" <<'PY'
import json, os, platform, subprocess, sys
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
# Fresh exclusive name, then rename: never writes through an existing file.
tmp = f"{out}/.campaign.json.{os.getpid()}"
fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
with os.fdopen(fd, "w", encoding="utf-8") as f:
    json.dump(record, f, indent=2)
os.replace(tmp, f"{out}/campaign.json")
PY

place "$runs" "$run/runs.tsv"

if [ "$failures" -gt 0 ]; then
  echo "fuzz-smoke: FAILED ($failures)"
  exit 1
fi
echo "fuzz-smoke: PASS"
