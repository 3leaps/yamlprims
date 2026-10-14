#!/usr/bin/env bash
# Copyright (c) 2025-2026 3 Leaps, LLC. SPDX-License-Identifier: MIT
# Adapted from decernor's synthetic signed-tag controls.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
# shellcheck source=scripts/release-tag-common.sh
source "$ROOT/scripts/release-tag-common.sh"
for tool in git gpg jq python3 make; do
	command -v "$tool" >/dev/null || { echo "error: tests require $tool" >&2; exit 1; }
done
# Short socket paths are required by gpg-agent on macOS.
TEST_ROOT="$(mktemp -d /tmp/ypt-XXXXXX)"
cleanup() {
	gpgconf --homedir "$TEST_ROOT/gpg" --kill gpg-agent >/dev/null 2>&1 || true
	rm -rf "$TEST_ROOT"
}
trap cleanup EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export YAMLPRIMS_GPG_HOMEDIR="$TEST_ROOT/gpg"
export YAMLPRIMS_RELEASE_TAG=spec-v1.0
export YAMLPRIMS_TAGGER_NAME='3 Leaps Infosec Team'
export YAMLPRIMS_TAGGER_EMAIL='infosec@3leaps.net'
mkdir -m 700 "$YAMLPRIMS_GPG_HOMEDIR"
export GNUPGHOME="$YAMLPRIMS_GPG_HOMEDIR"
gpg --batch --pinentry-mode loopback --passphrase '' --quick-gen-key \
	'3 Leaps Infosec Team <infosec@3leaps.net>' ed25519 sign never >/dev/null 2>&1
primary="$(gpg --batch --with-colons --list-keys | awk -F: '$1=="fpr" {print $10; exit}')"
gpg --batch --pinentry-mode loopback --passphrase '' --quick-add-key "$primary" ed25519 sign never >/dev/null 2>&1
subkey="$(gpg --batch --with-colons --list-keys | awk -F: '$1=="fpr" {n++; if(n==2) {print $10; exit}}')"
export YAMLPRIMS_PGP_KEY_ID="$subkey!"
export YAMLPRIMS_GPG_SIGNING_FINGERPRINT="$primary"
git init -q --bare "$TEST_ROOT/remote.git"
git init -q -b main "$TEST_ROOT/repo"
cd "$TEST_ROOT/repo"
git config user.name 'Synthetic Release Test'
git config user.email 'synthetic@example.invalid'
git remote add origin "$TEST_ROOT/remote.git"
mkdir keys
printf 'gpg %s\n' "$primary" >keys/expected-fingerprints.txt
printf '1.0\n' >SPEC_VERSION
printf 'check:\n\t@echo "synthetic fixture check"\n' >Makefile
git add .
git commit -q -m 'Synthetic release fixture'
git push -q -u origin main
export YAMLPRIMS_RELEASE_COMMIT
YAMLPRIMS_RELEASE_COMMIT="$(git rev-parse HEAD)"
expect_fail() {
	local label="$1"
	shift
	if ("$@") >/dev/null 2>&1; then
		echo "error: $label unexpectedly passed" >&2; exit 1
	fi
	printf 'PASS: rejects %s\n' "$label"
}
make_tag() {
	GIT_COMMITTER_NAME="$YAMLPRIMS_TAGGER_NAME" GIT_COMMITTER_EMAIL="$YAMLPRIMS_TAGGER_EMAIL" \
		git -c gpg.format=openpgp -c gpg.program=gpg tag -s -a \
		-u "${1:-$YAMLPRIMS_PGP_KEY_ID}" -m "${2:-yamlprims spec-v1.0}" \
		spec-v1.0 "${3:-$YAMLPRIMS_RELEASE_COMMIT}"
}
drop_tag() { git tag -d spec-v1.0 >/dev/null; }

