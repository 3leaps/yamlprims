#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# The leaf-dependency guard refuses another *prims crate or rsfulmen however
# it is declared, and accepts the workspace's own crates.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd -P)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
git -C "$root" archive HEAD Cargo.toml Cargo.lock crates | tar -x -C "$scratch"
cp "$root/Cargo.toml" "$scratch/Cargo.toml"
cp "$root/crates/yamlprims/Cargo.toml" "$scratch/crates/yamlprims/Cargo.toml"
cp "$root/crates/yamlprims-cli/Cargo.toml" "$scratch/crates/yamlprims-cli/Cargo.toml"
mkdir -p "$scratch/scripts"
cp "$root/scripts/deps-check.sh" "$scratch/scripts/"
manifest="$scratch/crates/yamlprims-cli/Cargo.toml"
cp "$manifest" "$scratch/cli.toml"
check() { (cd "$scratch" && scripts/deps-check.sh) >"$scratch/out" 2>&1; }
check || { cat "$scratch/out" >&2; echo 'error: pristine tree refused' >&2; exit 1; }
refused() {
	cp "$scratch/cli.toml" "$manifest"
	printf '%s\n' "$1" >>"$manifest"
	if check; then
		echo "error: accepted: $1" >&2
		exit 1
	fi
	grep -q 'depends on another prim or on rsfulmen' "$scratch/out" || { echo "error: refused for another reason: $1" >&2; cat "$scratch/out" >&2; exit 1; }
}
refused $'[dev-dependencies.rsfulmen]\nversion = "1" # : yamlprims-'
refused $'[dev-dependencies.other-prims]\nversion = "1"'
refused $'[dev-dependencies.other_prims]\nversion = "1"'
refused $'[dev-dependencies.x]\npackage = "sysprims"\nversion = "1"'
refused $'[build-dependencies.examplprims-core]\nversion = "1"'
refused $'[target.\'cfg(unix)\'.dependencies.someprims]\nversion = "1"'
refused $'[dev_dependencies.other-prims]\nversion = "1"'
refused $'[build_dependencies.rsfulmen]\nversion = "1"'
refused $'[target.\'cfg(unix)\'.dev_dependencies.someprims]\nversion = "1"'
# The resolved graph cannot be read (a lockfile that no longer matches): the
# check fails rather than skipping the graph.
cp "$scratch/cli.toml" "$manifest"
printf '%s\n' '[dev-dependencies.nonexistent_crate_xyz]' 'version = "1"' >>"$manifest"
if check; then
	echo 'error: accepted with an unreadable dependency graph' >&2
	exit 1
fi
grep -q 'cargo metadata failed' "$scratch/out" || { echo 'error: graph failure not reported' >&2; cat "$scratch/out" >&2; exit 1; }
cp "$scratch/cli.toml" "$manifest"
# A readable graph that names a forbidden package (a transitive dependency the
# manifests do not show) is refused.
mkdir -p "$scratch/bin"
real_cargo="$(command -v cargo)"
# shellcheck disable=SC2016 # expands in the stub
printf '#!/bin/bash\nif [[ "$1" == metadata ]]; then echo %q; exit 0; fi\nexec %q "$@"\n' \
	'{"packages":[{"name":"yamlprims"},{"name":"other-prims"}]}' "$real_cargo" >"$scratch/bin/cargo"
chmod +x "$scratch/bin/cargo"
if (cd "$scratch" && PATH="$scratch/bin:$PATH" scripts/deps-check.sh) >"$scratch/out" 2>&1; then
	echo 'error: accepted a forbidden package in the resolved graph' >&2
	exit 1
fi
grep -q 'appears in the resolved dependency graph' "$scratch/out" || { echo 'error: graph refusal not reported' >&2; cat "$scratch/out" >&2; exit 1; }
# shellcheck disable=SC2016 # expands in the stub
printf '#!/bin/bash\nif [[ "$1" == metadata ]]; then echo %q; exit 0; fi\nexec %q "$@"\n' \
	'{"packages":[{"name":"yamlprims"},{"name":"serde"}]}' "$real_cargo" >"$scratch/bin/cargo"
(cd "$scratch" && PATH="$scratch/bin:$PATH" scripts/deps-check.sh) >"$scratch/out" 2>&1 || { echo 'error: refused a clean graph' >&2; cat "$scratch/out" >&2; exit 1; }

# Go module checks: a clean zero-dependency module passes, while require/replace
# directives, go.sum, go.work, or vendor/ are refused.
mkdir -p "$scratch/yaml/internal/engine"
printf 'module github.com/3leaps/yamlprims\n\ngo 1.21\n' >"$scratch/go.mod"
printf 'package engine\n' >"$scratch/yaml/internal/engine/stub.go"
check || { cat "$scratch/out" >&2; echo 'error: clean Go module refused' >&2; exit 1; }

printf 'module github.com/3leaps/yamlprims\n\ngo 1.21\n\nrequire example.com/ext v1.0.0\n' >"$scratch/go.mod"
if check; then
	echo 'error: accepted require directive in go.mod' >&2
	exit 1
fi
grep -q 'go.mod must have no require or replace directives' "$scratch/out" || { echo 'error: go.mod require refusal not reported' >&2; cat "$scratch/out" >&2; exit 1; }
printf 'module github.com/3leaps/yamlprims\n\ngo 1.21\n' >"$scratch/go.mod"

for forbidden_path in go.sum go.work; do
	: >"$scratch/$forbidden_path"
	if check; then
		echo "error: accepted $forbidden_path" >&2
		exit 1
	fi
	grep -q "$forbidden_path must not exist" "$scratch/out" || { echo "error: $forbidden_path refusal not reported" >&2; cat "$scratch/out" >&2; exit 1; }
	rm -f "$scratch/$forbidden_path"
done

mkdir -p "$scratch/vendor"
if check; then
	echo 'error: accepted vendor/' >&2
	exit 1
fi
grep -q 'vendor must not exist' "$scratch/out" || { echo 'error: vendor refusal not reported' >&2; cat "$scratch/out" >&2; exit 1; }
rmdir "$scratch/vendor"

echo '[ok] leaf-dependency guard: comments, dev/build/target tables and aliases, renames, separators, unreadable graph, and transitive graph checks'
