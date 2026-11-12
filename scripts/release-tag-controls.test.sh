#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# shellcheck disable=SC2016,SC2251
# Tag inputs: the explicit release tag (vX.Y.Z or spec-vX.Y), the derived tag set,
# the per-tag message files, the subkey selector and the Makefile's refusal of a bare RELEASE_TAG.
set -euo pipefail
# shellcheck source=scripts/release-test-fixture.sh
source "$(dirname "$0")/release-test-fixture.sh"
version=1.2.3
spec_version=1.4
repo="$fixture_dir/repo"
fixture_repo "$repo" "$version" "$spec_version"
printf '%s\n' 0123456789ABCDEF0123456789ABCDEF01234567 >"$repo/config/release/signing-subkey.txt"
fixture_commit "$repo"
lib="$repo/scripts/release-tag-common.sh"
in_repo() { (cd "$repo" && env "$@"); }
call() { in_repo "${@:2}" bash -c "source '$lib'; $1"; }

# Every checklist `make` target exists.
while IFS= read -r target; do
	grep -Eq "^${target}:" "$fixture_root/Makefile" || {
		echo "error: RELEASE_CHECKLIST.md names missing make target $target" >&2
		exit 1
	}
done < <(grep -oE 'make [a-z][a-z0-9-]+' "$fixture_root/RELEASE_CHECKLIST.md" | awk '{print $2}' | sort -u)

# The Makefile refuses a bare RELEASE_TAG from any origin and derives no default.
expect_fail_with 'RELEASE_TAG is not accepted' env RELEASE_TAG=v9.9.9 make -s -C "$fixture_root" -n help
expect_fail_with 'RELEASE_TAG is not accepted' env -u RELEASE_TAG make -s -C "$fixture_root" -n help RELEASE_TAG=v9.9.9
expect_fail_with 'RELEASE_TAG is not accepted' env RELEASE_TAG= make -s -C "$fixture_root" -n help
expect_fail_with 'YAMLPRIMS_RELEASE_TAG=.* is required' env -u RELEASE_TAG -u YAMLPRIMS_RELEASE_TAG make -s -C "$fixture_root" release-tag
if grep -En 'YAMLPRIMS_RELEASE_TAG[[:space:]]*\?=|v\$\(VERSION\)' "$fixture_root/Makefile"; then
	echo 'error: the Makefile derives a release tag' >&2
	exit 1
fi

# The release tag is required, canonical, and equal to VERSION or SPEC_VERSION.
expect_fail_with 'YAMLPRIMS_RELEASE_TAG is required' call tag_version -u YAMLPRIMS_RELEASE_TAG
expect_fail_with 'canonical vX.Y.Z or spec-vX.Y' call tag_version YAMLPRIMS_RELEASE_TAG=1.2.3
expect_fail_with 'does not match VERSION' call tag_version YAMLPRIMS_RELEASE_TAG=v1.2.4
expect_fail_with 'does not match SPEC_VERSION' call tag_version YAMLPRIMS_RELEASE_TAG=spec-v1.5
call tag_version YAMLPRIMS_RELEASE_TAG="v$version"
call tag_version YAMLPRIMS_RELEASE_TAG="spec-v$spec_version"

# The tag set is fixed: the one vX.Y.Z or spec-vX.Y tag, and no file in the checkout
# widens or renames it.
[[ "$(call 'tag_load_set && printf "%s," ${tag_names[@]+"${tag_names[@]}"}' YAMLPRIMS_RELEASE_TAG="v$version")" == "v$version," ]]
[[ "$(call 'tag_load_set && printf "%s," ${tag_names[@]+"${tag_names[@]}"}' YAMLPRIMS_RELEASE_TAG="spec-v$spec_version")" == "spec-v$spec_version," ]]
[[ ! -e "$fixture_root/config/release/release-tags.txt" ]]
printf 'v\ngo/yamlprims/v\n' >"$repo/config/release/release-tags.txt"
[[ "$(call 'tag_load_set && printf "%s," ${tag_names[@]+"${tag_names[@]}"}' YAMLPRIMS_RELEASE_TAG="v$version")" == "v$version," ]]
rm -f "$repo/config/release/release-tags.txt"
expect_fail_with 'does not match VERSION' call tag_load_set YAMLPRIMS_RELEASE_TAG=v9.9.9
expect_fail_with 'does not match SPEC_VERSION' call tag_load_set YAMLPRIMS_RELEASE_TAG=spec-v9.9
git -C "$repo" checkout -q -- .

