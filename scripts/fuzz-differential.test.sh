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
if [ -z "$(find "$target_dir" -mindepth 1)" ]; then ok; else bad "files were written into the tree through a link"; fi
rmdir "$target_dir" || bad "link target directory not empty"
if [ "$before" = "$(tree_now)" ]; then ok; else bad "output-root controls changed the tree"; fi

echo "fuzz-differential controls: $pass passed, $failed failed"
[ "$failed" = 0 ]
