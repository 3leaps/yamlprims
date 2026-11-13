#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Per-release environment hygiene for the signing shell.
#
#   release-env.sh reset   print `unset ...` for every per-release variable;
#                          run as: eval "$(make -s release-env-reset)"
#                          (a make target cannot change the calling shell)
#   release-env.sh check   refuse any per-release value that does not belong
#                          to YAMLPRIMS_RELEASE_TAG, naming the variable, the
#                          value and what it should be
#
# Long-lived configuration (keys, tagger, GPG home, checker and reader
# binaries, token file) is not per-release and is neither reset nor checked.
set -euo pipefail

per_release=(
	YAMLPRIMS_RELEASE_TAG
	YAMLPRIMS_RELEASE_COMMIT
	YAMLPRIMS_EXPECTED_TAG_OBJECT
	YAMLPRIMS_EXPECTED_COMMIT
	YAMLPRIMS_RELEASE_ANCHOR_FILE
	YAMLPRIMS_RELEASE_RUN_ID
	YAMLPRIMS_RELEASE_RUN_ATTEMPT
	YAMLPRIMS_CRATE_RUN_ID
)

mode="${1:-}"
case "$mode" in
reset)
	printf 'unset'
	printf ' %s' ${per_release[@]+"${per_release[@]}"}
	printf '\n'
	exit 0
	;;
check) ;;
*)
	echo 'usage: release-env.sh check|reset' >&2
	exit 2
	;;
esac

root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$root"
# shellcheck source=scripts/release-gh.sh
source "$root/scripts/release-gh.sh"

problems=0
problem() {
	echo "error: $*" >&2
	problems=$((problems + 1))
}
ok() { echo "[ok] $*"; }

