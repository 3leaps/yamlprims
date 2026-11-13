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
# shellcheck source=scripts/release-common.sh
source "$root/scripts/release-common.sh"

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

# The strict loader from release-common.sh, run in a child shell so it cannot
# change this shell's expected values.
# shellcheck disable=SC2016 # Expands in the child shell.
anchor_loader='set -euo pipefail
source "$1/scripts/release-common.sh"
load_ceremony_anchor
printf "%s %s\n" "$YAMLPRIMS_EXPECTED_TAG_OBJECT" "$YAMLPRIMS_EXPECTED_COMMIT"'
anchor_object="" anchor_commit=""
if [[ -n "${YAMLPRIMS_RELEASE_ANCHOR_FILE+x}" ]]; then
	anchor="$YAMLPRIMS_RELEASE_ANCHOR_FILE"
	if [[ "$anchor" != /* ]]; then
		problem "YAMLPRIMS_RELEASE_ANCHOR_FILE='$anchor' is not an absolute path"
	elif [[ -L "$anchor" || (-e "$anchor" && ! -f "$anchor") ]]; then
		problem "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor exists but is not a regular file"
	elif [[ ! -e "$anchor" ]]; then
		ok "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor is not recorded yet"
	elif anchor_err="$(mktemp)" && ! loaded="$(env -u YAMLPRIMS_EXPECTED_TAG_OBJECT -u YAMLPRIMS_EXPECTED_COMMIT \
		bash -c "$anchor_loader" bash "$root" 2>"$anchor_err")"; then
		problem "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor is not a valid ceremony anchor for $tag: $(tr '\n' ' ' <"$anchor_err")"
	else
		read -r anchor_object anchor_commit <<<"$loaded"
		if [[ "$anchor_commit" != "$release_commit" ]]; then
			problem "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor records commit $anchor_commit, but $where is $release_commit"
		elif [[ "$(git cat-file -t "$anchor_object" 2>/dev/null)" != tag ||
			"$(git cat-file tag "$anchor_object" | sed -n 's/^object //p' | head -1)" != "$anchor_commit" ||
			"$(git cat-file tag "$anchor_object" | sed -n 's/^type //p' | head -1)" != commit ||
			"$(git cat-file tag "$anchor_object" | sed -n 's/^tag //p' | head -1)" != "$tag" ]]; then
			# The recorded object must be an annotated tag named $tag whose
			# direct target is the commit (not another tag, not another name).
			problem "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor records object $anchor_object, which is not an annotated tag $tag directly on $anchor_commit in this repository (fetch the tag or re-record)"
		elif [[ -n "$tag_object" && "$(git cat-file -t "$tag_object")" != tag ]]; then
			problem "local tag $tag is a lightweight tag ($tag_object); release tags are annotated"
		elif [[ -n "$tag_object" && "$anchor_object" != "$tag_object" ]]; then
			problem "YAMLPRIMS_RELEASE_ANCHOR_FILE=$anchor records $anchor_object -> $anchor_commit, local tag $tag is $tag_object -> $tag_commit"
		else
			ok "YAMLPRIMS_RELEASE_ANCHOR_FILE records $tag"
		fi
	fi
	rm -f "${anchor_err:-}"
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

# A selected workflow run must be a run for this tag at the release commit.
check_run() {
	local name="$1" run="$2" attempt="${3:-}" info branch sha run_attempt
	[[ -n "$run" ]] || return 0
	if [[ ! "$run" =~ ^[1-9][0-9]*$ ]]; then
		problem "$name='$run' is not a workflow run id"
		return 0
	fi
	if ! info="$(release_gh api "repos/3leaps/yamlprims/actions/runs/$run" --jq '"\(.head_branch) \(.head_sha) \(.run_attempt)"' 2>/dev/null)"; then
		problem "$name=$run could not be looked up on GitHub (offline, or not a yamlprims run)"
		return 0
	fi
	read -r branch sha run_attempt <<<"$info"
	if [[ "$branch" != "$tag" ]]; then
		problem "$name=$run is a run for '$branch', not $tag"
	elif [[ "$sha" != "$release_commit" ]]; then
		problem "$name=$run ran at $sha, but $where is $release_commit"
	# Attempts are small; bounding the digits keeps the arithmetic exact.
	elif [[ -n "$attempt" && ! ("$attempt" =~ ^[1-9][0-9]{0,8}$ && "$run_attempt" =~ ^[1-9][0-9]{0,8}$ && "$attempt" -le "$run_attempt") ]]; then
		problem "YAMLPRIMS_RELEASE_RUN_ATTEMPT='$attempt' is not an attempt of run $run (latest $run_attempt)"
	else
		ok "$name=$run is a run for $tag at $release_commit"
	fi
}
if [[ -n "${YAMLPRIMS_RELEASE_RUN_ATTEMPT:-}" && -z "${YAMLPRIMS_RELEASE_RUN_ID:-}" ]]; then
	problem "YAMLPRIMS_RELEASE_RUN_ATTEMPT is set without YAMLPRIMS_RELEASE_RUN_ID"
fi
check_run YAMLPRIMS_RELEASE_RUN_ID "${YAMLPRIMS_RELEASE_RUN_ID:-}" "${YAMLPRIMS_RELEASE_RUN_ATTEMPT:-}"
check_run YAMLPRIMS_CRATE_RUN_ID "${YAMLPRIMS_CRATE_RUN_ID:-}"

if ((problems > 0)); then
	echo "release-env: $problems problem(s) for $tag; reset with: eval \"\$(make -s release-env-reset)\"" >&2
	exit 1
fi
echo "[ok] per-release environment belongs to $tag"
