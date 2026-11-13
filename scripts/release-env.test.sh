#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Controls for scripts/release-env.sh: reset names every per-release variable,
# and check refuses values that belong to another release (the stale-shell
# failures of a real ceremony), with a fake gh and a throwaway repository.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd -P)"
scratch="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$scratch"' EXIT
fail() {
	echo "error: $*" >&2
	exit 1
}

# reset prints one unset line covering the per-release set, nothing else.
reset_line="$("$root/scripts/release-env.sh" reset)"
for name in YAMLPRIMS_RELEASE_TAG YAMLPRIMS_RELEASE_COMMIT YAMLPRIMS_EXPECTED_TAG_OBJECT YAMLPRIMS_EXPECTED_COMMIT \
	YAMLPRIMS_RELEASE_ANCHOR_FILE YAMLPRIMS_RELEASE_RUN_ID YAMLPRIMS_RELEASE_RUN_ATTEMPT YAMLPRIMS_CRATE_RUN_ID; do
	grep -qw "$name" <<<"$reset_line" || fail "reset does not unset $name"
done
[[ "$reset_line" == unset\ * && "$(wc -l <<<"$reset_line" | tr -d ' ')" == 1 ]] || fail "reset is not a single unset line"
grep -Eq 'GPG|MINISIGN|TAGGER|DECERNOR|YAML_BIN|TOKEN' <<<"$reset_line" && fail 'reset touches long-lived configuration'
"$root/scripts/release-env.sh" bogus >/dev/null 2>&1 && fail 'unknown mode accepted'

repo="$scratch/repo"
mkdir -p "$repo/scripts"
cp "$root/scripts/release-env.sh" "$root/scripts/release-gh.sh" "$repo/scripts/"
printf '1.2.3\n' >"$repo/VERSION"
printf '1.4\n' >"$repo/SPEC_VERSION"
git -C "$repo" init -q -b main
git -C "$repo" -c user.name=t -c user.email=t@example.invalid add -A
git -C "$repo" -c user.name=t -c user.email=t@example.invalid commit -qm one
first="$(git -C "$repo" rev-parse HEAD)"
git -C "$repo" -c user.name=t -c user.email=t@example.invalid commit -qm two --allow-empty
head="$(git -C "$repo" rev-parse HEAD)"

mkdir -p "$scratch/bin"
cat >"$scratch/bin/gh" <<'SH'
#!/usr/bin/env bash
for arg in "$@"; do
	case "$arg" in
	repos/3leaps/yamlprims/actions/runs/100) echo v1.2.3; exit 0 ;;
	repos/3leaps/yamlprims/actions/runs/200) echo v1.2.2; exit 0 ;;
	esac
done
echo 'HTTP 404' >&2
exit 1
SH
chmod +x "$scratch/bin/gh"

# Callers pass env options (-u NAME) first, then NAME=value assignments.
check() { (cd "$repo" && env "$@" PATH="$scratch/bin:$PATH" ./scripts/release-env.sh check) >"$scratch/out" 2>&1; }
passes() { check "$@" || { cat "$scratch/out" >&2; fail "refused: $*"; }; }
refused() {
	local pattern="$1"
	shift
	if check "$@"; then cat "$scratch/out" >&2; fail "accepted, expected: $pattern"; fi
	grep -qF -- "$pattern" "$scratch/out" || { cat "$scratch/out" >&2; fail "refusal did not mention: $pattern"; }
}
clean=(-u YAMLPRIMS_RELEASE_COMMIT -u YAMLPRIMS_EXPECTED_TAG_OBJECT -u YAMLPRIMS_EXPECTED_COMMIT -u YAMLPRIMS_RELEASE_ANCHOR_FILE
	-u YAMLPRIMS_RELEASE_RUN_ID -u YAMLPRIMS_RELEASE_RUN_ATTEMPT -u YAMLPRIMS_CRATE_RUN_ID)

# The tag itself.
refused "is not a canonical vX.Y.Z or spec-vX.Y tag" ${clean[@]+"${clean[@]}"} -u YAMLPRIMS_RELEASE_TAG
refused "does not match VERSION (v1.2.3)" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.2
passes ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3

# Before the tag: the release commit is HEAD; expected values cannot exist yet.
passes ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_COMMIT="$head"
refused "YAMLPRIMS_RELEASE_COMMIT=$first differs from HEAD (no local tag v1.2.3 yet) $head" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_COMMIT="$first"
refused "is not a full lowercase 40-hex commit" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_COMMIT="${head:0:12}"
refused "YAMLPRIMS_EXPECTED_TAG_OBJECT=$first is set but v1.2.3 is not tagged yet" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_EXPECTED_TAG_OBJECT="$first"

# After the tag: the release commit is the tag's; expected values must be the tag's.
git -C "$repo" -c user.name=t -c user.email=t@example.invalid tag -a -m v1.2.3 v1.2.3 "$head"
object="$(git -C "$repo" rev-parse refs/tags/v1.2.3)"
passes ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_COMMIT="$head" YAMLPRIMS_EXPECTED_TAG_OBJECT="$object" YAMLPRIMS_EXPECTED_COMMIT="$head"
refused "YAMLPRIMS_EXPECTED_TAG_OBJECT=$first differs from local tag v1.2.3 ($object)" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_EXPECTED_TAG_OBJECT="$first"
refused "YAMLPRIMS_EXPECTED_COMMIT=$first differs from local tag v1.2.3 ($head)" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_EXPECTED_COMMIT="$first"

# The anchor file: not yet recorded is fine; recorded must name this tag and object.
passes ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/absent.anchor"
refused "is not an absolute path" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_ANCHOR_FILE=rel.anchor
printf 'tag=v1.2.2\nobject=%s\ncommit=%s\n' "$object" "$head" >"$scratch/old.anchor"
refused "records tag 'v1.2.2', not v1.2.3" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/old.anchor"
printf 'tag=v1.2.3\nobject=%s\ncommit=%s\n' "$object" "$head" >"$scratch/good.anchor"
passes ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/good.anchor"
refused "differs from the recorded anchor ($object); unset it" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/good.anchor" YAMLPRIMS_EXPECTED_TAG_OBJECT="$first"
printf 'tag=v1.2.3\nobject=%s\ncommit=%s\n' "$first" "$first" >"$scratch/moved.anchor"
refused "local tag v1.2.3 is $object -> $head" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_ANCHOR_FILE="$scratch/moved.anchor"

# Selected runs must be runs for this tag.
passes ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_RUN_ID=100
refused "YAMLPRIMS_RELEASE_RUN_ID=200 is a run for 'v1.2.2', not v1.2.3" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_RUN_ID=200
refused "YAMLPRIMS_CRATE_RUN_ID=200 is a run for 'v1.2.2', not v1.2.3" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_CRATE_RUN_ID=200
refused "could not be looked up on GitHub" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_RUN_ID=300
refused "is not a workflow run id" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_RUN_ID=abc
refused "YAMLPRIMS_RELEASE_RUN_ATTEMPT is set without YAMLPRIMS_RELEASE_RUN_ID" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_RUN_ATTEMPT=1

# Every problem is reported, not only the first.
refused "3 problem(s) for v1.2.3" ${clean[@]+"${clean[@]}"} YAMLPRIMS_RELEASE_TAG=v1.2.3 YAMLPRIMS_RELEASE_COMMIT="$first" \
	YAMLPRIMS_EXPECTED_TAG_OBJECT="$first" YAMLPRIMS_RELEASE_RUN_ID=200
echo '[ok] per-release environment reset and check controls'