# Message files: one per tag, outside the repo, under a directory named for the
# tag, complete, with the expected subject.
messages="$fixture_dir/messages/v$version"
fixture_messages "$messages" "$version" "v$version"
msg() { call "tag_message_file '$1'" YAMLPRIMS_RELEASE_TAG="v$version" YAMLPRIMS_TAG_MESSAGE_DIR="${2:-$messages}"; }
[[ "$(msg "v$version")" == "$messages/v$version.txt" ]]
expect_fail_with 'must end in the release tag' msg "v$version" "$fixture_dir/messages"
# A symlinked ancestor is allowed (macOS /tmp and /var are symlinks); the
# directory itself must not be a symlink, and its resolved path must still be
# outside the repository and named for the tag.
mkdir -p "$fixture_dir/real/v$version"
cp "$messages/v$version.txt" "$fixture_dir/real/v$version/"
ln -s "$fixture_dir/real" "$fixture_dir/link"
[[ "$(msg "v$version" "$fixture_dir/link/v$version")" == "$fixture_dir/link/v$version/v$version.txt" ]]
mkdir -p "$fixture_dir/aliases"
ln -s "$fixture_dir/real/v$version" "$fixture_dir/aliases/v$version"
expect_fail_with 'must name an existing directory' msg "v$version" "$fixture_dir/aliases/v$version"
mkdir -p "$repo/inside/v$version"
ln -s "$repo/inside" "$fixture_dir/into-repo"
expect_fail_with 'outside the repository' msg "v$version" "$fixture_dir/into-repo/v$version"
# A raw macOS temporary path (an unresolved /var/folders or /tmp) is accepted.
raw_tmp="$(mktemp -d)/v$version"
mkdir "$raw_tmp"
cp "$messages/v$version.txt" "$raw_tmp/"
msg "v$version" "$raw_tmp" >/dev/null
rm -rf "$(dirname "$raw_tmp")"
mkdir -p "$repo/inside/v$version"
expect_fail_with 'outside the repository' msg "v$version" "$repo/inside/v$version"
cp "$messages/v$version.txt" "$fixture_dir/good.txt"
sed -i.bak '1s/.*/Release 9.9.9/' "$messages/v$version.txt"
expect_fail_with 'subject must be' msg "v$version"
cp "$fixture_dir/good.txt" "$messages/v$version.txt"
sed -i.bak 's/^Role: devlead$/Role: <role>/' "$messages/v$version.txt"
expect_fail_with 'placeholder' msg "v$version"
cp "$fixture_dir/good.txt" "$messages/v$version.txt"
sed -i.bak '/^Committer-of-Record/d' "$messages/v$version.txt"
expect_fail_with 'attribution footer' msg "v$version"
cp "$fixture_dir/good.txt" "$messages/v$version.txt"
printf 'trailing space \n' >>"$messages/v$version.txt"
expect_fail_with 'trailing whitespace' msg "v$version"
cp "$fixture_dir/good.txt" "$messages/v$version.txt"
printf 'Release 1.2.3\n\nSynthetic release text.\n\nGenerated by Model via Tool under supervision of @3leapsdave\n\nCo-Authored-By: Model <noreply@3leaps.net>\nRole: devlead\nCommitter-of-Record: @3leapsdave\n' >"$messages/v$version.txt"
expect_fail_with 'attribution footer' msg "v$version"
cp "$fixture_dir/good.txt" "$messages/v$version.txt"
msg "v$version" >/dev/null

# The preparer writes a skeleton per tag with the 3-line trailer and never clobbers a reviewed file.
prep="$fixture_dir/prep/v$version"
in_repo YAMLPRIMS_RELEASE_TAG="v$version" YAMLPRIMS_TAG_MESSAGE_DIR="$prep" ./scripts/release-prepare-tag-message.py >/dev/null
[[ -f "$prep/v$version.txt" ]]
[[ "$(find "$prep" -mindepth 1 | wc -l | tr -d ' ')" == 1 ]]
grep -q '^Release 1.2.3$' "$prep/v$version.txt"
grep -q '^Role: <role>$' "$prep/v$version.txt"
grep -q '^Co-authored-by: <model> <noreply@3leaps\.net>$' "$prep/v$version.txt"
! grep -q 'Generated by' "$prep/v$version.txt"
cp "$fixture_dir/good.txt" "$prep/v$version.txt"
in_repo YAMLPRIMS_RELEASE_TAG="v$version" YAMLPRIMS_TAG_MESSAGE_DIR="$prep" ./scripts/release-prepare-tag-message.py >/dev/null
cmp "$fixture_dir/good.txt" "$prep/v$version.txt"
expect_fail_with 'must match VERSION' in_repo YAMLPRIMS_RELEASE_TAG=v0.0.1 YAMLPRIMS_TAG_MESSAGE_DIR="$fixture_dir/prep/v0.0.1" ./scripts/release-prepare-tag-message.py

