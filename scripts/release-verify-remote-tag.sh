#!/usr/bin/env bash
set -euo pipefail
script_dir="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=scripts/release-tag-common.sh
source "$script_dir/release-tag-common.sh"
cd "$(release_repo_root)"
release_validate_verification_env
release_assert_checkout
release_verify_local_tag
local_sha="$(git rev-parse "refs/tags/$YAMLPRIMS_RELEASE_TAG")"
ref="$(gh api "repos/3leaps/yamlprims/git/ref/tags/$YAMLPRIMS_RELEASE_TAG")"
jq -e --arg sha "$local_sha" '.object.type == "tag" and .object.sha == $sha' <<<"$ref" >/dev/null
object="$(gh api "repos/3leaps/yamlprims/git/tags/$local_sha")"
jq -e --arg tag "$YAMLPRIMS_RELEASE_TAG" --arg commit "$YAMLPRIMS_RELEASE_COMMIT" \
	--arg name "$YAMLPRIMS_TAGGER_NAME" --arg email "$YAMLPRIMS_TAGGER_EMAIL" '
	.tag == $tag and .object.type == "commit" and .object.sha == $commit and
	.tagger.name == $name and .tagger.email == $email and
	.verification.verified == true and .verification.reason == "valid"
' <<<"$object" >/dev/null
echo '[ok] GitHub reports the same verified tag object and release commit'
