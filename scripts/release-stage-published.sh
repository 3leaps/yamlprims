#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# shellcheck disable=SC2154
# Download the published (non-draft) GitHub release for YAMLPRIMS_RELEASE_TAG
# into a clean directory, verify that the tag and release point at the same
# commit, verify that the public keys/anchors/notes match the tagged commit,
# verify both minisign and OpenPGP subkey signatures on SHA256SUMS and
# SHA512SUMS, and verify all asset checksums before moving the directory into
# place.
#
# Usage: scripts/release-stage-published.sh [tag] [out-dir]
set -euo pipefail

die() {
	echo "error: $*" >&2
	exit 1
}

root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$root"
# shellcheck source=scripts/release-common.sh
source "$root/scripts/release-common.sh"
# shellcheck source=scripts/release-decernor.sh
source "$root/scripts/release-decernor.sh"

tag="$(release_tag "${1:-${YAMLPRIMS_RELEASE_TAG:-}}")"
out_dir="${2:-dist/published}"
[[ ! -e "$out_dir" && ! -L "$out_dir" ]] || die "$out_dir already exists; clean explicitly first"

resolve_release_decernor || die 'the checker is required to verify the published release'
export RELEASE_DECERNOR_BIN

if [[ -n "${YAMLPRIMS_GPG_SIGNING_FINGERPRINT:-}" ]]; then
	if [[ -n "${YAMLPRIMS_RELEASE_ANCHOR_FILE:-}" ]]; then
		load_ceremony_anchor
	fi
	YAMLPRIMS_RELEASE_TAG="$tag" "$root/scripts/release-verify-published-tag.sh"
fi
YAMLPRIMS_RELEASE_TAG="$tag" "$root/scripts/verify-pinned-tag.sh" --published

commit="$(git rev-parse "refs/tags/${tag}^{commit}")"
object="$(git rev-parse "refs/tags/${tag}")"

if [[ "$(release_gh repo view "$YAMLPRIMS_REPOSITORY" --json nameWithOwner --jq .nameWithOwner)" != "$YAMLPRIMS_REPOSITORY" ]]; then
	die 'GitHub repository identity mismatch'
fi

scratch="$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-published.XXXXXX")"
trap 'rc=$?; rm -rf "$scratch"; exit "$rc"' EXIT

state="$scratch/state.json"
expected="$scratch/expected.txt"
actual="$scratch/actual.txt"
release_gh release view "$tag" --repo "$YAMLPRIMS_REPOSITORY" \
	--json tagName,targetCommitish,isDraft,assets >"$state"

if [[ "$(jq -r .tagName "$state")" != "$tag" ||
	"$(jq -r .targetCommitish "$state")" != "$commit" ||
	"$(jq -r .isDraft "$state")" != "false" ]]; then
	die 'GitHub release must be published (non-draft) at the verified tagged commit'
fi

YAMLPRIMS_RELEASE_TAG="$tag" release_signed_assets | LC_ALL=C sort >"$expected"
jq -r '.assets[].name' "$state" | LC_ALL=C sort >"$actual"
if ! cmp -s "$expected" "$actual"; then
	diff -u "$expected" "$actual" >&2 || true
	die 'published GitHub release asset inventory mismatch'
fi

stage="$scratch/stage"
mkdir -p "$stage"
release_gh release download "$tag" --repo "$YAMLPRIMS_REPOSITORY" --pattern '*' --dir "$stage"
YAMLPRIMS_RELEASE_TAG="$tag" assert_exact_directory_inventory "$stage" release_signed_assets

compare_blob() {
	local path="$1" file="$2" mode
	mode="$(git ls-tree "$commit" -- "$path" | awk '{print $1}')"
	[[ "$mode" == 100644 || "$mode" == 100755 ]] || die "committed public file missing: $path"
	git cat-file blob "$commit:$path" >"$scratch/blob"
	if [[ ! -s "$scratch/blob" || ! -f "$file" || -L "$file" ]] || ! cmp -s "$scratch/blob" "$file"; then
		die "published release public material differs from verified commit ($path)"
	fi
}
for ext in txt ndjson; do
	compare_blob "keys/expected-fingerprints.$ext" "$stage/expected-fingerprints.$ext"
done
compare_blob docs/security/release-signing-keys.asc "$stage/yamlprims-release-signing-key.asc"
compare_blob docs/security/yamlprims-minisign.pub "$stage/yamlprims-minisign.pub"
compare_blob "docs/releases/$tag.md" "$stage/release-notes-$tag.md"
"$root/scripts/verify-public-keys.sh" "$stage"
"$root/scripts/verify-signatures.sh" "$stage"
YAMLPRIMS_RELEASE_TAG="$tag" "$root/scripts/verify-checksums.sh" "$stage"

printf 'tag=%s\nobject=%s\ncommit=%s\n' "$tag" "$object" "$commit" >"$stage.anchor"
mkdir -p "$(dirname "$out_dir")"
mv "$stage" "$out_dir"
mv "$stage.anchor" "$out_dir.anchor"
echo "[ok] verified published release $tag staged in $out_dir"
