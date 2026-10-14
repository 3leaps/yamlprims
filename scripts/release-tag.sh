#!/usr/bin/env bash
set -euo pipefail
script_dir="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=scripts/release-tag-common.sh
source "$script_dir/release-tag-common.sh"
cd "$(release_repo_root)"
release_validate_signing_env
release_setup_gpg_tty
release_assert_checkout
release_assert_tag_absent
make check
export GIT_COMMITTER_NAME="$YAMLPRIMS_TAGGER_NAME"
export GIT_COMMITTER_EMAIL="$YAMLPRIMS_TAGGER_EMAIL"
git -c gpg.format=openpgp -c gpg.program=gpg tag -s -a \
	-u "$YAMLPRIMS_PGP_KEY_ID" -m "yamlprims $YAMLPRIMS_RELEASE_TAG" \
	"$YAMLPRIMS_RELEASE_TAG" "$YAMLPRIMS_RELEASE_COMMIT"
release_verify_local_tag
echo '[ok] signed tag created and verified locally; not pushed'