release_validate_signing_env
release_assert_checkout
release_assert_tag_absent
YAMLPRIMS_RELEASE_TAG=v1.0 expect_fail 'code tag for spec release' release_assert_tag_version
YAMLPRIMS_RELEASE_TAG=spec-v1.1 expect_fail 'tag/version mismatch' release_assert_tag_version
YAMLPRIMS_RELEASE_COMMIT='' expect_fail 'missing commit' release_assert_checkout
YAMLPRIMS_RELEASE_COMMIT=short expect_fail 'short commit' release_assert_checkout
YAMLPRIMS_RELEASE_COMMIT="$(printf '%040d' 0)" expect_fail 'wrong commit' release_assert_checkout
YAMLPRIMS_PGP_KEY_ID="$primary!" expect_fail 'primary as subkey selector' release_validate_verification_env
YAMLPRIMS_PGP_KEY_ID="$subkey" expect_fail 'selector without exact bang' release_validate_verification_env
YAMLPRIMS_GPG_SIGNING_FINGERPRINT="$(printf '%040d' 0)" expect_fail 'unanchored primary' release_validate_verification_env
YAMLPRIMS_TAGGER_NAME='Wrong Tagger' expect_fail 'wrong tagger name' release_validate_verification_env
YAMLPRIMS_TAGGER_EMAIL='wrong@example.invalid' expect_fail 'wrong tagger email' release_validate_verification_env
YAMLPRIMS_GPG_HOMEDIR="$PWD/keys" expect_fail 'GPG home in repository' release_validate_verification_env
expect_fail 'expired key' release_validate_key_state s e 0
expect_fail 'revoked key' release_validate_key_state s r 0
expect_fail 'disabled key' release_validate_key_state sD u 0
expect_fail 'past expiry' release_validate_key_state s u 1
expect_fail 'non-signing subkey' release_resolve_signing_subkey "$(printf 'pub:u:256:22:ABC:0:0:::::c:\nfpr:::::::::%s:\nsub:u:256:22:DEF:0:0:::::e:\nfpr:::::::::%s:\n' "$primary" "$subkey")"
expect_fail 'noninteractive signing' bash "$ROOT/scripts/release-tag.sh"
if git show-ref --verify --quiet refs/tags/spec-v1.0; then
	echo 'error: failed signing left a tag' >&2; exit 1
fi
python3 - "$ROOT/scripts/release-tag.sh" <<'PY'
import os
import pty
import sys

status = pty.spawn(["bash", sys.argv[1]])
if os.waitstatus_to_exitcode(status) != 0:
    raise SystemExit("synthetic signed-tag creation failed")
PY
release_verify_local_tag
bash "$ROOT/scripts/release-verify-tag.sh"
expect_fail 'existing local tag' release_assert_tag_absent
if git --git-dir="$TEST_ROOT/remote.git" show-ref --verify --quiet refs/tags/spec-v1.0; then
	echo 'error: local signing pushed a tag' >&2; exit 1
fi
drop_tag
git tag spec-v1.0
expect_fail 'lightweight tag' release_verify_local_tag
drop_tag
git tag -a -m 'yamlprims spec-v1.0' spec-v1.0
expect_fail 'unsigned annotated tag' release_verify_local_tag
drop_tag
make_tag "$YAMLPRIMS_PGP_KEY_ID" 'unexpected message'
expect_fail 'wrong signed message' release_verify_local_tag
drop_tag
YAMLPRIMS_TAGGER_NAME='Wrong Tagger' make_tag
expect_fail 'wrong signed tagger' release_verify_local_tag
drop_tag
alternate="$(git commit-tree 'HEAD^{tree}' -p HEAD -m 'Alternate synthetic target')"
make_tag "$YAMLPRIMS_PGP_KEY_ID" 'yamlprims spec-v1.0' "$alternate"
expect_fail 'wrong signed target' release_verify_local_tag
drop_tag
make_tag "$primary!"
expect_fail 'signature by primary instead of subkey' release_verify_local_tag
drop_tag
gpg --batch --pinentry-mode loopback --passphrase '' --quick-add-key "$primary" ed25519 sign never >/dev/null 2>&1
other="$(gpg --batch --with-colons --list-keys | awk -F: '$1=="fpr" {n++; if(n==3) {print $10; exit}}')"
make_tag "$other!"
expect_fail 'signature by other subkey' release_verify_local_tag
drop_tag
make_tag
# Even with followTags enabled, the helper must push only the release tag.
git tag -a -m unrelated unrelated
git config push.followTags true
bash "$ROOT/scripts/release-push-tag.sh"
git --git-dir="$TEST_ROOT/remote.git" show-ref --verify --quiet refs/tags/spec-v1.0
if git --git-dir="$TEST_ROOT/remote.git" show-ref --verify --quiet refs/tags/unrelated; then
	echo 'error: push leaked unrelated tag' >&2; exit 1
