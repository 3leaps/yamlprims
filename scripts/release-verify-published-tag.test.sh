#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Synthetic controls for the published-tag gate; all keys and remotes are temporary.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd -P)"
scratch="$(mktemp -d)"
cleanup() {
    local home
    for home in "$scratch/gpg" "$scratch/attacker"; do
        [[ -d "$home" ]] && gpgconf --homedir "$home" --kill all >/dev/null 2>&1 || true
    done
    rm -rf "$scratch"
}
trap cleanup EXIT
decernor="${YAMLPRIMS_DECERNOR_BIN:-$(command -v decernor)}"
decernor="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$decernor")"
export GNUPGHOME="$scratch/gpg"
mkdir -m 700 "$GNUPGHOME"
gpg --batch --pinentry-mode loopback --passphrase '' --quick-generate-key \
    '3 Leaps Infosec Team <infosec@3leaps.net>' ed25519 cert 0 >/dev/null 2>&1
primary="$(gpg --batch --with-colons --fingerprint --list-keys | awk -F: '$1=="fpr" {print $10;exit}')"
gpg --batch --pinentry-mode loopback --passphrase '' --quick-add-key "$primary" ed25519 sign 1d >/dev/null 2>&1
subkey="$(gpg --batch --with-colons --with-subkey-fingerprint --list-keys "$primary" | awk -F: '$1=="sub" {s=1;next} s && $1=="fpr" {print $10;exit}')"

git init -q --bare -b main "$scratch/remote.git"
src="$scratch/source"
git init -q -b main "$src"
git -C "$src" config user.name '3 Leaps Infosec Team'
git -C "$src" config user.email infosec@3leaps.net
mkdir -p "$src/scripts" "$src/docs/security" "$src/keys" "$src/config/release"
verifier=(verify-pinned-tag.sh validate-release-anchors.sh release-verify-published-tag.sh release-tag-common.sh release-gh.sh release-workflow-pin.py
    yaml_json.py release-common.sh release-decernor.sh release-insert-anchors.sh install-release-anchors.sh)
for script in ${verifier[@]+"${verifier[@]}"}; do cp "$root/scripts/$script" "$src/scripts/"; done
cp "$root/config/yamlprims-cli.txt" "$src/config/"
cp "$root/config/release/tagger-identity.txt" "$src/config/release/"
printf '1.2.3\n' >"$src/VERSION"
printf 'module github.com/3leaps/yamlprims\n' >"$src/go.mod"
mkdir -p "$src/.github/workflows"
printf 'on:\n  push:\n    tags: [v*]\npermissions:\n  contents: read\njobs: {}\n' >"$src/.github/workflows/release.yml"
printf '%s  .github/workflows/release.yml\n' "$(shasum -a 256 "$src/.github/workflows/release.yml" | cut -c1-64)" \
    >"$src/config/release/release-workflow.sha256"
printf '%s\n' "$subkey" >"$src/config/release/signing-subkey.txt"
gpg --batch --armor --export-options export-minimal --export "$subkey!" >"$src/docs/security/release-signing-keys.asc"
python3 - "$src/docs/security/yamlprims-minisign.pub" <<'PYEOF'
import base64, pathlib, sys
pathlib.Path(sys.argv[1]).write_text('untrusted comment: synthetic public key\n' + base64.b64encode(b'Ed' + b'\x01' * 40).decode() + '\n')
PYEOF
git -C "$src" add -A
git -C "$src" commit -qm pins
YAMLPRIMS_DECERNOR_BIN="$decernor" "$src/scripts/release-insert-anchors.sh" >/dev/null
git -C "$src" add -A
git -C "$src" commit -qm anchors
commit="$(git -C "$src" rev-parse HEAD)"
# The release tag, signed by the committed subkey.
sign_tag() {
    local target="${2:-HEAD}"
    printf '%s\n' "$1" >"$scratch/msg"
    GIT_COMMITTER_NAME='3 Leaps Infosec Team' GIT_COMMITTER_EMAIL=infosec@3leaps.net \
        git -C "$src" -c gpg.format=openpgp -c gpg.program=gpg -c gpg.openpgp.program=gpg tag -fs -a --cleanup=verbatim -u "$subkey!" -F "$scratch/msg" v1.2.3 "$target" >/dev/null
}
sign_tag 'Fixture release'
git -C "$src" remote add origin "$scratch/remote.git"
git -C "$src" push -q origin main refs/tags/v1.2.3