# Also test spec-vX.Y skeleton preparation, and exercise the maintainer wrapper
# (scripts/release-tag-operator.sh) across both vX.Y.Z and spec-vX.Y families.
spec_prep="$fixture_dir/prep/spec-v$spec_version"
in_repo YAMLPRIMS_RELEASE_TAG="spec-v$spec_version" YAMLPRIMS_TAG_MESSAGE_DIR="$spec_prep" ./scripts/release-prepare-tag-message.py >/dev/null
[[ -f "$spec_prep/spec-v$spec_version.txt" ]]
grep -q "^spec-v$spec_version$" "$spec_prep/spec-v$spec_version.txt"
expect_fail_with 'must match SPEC_VERSION' in_repo YAMLPRIMS_RELEASE_TAG=spec-v9.9 YAMLPRIMS_TAG_MESSAGE_DIR="$fixture_dir/prep/spec-v9.9" ./scripts/release-prepare-tag-message.py

op_prep_v="$fixture_dir/op-prep/v$version"
in_repo YAMLPRIMS_RELEASE_TAG="v$version" YAMLPRIMS_TAG_MESSAGE_DIR="$op_prep_v" ./scripts/release-tag-operator.sh prepare-message >/dev/null
[[ -f "$op_prep_v/v$version.txt" ]]
expect_fail_with 'must match VERSION' in_repo YAMLPRIMS_RELEASE_TAG=v0.0.1 YAMLPRIMS_TAG_MESSAGE_DIR="$fixture_dir/op-prep/v0.0.1" ./scripts/release-tag-operator.sh prepare-message

op_prep_spec="$fixture_dir/op-prep/spec-v$spec_version"
in_repo YAMLPRIMS_RELEASE_TAG="spec-v$spec_version" YAMLPRIMS_TAG_MESSAGE_DIR="$op_prep_spec" ./scripts/release-tag-operator.sh prepare-message >/dev/null
[[ -f "$op_prep_spec/spec-v$spec_version.txt" ]]
expect_fail_with 'must match SPEC_VERSION' in_repo YAMLPRIMS_RELEASE_TAG=spec-v9.9 YAMLPRIMS_TAG_MESSAGE_DIR="$fixture_dir/op-prep/spec-v9.9" ./scripts/release-tag-operator.sh prepare-message

# Case-folded alias of the checkout is refused by message_dir, tag_message_dir,
# and release-record-anchor.sh on case-insensitive volumes.
repo_alias="$(dirname "$repo")/$(basename "$repo" | tr '[:lower:]' '[:upper:]')"
if [[ -d "$repo_alias" && "$repo_alias" -ef "$repo" ]]; then
	expect_fail_with 'outside the repository' in_repo YAMLPRIMS_RELEASE_TAG="v$version" YAMLPRIMS_TAG_MESSAGE_DIR="$repo_alias/v$version" ./scripts/release-prepare-tag-message.py
	[[ ! -e "$repo/v$version" ]]
	expect_fail_with 'outside trusted checkout' in_repo YAMLPRIMS_RELEASE_ANCHOR_FILE="$repo_alias/ceremony.anchor" YAMLPRIMS_EXPECTED_TAG_OBJECT=1111111111111111111111111111111111111111 YAMLPRIMS_EXPECTED_COMMIT=2222222222222222222222222222222222222222 ./scripts/release-record-anchor.sh
	[[ ! -e "$repo/ceremony.anchor" ]]
fi

# The subkey selector must be the committed subkey, exact and with "!".
call tag_selector YAMLPRIMS_PGP_KEY_ID='0123456789ABCDEF0123456789ABCDEF01234567!'
expect_fail_with 'followed by !' call tag_selector YAMLPRIMS_PGP_KEY_ID=0123456789ABCDEF0123456789ABCDEF01234567
expect_fail_with 'followed by !' call tag_selector YAMLPRIMS_PGP_KEY_ID='0123456789abcdef0123456789abcdef01234567!'
expect_fail_with 'differs from config/release/signing-subkey.txt' call tag_selector YAMLPRIMS_PGP_KEY_ID='FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF!'

