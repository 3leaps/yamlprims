#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# The release tag verifies only against the pin committed in the tagged commit,
# and only when it is annotated, signed by the committed signing subkey and
# points at the release commit. Covers both vX.Y.Z and spec-vX.Y tags.
set -euo pipefail
# shellcheck disable=SC2119 # verify is called with and without --published.
# shellcheck source=scripts/release-test-fixture.sh
source "$(dirname "$0")/release-test-fixture.sh"
decernor="$(fixture_decernor)"
version=1.2.3
spec_version=1.4
repo="$fixture_dir/repo"
messages="$fixture_dir/messages/v$version"
spec_messages="$fixture_dir/messages/spec-v$spec_version"
read -r primary subkey < <(fixture_keys "$fixture_dir/gpg")
read -r other_primary other_subkey < <(fixture_keys "$fixture_dir/gpg-other")

fixture_repo "$repo" "$version" "$spec_version"
gpg --homedir "$fixture_dir/gpg" --batch --armor --export-options export-minimal --export "$subkey!" \
	>"$repo/docs/security/release-signing-keys.asc"
fixture_minisign_pub "$repo/docs/security/yamlprims-minisign.pub" 1
printf '%s\n' "$subkey" >"$repo/config/release/signing-subkey.txt"
fixture_commit "$repo" pins
YAMLPRIMS_DECERNOR_BIN="$decernor" "$repo/scripts/release-insert-anchors.sh" >/dev/null
[[ "$(awk '$1=="gpg" {print $2}' "$repo/keys/expected-fingerprints.txt")" == "$primary" ]]
fixture_commit "$repo" anchors

tags=("v$version")
fixture_messages "$messages" "$version" ${tags[@]+"${tags[@]}"}
fixture_messages "$spec_messages" "$version" "spec-v$spec_version"
sign() {
	local home="$1" key="$2" tag="$3" target="${4:-HEAD}" msg_dir="${5:-$messages}"
	GNUPGHOME="$home" GIT_COMMITTER_NAME='3 Leaps Infosec Team' GIT_COMMITTER_EMAIL=infosec@3leaps.net \
		git -C "$repo" -c gpg.format=openpgp -c gpg.program=gpg -c gpg.openpgp.program=gpg tag -s -a --cleanup=verbatim -u "$key!" -F "$msg_dir/${tag//\//__}.txt" "$tag" "$target"
}
for tag in ${tags[@]+"${tags[@]}"}; do sign "$fixture_dir/gpg" "$subkey" "$tag"; done

# shellcheck disable=SC2120 # --published is passed by some calls only.
verify() {
	(cd "$repo" && YAMLPRIMS_RELEASE_TAG="v$version" RELEASE_DECERNOR_BIN="$decernor" ./scripts/verify-pinned-tag.sh "$@")
}
out="$(verify)"
[[ "$out" == *"1 signed tag(s) verified"* ]]

# Dual tag family: spec-vX.Y tag signed by the committed subkey verifies against SPEC_VERSION.
sign "$fixture_dir/gpg" "$subkey" "spec-v$spec_version" HEAD "$spec_messages"
spec_out="$(cd "$repo" && YAMLPRIMS_RELEASE_TAG="spec-v$spec_version" RELEASE_DECERNOR_BIN="$decernor" ./scripts/verify-pinned-tag.sh)"
[[ "$spec_out" == *"1 signed tag(s) verified"* ]]
fixture_messages "$spec_messages" "$version" "spec-v9.9"
sign "$fixture_dir/gpg" "$subkey" "spec-v9.9" HEAD "$spec_messages"
expect_fail_with 'does not match SPEC_VERSION' env -C "$repo" YAMLPRIMS_RELEASE_TAG="spec-v9.9" RELEASE_DECERNOR_BIN="$decernor" ./scripts/verify-pinned-tag.sh
git -C "$repo" tag -d "spec-v$spec_version" "spec-v9.9" >/dev/null

retag() { git -C "$repo" tag -d "$1" >/dev/null; }

# A tag that points at a different commit fails, even though it is validly signed.
release_commit="$(git -C "$repo" rev-parse HEAD)"
git -C "$repo" commit -q --allow-empty -m later
expect_fail_with 'tag must point at HEAD' verify
git -C "$repo" reset -q --hard "$release_commit"
verify >/dev/null

# Only refs/tags is read by default: preserved good tags in a private
# namespace, selected through the environment, cannot stand in for an
# unsigned replacement at refs/tags.
for tag in ${tags[@]+"${tags[@]}"}; do git -C "$repo" update-ref "refs/yamlprims-verify/1.1/tags/$tag" "refs/tags/$tag"; done
retag "v$version"
GIT_COMMITTER_NAME='3 Leaps Infosec Team' GIT_COMMITTER_EMAIL=infosec@3leaps.net \
	git -C "$repo" tag -a --cleanup=verbatim -F "$messages/v$version.txt" "v$version" HEAD
(cd "$repo" && expect_fail_with "signature is invalid under the pin|not signed by the committed signing subkey" \
	env YAMLPRIMS_VERIFY_REF_PREFIX=refs/yamlprims-verify/1.1/tags/ YAMLPRIMS_RELEASE_TAG="v$version" \
	RELEASE_DECERNOR_BIN="$decernor" ./scripts/verify-pinned-tag.sh)
