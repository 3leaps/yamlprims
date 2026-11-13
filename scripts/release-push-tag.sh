#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Push the verified local release tag set in one atomic push, then verify the
# remote tags.
set -euo pipefail
# shellcheck source=scripts/release-tag-common.sh
source "$(dirname "$0")/release-tag-common.sh"
cd "$tag_root"
tag_version
tag_origin
tag_approved_primary
tag_load_set
tags=(${tag_names[@]+"${tag_names[@]}"})
# Capture each tag's annotated object before verification, verify, then
# require the ref to still name that object; push the object by id, never by
# ref name, so a concurrent replacement of the local ref cannot be published.
captured=" "
for tag in ${tags[@]+"${tags[@]}"}; do
	object="$(git rev-parse --verify --quiet "refs/tags/$tag")" || tag_die "local tag $tag is absent"
	[[ "$object" =~ ^[0-9a-f]{40}$ && "$(git cat-file -t "$object")" == tag ]] || tag_die "$tag must be an annotated tag"
	tag_release_commit_matches "$(git rev-parse --verify --quiet "$object^{commit}")" "local tag $tag commit"
	captured+="$tag=$object "
done
# Verify exactly the captured objects; no ref is re-resolved before the push.
for tag in ${tags[@]+"${tags[@]}"}; do
	object="${captured#* "$tag"=}"
	object="${object%% *}"
	"$tag_root/scripts/release-verify-tag.sh" --object "$object"
done
refs=()
for tag in ${tags[@]+"${tags[@]}"}; do
	object="${captured#* "$tag"=}"
	object="${object%% *}"
	[[ "$(git rev-parse --verify --quiet "refs/tags/$tag")" == "$object" ]] ||
		tag_die "local tag $tag moved during verification (verified $object)"
	[[ -z "$(git ls-remote --tags origin "refs/tags/$tag")" ]] || tag_die "remote tag $tag already exists"
	refs+=("$object:refs/tags/$tag")
done
git push --atomic origin ${refs[@]+"${refs[@]}"}
for tag in ${tags[@]+"${tags[@]}"}; do
	object="${captured#* "$tag"=}"
	object="${object%% *}"
	remote_object="$(git ls-remote --tags origin "refs/tags/$tag" | awk -v ref="refs/tags/$tag" '$2==ref {print $1}')"
	[[ "$remote_object" == "$object" ]] || tag_die "remote tag $tag is $remote_object, not the verified object $object"
done
"$tag_root/scripts/release-verify-remote-tag.sh"
