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
work=$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-fuzz-diff-test.XXXXXX")
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
before=$(git status --porcelain --untracked-files=all --ignored)
if YAMLPRIMS_FUZZ_OUT="$work/link-to-tree/x" bash scripts/fuzz-differential.sh raw:"$work/corpus" >/dev/null 2>&1; then
  bad "fuzz-differential accepted a symlinked output dir"
else
  ok
fi
if [ "$before" = "$(git status --porcelain --untracked-files=all --ignored)" ]; then ok; else bad "refused run changed the tree"; fi

echo "fuzz-differential controls: $pass passed, $failed failed"
[ "$failed" = 0 ]