(cd "$repo" && YAMLPRIMS_RELEASE_TAG="v$version" RELEASE_DECERNOR_BIN="$decernor" \
	./scripts/verify-pinned-tag.sh --staged-prefix refs/yamlprims-verify/1.1/tags/ >/dev/null)
(cd "$repo" && expect_fail_with 'invalid --staged-prefix' env YAMLPRIMS_RELEASE_TAG="v$version" \
	RELEASE_DECERNOR_BIN="$decernor" ./scripts/verify-pinned-tag.sh --staged-prefix refs/heads/)
retag "v$version"
sign "$fixture_dir/gpg" "$subkey" "v$version"
for tag in ${tags[@]+"${tags[@]}"}; do git -C "$repo" update-ref -d "refs/yamlprims-verify/1.1/tags/$tag"; done
verify >/dev/null

# A publishing workflow in the tagged commit that differs from its pin fails,
# even though every tag is validly signed.
git -C "$repo" tag -d ${tags[@]+"${tags[@]}"} >/dev/null
printf '# unreviewed\n' >>"$repo/.github/workflows/release.yml"
fixture_commit "$repo" unreviewed-workflow
for tag in ${tags[@]+"${tags[@]}"}; do sign "$fixture_dir/gpg" "$subkey" "$tag"; done
expect_fail_with 'publishing workflow in the tagged commit differs' verify
git -C "$repo" tag -d ${tags[@]+"${tags[@]}"} >/dev/null
git -C "$repo" reset -q --hard HEAD~1
for tag in ${tags[@]+"${tags[@]}"}; do sign "$fixture_dir/gpg" "$subkey" "$tag"; done
verify >/dev/null

# A lightweight tag fails.
retag "v$version"
git -C "$repo" tag "v$version"
expect_fail_with 'must be an annotated tag' verify
retag "v$version"
sign "$fixture_dir/gpg" "$subkey" "v$version"
verify >/dev/null
good_object="$(git -C "$repo" rev-parse "refs/tags/v$version")"
verify --object "$good_object" >/dev/null

# A tag signed by a key that is not the committed subkey fails.
retag "v$version"
sign "$fixture_dir/gpg-other" "$other_subkey" "v$version"
expect_fail_with 'signature is invalid under the pin|not signed by the committed signing subkey' verify
# --object verifies exactly the named object, whatever the ref now holds: the
# captured good object still verifies while the ref names the bad one, and
# the bad object fails when named.
bad_object="$(git -C "$repo" rev-parse "refs/tags/v$version")"
[[ "$bad_object" != "$good_object" ]]
verify --object "$good_object" >/dev/null
expect_fail_with 'signature is invalid under the pin|not signed by the committed signing subkey' verify --object "$bad_object"
expect_fail_with 'invalid --object' verify --object "$(git -C "$repo" rev-parse HEAD)"
expect_fail_with 'invalid --object' verify --object nothex
expect_fail_with 'usage: verify-pinned-tag.sh' verify --object "$good_object" --staged-prefix refs/yamlprims-verify/1.1/tags/
retag "v$version"
sign "$fixture_dir/gpg" "$subkey" "v$version"
verify >/dev/null

# A leftover path-prefixed module tag from a two-tag layout is not part of
# the release: it is neither required nor counted.
fixture_messages "$messages" "$version" "go/yamlprims/v$version"
sign "$fixture_dir/gpg" "$subkey" "go/yamlprims/v$version"
out="$(verify)"
[[ "$out" == *"1 signed tag(s) verified"* ]]
retag "go/yamlprims/v$version"

# A wrong tagger fails.
retag "v$version"
GNUPGHOME="$fixture_dir/gpg" GIT_COMMITTER_NAME='Someone Else' GIT_COMMITTER_EMAIL=else@example.invalid \
	git -C "$repo" -c gpg.format=openpgp -c gpg.program=gpg -c gpg.openpgp.program=gpg tag -s -a --cleanup=verbatim -u "$subkey!" -F "$messages/v$version.txt" "v$version" HEAD
expect_fail_with 'tagger differs' verify
retag "v$version"
sign "$fixture_dir/gpg" "$subkey" "v$version"
verify >/dev/null

# A missing release tag fails.
retag "v$version"
expect_fail_with "v$version is absent" verify
sign "$fixture_dir/gpg" "$subkey" "v$version"
verify >/dev/null

# Working-tree edits to the pins are never consulted: replacing them with the
# other key leaves verification bound to the tagged commit.
gpg --homedir "$fixture_dir/gpg-other" --batch --armor --export-options export-minimal --export "$other_subkey!" \
	>"$repo/docs/security/release-signing-keys.asc"
printf '%s\n' "$other_subkey" >"$repo/config/release/signing-subkey.txt"
verify >/dev/null
git -C "$repo" checkout -q -- docs/security config/release

# A subkey file that names a different subkey fails at the committed source.
git -C "$repo" tag -d ${tags[@]+"${tags[@]}"} >/dev/null
printf '%s\n' "$other_subkey" >"$repo/config/release/signing-subkey.txt"
fixture_commit "$repo" wrong-subkey
for tag in ${tags[@]+"${tags[@]}"}; do sign "$fixture_dir/gpg" "$subkey" "$tag"; done
expect_fail_with 'not signed by the committed signing subkey' verify

[[ "$other_primary" != "$primary" ]]
echo '[ok] verify-pinned-tag controls passed'