# The approved primary must be set and equal the committed anchor at HEAD.
printf 'gpg %s\nminisign %064d\n' 0123456789ABCDEF0123456789ABCDEF01234567 0 >"$repo/keys/expected-fingerprints.txt"
fixture_commit "$repo" anchors
call tag_approved_primary YAMLPRIMS_GPG_SIGNING_FINGERPRINT=0123456789ABCDEF0123456789ABCDEF01234567
expect_fail_with 'approved primary YAMLPRIMS_GPG_SIGNING_FINGERPRINT required' call tag_approved_primary -u YAMLPRIMS_GPG_SIGNING_FINGERPRINT
expect_fail_with 'approved primary YAMLPRIMS_GPG_SIGNING_FINGERPRINT required' call tag_approved_primary YAMLPRIMS_GPG_SIGNING_FINGERPRINT=0123456789abcdef0123456789abcdef01234567
expect_fail_with 'not the approved primary' call tag_approved_primary YAMLPRIMS_GPG_SIGNING_FINGERPRINT=FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
# The working tree is not consulted: an uncommitted edit does not satisfy it.
printf 'gpg %s\nminisign %064d\n' FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF 0 >"$repo/keys/expected-fingerprints.txt"
expect_fail_with 'not the approved primary' call tag_approved_primary YAMLPRIMS_GPG_SIGNING_FINGERPRINT=FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
git -C "$repo" checkout -q -- keys

# Tagger identity must be the committed one, and free of header injection.
call tag_identity YAMLPRIMS_TAGGER_NAME='3 Leaps Infosec Team' YAMLPRIMS_TAGGER_EMAIL=infosec@3leaps.net
expect_fail_with 'committed tagger identity' call tag_identity YAMLPRIMS_TAGGER_NAME=Someone YAMLPRIMS_TAGGER_EMAIL=infosec@3leaps.net

# Origin must be the GitHub repository.
call tag_origin
git -C "$repo" remote set-url origin https://example.invalid/3leaps/yamlprims.git
expect_fail_with 'origin must be' call tag_origin
git -C "$repo" remote set-url origin https://github.com/3leaps/yamlprims.git

# Signing home must be an external non-symlink directory and never the default ~/.gnupg.
mkdir -p "$fixture_dir/ext-gpg" "$fixture_dir/fake-home/.gnupg" "$repo/in-repo-gpg"
ln -s "$fixture_dir/ext-gpg" "$fixture_dir/gpg-link"
call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/ext-gpg"
expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home -u YAMLPRIMS_GPG_HOMEDIR
expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/gpg-link"
expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/gpg-link/"
expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/gpg-link/."
expect_fail_with 'outside the repository' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$repo/in-repo-gpg"
expect_fail_with 'default GPG home is not approved' call tag_key_home HOME="$fixture_dir/fake-home" YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/fake-home/.gnupg"
expect_fail_with 'default GPG home is not approved' call tag_key_home USERPROFILE="$fixture_dir/fake-home" YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/fake-home/.gnupg"
real_python3="$(command -v python3)"
mkdir -p "$fixture_dir/no-pwd-py-bin"
cat >"$fixture_dir/no-pwd-py-bin/python3" <<SH
#!/usr/bin/env bash
if [[ "\${1:-}" == "-I" ]]; then shift; fi
if [[ "\${1:-}" == "-" ]]; then shift; fi
exec "$real_python3" -I -c 'import sys; sys.modules["pwd"] = None; exec(sys.stdin.read())' "\$@"
SH
chmod +x "$fixture_dir/no-pwd-py-bin/python3"
call tag_key_home PATH="$fixture_dir/no-pwd-py-bin:$PATH" USERPROFILE="$fixture_dir/fake-home" YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/ext-gpg"
expect_fail_with 'default GPG home is not approved' call tag_key_home PATH="$fixture_dir/no-pwd-py-bin:$PATH" HOME="" USERPROFILE="$fixture_dir/fake-home" YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/fake-home/.gnupg"
if [[ "$OSTYPE" != msys* && "$OSTYPE" != cygwin* ]]; then
	real_gpgconf="$(command -v gpgconf)"
	win_drive_gpg="C:\\Users\\alice\\AppData\\Roaming\\gnupg"
	win_unc_gpg="\\\\server\\share\\gnupg"
	mkdir -p "$fixture_dir/$win_drive_gpg" "$fixture_dir/$win_unc_gpg" "$fixture_dir/win-gpgconf-bin"
	chmod 700 "$fixture_dir/$win_drive_gpg" "$fixture_dir/$win_unc_gpg"
	cat >"$fixture_dir/win-gpgconf-bin/gpgconf" <<SH
