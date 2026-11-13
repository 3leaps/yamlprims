#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Verify the local release tag set: messages, tagger and common target from the
# prepared files, then signatures against the pin committed in the tagged commit.
set -euo pipefail
# shellcheck source=scripts/release-tag-common.sh
source "$(dirname "$0")/release-tag-common.sh"
# shellcheck source=scripts/release-decernor.sh
source "$(dirname "$0")/release-decernor.sh"
cd "$tag_root"
pinned_object=
if [[ "${1:-}" == --object ]]; then
	[[ "${2:-}" =~ ^[0-9a-f]{40}$ ]] || tag_die 'usage: release-verify-tag.sh [--object OID]'
	pinned_object="$2"
elif [[ $# -gt 0 ]]; then
	tag_die 'usage: release-verify-tag.sh [--object OID]'
fi
tag_version
tag_selector
resolve_release_decernor
export RELEASE_DECERNOR_BIN
# Every tag must name the reviewed release commit, not whatever HEAD is now.
tag_release_commit_matches "$YAMLPRIMS_RELEASE_COMMIT" "release commit"
target="$YAMLPRIMS_RELEASE_COMMIT"
tag_load_set
[[ -z "$pinned_object" || "${#tag_names[@]}" -eq 1 ]] || tag_die '--object applies to a single-tag release set'
for tag in ${tag_names[@]+"${tag_names[@]}"}; do
	tag_verify_object "$tag" "$target" "$pinned_object"
done
"$tag_root/scripts/verify-pinned-tag.sh" ${pinned_object:+--object "$pinned_object"}