# The operator checkout is on a later main: the gate must not depend on it.
op="$scratch/operator"
git clone -q "$scratch/remote.git" "$op"
git -C "$op" config user.name 'Fixture Operator'
git -C "$op" config user.email operator@example.invalid
printf 'later\n' >"$op/later"
git -C "$op" add later
git -C "$op" commit -qm 'later main'
git -C "$op" remote set-url origin https://github.com/3leaps/yamlprims.git
git -C "$op" config url."$scratch/remote.git".insteadOf https://github.com/3leaps/yamlprims.git

# Stub gh: report the GitHub verification for the tag object named in the call.
mkdir -p "$scratch/bin"
cat >"$scratch/bin/gh" <<'SH'
#!/usr/bin/env bash
object="${2##*/}"
git -C "$FIXTURE_REMOTE" cat-file -e "$object" 2> /dev/null || exit 1
name="$(git -C "$FIXTURE_REMOTE" cat-file tag "$object" | sed -n 's/^tag //p' | head -1)"
verified="${FIXTURE_GH_VERIFIED:-true}"
[[ "$name" != "${FIXTURE_GH_UNVERIFIED_TAG:-}" ]] || verified=false
commit="$(git -C "$FIXTURE_REMOTE" rev-parse "$object^{}")"
printf '{"tag":"%s","object":{"type":"commit","sha":"%s"},"verification":{"verified":%s,"reason":"valid"}}\n' "$name" "$commit" "$verified"
SH
chmod +x "$scratch/bin/gh"
export PATH="$scratch/bin:$PATH" FIXTURE_REMOTE="$scratch/remote.git" YAMLPRIMS_RELEASE_TAG=v1.2.3
export YAMLPRIMS_DECERNOR_BIN="$decernor"
export YAMLPRIMS_GPG_SIGNING_FINGERPRINT="$primary"

# The operator's trusted checkout carries the current verifier and helpers.
for script in ${verifier[@]+"${verifier[@]}"}; do cp "$root/scripts/$script" "$op/scripts/"; done
gate() { (cd "$op" && ./scripts/release-verify-published-tag.sh); }
expect_fail() {
    if gate >"$scratch/out" 2>&1; then
        echo "error: expected rejection: $1" >&2
        exit 1
    fi
    grep -q "$1" "$scratch/out" || {
        echo "error: missing named rejection: $1" >&2
        cat "$scratch/out" >&2
        exit 1
    }
}
gate >/dev/null
# The temporary verification worktree is always removed.
[[ "$(git -C "$op" worktree list --porcelain | grep -c '^worktree ')" == 1 ]] || {
    echo 'error: verification worktree left behind' >&2
    exit 1
}

FIXTURE_GH_VERIFIED=false expect_fail 'GitHub does not report v1.2.3 verified'
# A leftover path-prefixed module tag from the two-tag layout is not consulted:
# present, unsigned and at another commit, it changes nothing, and the gate
# installs no local ref for it.
git -C "$src" commit -q --allow-empty -m 'stray module tag elsewhere'
git -C "$src" tag -fa -m 'stray' go/canon/v1.2.3 HEAD
git -C "$src" push -q origin main refs/tags/go/canon/v1.2.3
gate >/dev/null
[[ -z "$(git -C "$op" for-each-ref refs/tags/go/)" ]] || {
    echo 'error: the gate installed a module tag ref' >&2
    exit 1
}
git -C "$scratch/remote.git" update-ref -d refs/tags/go/canon/v1.2.3
git -C "$src" tag -d go/canon/v1.2.3 >/dev/null
git -C "$src" reset -q --hard "$commit"
git -C "$src" push -q --force origin main
gate >/dev/null

# Same commit, different (unsigned) annotated object: must fail, and the
# verified local tags must survive the attempt untouched.
signed_local="$(git -C "$op" rev-parse refs/tags/v1.2.3)"
GIT_COMMITTER_NAME='3 Leaps Infosec Team' GIT_COMMITTER_EMAIL=infosec@3leaps.net \
    git -C "$src" tag -fa -m 'Replacement' v1.2.3 "$commit"
git -C "$src" push -q --force origin refs/tags/v1.2.3
expect_fail 'does not verify against the pin'
[[ "$(git -C "$op" rev-parse refs/tags/v1.2.3)" == "$signed_local" ]] || {
    echo 'error: a failed verification replaced the local tag' >&2
    exit 1
}
if [[ -n "$(git -C "$op" for-each-ref refs/yamlprims-verify/)" ]]; then
    echo 'error: verification staging refs left behind' >&2
    exit 1
