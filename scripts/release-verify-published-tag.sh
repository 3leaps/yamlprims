#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Verify the published release tag independent of the current main: the remote
# annotated tag object must equal the local one, its signature must verify
# against the public pin committed in the tagged commit (using this checkout's
# verifier, never code from the target), and GitHub must report it verified.
# Run before downloading, signing, uploading or promoting release assets. This narrows, but cannot remove, the window in which a mutable tag
# ref could move before a later GitHub API call.
set -euo pipefail
die() {
    echo "error: $*" >&2
    exit 1
}
tag="${YAMLPRIMS_RELEASE_TAG:-}"
# The approved primary comes from the maintainer environment, never from the
# tagged commit: a commit can carry any key with a matching fingerprint file,
# so internal consistency alone does not prove the key is the approved one.
approved="${YAMLPRIMS_GPG_SIGNING_FINGERPRINT:-}"
[[ "$approved" =~ ^[0-9A-F]{40}$ ]] || die 'approved primary YAMLPRIMS_GPG_SIGNING_FINGERPRINT required (40 uppercase hex)'
[[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ || "$tag" =~ ^spec-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || die 'canonical YAMLPRIMS_RELEASE_TAG required'
root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$root"
# shellcheck source=scripts/release-decernor.sh
source "$root/scripts/release-decernor.sh"
resolve_release_decernor || die 'the checker is required to verify the published tags'
export RELEASE_DECERNOR_BIN
# One origin rule for every release step (release-tag-common.sh tag_origin).
# shellcheck source=scripts/release-tag-common.sh
source "$root/scripts/release-tag-common.sh"
# shellcheck source=scripts/release-gh.sh
source "$root/scripts/release-gh.sh"
tag_origin || die 'origin must be 3leaps/yamlprims on GitHub'
remote_object="$(git ls-remote --exit-code origin "refs/tags/$tag" | awk '{print $1}')" || die 'remote release tag is absent'
[[ "$remote_object" =~ ^[0-9a-f]{40}$ ]] || die 'remote tag object is invalid'
# When a verified anchor exists (from staging the packages), the remote object
# must still equal it before any local ref changes: a replacement tag, even one
# signed by the approved key, must not move the release to other packages.
# Once this tag's ceremony anchor is recorded (YAMLPRIMS_RELEASE_ANCHOR_FILE
# names an existing file), it is the binding: values exported in the shell
# must agree with it, so a stale export from another release is refused by
# name instead of silently binding the run. Callers that record or stage an
# anchor (YAMLPRIMS_ANCHOR_OUT) bind explicitly and skip this.
env_object="${YAMLPRIMS_EXPECTED_TAG_OBJECT:-}"
env_commit="${YAMLPRIMS_EXPECTED_COMMIT:-}"
if [[ -z "${YAMLPRIMS_ANCHOR_OUT:-}" && -n "${YAMLPRIMS_RELEASE_ANCHOR_FILE:-}" && -f "$YAMLPRIMS_RELEASE_ANCHOR_FILE" ]]; then
    # shellcheck source=scripts/release-common.sh
    source "$root/scripts/release-common.sh"
    load_ceremony_anchor || die "ceremony anchor $YAMLPRIMS_RELEASE_ANCHOR_FILE is not a valid anchor for $tag"
    [[ -z "$env_object" || "$env_object" == "$YAMLPRIMS_EXPECTED_TAG_OBJECT" ]] ||
        die "exported YAMLPRIMS_EXPECTED_TAG_OBJECT $env_object differs from the recorded anchor $YAMLPRIMS_EXPECTED_TAG_OBJECT ($YAMLPRIMS_RELEASE_ANCHOR_FILE); unset it"
    [[ -z "$env_commit" || "$env_commit" == "$YAMLPRIMS_EXPECTED_COMMIT" ]] ||
        die "exported YAMLPRIMS_EXPECTED_COMMIT $env_commit differs from the recorded anchor $YAMLPRIMS_EXPECTED_COMMIT ($YAMLPRIMS_RELEASE_ANCHOR_FILE); unset it"
fi
expected_object="${YAMLPRIMS_EXPECTED_TAG_OBJECT:-}"
expected_commit="${YAMLPRIMS_EXPECTED_COMMIT:-}"
if [[ -n "$expected_object" || -n "$expected_commit" ]]; then
    [[ "$expected_object" =~ ^[0-9a-f]{40}$ && "$expected_commit" =~ ^[0-9a-f]{40}$ ]] ||
        die 'expected tag object and commit must both be 40-hex'
    [[ "$remote_object" == "$expected_object" ]] ||
        die "remote tag object differs from the verified anchor: remote $remote_object, expected $expected_object (YAMLPRIMS_EXPECTED_TAG_OBJECT; stale from another release?)"
fi
# Fetch into a private namespace and verify there; local refs/tags change only
# after every check passes.
staging="refs/yamlprims-verify/$$.$RANDOM/tags/"
cleanup_staging() { git for-each-ref --format='%(refname)' "${staging%tags/}" | while IFS= read -r ref; do git update-ref -d "$ref"; done; }
trap cleanup_staging EXIT
git fetch --quiet --no-tags origin "+refs/tags/$tag:$staging$tag"
[[ "$(git rev-parse "$staging$tag")" == "$remote_object" ]] || die 'fetched tag differs from remote tag object'
[[ "$(git cat-file -t "$remote_object")" == tag ]] || die 'annotated tag required'
commit="$(git rev-parse "$staging$tag^{}")"
[[ -z "$expected_commit" || "$commit" == "$expected_commit" ]] ||
    die "tagged commit differs from the verified anchor: tagged $commit, expected $expected_commit (YAMLPRIMS_EXPECTED_COMMIT; stale from another release?)"
# The verifier and its helpers run from this trusted operator checkout. The
# tagged commit is read only as inert git objects (tag object, pin, anchors,
# tagger identity); no file from the unverified target is ever executed.
pinned="$(git cat-file blob "$commit:keys/expected-fingerprints.txt" 2>/dev/null | awk '$1=="gpg" {print $2}')" ||
    die 'tagged commit has no fingerprint anchors'
[[ "$pinned" == "$approved" ]] || die 'pin in the tagged commit is not the approved primary'
YAMLPRIMS_RELEASE_TAG="$tag" "$root/scripts/verify-pinned-tag.sh" --published --staged-prefix "$staging" >/dev/null ||
    die 'published tag signature does not verify against the pin committed in its commit'
tag_json="$(release_gh api "repos/3leaps/yamlprims/git/tags/$remote_object")" || die "GitHub tag object for $tag unavailable"
jq -e --arg tag "$tag" --arg commit "$commit" \
    '.tag == $tag and .object.type == "commit" and .object.sha == $commit and .verification.verified == true and .verification.reason == "valid"' \
    <<<"$tag_json" >/dev/null || die "GitHub does not report $tag verified"
# Every check passed: now, and only now, install the verified object as the
# local tag.
git update-ref "refs/tags/$tag" "$remote_object"
echo "[ok] published $tag verified against the committed pin and GitHub"
# Machine-readable anchor for callers that bind later steps to this result.
if [[ -n "${YAMLPRIMS_ANCHOR_OUT:-}" ]]; then
    printf 'tag=%s\nobject=%s\ncommit=%s\n' "$tag" "$remote_object" "$commit" >"$YAMLPRIMS_ANCHOR_OUT"
elif [[ -z "$expected_object" ]]; then
    # First, unbound verification (make release-verify-remote-tag): print the
    # exports for release-record-anchor from the object verified above.
    tag_print_anchor_exports "$tag" "$remote_object"
fi