tag="${YAMLPRIMS_RELEASE_TAG:-}"
if [[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	want="v$(tr -d '\r\n' <VERSION)"
	if [[ "$tag" == "$want" ]]; then
		ok "YAMLPRIMS_RELEASE_TAG=$tag matches VERSION"
	else
		problem "YAMLPRIMS_RELEASE_TAG=$tag does not match VERSION ($want)"
	fi
elif [[ "$tag" =~ ^spec-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	want="spec-v$(tr -d '\r\n' <SPEC_VERSION)"
	if [[ "$tag" == "$want" ]]; then
		ok "YAMLPRIMS_RELEASE_TAG=$tag matches SPEC_VERSION"
	else
		problem "YAMLPRIMS_RELEASE_TAG=$tag does not match SPEC_VERSION ($want)"
	fi
else
	problem "YAMLPRIMS_RELEASE_TAG='$tag' is not a canonical vX.Y.Z or spec-vX.Y tag"
	echo "release-env: $problems problem(s); start from: eval \"\$(make -s release-env-reset)\"" >&2
	exit 1
fi

# The commit this release tags: the local tag's once it exists, else HEAD.
tag_object="$(git rev-parse --verify --quiet "refs/tags/$tag" || true)"
if [[ -n "$tag_object" ]]; then
	tag_commit="$(git rev-parse --verify --quiet "refs/tags/$tag^{commit}")"
	release_commit="$tag_commit"
	where="local tag $tag"
else
	tag_commit=""
	release_commit="$(git rev-parse HEAD)"
	where="HEAD (no local tag $tag yet)"
fi

if [[ -n "${YAMLPRIMS_RELEASE_COMMIT+x}" ]]; then
	if [[ ! "$YAMLPRIMS_RELEASE_COMMIT" =~ ^[0-9a-f]{40}$ ]]; then
		problem "YAMLPRIMS_RELEASE_COMMIT='$YAMLPRIMS_RELEASE_COMMIT' is not a full lowercase 40-hex commit"
	elif [[ "$YAMLPRIMS_RELEASE_COMMIT" != "$release_commit" ]]; then
		problem "YAMLPRIMS_RELEASE_COMMIT=$YAMLPRIMS_RELEASE_COMMIT differs from $where $release_commit"
	else
		ok "YAMLPRIMS_RELEASE_COMMIT matches $where"
	fi
fi

anchor_object="" anchor_commit=""
if [[ -n "${YAMLPRIMS_RELEASE_ANCHOR_FILE+x}" ]]; then
	anchor="$YAMLPRIMS_RELEASE_ANCHOR_FILE"
	if [[ "$anchor" != /* ]]; then
		problem "YAMLPRIMS_RELEASE_ANCHOR_FILE='$anchor' is not an absolute path"
	elif [[ ! -e "$anchor" ]]; then
		ok "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor is not recorded yet"
	else
		anchor_tag="$(awk -F= '$1=="tag" {print $2}' "$anchor")"
		anchor_object="$(awk -F= '$1=="object" {print $2}' "$anchor")"
		anchor_commit="$(awk -F= '$1=="commit" {print $2}' "$anchor")"
		if [[ "$anchor_tag" != "$tag" ]]; then
			problem "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor records tag '$anchor_tag', not $tag"
			anchor_object="" anchor_commit=""
		elif [[ -n "$tag_object" && ("$anchor_object" != "$tag_object" || "$anchor_commit" != "$tag_commit") ]]; then
			problem "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor records $anchor_object -> $anchor_commit, local tag $tag is $tag_object -> $tag_commit"
		else
			ok "YAMLPRIMS_RELEASE_ANCHOR_FILE records $tag"
		fi
	fi
fi

check_expected() {
	local name="$1" value="$2" local_value="$3" anchor_value="$4"
	[[ -n "$value" ]] || return 0
	if [[ -n "$anchor_value" && "$value" != "$anchor_value" ]]; then
		problem "$name=$value differs from the recorded anchor ($anchor_value); unset it"
	elif [[ -n "$anchor_value" ]]; then
		ok "$name matches the recorded anchor (no longer needed: unset it)"
	elif [[ -z "$tag_object" ]]; then
		problem "$name=$value is set but $tag is not tagged yet; it belongs to another release"
	elif [[ "$value" != "$local_value" ]]; then
		problem "$name=$value differs from local tag $tag ($local_value)"
	else
		ok "$name matches local tag $tag"
	fi
}
check_expected YAMLPRIMS_EXPECTED_TAG_OBJECT "${YAMLPRIMS_EXPECTED_TAG_OBJECT:-}" "$tag_object" "$anchor_object"
check_expected YAMLPRIMS_EXPECTED_COMMIT "${YAMLPRIMS_EXPECTED_COMMIT:-}" "$tag_commit" "$anchor_commit"

# A selected workflow run must be a run for this tag.
check_run() {
	local name="$1" run="$2" branch
	[[ -n "$run" ]] || return 0
	if [[ ! "$run" =~ ^[1-9][0-9]*$ ]]; then
		problem "$name='$run' is not a workflow run id"
		return 0
	fi
	if ! branch="$(release_gh api "repos/3leaps/yamlprims/actions/runs/$run" --jq .head_branch 2>/dev/null)"; then
		problem "$name=$run could not be looked up on GitHub (offline, or not a yamlprims run)"
	elif [[ "$branch" != "$tag" ]]; then
		problem "$name=$run is a run for '$branch', not $tag"
	else
		ok "$name=$run is a run for $tag"
	fi
}
check_run YAMLPRIMS_RELEASE_RUN_ID "${YAMLPRIMS_RELEASE_RUN_ID:-}"
check_run YAMLPRIMS_CRATE_RUN_ID "${YAMLPRIMS_CRATE_RUN_ID:-}"
if [[ -n "${YAMLPRIMS_RELEASE_RUN_ATTEMPT:-}" && -z "${YAMLPRIMS_RELEASE_RUN_ID:-}" ]]; then
	problem "YAMLPRIMS_RELEASE_RUN_ATTEMPT is set without YAMLPRIMS_RELEASE_RUN_ID"
fi

if ((problems > 0)); then
	echo "release-env: $problems problem(s) for $tag; reset with: eval \"\$(make -s release-env-reset)\"" >&2
	exit 1
fi
echo "[ok] per-release environment belongs to $tag"
