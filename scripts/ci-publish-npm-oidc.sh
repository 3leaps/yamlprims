#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Re-verify the staged npm tarball against the stage job's verified SHA256SUMS
# and SHA512SUMS, ensure the OIDC-capable Node/npm floor (Node >= 22.14.0,
# npm >= 11.5.1) using a SHA-256/SHA-512-pinned self-contained npm CLI tarball
# when the bundled npm is older than 11.5.1, publish the tarball via npm OIDC
# Trusted Publishing with provenance, and verify that the npm registry serves
# the exact signed bytes.
#
# Usage: scripts/ci-publish-npm-oidc.sh <version> <expected-commit> <stage-dir>
set -euo pipefail

die() {
	echo "error: $*" >&2
	exit 1
}

version="${1:?version required}"
expected_commit="${2:?expected commit required}"
stage_dir="${3:?stage directory required}"

root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$root"

[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || die "invalid version $version"
[[ "$expected_commit" =~ ^[0-9a-f]{40}$ ]] || die 'invalid expected commit'
[[ "$(git rev-parse HEAD)" == "$expected_commit" ]] || die 'HEAD differs from verified stage commit'
[[ "$(tr -d ' \t\r\n' < "$root/VERSION")" == "$version" ]] || die 'VERSION differs from verified stage version'

# Pinned self-contained npm CLI archive used when Node's bundled npm is < 11.5.1
# (for example Node 22.23.4 bundles npm 10.9.9). Extracted directly with
# Python tarfile(filter="data") after SHA-256 and SHA-512 verification;
# `npm install` is never invoked.
NPM_CLI_VERSION="11.5.1"
NPM_CLI_TARBALL="npm-${NPM_CLI_VERSION}.tgz"
NPM_CLI_URL="https://registry.npmjs.org/npm/-/${NPM_CLI_TARBALL}"
NPM_CLI_SHA256="f4c82fbff74154f73bd5ce5a2b749700d55eaddebda97b16076bf7033040de34"
NPM_CLI_SHA512="232e6f5d9e799bcb486920b3e9ba907fdf96e576cf7e8c9446c8162e33a416096a1d37a9e910d9a918f6b1f606791c99bc6bb61ee2569b496ec74af13d0dbd95"

tgz="$stage_dir/3leaps-yamlprims-${version}.tgz"
python3 -I - "$stage_dir" "$version" "$root/config/release/npm-cli-ci-sha256.txt" "$NPM_CLI_SHA256" "$NPM_CLI_TARBALL" <<'PY'
import hashlib
import json
import pathlib
import re
import sys
import tarfile

stage = pathlib.Path(sys.argv[1])
version = sys.argv[2]
npm_pin_file = pathlib.Path(sys.argv[3])
npm_expected_sha256 = sys.argv[4]
npm_expected_tarball = sys.argv[5]
tgz_name = f"3leaps-yamlprims-{version}.tgz"
tgz = stage / tgz_name

if not npm_pin_file.is_file() or npm_pin_file.is_symlink():
    sys.exit(f"error: missing or unsafe {npm_pin_file}")
pin_line = npm_pin_file.read_text(encoding="utf-8").strip()
if pin_line != f"{npm_expected_sha256}  {npm_expected_tarball}":
    sys.exit(f"error: {npm_pin_file.name} does not match pinned {npm_expected_tarball} sha256")

def read_manifest(path: pathlib.Path, width: int) -> dict[str, str]:
    if not path.is_file() or path.is_symlink():
        sys.exit(f"error: missing or unsafe {path}")
    entries = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        digest, sep, name = line.partition("  ")
        if not sep or not re.fullmatch(f"[0-9a-f]{{{width}}}", digest) or name in entries:
            sys.exit(f"error: malformed entry in {path.name}")
        entries[name] = digest
    return entries

sha256s = read_manifest(stage / "SHA256SUMS", 64)
sha512s = read_manifest(stage / "SHA512SUMS", 128)
if not tgz.is_file() or tgz.is_symlink():
    sys.exit(f"error: missing staged tarball {tgz_name}")
raw = tgz.read_bytes()
if hashlib.sha256(raw).hexdigest() != sha256s.get(tgz_name):
    sys.exit(f"error: sha256 mismatch for staged tarball {tgz_name}")
if hashlib.sha512(raw).hexdigest() != sha512s.get(tgz_name):
    sys.exit(f"error: sha512 mismatch for staged tarball {tgz_name}")

with tarfile.open(tgz, "r:gz") as tf:
    member = tf.extractfile("package/package.json")
    if member is None:
        sys.exit(f"error: {tgz_name} missing package/package.json")
    pkg = json.loads(member.read().decode("utf-8"))
repo_url = (pkg.get("repository") or {}).get("url")
if (
    pkg.get("name") != "@3leaps/yamlprims"
    or pkg.get("version") != version
    or repo_url != "git+https://github.com/3leaps/yamlprims.git"
):
    sys.exit(f"error: {tgz_name} package.json mismatch: {pkg}")
print(f"[ok] verified {tgz_name} against SHA256SUMS and SHA512SUMS")
PY

work_dir="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/yamlprims-npm-publish.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

# npm Trusted Publishing requires Node >= 22.14.0 and npm >= 11.5.1.
node_ver="$(node --version)"
npm_ver="$(npm --version)"
if ! python3 -I -c 'import sys; a=tuple(map(int,sys.argv[1].lstrip("v").split("."))); b=tuple(map(int,sys.argv[2].split("."))); sys.exit(0 if a>=b else 1)' "$node_ver" "22.14.0"; then
	die "Node >= 22.14.0 required for npm OIDC Trusted Publishing (found $node_ver)"
fi

npm_cli_js=""
if ! python3 -I -c 'import sys; a=tuple(map(int,sys.argv[1].split("."))); b=tuple(map(int,sys.argv[2].split("."))); sys.exit(0 if a>=b else 1)' "$npm_ver" "$NPM_CLI_VERSION"; then
	npm_tgz="$work_dir/$NPM_CLI_TARBALL"
	if [[ -n "${YAMLPRIMS_NPM_CLI_TGZ_FILE:-}" ]]; then
		if [[ -n "${CI:-}" || -n "${GITHUB_ACTIONS:-}" ]]; then
			die 'YAMLPRIMS_NPM_CLI_TGZ_FILE is forbidden in CI'
		fi
		cp "$YAMLPRIMS_NPM_CLI_TGZ_FILE" "$npm_tgz"
	else
		curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location --retry 3 \
			"$NPM_CLI_URL" --output "$npm_tgz"
	fi
	python3 -I - "$npm_tgz" "$work_dir/npm-cli" "$NPM_CLI_SHA256" "$NPM_CLI_SHA512" "$NPM_CLI_TARBALL" <<'PY'
import hashlib
import pathlib
import sys
import tarfile

npm_tgz = pathlib.Path(sys.argv[1])
dest = pathlib.Path(sys.argv[2])
expected_sha256 = sys.argv[3]
expected_sha512 = sys.argv[4]
tarball_name = sys.argv[5]

if not npm_tgz.is_file() or npm_tgz.is_symlink():
    sys.exit(f"error: missing or unsafe {npm_tgz}")
raw = npm_tgz.read_bytes()
actual_sha256 = hashlib.sha256(raw).hexdigest()
if actual_sha256 != expected_sha256:
    sys.exit(f"error: sha256 mismatch for {tarball_name}: expected {expected_sha256}, got {actual_sha256}")
actual_sha512 = hashlib.sha512(raw).hexdigest()
if actual_sha512 != expected_sha512:
    sys.exit(f"error: sha512 mismatch for {tarball_name}: expected {expected_sha512}, got {actual_sha512}")
dest.mkdir(parents=True, exist_ok=True)
with tarfile.open(npm_tgz, "r:gz") as tf:
    tf.extractall(dest, filter="data")
cli_js = dest / "package" / "bin" / "npm-cli.js"
if not cli_js.is_file() or cli_js.is_symlink():
    sys.exit(f"error: {tarball_name} missing package/bin/npm-cli.js")
PY
	npm_cli_js="$work_dir/npm-cli/package/bin/npm-cli.js"
	npm_ver="$(node "$npm_cli_js" --version)"
	[[ "$npm_ver" == "$NPM_CLI_VERSION" ]] ||
		die "pinned $NPM_CLI_TARBALL reported unexpected version $npm_ver (expected $NPM_CLI_VERSION)"
fi

unset NODE_AUTH_TOKEN NPM_TOKEN
npmrc="$work_dir/npmrc"
printf 'registry=https://registry.npmjs.org/\nalways-auth=false\n' >"$npmrc"
export NPM_CONFIG_USERCONFIG="$npmrc"

if [[ -n "$npm_cli_js" ]]; then
	node "$npm_cli_js" publish "$tgz" --provenance --access public
else
	npm publish "$tgz" --provenance --access public
fi
python3 -I -B "$root/scripts/release-verify-registries.py" --expect-provenance "$stage_dir" "$version" npm