fi

# Lightweight replacement: must fail.
git -C "$scratch/remote.git" update-ref refs/tags/v1.2.3 "$commit"
expect_fail 'annotated tag required'

# Restore a valid signed tag: passes again.
sign_tag 'Fixture release'
git -C "$src" push -q --force origin refs/tags/v1.2.3
gate >/dev/null

# The approved primary must come from the maintainer environment.
(
    unset YAMLPRIMS_GPG_SIGNING_FINGERPRINT
    expect_fail 'approved primary'
)
YAMLPRIMS_GPG_SIGNING_FINGERPRINT="$(printf '%040d' 0)" expect_fail 'not the approved primary'

# An attacker commit carrying its own key and a matching fingerprint file is
# internally consistent but not approved, so it is rejected.
export GNUPGHOME="$scratch/attacker"
mkdir -m 700 "$GNUPGHOME"
gpg --batch --pinentry-mode loopback --passphrase '' --quick-generate-key \
    '3 Leaps Infosec Team <infosec@3leaps.net>' ed25519 cert 0 >/dev/null 2>&1
attacker="$(gpg --batch --with-colons --fingerprint --list-keys | awk -F: '$1=="fpr" {print $10;exit}')"
gpg --batch --pinentry-mode loopback --passphrase '' --quick-add-key "$attacker" ed25519 sign 1d >/dev/null 2>&1
attacker_sub="$(gpg --batch --with-colons --with-subkey-fingerprint --list-keys "$attacker" | awk -F: '$1=="sub" {s=1;next} s && $1=="fpr" {print $10;exit}')"
gpg --batch --armor --export "$attacker" >"$src/docs/security/release-signing-keys.asc"
printf 'gpg %s\nminisign %064d\n' "$attacker" 0 >"$src/keys/expected-fingerprints.txt"
printf '{"fingerprint_scheme":"openpgp-fingerprint-v1","key_role":"primary","fingerprint":"%s"}\n{"fingerprint_scheme":"minisign-public-blob-sha256-v1","fingerprint":"%064d"}\n' "$attacker" 0 >"$src/keys/expected-fingerprints.ndjson"
git -C "$src" add -A
git -C "$src" commit -qm 'attacker pin'
printf 'Fixture release\n' >"$scratch/msg"
GIT_COMMITTER_NAME='3 Leaps Infosec Team' GIT_COMMITTER_EMAIL=infosec@3leaps.net \
    git -C "$src" -c gpg.format=openpgp -c gpg.program=gpg -c gpg.openpgp.program=gpg tag -fs -a --cleanup=verbatim -u "$attacker_sub!" -F "$scratch/msg" v1.2.3 >/dev/null
git -C "$src" push -q origin main
git -C "$src" push -q --force origin refs/tags/v1.2.3
expect_fail 'not the approved primary'
gpgconf --homedir "$GNUPGHOME" --kill all >/dev/null 2>&1 || true
export GNUPGHOME="$scratch/gpg"
git -C "$src" reset -q --hard "$commit"
sign_tag 'Fixture release'
git -C "$src" push -q --force origin main refs/tags/v1.2.3
gate >/dev/null

# An unverified target carrying a marker-writing fake verifier and helper must
# never be executed: the gate runs only the operator's trusted scripts.
marker="$scratch/target-code-executed"
for script in verify-pinned-tag.sh validate-release-anchors.sh release-verify-published-tag.sh; do
    printf '#!/usr/bin/env bash\ntouch %q\nexit 0\n' "$marker" >"$src/scripts/$script"
    chmod +x "$src/scripts/$script"
done
git -C "$src" add -A
git -C "$src" commit -qm 'unverified target with fake verifier'
GIT_COMMITTER_NAME='3 Leaps Infosec Team' GIT_COMMITTER_EMAIL=infosec@3leaps.net \
    git -C "$src" tag -fa -m 'Unsigned replacement' v1.2.3
git -C "$src" push -q origin main
git -C "$src" push -q --force origin refs/tags/v1.2.3
expect_fail 'does not verify against the pin'
[[ ! -e "$marker" ]] || {
    echo 'error: code from the unverified tag target was executed' >&2
    exit 1
}
# Even a correctly signed target cannot run its own scripts in the gate.
sign_tag 'Fixture release'
git -C "$src" push -q --force origin refs/tags/v1.2.3
gate >/dev/null
[[ ! -e "$marker" ]] || {
    echo 'error: code from the tag target was executed' >&2
    exit 1
}

