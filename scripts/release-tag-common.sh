#!/usr/bin/env bash
# Adapted from decernor's signed-tag controls.
# Copyright (c) 2025-2026 3 Leaps, LLC. SPDX-License-Identifier: MIT
set -euo pipefail

release_die() { echo "error: $*" >&2; return 1; }
release_repo_root() { git rev-parse --show-toplevel; }
release_require_env() {
	[ -n "${!1:-}" ] || release_die "$1 is required"
}

release_assert_tag_version() {
	release_require_env YAMLPRIMS_RELEASE_TAG || return 1
	local version
	version="$(cat SPEC_VERSION)" || return 1
	[[ "$version" =~ ^[0-9]+\.[0-9]+$ ]] || {
		release_die 'SPEC_VERSION must be MAJOR.MINOR'; return 1;
	}
	[ "$YAMLPRIMS_RELEASE_TAG" = "spec-v${version}" ] || {
		release_die 'YAMLPRIMS_RELEASE_TAG must match spec-v<SPEC_VERSION>'; return 1;
	}
}

release_assert_checkout() {
	release_require_env YAMLPRIMS_RELEASE_COMMIT || return 1
	[[ "$YAMLPRIMS_RELEASE_COMMIT" =~ ^[0-9a-f]{40}$ ]] || {
		release_die 'YAMLPRIMS_RELEASE_COMMIT must be a full lowercase commit SHA'; return 1;
	}
	[ "$(git branch --show-current)" = main ] || {
		release_die 'release checkout must be on main'; return 1;
	}
	[ -z "$(git status --porcelain --untracked-files=all)" ] || {
		release_die 'release checkout must be clean'; return 1;
	}
	git fetch --quiet origin 'refs/heads/main:refs/remotes/origin/main' || return 1
	[ "$(git rev-parse HEAD)" = "$YAMLPRIMS_RELEASE_COMMIT" ] &&
		[ "$(git rev-parse refs/remotes/origin/main)" = "$YAMLPRIMS_RELEASE_COMMIT" ] || {
		release_die 'HEAD and fetched origin/main must equal YAMLPRIMS_RELEASE_COMMIT'; return 1;
	}
}

release_assert_remote_absent() {
	local status=0
	git ls-remote --exit-code --tags origin "refs/tags/$YAMLPRIMS_RELEASE_TAG" >/dev/null 2>&1 || status=$?
	case "$status" in
		2) ;;
		0) release_die 'remote tag already exists'; return 1 ;;
		*) release_die 'unable to determine remote tag state'; return 1 ;;
	esac
}

release_assert_tag_absent() {
	if git show-ref --verify --quiet "refs/tags/$YAMLPRIMS_RELEASE_TAG"; then
		release_die 'local tag already exists'; return 1
	fi
	release_assert_remote_absent
}

release_validate_key_state() {
	local capabilities="$1" validity="$2" expires="$3"
	[[ "$capabilities" != *D* && ! "$validity" =~ ^[redi]$ ]] || {
		release_die 'key is revoked, expired, disabled or invalid'; return 1;
	}
	if [ -n "$expires" ] && [ "$expires" != 0 ]; then
		[[ "$expires" =~ ^[0-9]+$ ]] && [ "$expires" -gt "$(date +%s)" ] || {
			release_die 'key expiry is invalid or in the past'; return 1;
		}
	fi
}

release_resolve_signing_subkey() {
	local listing="$1" records count primary capabilities validity expires subkey
	records="$(awk -F: '
		$1 == "pub" || $1 == "sec" { want=1; cap=$12; valid=$2; expiry=$7; next }
		want && $1 == "fpr" { print $10 "|" cap "|" valid "|" expiry; want=0 }
	' <<<"$listing")"
	count="$(awk 'NF {n++} END {print n+0}' <<<"$records")"
	[ "$count" = 1 ] || { release_die 'exactly one primary key required'; return 1; }
	IFS='|' read -r primary capabilities validity expires <<<"$records"
	[ "$primary" = "$YAMLPRIMS_GPG_SIGNING_FINGERPRINT" ] || {
		release_die 'selected subkey does not belong to the authorized primary'; return 1;
	}
	release_validate_key_state "$capabilities" "$validity" "$expires" || return 1
	records="$(awk -F: -v selected="${YAMLPRIMS_PGP_KEY_ID%!}" '
		$1 == "sub" || $1 == "ssb" { want=1; cap=$12; valid=$2; expiry=$7; next }
		want && $1 == "fpr" {
			if ($10 == selected) print $10 "|" cap "|" valid "|" expiry
			want=0
		}
	' <<<"$listing")"
	count="$(awk 'NF {n++} END {print n+0}' <<<"$records")"
	[ "$count" = 1 ] || { release_die 'exactly one matching subkey required'; return 1; }
	IFS='|' read -r subkey capabilities validity expires <<<"$records"
	[[ "$capabilities" =~ [sS] ]] || { release_die 'subkey cannot sign'; return 1; }
	release_validate_key_state "$capabilities" "$validity" "$expires" || return 1
	printf '%s %s\n' "$subkey" "$primary"
}

