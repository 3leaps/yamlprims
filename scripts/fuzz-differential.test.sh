#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Controls for scripts/fuzz-differential.sh and scripts/fuzz_differential.py:
# known-differences validation, per-twin round-trip and cross-language
# comparison (with stub drivers), corpus decode failures, and output-directory
# containment.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
helper=scripts/fuzz_differential.py
# Like the scripts under test, check TMPDIR by filesystem identity first.
tmp_base=$(python3 -I "$helper" resolve-out "$root" "${TMPDIR:-/tmp}") || {
  echo "fuzz-differential.test: TMPDIR must be outside the working tree" >&2
  exit 2
}
work=$(mktemp -d "$tmp_base/yamlprims-fuzz-diff-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
pass=0
failed=0
ok() { pass=$((pass + 1)); }
bad() { echo "FAIL: $1" >&2; failed=$((failed + 1)); }

digest=$(printf 'a: 1\n' | shasum -a 256 | cut -c1-64)
rev=0123456789abcdef0123456789abcdef01234567
entry() { # field=json-value overrides on a valid entry
  python3 -I - "$digest" "$rev" "$@" <<'PY'
import json, sys
digest, rev, *overrides = sys.argv[1:]
e = {"id": "KD-T", "input_sha256": digest, "operation": "wire", "options": "default",
     "revisions": [rev], "owner": "maintainer", "status": "open"}
for o in overrides:
    k, _, v = o.partition("=")
    if v == "DELETE":
        e.pop(k, None)
    else:
        e[k] = json.loads(v)
print(json.dumps(e))
PY
}

# 1. known-differences validation: each rule rejects a bad entry.
check_rejects() { # name expected-message field=value...
  local name=$1 msg=$2
  shift 2
  entry "$@" >"$work/known.jsonl"
  if out=$(python3 -I "$helper" check-known "$work/known.jsonl" 2>&1); then
    bad "known-differences accepted $name"
  elif ! grep -q -- "$msg" <<<"$out"; then
    bad "known-differences $name: unexpected message"
  else
    ok
  fi
}
check_rejects "revisions true" "revisions must be" 'revisions=true'
check_rejects "empty revisions" "revisions must be" 'revisions=[]'
check_rejects "revision not an id" "revisions must be" 'revisions=["main"]'
check_rejects "owner true" "owner must be" 'owner=true'
check_rejects "blank owner" "owner must be" 'owner="  "'
check_rejects "unknown status" "status must be" 'status="wontfix"'
check_rejects "missing status" "status must be" 'status=DELETE'
check_rejects "unknown operation" "operation must be" 'operation="everything"'
check_rejects "unknown options" "options must be" 'options="all"'
check_rejects "category digest" "input_sha256 must be" 'input_sha256="*"'
check_rejects "unknown field" "unknown fields" 'scope="category"'

suppressing() { # status -> count of suppressing entries
  entry "status=\"$1\"" >"$work/known.jsonl"
  python3 -I "$helper" check-known "$work/known.jsonl" | sed -n 's/^known-differences: \([0-9]*\) suppressing.*/\1/p'
}
if [ "$(suppressing open)" = 1 ]; then ok; else bad "open entry does not suppress"; fi
if [ "$(suppressing accepted)" = 1 ]; then ok; else bad "accepted entry does not suppress"; fi
if [ "$(suppressing closed)" = 0 ]; then ok; else bad "closed entry suppresses"; fi

# The README example is a valid, suppressing entry.
python3 -I - fuzz/README.md >"$work/readme-known.jsonl" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
section = text[text.index("## Known differences"):]
print(re.search(r"```json\n(.*?)\n```", section, re.S).group(1))
PY
if [ "$(python3 -I "$helper" check-known "$work/readme-known.jsonl" 2>&1)" = "known-differences: 1 suppressing entries" ]; then
  ok
else
  bad "README known-differences example does not pass the validator"
fi

# 2. Comparisons with stub drivers. Each stub answers every record with fixed
# outputs: wire W, round trip R.
stub() { # path wire roundtrip
  cat >"$1" <<EOF
#!/usr/bin/env python3
import json, sys
for line in sys.stdin:
    if line.strip():
        r = json.loads(line)
        print(json.dumps({"id": r["id"], "wire": "$2", "roundtrip": "$3", "plain": "p", "emit": "e", "ijson": "i", "lint": "l"}))
EOF
  chmod +x "$1"
}
mkdir -p "$work/corpus"
printf 'a: 1\n' >"$work/corpus/input"
stub "$work/rust" W W
diffrun() { # go-wire go-roundtrip [known-status] -> DIFF operations, sorted, comma-joined
  stub "$work/go" "$1" "$2"
  if [ -n "${3:-}" ]; then entry "status=\"$3\"" >"$work/known.jsonl"; else : >"$work/known.jsonl"; fi
  python3 -I "$helper" run "$work/known.jsonl" "$work/rust" "$work/go" "$work" "raw:$work/corpus" 2>&1 |
    sed -n 's/^DIFF \([a-z-]*\):.*/\1/p' | sort -u | paste -sd, -
}
if [ -z "$(diffrun W W)" ]; then ok; else bad "identical twins reported a difference"; fi
if [ "$(diffrun W X)" = go-roundtrip ]; then ok; else bad "Go round trip against its own wire"; fi
if [ "$(diffrun X X)" = wire ]; then ok; else bad "cross-language wire difference kept apart from round trip"; fi
if [ "$(diffrun X Y)" = go-roundtrip,wire ]; then ok; else bad "both Go checks"; fi
if [ -z "$(diffrun X X open)" ]; then ok; else bad "open known difference does not suppress"; fi
if [ "$(diffrun X X closed)" = wire ]; then ok; else bad "closed known difference suppresses"; fi

# 2b. A driver that hangs on one input is killed, bisected down to that input,
# and reported as a timeout finding with its digest only. Other inputs in the
# same chunk are still compared, a known entry cannot suppress a timeout, and the
# per-record limit is validated.
mkdir -p "$work/corpus-hang"
printf 'a: 1\n' >"$work/corpus-hang/plain"
hang_marker=HANGMARK-$$
printf '%s\n' "$hang_marker" >"$work/corpus-hang/hang"
hang_hex=$(printf '%s' "$hang_marker" | od -An -tx1 | tr -d ' \n')
cat >"$work/go-hang" <<STUB
#!/usr/bin/env python3
import json, sys, time
for line in sys.stdin:
    if line.strip():
        r = json.loads(line)
        if "$hang_hex" in r.get("input_hex", ""):
            time.sleep(600)
        print(json.dumps({"id": r["id"], "wire": "W", "roundtrip": "W", "plain": "p", "emit": "e", "ijson": "i", "lint": "l"}), flush=True)
STUB
chmod +x "$work/go-hang"
: >"$work/known.jsonl"
start=$(date +%s)
status=0
out=$(YAMLPRIMS_FUZZ_DRIVER_TIMEOUT=1 python3 -I "$helper" run "$work/known.jsonl" "$work/rust" "$work/go-hang" "$work" "raw:$work/corpus-hang" 2>&1) || status=$?
elapsed=$(($(date +%s) - start))
if [ "$status" -ne 0 ]; then ok; else bad "a hanging driver passed"; fi
if [ "$elapsed" -lt 60 ]; then ok; else bad "a hanging driver was not bounded (${elapsed}s)"; fi
if [ "$(grep -c '^DIFF go-driver-timeout:' <<<"$out")" = 2 ]; then ok; else bad "hanging input not reported once per mode"; fi
if [ "$(grep -c '^DIFF ' <<<"$out")" = 2 ]; then ok; else bad "inputs beside the hanging one were not compared cleanly"; fi
if grep -q -e "$hang_marker" -e "$hang_hex" <<<"$out"; then bad "timeout output contains input bytes"; else ok; fi
if grep -q '(2 driver timeouts)' <<<"$out"; then ok; else bad "summary does not count driver timeouts"; fi
if pgrep -f "$work/go-hang" >/dev/null; then bad "hanging driver left running"; else ok; fi
entry 'operation="go-driver-timeout"' >"$work/known-hang.jsonl"
if python3 -I "$helper" check-known "$work/known-hang.jsonl" >/dev/null 2>&1; then bad "known list accepted a timeout entry"; else ok; fi
for v in 0 abc 3601; do
  if YAMLPRIMS_FUZZ_DRIVER_TIMEOUT=$v python3 -I "$helper" run "$work/known.jsonl" "$work/rust" "$work/go-hang" "$work" "raw:$work/corpus-ok" >/dev/null 2>&1; then
    bad "driver timeout $v accepted"
  else
    ok
  fi
done

# 2c. A driver that hangs only when one process sees both records of an input,
# but finishes each record alone, is reported as a chunk timeout naming the
# smallest such batch, not as a pass.
mkdir -p "$work/corpus-chunk"
chunk_marker=CHUNKMARK-$$
printf '%s\n' "$chunk_marker" >"$work/corpus-chunk/pair"
printf 'b: 2\n' >"$work/corpus-chunk/zplain"
chunk_hex=$(printf '%s' "$chunk_marker" | od -An -tx1 | tr -d ' \n')
cat >"$work/go-chunk" <<STUB
#!/usr/bin/env python3
import json, sys, time
seen = 0
for line in sys.stdin:
    if line.strip():
        r = json.loads(line)
        if "$chunk_hex" in r.get("input_hex", ""):
            seen += 1
            if seen >= 2:
                time.sleep(600)
        print(json.dumps({"id": r["id"], "wire": "W", "roundtrip": "W", "plain": "p", "emit": "e", "ijson": "i", "lint": "l"}), flush=True)
STUB
chmod +x "$work/go-chunk"
: >"$work/known.jsonl"
status=0
out=$(YAMLPRIMS_FUZZ_DRIVER_TIMEOUT=1 python3 -I "$helper" run "$work/known.jsonl" "$work/rust" "$work/go-chunk" "$work" "raw:$work/corpus-chunk" 2>&1) || status=$?
if [ "$status" -ne 0 ]; then ok; else bad "a chunk-only hang passed"; fi
if [ "$(grep -c '^DIFF go-driver-chunk-timeout: chunk:2:' <<<"$out")" = 1 ]; then ok; else bad "chunk-only hang not reported once at the smallest batch"; fi
if [ "$(grep -c '^DIFF ' <<<"$out")" = 1 ]; then ok; else bad "chunk-only hang reported other differences"; fi
if grep -q -e "$chunk_marker" -e "$chunk_hex" <<<"$out"; then bad "chunk timeout output contains input bytes"; else ok; fi
if grep -q '(1 driver timeouts)' <<<"$out"; then ok; else bad "summary does not count the chunk timeout"; fi
if pgrep -f "$work/go-chunk" >/dev/null; then bad "chunk-hanging driver left running"; else ok; fi
entry 'operation="go-driver-chunk-timeout"' >"$work/known-chunk.jsonl"
if python3 -I "$helper" check-known "$work/known-chunk.jsonl" >/dev/null 2>&1; then bad "known list accepted a chunk-timeout entry"; else ok; fi

# 3. Corpus decode failures report a digest and a fixed kind, never the input.
marker=MARKER-$$-SECRET
printf 'go test fuzz v1\n[]byte("%s\\xZZ")\n' "$marker" >"$work/corpus/bad-escape"
printf 'go test fuzz v1\n[]byte("%s\\777")\n' "$marker" >"$work/corpus/bad-octal"
printf 'go test fuzz v1\n[]byte("%s\xff")\n' "$marker" >"$work/corpus/bad-utf8"
stub "$work/go" W W
: >"$work/known.jsonl"
out=$(python3 -I "$helper" run "$work/known.jsonl" "$work/rust" "$work/go" "$work" "raw:$work/corpus" 2>&1 || true)
if [ "$(grep -c '^DIFF corpus-decode:' <<<"$out")" = 3 ]; then ok; else bad "corpus decode failures not reported"; fi
if grep -q "$marker" <<<"$out"; then bad "corpus decode output contains input bytes"; else ok; fi
if grep -q Traceback <<<"$out"; then bad "corpus decode raised a traceback"; else ok; fi

# 4. Output directory containment, resolved before anything is created.
refuses() { # name path
  if python3 -I "$helper" resolve-out "$root" "$2" >/dev/null 2>&1; then bad "resolve-out accepted $1"; else ok; fi
}
refuses "tree itself" "$root"
refuses "path inside" "$root/out"
refuses "dot path" "$(dirname "$root")/./$(basename "$root")/out"
refuses "dotdot path" "$root/fuzz/../out"
refuses "ancestor" "$(dirname "$root")"
ln -s "$root" "$work/link-to-tree"
refuses "symlink into tree" "$work/link-to-tree/out"
if python3 -I "$helper" resolve-out "$root" "$work/fine" >/dev/null; then ok; else bad "resolve-out refused a path outside"; fi
# Case alias: on a case-insensitive filesystem an uppercased worktree name is
# the same directory; identity, not spelling, decides.
alias="$(dirname "$root")/$(basename "$root" | tr '[:lower:]' '[:upper:]')"
if [ -d "$alias" ]; then refuses "case alias" "$alias/out"; else ok; fi
# A TMPDIR inside the tree is refused before anything is created, by every
# script. Each fixture is a fresh directory this run creates, so only the
# check (not a failing mktemp) can refuse it, and cleanup removes only it.
for script in fuzz-differential.sh fuzz-smoke.sh fuzz-replay.sh; do
  inside=$(mktemp -d "$root/fuzz/tmp-inside.XXXXXX")
  before=$(git status --porcelain --untracked-files=all --ignored)
  args=()
  if [ "$script" = fuzz-differential.sh ]; then args=(--seeds); fi
  if TMPDIR="$inside" YAMLPRIMS_FUZZ_OUT='' bash "scripts/$script" ${args[@]+"${args[@]}"} >/dev/null 2>&1; then
    bad "$script accepted a TMPDIR inside the tree"
  else
    ok
  fi
  if [ "$before" = "$(git status --porcelain --untracked-files=all --ignored)" ]; then ok; else bad "$script changed the tree"; fi
  rmdir "$inside" || bad "$script left files in its refused TMPDIR"
done

# 5. Output roots: created exclusively or reused only when marked, and no
# symlink is followed anywhere below them.
tree_now() { git status --porcelain --untracked-files=all --ignored; }
target_dir=$(mktemp -d "$root/fuzz/link-target.XXXXXX")
before=$(tree_now)
stub "$work/go" X X
run_out() { # out-dir -> exit status of fuzz-differential.sh with stub drivers
  YAMLPRIMS_FUZZ_OUT="$1" YAMLPRIMS_FUZZ_RUST_DRIVER="$work/rust" YAMLPRIMS_FUZZ_GO_DRIVER="$work/go" \
    bash scripts/fuzz-differential.sh "raw:$work/corpus-ok" >/dev/null 2>&1
}
mkdir "$work/corpus-ok"
printf 'a: 1\n' >"$work/corpus-ok/input"

# Child symlink into the tree, under a marked output root.
o=$(python3 -I "$helper" claim-out "$root" "$work/out-child")
ln -s "$target_dir" "$o/logs"
if run_out "$o"; then bad "differential accepted an output root with a child symlink"; else ok; fi
if YAMLPRIMS_FUZZ_OUT="$o" bash scripts/fuzz-smoke.sh >/dev/null 2>&1; then bad "smoke accepted an output root with a child symlink"; else ok; fi
# Dangling child symlink where the report would be written.
o=$(python3 -I "$helper" claim-out "$root" "$work/out-dangling")
ln -s "$target_dir/report.jsonl" "$o/differential-failures.jsonl"
if run_out "$o"; then bad "differential accepted a dangling child symlink"; else ok; fi
if [ -e "$target_dir/report.jsonl" ]; then bad "report written through a dangling symlink"; else ok; fi
# Seeds: an output directory holding a child symlink into the tree.
mkdir "$work/seeds-child"
ln -s "$target_dir" "$work/seeds-child/load"
if python3 -I fuzz/seeds.py "$work/seeds-child" >/dev/null 2>&1; then bad "seeds.py adopted a directory with a child symlink"; else ok; fi
# Non-empty, unmarked output root: refused, contents untouched.
mkdir "$work/out-unmarked"
printf 'keep\n' >"$work/out-unmarked/sentinel"
if run_out "$work/out-unmarked"; then bad "differential adopted a non-empty unmarked directory"; else ok; fi
if YAMLPRIMS_FUZZ_OUT="$work/out-unmarked" bash scripts/fuzz-smoke.sh >/dev/null 2>&1; then bad "smoke adopted a non-empty unmarked directory"; else ok; fi
if [ "$(cat "$work/out-unmarked/sentinel")" = keep ] && [ "$(find "$work/out-unmarked" -mindepth 1 | wc -l | tr -d ' ')" = 1 ]; then ok; else bad "unmarked directory was changed"; fi
# A fresh output root works, and a marked one can be reused.
run_out "$work/out-fresh" || true
reports() { find "$work/out-fresh" -maxdepth 1 -name 'differential-failures*.jsonl' | wc -l | tr -d ' '; }
if [ -f "$work/out-fresh/.yamlprims-fuzz-out" ] && [ "$(reports)" = 1 ]; then ok; else bad "fresh output root not created and used"; fi
run_out "$work/out-fresh" || true
if [ "$(reports)" = 2 ]; then ok; else bad "marked output root not reused without overwriting"; fi
# Hard link to a tracked file under a marked output root: refused, and the
# tracked file is unchanged.
o=$(python3 -I "$helper" claim-out "$root" "$work/out-hardlink")
tracked="$root/fuzz/README.md"
sum_before=$(shasum -a 256 "$tracked")
if ln "$tracked" "$o/settings.txt" 2>/dev/null; then
  ln "$tracked" "$o/differential-failures.jsonl"
  if run_out "$o"; then bad "differential accepted a hard-linked file under the output root"; else ok; fi
  if YAMLPRIMS_FUZZ_OUT="$o" bash scripts/fuzz-smoke.sh >/dev/null 2>&1; then bad "smoke accepted a hard-linked file under the output root"; else ok; fi
  if python3 -I "$helper" check-tree "$o" 2>/dev/null; then bad "check-tree accepted a hard-linked file"; else ok; fi
  if [ "$sum_before" = "$(shasum -a 256 "$tracked")" ]; then ok; else bad "tracked file changed through a hard link"; fi
else
  echo "note: hard-link control skipped (output root on another filesystem)" >&2
fi
# One campaign per output root. The lock is an flock held for the holder's
# lifetime: a second contender is refused while the first is alive, nothing
# unlinks or replaces the lock file, and the kernel frees it when the holder
# exits. The race the PID-file lock lost (read a stale PID, another run
# replaces the lock, unlink it) cannot occur: there is no read-then-unlink.
o=$(python3 -I "$helper" claim-out "$root" "$work/out-lock")
printf '999999\n' >"$o/.yamlprims-fuzz-lock" # a stale PID from a dead run
inode() { python3 -I -c 'import os,sys; print(os.lstat(sys.argv[1]).st_ino)' "$1"; }
lock_inode=$(inode "$o/.yamlprims-fuzz-lock")
python3 -I "$helper" hold-lock "$o" -- sleep 60 &
holder=$!
for _ in $(seq 50); do python3 -I "$helper" lock-held "$o" && break; sleep 0.1; done
if python3 -I "$helper" lock-held "$o"; then ok; else bad "holder did not take the lock over a stale PID"; fi
if python3 -I "$helper" hold-lock "$o" -- true 2>/dev/null; then bad "second contender ran while the lock was held"; else ok; fi
if YAMLPRIMS_FUZZ_OUT="$o" bash scripts/fuzz-smoke.sh >/dev/null 2>&1; then bad "smoke ran while another run held the root"; else ok; fi
if [ ! -e "$o/runs" ]; then ok; else bad "refused smoke created a run directory"; fi
if [ "$(inode "$o/.yamlprims-fuzz-lock")" = "$lock_inode" ]; then ok; else bad "lock file was replaced"; fi
kill "$holder" 2>/dev/null || true
wait "$holder" 2>/dev/null || true
if python3 -I "$helper" lock-held "$o"; then bad "lock not released when its holder died"; else ok; fi
if python3 -I "$helper" hold-lock "$o" -- true; then ok; else bad "free lock refused"; fi
# Simultaneous starts: in every trial exactly one of two contenders runs.
races_ok=1
for trial in 1 2 3 4 5; do
  rm -f "$work"/winner-*
  for c in a b; do
    # shellcheck disable=SC2016 # $1 expands in the inner shell
    python3 -I "$helper" hold-lock "$o" -- sh -c 'touch "$1"; sleep 1' _ "$work/winner-$c" 2>/dev/null &
  done
  wait || true
  n=$(find "$work" -maxdepth 1 -name 'winner-*' | wc -l | tr -d ' ')
  [ "$n" = 1 ] || { races_ok=0; echo "race trial $trial: $n winners" >&2; }
done
if [ "$races_ok" = 1 ]; then ok; else bad "simultaneous contenders did not resolve to exactly one owner"; fi
if [ "$(inode "$o/.yamlprims-fuzz-lock")" = "$lock_inode" ]; then ok; else bad "lock file was replaced during the races"; fi

# The locked descriptor is inherited by the whole campaign: the lock outlives
# a killed wrapper or a killed shell while any worker still holds it, and is
# free again once every holder has exited.
wait_held() { for _ in $(seq 50); do python3 -I "$helper" lock-held "$o" && return 0; sleep 0.1; done; return 1; }
wait_pidfile() { for _ in $(seq 50); do [ -s "$1" ] && return 0; sleep 0.1; done; return 1; }
contender_refused() { if python3 -I "$helper" hold-lock "$o" -- true 2>/dev/null; then return 1; fi; }
# (a) Wrapper killed with SIGKILL while its child is alive.
rm -f "$work/child.pid"
# shellcheck disable=SC2016 # $$ expands in the inner shell
python3 -I "$helper" hold-lock "$o" -- sh -c 'echo $$ >"$1"; exec sleep 60' _ "$work/child.pid" &
wrapper=$!
wait_held && wait_pidfile "$work/child.pid"
kill -KILL "$wrapper"
wait "$wrapper" 2>/dev/null || true
if contender_refused; then ok; else bad "lock lost when the wrapper was SIGKILLed with its child alive"; fi
kill "$(cat "$work/child.pid")" 2>/dev/null || true
for _ in $(seq 50); do python3 -I "$helper" lock-held "$o" || break; sleep 0.1; done
if python3 -I "$helper" hold-lock "$o" -- true; then ok; else bad "lock not free after the last holder exited"; fi
# (b) Shell child killed with SIGTERM while a background worker survives.
rm -f "$work/worker.pid" "$work/shell.pid"
# shellcheck disable=SC2016 # expands in the inner shell
python3 -I "$helper" hold-lock "$o" -- sh -c 'echo $$ >"$1"; sleep 60 & echo $! >"$2"; wait' _ "$work/shell.pid" "$work/worker.pid" &
wrapper=$!
wait_held && wait_pidfile "$work/worker.pid" && wait_pidfile "$work/shell.pid"
kill -TERM "$(cat "$work/shell.pid")"
wait "$wrapper" 2>/dev/null || true
if contender_refused; then ok; else bad "lock lost while a background worker still held it"; fi
kill "$(cat "$work/worker.pid")" 2>/dev/null || true
for _ in $(seq 50); do python3 -I "$helper" lock-held "$o" || break; sleep 0.1; done
if python3 -I "$helper" hold-lock "$o" -- true; then ok; else bad "lock not free after the worker exited"; fi
# The child can confirm it owns the lock through the inherited descriptor.
# shellcheck disable=SC2016 # expands in the inner shell
if python3 -I "$helper" hold-lock "$o" -- sh -c 'python3 -I "$1" lock-owned "$2" "$YAMLPRIMS_FUZZ_LOCK_FD"' _ "$root/$helper" "$o"; then ok; else bad "child could not confirm it owns the lock"; fi
if python3 -I "$helper" lock-owned "$o" 0 2>/dev/null; then bad "an unrelated descriptor passed as the lock"; else ok; fi
# Claiming to be the locked child without the inherited descriptor is refused
# before the run starts.
if YAMLPRIMS_FUZZ_LOCKED_OUT="$o" YAMLPRIMS_FUZZ_OUT="$o" bash scripts/fuzz-smoke.sh >/dev/null 2>&1; then
  bad "smoke ran with a spoofed lock environment"
else
  ok
fi
if [ ! -e "$o/runs" ]; then ok; else bad "spoofed smoke created a run directory"; fi
# Exit status: a child killed by signal N gives 128+N.
for sig in INT TERM HUP; do
  # shellcheck disable=SC2016 # expands in the inner shell
  python3 -I "$helper" hold-lock "$o" -- sh -c 'kill -'"$sig"' $$' 2>/dev/null && status=0 || status=$?
  want=$((128 + $(kill -l "$sig")))
  if [ "$status" = "$want" ]; then ok; else bad "child killed by SIG$sig gave status $status, want $want"; fi
done

# Seeds enter the corpus by exclusive creation: an existing name is skipped,
# never opened. Model a hard link created after the walk by disabling the walk.
o=$(python3 -I "$helper" claim-out "$root" "$work/out-corpus")
mkdir -p "$work/seed-src"
printf 'new\n' >"$work/seed-src/seed-0000"
printf 'new\n' >"$work/seed-src/seed-0001"
mkdir -p "$o/corpus/load"
sum_before=$(shasum -a 256 "$tracked")
if ln "$tracked" "$o/corpus/load/seed-0000" 2>/dev/null; then
  result=$(python3 -I - "$root/$helper" "$o" "$work/seed-src" <<'PY'
import importlib.util, sys
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("fd", sys.argv[1])
fd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fd)
fd.check_tree = lambda out: None  # the link appeared after the walk
fd.import_new(sys.argv[2], "corpus/load", sys.argv[3])
PY
)
  if [ "$result" = "1 added, 1 already present" ]; then ok; else bad "import did not skip the existing name ($result)"; fi
  if [ "$sum_before" = "$(shasum -a 256 "$tracked")" ]; then ok; else bad "corpus import wrote through a hard link"; fi
  rm "$o/corpus/load/seed-0000"
else
  echo "note: corpus hard-link control skipped (output root on another filesystem)" >&2
fi
# With the walk in place the same link is refused outright.
ln "$tracked" "$o/corpus/load/seed-0000" 2>/dev/null &&
  if python3 -I "$helper" import-new "$o" corpus/load "$work/seed-src" >/dev/null 2>&1; then bad "import accepted a hard-linked corpus file"; else ok; fi
rm -f "$o/corpus/load/seed-0000"

if [ -z "$(find "$target_dir" -mindepth 1)" ]; then ok; else bad "files were written into the tree through a link"; fi
rmdir "$target_dir" || bad "link target directory not empty"
if [ "$before" = "$(tree_now)" ]; then ok; else bad "output-root controls changed the tree"; fi

echo "fuzz-differential controls: $pass passed, $failed failed"
[ "$failed" = 0 ]