# Anchor binding with REAL signed replacements from the approved key. Packages
# are staged against the anchor below; a later replacement must not pass.
anchor_file="$scratch/anchor"
(cd "$op" && YAMLPRIMS_ANCHOR_OUT="$anchor_file" ./scripts/release-verify-published-tag.sh) >/dev/null
anchor_object="$(awk -F= '$1=="object" {print $2}' "$anchor_file")"
# The anchor carries exactly tag, object and commit.
[[ "$(cut -d= -f1 "$anchor_file" | tr '\n' ' ')" == "tag object commit " ]]
anchor_commit="$(awk -F= '$1=="commit" {print $2}' "$anchor_file")"
[[ "$anchor_commit" == "$(git -C "$src" rev-parse HEAD)" ]]
anchored() {
    (cd "$op" && YAMLPRIMS_EXPECTED_TAG_OBJECT="$anchor_object" YAMLPRIMS_EXPECTED_COMMIT="$anchor_commit" \
        ./scripts/release-verify-published-tag.sh)
}
anchored >/dev/null
# The first, unbound verification (make release-verify-remote-tag) prints the
# exports for release-record-anchor from the verified object; a bound run or
# an anchor-file run prints none.
unbound="$(cd "$op" && ./scripts/release-verify-published-tag.sh)"
[[ "$(grep -c '^export ' <<<"$unbound")" == 2 ]]
grep -qx "export YAMLPRIMS_EXPECTED_TAG_OBJECT=$anchor_object" <<<"$unbound"
grep -qx "export YAMLPRIMS_EXPECTED_COMMIT=$anchor_commit" <<<"$unbound"
for bound in "$(anchored)" "$(cd "$op" && YAMLPRIMS_ANCHOR_OUT="$scratch/anchor2" ./scripts/release-verify-published-tag.sh)"; do
    if grep -q '^export ' <<<"$bound"; then
        echo 'error: a bound verification printed anchor exports' >&2
        exit 1
    fi
done
# Once the anchor is recorded, it binds the run: no exports are needed, none
# are printed, and a stale export from another release is refused by name.
printf 'tag=v1.2.3\nobject=%s\ncommit=%s\n' "$anchor_object" "$anchor_commit" >"$scratch/ceremony.anchor"
recorded="$(cd "$op" && env -u YAMLPRIMS_EXPECTED_TAG_OBJECT -u YAMLPRIMS_EXPECTED_COMMIT YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/ceremony.anchor" ./scripts/release-verify-published-tag.sh)"
if grep -q '^export ' <<<"$recorded"; then
    echo 'error: an anchor-bound verification printed exports' >&2
    exit 1