release_validate_verification_env() {
	release_assert_tag_version || return 1
	release_require_env YAMLPRIMS_GPG_HOMEDIR || return 1
	release_require_env YAMLPRIMS_PGP_KEY_ID || return 1
	release_require_env YAMLPRIMS_GPG_SIGNING_FINGERPRINT || return 1
	[[ "$YAMLPRIMS_PGP_KEY_ID" =~ ^[0-9A-F]{40}!$ ]] || {
		release_die 'exact uppercase signing-subkey fingerprint followed by ! required'; return 1;
	}
	[[ "$YAMLPRIMS_GPG_SIGNING_FINGERPRINT" =~ ^[0-9A-F]{40}$ ]] || {
		release_die 'full uppercase primary fingerprint required'; return 1;
	}
	local anchor home listing
	anchor="$(cat keys/expected-fingerprints.txt)" || return 1
	[ "$anchor" = "gpg $YAMLPRIMS_GPG_SIGNING_FINGERPRINT" ] || {
		release_die 'primary fingerprint differs from committed anchor'; return 1;
	}
	[ "${YAMLPRIMS_TAGGER_NAME:-}" = '3 Leaps Infosec Team' ] &&
		[ "${YAMLPRIMS_TAGGER_EMAIL:-}" = 'infosec@3leaps.net' ] || {
		release_die 'established release tagger identity required'; return 1;
	}
	home="$(CDPATH='' cd -- "$YAMLPRIMS_GPG_HOMEDIR" && pwd -P)" || return 1
	if git -C "$home" rev-parse --git-dir >/dev/null 2>&1; then
		release_die 'GPG home must remain outside Git repositories'; return 1
	fi
	export GNUPGHOME="$home"
	listing="$(gpg --batch --with-colons --fingerprint --with-subkey-fingerprint \
		--list-keys "$YAMLPRIMS_PGP_KEY_ID")" || return 1
	release_resolve_signing_subkey "$listing" >/dev/null || return 1
	if ! awk -F: '$1 == "uid" && $2 !~ /^[redi]$/ {print $10}' <<<"$listing" |
		sed -n 's/.*<\([^<>]*\)>.*/\1/p' | grep -qxF "$YAMLPRIMS_TAGGER_EMAIL"; then
		release_die 'release tagger email must match a valid signing-key UID'; return 1
	fi
}

release_validate_signing_env() {
	release_validate_verification_env || return 1
	local listing
	listing="$(gpg --batch --with-colons --fingerprint --with-subkey-fingerprint \
		--list-secret-keys "$YAMLPRIMS_PGP_KEY_ID")" || return 1
	release_resolve_signing_subkey "$listing" >/dev/null
}

release_setup_gpg_tty() {
	[ -t 0 ] && [ -t 1 ] || {
		release_die 'release-tag requires an interactive terminal for GPG pinentry'; return 1;
	}
	export GPG_TTY
	GPG_TTY="$(tty)"
	if command -v gpg-connect-agent >/dev/null 2>&1; then
		gpg-connect-agent updatestartuptty /bye >/dev/null 2>&1 || true
	fi
}

release_verify_local_tag() {
	local ref="refs/tags/$YAMLPRIMS_RELEASE_TAG" text header message tagger verification
	[ "$(git cat-file -t "$ref")" = tag ] || {
		release_die 'annotated tag object required'; return 1;
	}
	text="$(git cat-file tag "$ref")" || return 1
	header="$(sed '/^$/q' <<<"$text")"
	[ "$(sed -n 's/^object //p' <<<"$header")" = "$YAMLPRIMS_RELEASE_COMMIT" ] &&
		[ "$(sed -n 's/^type //p' <<<"$header")" = commit ] &&
		[ "$(sed -n 's/^tag //p' <<<"$header")" = "$YAMLPRIMS_RELEASE_TAG" ] || {
		release_die 'tag must directly target the selected commit with the expected name'; return 1;
	}
	tagger="$(sed -n 's/^tagger //p' <<<"$header")"
	[[ "$tagger" =~ ^"$YAMLPRIMS_TAGGER_NAME <$YAMLPRIMS_TAGGER_EMAIL> "[0-9]+\ [+-][0-9]{4}$ ]] || {
		release_die 'tagger identity mismatch'; return 1;
	}
	[ "$(grep -c '^-----BEGIN PGP SIGNATURE-----$' <<<"$text")" = 1 ] &&
		[ "$(grep -c '^-----END PGP SIGNATURE-----$' <<<"$text")" = 1 ] || {
		release_die 'exactly one OpenPGP signature required'; return 1;
	}
	message="$(awk 'BEGIN {body=0} !body && NF==0 {body=1; next}
		body && /^-----BEGIN PGP SIGNATURE-----$/ {exit} body && NF {print}' <<<"$text")"
	[ "$message" = "yamlprims $YAMLPRIMS_RELEASE_TAG" ] || {
		release_die 'unexpected signed tag message'; return 1;
	}
	verification="$(git -c gpg.format=openpgp -c gpg.program=gpg verify-tag --raw "$ref" 2>&1)" || {
		release_die 'tag signature verification failed'; return 1;
	}
	if grep -Eq '^\[GNUPG:\] (EXPKEYSIG|EXPSIG|REVKEYSIG|KEYEXPIRED|SIGEXPIRED) ' <<<"$verification"; then
		release_die 'expired or revoked signature/key'; return 1
	fi
	grep -q '^\[GNUPG:\] GOODSIG ' <<<"$verification" || return 1
	local fingerprints
	fingerprints="$(awk '$1 == "[GNUPG:]" && $2 == "VALIDSIG" {print $3, $12}' <<<"$verification")"
	[ "$fingerprints" = "${YAMLPRIMS_PGP_KEY_ID%!} $YAMLPRIMS_GPG_SIGNING_FINGERPRINT" ] || {
		release_die 'signature must use the exact subkey under the pinned primary'; return 1;
	}
}