fi
[ "$(git --git-dir="$TEST_ROOT/remote.git" rev-parse main)" = "$YAMLPRIMS_RELEASE_COMMIT" ]
expect_fail 'existing remote tag' bash "$ROOT/scripts/release-push-tag.sh"

# Only GitHub HTTP responses are simulated; tag signatures above are real.
mkdir "$TEST_ROOT/bin"
cat >"$TEST_ROOT/bin/gh" <<'SH'
#!/usr/bin/env bash
case "$*" in
  *git/ref/tags/*) cat "$GH_TAG_REF_FILE" ;;
  *git/tags/*) cat "$GH_TAG_OBJECT_FILE" ;;
  *) exit 1 ;;
esac
SH
chmod +x "$TEST_ROOT/bin/gh"
export GH_TAG_REF_FILE="$TEST_ROOT/ref.json" GH_TAG_OBJECT_FILE="$TEST_ROOT/object.json"
jq -n --arg sha "$(git rev-parse refs/tags/spec-v1.0)" '{object:{type:"tag",sha:$sha}}' >"$GH_TAG_REF_FILE"
jq -n --arg tag "$YAMLPRIMS_RELEASE_TAG" --arg sha "$YAMLPRIMS_RELEASE_COMMIT" \
	--arg name "$YAMLPRIMS_TAGGER_NAME" --arg email "$YAMLPRIMS_TAGGER_EMAIL" \
	'{tag:$tag,object:{type:"commit",sha:$sha},tagger:{name:$name,email:$email},verification:{verified:true,reason:"valid"}}' >"$GH_TAG_OBJECT_FILE"
PATH="$TEST_ROOT/bin:$PATH" bash "$ROOT/scripts/release-verify-remote-tag.sh"
jq '.verification.verified=false | .verification.reason="unknown_key"' "$GH_TAG_OBJECT_FILE" >"$TEST_ROOT/bad.json"
cp "$TEST_ROOT/bad.json" "$GH_TAG_OBJECT_FILE"
PATH="$TEST_ROOT/bin:$PATH" expect_fail 'remote unknown signing key' bash "$ROOT/scripts/release-verify-remote-tag.sh"
jq -n '{object:{type:"tag",sha:"0000000000000000000000000000000000000000"}}' >"$GH_TAG_REF_FILE"
PATH="$TEST_ROOT/bin:$PATH" expect_fail 'remote object mismatch' bash "$ROOT/scripts/release-verify-remote-tag.sh"
printf 'dirty\n' >dirty.txt
expect_fail 'dirty checkout' release_assert_checkout
rm dirty.txt
git switch -q -c other
expect_fail 'non-main checkout' release_assert_checkout
git switch -q main
git commit -q --allow-empty -m 'Synthetic drift'
expect_fail 'HEAD drift' release_assert_checkout
git reset -q --hard "$YAMLPRIMS_RELEASE_COMMIT"
drop_tag
expect_fail 'existing remote without local tag' release_assert_tag_absent
echo 'signed-tag controls passed'