fi
if (cd "$op" && YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/ceremony.anchor" YAMLPRIMS_EXPECTED_TAG_OBJECT=26077e1769ea950639655302263aba5717987897 \
    ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1; then
    echo 'error: a stale exported tag object passed against the recorded anchor' >&2
    exit 1
fi
grep -q "exported YAMLPRIMS_EXPECTED_TAG_OBJECT 26077e1769ea950639655302263aba5717987897 differs from the recorded anchor $anchor_object" "$scratch/out" || {
    cat "$scratch/out" >&2
    exit 1
}
if (cd "$op" && YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/ceremony.anchor" YAMLPRIMS_EXPECTED_COMMIT=1b365fcc563d3ab9057010c0a28178a7b88b6eb5 \
    ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1; then
    echo 'error: a stale exported commit passed against the recorded anchor' >&2
    exit 1
fi
grep -q 'exported YAMLPRIMS_EXPECTED_COMMIT 1b365fcc563d3ab9057010c0a28178a7b88b6eb5 differs from the recorded anchor' "$scratch/out"
# An anchor for another tag is not silently ignored.
printf 'tag=v9.9.9\nobject=%s\ncommit=%s\n' "$anchor_object" "$anchor_commit" >"$scratch/other.anchor"
if (cd "$op" && YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/other.anchor" ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1; then
    echo 'error: an anchor for another tag was accepted' >&2
    exit 1
fi
grep -q 'is not a valid anchor for v1.2.3' "$scratch/out"
# A configured anchor that exists but is not a regular file never falls back
# to unbound verification.
mkdir "$scratch/dir.anchor"
ln -s "$scratch/ceremony.anchor" "$scratch/link.anchor"
ln -s "$scratch/nowhere" "$scratch/broken.anchor"
for bad_anchor in "$scratch/dir.anchor" "$scratch/link.anchor" "$scratch/broken.anchor"; do
    if (cd "$op" && YAMLPRIMS_RELEASE_ANCHOR_FILE="$bad_anchor" ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1; then
        echo "error: unusable anchor $bad_anchor fell back to unbound verification" >&2
        exit 1
    fi
    grep -q 'exists but is not a regular file' "$scratch/out" || { cat "$scratch/out" >&2; exit 1; }
done
# Output mode (YAMLPRIMS_ANCHOR_OUT) binds to an existing anchor too.
if (cd "$op" && YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/ceremony.anchor" YAMLPRIMS_EXPECTED_TAG_OBJECT=26077e1769ea950639655302263aba5717987897 \
    YAMLPRIMS_ANCHOR_OUT="$scratch/anchor3" ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1; then
    echo 'error: output mode ignored the recorded anchor' >&2
    exit 1
fi
grep -q 'differs from the recorded anchor' "$scratch/out"
# Unbound initial verification says so.
(cd "$op" && env -u YAMLPRIMS_EXPECTED_TAG_OBJECT -u YAMLPRIMS_EXPECTED_COMMIT YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/not-yet.anchor" \
    ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1
grep -q "note: no recorded ceremony anchor at $scratch/not-yet.anchor; verifying unbound" "$scratch/out"
# Same version, approved key, DIFFERENT target commit: fails before any ref change.
printf 'other build\n' >>"$src/file"
git -C "$src" add file
git -C "$src" commit -qm 'different target, same version'
sign_tag 'Fixture release'
git -C "$src" push -q origin main
git -C "$src" push -q --force origin refs/tags/v1.2.3
before="$(git -C "$op" rev-parse refs/tags/v1.2.3)"
if anchored >"$scratch/out" 2>&1; then
    echo 'error: approved-key replacement on another commit passed the anchor' >&2
    exit 1
fi
grep -q 'differs from the verified anchor' "$scratch/out"
[[ "$(git -C "$op" rev-parse refs/tags/v1.2.3)" == "$before" ]] || {
    echo 'error: local tag ref changed before the anchor check' >&2
    exit 1
}
gate >/dev/null
# Same commit, approved key, DIFFERENT tag object (new message): fails.
git -C "$src" reset -q --hard "$anchor_commit"
sign_tag 'Fixture release, re-signed'
git -C "$src" push -q --force origin main refs/tags/v1.2.3
[[ "$(git -C "$scratch/remote.git" rev-parse 'refs/tags/v1.2.3^{}')" == "$anchor_commit" ]]
[[ "$(git -C "$scratch/remote.git" rev-parse refs/tags/v1.2.3)" != "$anchor_object" ]]
if anchored >"$scratch/out" 2>&1; then
    echo 'error: same-commit replacement object passed the anchor' >&2
    exit 1
fi
grep -q 'differs from the verified anchor' "$scratch/out"
# The refusal names both objects, so a stale anchor is obvious.
replacement="$(git -C "$scratch/remote.git" rev-parse refs/tags/v1.2.3)"
grep -q "remote $replacement, expected $anchor_object" "$scratch/out" || {
    echo 'error: anchor refusal does not name the remote and expected objects' >&2
    cat "$scratch/out" >&2
    exit 1
}
# A partial anchor is refused.
(cd "$op" && YAMLPRIMS_EXPECTED_TAG_OBJECT="$anchor_object" ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1 &&
    {
        echo 'error: partial anchor accepted' >&2
        exit 1
    }
grep -q 'must both be 40-hex' "$scratch/out"
(cd "$op" && YAMLPRIMS_EXPECTED_COMMIT="$anchor_commit" ./scripts/release-verify-published-tag.sh) >"$scratch/out" 2>&1 && exit 1
grep -q 'must both be 40-hex' "$scratch/out"
# Restore the original signed object: the anchored check passes again.
git -C "$scratch/remote.git" update-ref refs/tags/v1.2.3 "$anchor_object"
anchored >/dev/null

# Absent remote tag: must fail.
git -C "$scratch/remote.git" update-ref -d refs/tags/v1.2.3
expect_fail 'remote release tag is absent'
echo '[ok] published-tag gate controls passed'
