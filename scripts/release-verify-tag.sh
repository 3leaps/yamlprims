#!/usr/bin/env bash
set -euo pipefail
script_dir="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=scripts/release-tag-common.sh
source "$script_dir/release-tag-common.sh"
cd "$(release_repo_root)"
release_validate_verification_env
release_assert_checkout
release_verify_local_tag
echo '[ok] local signed release tag verified'