#!/usr/bin/env bash
if [[ "\${1:-}" == "--list-dirs" && "\${2:-}" == "homedir" ]]; then
	printf '%s\r\n' "\$MOCK_GPGCONF_HOMEDIR"
	exit 0
fi
exec "$real_gpgconf" "\$@"
SH
	chmod +x "$fixture_dir/win-gpgconf-bin/gpgconf"
	expect_fail_with 'default GPG home is not approved' call 'cd "$FIXTURE_DIR" && tag_key_home' FIXTURE_DIR="$fixture_dir" PATH="$fixture_dir/win-gpgconf-bin:$PATH" MOCK_GPGCONF_HOMEDIR="$win_drive_gpg" YAMLPRIMS_GPG_HOMEDIR="$win_drive_gpg"
	expect_fail_with 'default GPG home is not approved' call 'cd "$FIXTURE_DIR" && tag_key_home' FIXTURE_DIR="$fixture_dir" PATH="$fixture_dir/win-gpgconf-bin:$PATH" MOCK_GPGCONF_HOMEDIR="$win_unc_gpg" YAMLPRIMS_GPG_HOMEDIR="$win_unc_gpg"
fi
mkdir -p "$fixture_dir/fake-home-upper/.GNUPG"
expect_fail_with 'default GPG home is not approved' call tag_key_home HOME="$fixture_dir/fake-home-upper" YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/fake-home-upper/.GNUPG"
repo_upper_alias="$(dirname "$repo")/$(basename "$repo" | tr '[:lower:]' '[:upper:]')/in-repo-gpg"
if [[ -d "$repo_upper_alias" ]]; then
	expect_fail_with 'outside the repository' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$repo_upper_alias"
fi
git -C "$repo" worktree add --detach "$fixture_dir/linked-wt" HEAD >/dev/null 2>&1
mkdir -p "$fixture_dir/linked-wt/in-linked-wt-gpg"
expect_fail_with 'outside the repository' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/linked-wt/in-linked-wt-gpg"
git -C "$repo" worktree remove --force "$fixture_dir/linked-wt" >/dev/null 2>&1
git -C "$repo" worktree add --detach "$fixture_dir/linked-wt " HEAD >/dev/null 2>&1
mkdir -p "$fixture_dir/linked-wt /in-linked-wt-gpg"
expect_fail_with 'outside the repository' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/linked-wt /in-linked-wt-gpg"
git -C "$repo" worktree remove --force "$fixture_dir/linked-wt " >/dev/null 2>&1
if [[ "$OSTYPE" != msys* && "$OSTYPE" != cygwin* ]]; then
	nl_wt="$fixture_dir/linked"$'\n'"wt"
	git -C "$repo" worktree add --detach "$nl_wt" HEAD >/dev/null 2>&1
	mkdir -p "$nl_wt/in-linked-wt-gpg"
	expect_fail_with 'outside the repository' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$nl_wt/in-linked-wt-gpg"
	git -C "$repo" worktree remove --force "$nl_wt" >/dev/null 2>&1

	cr_wt="$fixture_dir/linked"$'\r'"wt"
	git -C "$repo" worktree add --detach "$cr_wt" HEAD >/dev/null 2>&1
	mkdir -p "$cr_wt/in-linked-wt-gpg"
	expect_fail_with 'outside the repository' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$cr_wt/in-linked-wt-gpg"
	git -C "$repo" worktree remove --force "$cr_wt" >/dev/null 2>&1

	trailing_nl_wt="$fixture_dir/linked-trailing"$'\n'
	git -C "$repo" worktree add --detach "$trailing_nl_wt" HEAD >/dev/null 2>&1
	chmod 700 "$trailing_nl_wt"
	expect_fail_with 'outside the repository' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$trailing_nl_wt"
	git -C "$repo" worktree remove --force "$trailing_nl_wt" >/dev/null 2>&1

	cr_gitdir="$fixture_dir/sep-gitdir"$'\r'
	cp -R "$repo/.git" "$cr_gitdir"
	mkdir -p "$cr_gitdir/in-gitdir-gpg"
	chmod 700 "$cr_gitdir/in-gitdir-gpg"
	expect_fail_with 'outside the repository' call tag_key_home GIT_DIR="$cr_gitdir" GIT_WORK_TREE="$repo" YAMLPRIMS_GPG_HOMEDIR="$cr_gitdir/in-gitdir-gpg"

	ext_nl_gpg="$fixture_dir/ext-gpg"$'\n'
	mkdir -p "$ext_nl_gpg"
	chmod 700 "$ext_nl_gpg"
	resolved_nl_gpg="$(call 'tag_key_home && printf "%sX" "$GNUPGHOME"' YAMLPRIMS_GPG_HOMEDIR="$ext_nl_gpg")"
	[[ "${resolved_nl_gpg%X}" == "$ext_nl_gpg" ]]

	ln -s "$fixture_dir/ext-gpg" "$fixture_dir/gpg-link\\"
	ln -s "$fixture_dir/ext-gpg" "$fixture_dir/gpg-link\\."
	expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/gpg-link\\/"
	expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/gpg-link\\/."
	expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/gpg-link\\./"
	expect_fail_with 'external YAMLPRIMS_GPG_HOMEDIR is required' call tag_key_home YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/gpg-link\\./."
