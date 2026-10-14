#!/usr/bin/env bash
set -euo pipefail
script_dir="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=scripts/release-tag-common.sh
source "$script_dir/release-tag-common.sh"
cd "$(release_repo_root)"
release_validate_verification_env
release_assert_checkout
release_verify_local_tag
release_assert_remote_absent
git -c push.followTags=false push origin "refs/tags/$YAMLPRIMS_RELEASE_TAG:refs/tags/$YAMLPRIMS_RELEASE_TAG"
echo '[ok] pushed only the verified release tag'
