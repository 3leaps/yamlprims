#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# shellcheck disable=SC2059
# Guard the reviewed release workflow's read-only artifact boundary: the
# workflow passes, and every listed mutation of it is refused.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd -P)"
[[ -n "${YAMLPRIMS_YAML_BIN:-}" ]] || {
	echo 'error: the workflow permission test needs YAMLPRIMS_YAML_BIN (absolute path to the yamlprims CLI from a release archive pinned in config/yamlprims-cli.txt)' >&2
	exit 1
}
# Every publishing workflow is pinned by digest (release-workflow-pin.py);
# the semantic checks below are a second layer that explains why an edit
# would be refused. The pin is the boundary.
(cd "$root" && python3 -B -E scripts/release-workflow-pin.py check) >/dev/null
for workflow in release crates-verify registry-publish; do
	PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/release-workflow-permissions.py" mutations "$root/.github/workflows/$workflow.yml"
done
# Pin controls, on a scratch repository holding the real workflows and pin.
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
repo="$scratch/repo"
mkdir -p "$repo/.github/workflows" "$repo/.github/actions/check-docs" "$repo/config/release"
cp "$root"/.github/workflows/*.yml "$repo/.github/workflows/"
cp "$root/.github/actions/check-docs/action.yml" "$repo/.github/actions/check-docs/"
cp "$root/config/release/release-workflow.sha256" "$repo/config/release/"
git -C "$repo" init -q -b main
git -C "$repo" config user.name fixture
git -C "$repo" config user.email fixture@example.invalid
git -C "$repo" config commit.gpgsign false
git -C "$repo" add -A
git -C "$repo" commit -qm base
# Edit in place without leaving a backup file among the workflows.
sed_in_place() {
	local script="$1" file="$2"
	sed "$script" "$file" >"$file.tmp" && mv "$file.tmp" "$file"
}
pin() { (cd "$repo" && PYTHONDONTWRITEBYTECODE=1 python3 -B -E "$root/scripts/release-workflow-pin.py" "$@") >"$scratch/out" 2>&1; }
refused() {
	if pin "${@:2}"; then
		echo "error: workflow pin accepted: $1" >&2
		exit 1
	fi
	grep -q "$1" "$scratch/out" || { cat "$scratch/out" >&2; exit 1; }
}
pin_with() {
	(cd "$repo" && for f in .github/actions/*/*.yml .github/workflows/*.yml; do
		printf '%s  %s\n' "$(shasum -a 256 "$f" | cut -c1-64)" "$f"
	done) >"$repo/config/release/release-workflow.sha256"
}
pin check
pin verify-commit HEAD
# A workflow edited without its pin fails, however small the edit.
sed_in_place 's/timeout-minutes: 15/timeout-minutes: 16/' "$repo/.github/workflows/release.yml"
refused 'differs from its reviewed digest' check
git -C "$repo" checkout -q -- .github
# A local action edited without its pin fails: it changes what a workflow runs.
printf '# changed\n' >>"$repo/.github/actions/check-docs/action.yml"
refused 'action.yml differs from its reviewed digest' check
git -C "$repo" checkout -q -- .github
printf 'name: x\n' >"$repo/.github/actions/check-docs/extra.yml"
refused 'extra.yml is not pinned' check
rm "$repo/.github/actions/check-docs/extra.yml"
# The pin edited without the workflow fails.
git -C "$repo" checkout -q -- .github
sed_in_place '1s/^./0/' "$repo/config/release/release-workflow.sha256"
refused 'differs from its reviewed digest' check
git -C "$repo" checkout -q -- config
# Both edited together, as one reviewed change, passes.
sed_in_place 's/timeout-minutes: 15/timeout-minutes: 16/' "$repo/.github/workflows/release.yml"
pin_with
pin check
git -C "$repo" checkout -q -- .
# Every workflow file is pinned, whatever its triggers (a plain push, push
# with no filter, create, or anything else can run on a tag).
for body in 'on: push' 'on: [push]' 'on:\n  push:\n' 'on:\n  push: {}\n' 'on:\n  push:\n    paths: [x]\n' 'on: create' 'on:\n  workflow_dispatch:\n'; do
	printf "$body\npermissions:\n  contents: read\njobs: {}\n" >"$repo/.github/workflows/extra.yml"
	refused 'extra.yml is not pinned' check
done
rm "$repo/.github/workflows/extra.yml"
# A pinned workflow must be read-only, at the top and in every job.
sed_in_place 's/^  contents: read$/  contents: write/' "$repo/.github/workflows/check.yml"
pin_with
refused 'check.yml must declare top-level permissions: contents: read' check
git -C "$repo" checkout -q -- .
sed_in_place 's/^    timeout-minutes: 15$/    timeout-minutes: 15\n    permissions:\n      id-token: write/' "$repo/.github/workflows/release.yml"
pin_with
refused 'grants more than read' check
git -C "$repo" checkout -q -- .
# registry-publish.yml may grant id-token: write only on publish-pypi and
# publish-npm when bound to their matching environments and workflow_dispatch.
sed_in_place 's/^    environment: publish-pypi$/    environment: other-env/' "$repo/.github/workflows/registry-publish.yml"
pin_with
refused 'grants more than read' check
git -C "$repo" checkout -q -- .
sed_in_place 's/^  workflow_dispatch:$/  push:\n  workflow_dispatch:/' "$repo/.github/workflows/registry-publish.yml"
pin_with
refused 'must trigger on workflow_dispatch only' check
git -C "$repo" checkout -q -- .
pin check
# A module beside the checkers or their reader is never imported (a script's
# own directory is the one path Python adds by itself): neither a yaml.py nor
# a json.py that would shadow the standard library.
mkdir -p "$scratch/shadow/scripts" "$scratch/shadow/config"
cp "$root/scripts/release-workflow-pin.py" "$root/scripts/release-workflow-permissions.py" "$root/scripts/yaml_json.py" "$scratch/shadow/scripts/"
cp "$root/config/yamlprims-cli.txt" "$scratch/shadow/config/"
for module in yaml json; do
	printf 'raise SystemExit("shadow %s imported")\n' "$module" >"$scratch/shadow/scripts/$module.py"
done
(cd "$repo" && python3 -B -E "$scratch/shadow/scripts/release-workflow-pin.py" check) >"$scratch/out" 2>&1 || true
if grep -q 'shadow' "$scratch/out"; then
	echo 'error: a module beside the pin checker was imported' >&2
	exit 1
fi
grep -q 'every workflow matches' "$scratch/out" || { cat "$scratch/out" >&2; exit 1; }
python3 -B -E "$scratch/shadow/scripts/release-workflow-permissions.py" check "$root/.github/workflows/release.yml" >"$scratch/out" 2>&1 || true
if grep -q 'shadow' "$scratch/out"; then
	echo 'error: a module beside the workflow checker was imported' >&2
	exit 1
fi
[[ ! -s "$scratch/out" ]] || { cat "$scratch/out" >&2; exit 1; }
# A duplicate key (here a second `contents` granting write in an OIDC job,
# which a last-wins parser would honour) is refused by the reader.
mkdir -p "$scratch/dup"
awk '{print} /^      id-token: write$/ && !done {print "      contents: write"; done=1}' \
	"$root/.github/workflows/registry-publish.yml" >"$scratch/dup/registry-publish.yml"
if python3 -B -E "$root/scripts/release-workflow-permissions.py" check "$scratch/dup/registry-publish.yml" >"$scratch/out" 2>&1; then
	echo 'error: a duplicate contents key in an OIDC job was accepted' >&2
	exit 1
fi
grep -q 'duplicate_key' "$scratch/out" || { cat "$scratch/out" >&2; exit 1; }
# A forged bytecode cache for the reader is never executed: a .pyc in the
# untracked scripts/__pycache__ whose recorded source size and mtime match
# yaml_json.py, and whose code exits 0, must not let any gate mode pass.
forge="$scratch/forge"
mkdir -p "$forge/scripts" "$forge/config"
cp "$root/scripts/release-workflow-pin.py" "$root/scripts/release-workflow-permissions.py" "$root/scripts/yaml_json.py" "$forge/scripts/"
cp "$root/config/yamlprims-cli.txt" "$forge/config/"
python3 -I -B - "$forge/scripts/yaml_json.py" <<'PY'
import importlib.util, os, py_compile, sys
real = sys.argv[1]
st = os.stat(real)
body = b"import sys\nsys.exit(0)\n"
if len(body) > st.st_size:
    raise SystemExit("forge body larger than the reader")
evil = real + ".forged"
with open(evil, "wb") as handle:
    handle.write(body + b"#" * (st.st_size - len(body) - 1) + b"\n")
os.utime(evil, ns=(st.st_atime_ns, st.st_mtime_ns))
cache = importlib.util.cache_from_source(real)
os.makedirs(os.path.dirname(cache), exist_ok=True)
py_compile.compile(evil, cfile=cache, dfile=real, doraise=True,
                   invalidation_mode=py_compile.PycInvalidationMode.TIMESTAMP)
os.remove(evil)
os.utime(real, ns=(st.st_atime_ns, st.st_mtime_ns))
PY
[[ -n "$(find "$forge/scripts/__pycache__" -name 'yaml_json.*.pyc')" ]] || { echo 'error: forged cache not written' >&2; exit 1; }
forged_modes=(
	"python3 -B -E $forge/scripts/release-workflow-pin.py check"
	"python3 -B -E $forge/scripts/release-workflow-pin.py verify-commit HEAD"
	"python3 -B -E $forge/scripts/release-workflow-permissions.py check $root/.github/workflows/release.yml"
	"python3 -B -E $forge/scripts/release-workflow-permissions.py mutations $root/.github/workflows/release.yml"
)
for mode in ${forged_modes[@]+"${forged_modes[@]}"}; do
	# shellcheck disable=SC2086 # Each mode is a word list built above.
	if (cd "$repo" && env -u YAMLPRIMS_YAML_BIN ${mode}) >"$scratch/out" 2>&1; then
		echo "error: a forged reader bytecode cache passed: $mode" >&2
		exit 1
	fi
done
# Without an authenticated reader binary the checkers fail closed.
for bad in "" relative/yamlprims "$scratch/absent" "$(command -v python3)"; do
	if (cd "$root" && YAMLPRIMS_YAML_BIN="$bad" python3 -B -E scripts/release-workflow-pin.py check) >"$scratch/out" 2>&1; then
		echo "error: pin check passed with YAMLPRIMS_YAML_BIN='$bad'" >&2
		exit 1
	fi
	if grep -q Traceback "$scratch/out" || ! grep -q 'YAMLPRIMS_YAML_BIN' "$scratch/out"; then
		echo "error: YAMLPRIMS_YAML_BIN='$bad' was not refused by the reader" >&2
		cat "$scratch/out" >&2
		exit 1
	fi
done
# The ceremony reads the tagged commit, not the working tree: an uncommitted
# fix does not rescue a bad commit, and an uncommitted edit does not break a
# good one.
sed_in_place 's/timeout-minutes: 15/timeout-minutes: 16/' "$repo/.github/workflows/release.yml"
pin verify-commit HEAD
git -C "$repo" add .github/workflows/release.yml
git -C "$repo" commit -qm unreviewed
git -C "$repo" checkout -q HEAD~1 -- .github/workflows/release.yml
refused 'differs from its reviewed digest' verify-commit HEAD
echo '[ok] read-only workflow boundary and mutation controls'