fi
real_git="$(command -v git)"
mkdir -p "$fixture_dir/bad-git-bin"
cat >"$fixture_dir/bad-git-bin/git" <<SH
#!/usr/bin/env bash
for arg in "\$@"; do
	if [[ "\$arg" == "worktree" ]]; then
		exit 1
	fi
done
exec "$real_git" "\$@"
SH
chmod +x "$fixture_dir/bad-git-bin/git"
expect_fail_with 'failed to enumerate repository worktrees' call tag_key_home PATH="$fixture_dir/bad-git-bin:$PATH" YAMLPRIMS_GPG_HOMEDIR="$fixture_dir/ext-gpg"

# PDR-0008 §2.10: signing command pins gpg.format=openpgp, gpg.program=gpg and
# gpg.openpgp.program=gpg, overriding repo/global config (which is reported, not refused).
git -C "$repo" config gpg.format ssh
git -C "$repo" config gpg.program /nonexistent/wrapper
note_out="$(call 'tag_signing_program && tag_signing_git config --get gpg.format && tag_signing_git config --get gpg.program && tag_signing_git config --get gpg.openpgp.program' 2>&1)"
grep -q "note: git config gpg.format is 'ssh'; release signing uses openpgp" <<<"$note_out"
grep -q "note: git config gpg.program is '/nonexistent/wrapper'; release signing uses gpg" <<<"$note_out"
[[ "$(tail -n 3 <<<"$note_out" | tr '\n' ' ')" == "openpgp gpg gpg " ]]
git -C "$repo" config --unset gpg.format
git -C "$repo" config --unset gpg.program

# Post-push anchor exports name the annotated object and its commit, and refuse
# absent or lightweight tags.
expect_fail_with 'tag v9.9.8 is absent' call 'tag_print_anchor_exports v9.9.8'
git -C "$repo" tag lw-anchor-probe HEAD
expect_fail_with 'lw-anchor-probe must be an annotated tag' call 'tag_print_anchor_exports lw-anchor-probe'
git -C "$repo" -c user.name=probe -c user.email=probe@example.invalid tag -a -m probe annotated-anchor-probe HEAD
exports_out="$(call 'tag_print_anchor_exports annotated-anchor-probe')"
[[ "$(sed -n 2p <<<"$exports_out")" == "export YAMLPRIMS_EXPECTED_TAG_OBJECT=$(git -C "$repo" rev-parse refs/tags/annotated-anchor-probe)" ]]
[[ "$(sed -n 3p <<<"$exports_out")" == "export YAMLPRIMS_EXPECTED_COMMIT=$(git -C "$repo" rev-parse HEAD)" ]]
[[ "$(wc -l <<<"$exports_out" | tr -d ' ')" == 3 ]]
git -C "$repo" tag -d lw-anchor-probe annotated-anchor-probe >/dev/null

echo '[ok] tag input controls passed'
