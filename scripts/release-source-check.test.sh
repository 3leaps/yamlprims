#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# shellcheck disable=SC2016,SC2059
# The tagged release source is refused when it carries anything Cargo or rustup
# would act on: toolchain selectors, .cargo configuration, build scripts,
# symlinks, submodules or manifest paths that leave the crate. Also: the
# registry Cargo runner clears the environment and pins the toolchain.
set -euo pipefail
# shellcheck source=scripts/release-test-fixture.sh
source "$(dirname "$0")/release-test-fixture.sh"
export PYTHONDONTWRITEBYTECODE=1
repo="$fixture_dir/source"
mkdir -p "$repo/crates/yamlprims-core/src" "$repo/crates/yamlprims-json/src" "$repo/config/release"
git -C "$repo" init -q -b main
git -C "$repo" config user.name fixture
git -C "$repo" config user.email fixture@example.invalid
git -C "$repo" config commit.gpgsign false
printf '[workspace]\nmembers = ["crates/yamlprims-core", "crates/yamlprims-json"]\n' >"$repo/Cargo.toml"
for crate in yamlprims-core yamlprims-json; do
	printf '[package]\nname = "%s"\nreadme = "README.md"\ninclude = ["src/**"]\n\n[lib]\npath = "src/lib.rs"\n' "$crate" \
		>"$repo/crates/$crate/Cargo.toml"
	printf '// synthetic\n' >"$repo/crates/$crate/src/lib.rs"
	printf 'synthetic\n' >"$repo/crates/$crate/README.md"
done
printf 'yamlprims-core\nyamlprims-json\n' >"$repo/config/release/publishable-crates.txt"
fixture_commit "$repo" base
base="$(git -C "$repo" rev-parse HEAD)"
check() { "$fixture_root/scripts/release-source-check.py" "$(git -C "$repo" rev-parse HEAD)" --repo "$repo"; }
check >/dev/null

# refuse <expected message> : the current HEAD must be refused with that message;
# then the tree is reset to the clean base.
refuse() {
	expect_fail_with "$1" check
	git -C "$repo" reset -q --hard "$base"
}
printf '[toolchain]\nchannel = "nightly"\n' >"$repo/rust-toolchain.toml"
fixture_commit "$repo" toolchain
refuse 'toolchain selector'
printf 'nightly\n' >"$repo/crates/yamlprims-core/rust-toolchain"
fixture_commit "$repo" nested-toolchain
refuse 'toolchain selector'
mkdir -p "$repo/crates/yamlprims-json/.cargo"
printf '[net]\nretry = 1\n' >"$repo/crates/yamlprims-json/.cargo/config.toml"
fixture_commit "$repo" cargo-config
refuse 'Cargo configuration'
printf 'fn main() {}\n' >"$repo/crates/yamlprims-core/build.rs"
fixture_commit "$repo" build-rs
refuse 'build script'
mkdir -p "$repo/libs/yamlprims-x"
printf 'fn main() {}\n' >"$repo/libs/yamlprims-x/build.rs"
fixture_commit "$repo" build-rs-elsewhere
refuse 'build script'
printf '[package]\nname = "yamlprims-core"\nbuild = "tools/gen.rs"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" package-build
refuse 'build script'
ln -s "$HOME/.cargo/credentials.toml" "$repo/crates/yamlprims-core/notes"
fixture_commit "$repo" symlink
refuse 'symlinks'
printf '[package]\nname = "yamlprims-core"\nreadme = "../../../outside.md"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" readme-escape
refuse 'package.readme must stay inside'
printf '[package]\nname = "yamlprims-core"\nlicense-file = "/etc/hosts"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" license-absolute
refuse 'package.license-file must stay inside'
printf '[package]\nname = "yamlprims-core"\ninclude = ["../../secret"]\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" include-escape
refuse 'package.include entries'
printf '[package]\nname = "yamlprims-core"\n\n[lib]\npath = "../../elsewhere/lib.rs"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" lib-escape
refuse '\[lib\] path must stay inside'
printf '[package]\nname = "yamlprims-core"\n\n[[bin]]\nname = "x"\npath = "/tmp/x.rs"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" bin-absolute
refuse '\[bin\] path must stay inside'
printf '[workspace]\nmembers = ["crates/yamlprims-core", "libs/yamlprims-json"]\n' >"$repo/Cargo.toml"
fixture_commit "$repo" member-outside
refuse 'workspace members must be under crates/'
git -C "$repo" mv crates/yamlprims-json libs-yamlprims-json
fixture_commit "$repo" relocated-crate
refuse 'published crate yamlprims-json must be at crates/yamlprims-json/Cargo.toml'
printf '[workspace]\nmembers = ["crates/yamlprims-core", "crates/yamlprims-json"]\n\n[workspace.package]\nreadme = "/opt/outside/secret.md"\n' >"$repo/Cargo.toml"
fixture_commit "$repo" workspace-readme-escape
refuse 'workspace.package.readme must stay inside'
printf '[package]\nname = "yamlprims-core"\nreadme.workspace = true\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" inherited-readme
refuse 'package.readme may not be inherited'
printf '[package]\nname = "yamlprims-core"\nlicense-file = { workspace = true }\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" inherited-license
refuse 'package.license-file may not be inherited'
printf '[workspace]\nmembers = ["crates/yamlprims-core", "crates/yamlprims-json"]\n\n[workspace.package]\ninclude = ["../../outside"]\n' >"$repo/Cargo.toml"
fixture_commit "$repo" workspace-include-escape
refuse 'workspace.package.include entries'
printf '[package]\nname = "yamlprims-core"\n\n[lib]\nproc-macro = true\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" proc-macro
refuse 'procedural macro'
printf '[package]\nname = "yamlprims-core"\n\n[lib]\nproc_macro = true\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" proc-macro-underscore
refuse 'procedural macro'
printf '[package]\nname = "yamlprims-core"\n\n[lib]\ncrate-type = ["proc-macro"]\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" crate-type-proc-macro
refuse 'procedural macro'
printf '[package]\nname = "yamlprims-core"\n\n[lib]\ncrate_type = ["rlib", "proc-macro"]\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" crate-type-underscore
refuse 'procedural macro'
mkdir -p "$repo/crates/yamlprims-json/.CARGO"
printf '[http]\nproxy = "http://127.0.0.1:9"\n' >"$repo/crates/yamlprims-json/.CARGO/config.toml"
fixture_commit "$repo" uppercase-cargo-dir
refuse 'Cargo configuration'
printf 'nightly\n' >"$repo/Rust-Toolchain.TOML"
fixture_commit "$repo" mixed-case-toolchain
refuse 'toolchain selector'
printf 'fn main() {}\n' >"$repo/crates/yamlprims-core/BUILD.RS"
fixture_commit "$repo" uppercase-build
refuse 'build script'
# A second tracked path differing only in case (built in the index, since a
# case-insensitive host cannot hold both in one working tree).
blob="$(printf '[package]\nname = "x"\n' | git -C "$repo" hash-object -w --stdin)"
git -C "$repo" update-index --add --cacheinfo "100644,$blob,crates/yamlprims-core/cargo.toml"
git -C "$repo" commit -qm lowercase-manifest
refuse 'collides with'
# Dependencies: crates.io or a crate in this tree only.
core='[package]\nname = "yamlprims-core"\n'
dep() { printf "$core"'\n[%s]\n%s\n' "$1" "$2" >"$repo/crates/yamlprims-core/Cargo.toml"; fixture_commit "$repo" "dep-$1"; refuse "$3"; }
dep dependencies 'evil = { git = "https://example.invalid/evil.git" }' 'dependencies.evil uses git'
dep dependencies 'evil = { path = "/tmp/evil" }' 'dependencies.evil path must stay inside'
dep dependencies 'evil = { path = "../../../outside" }' 'path must stay inside the tree'
dep dependencies 'evil = { path = "../not-a-crate" }' 'path must name a crate in the tree'
dep dependencies 'evil = { version = "1", registry = "other" }' 'dependencies.evil uses registry'
dep dev-dependencies 'evil = { git = "https://example.invalid/evil.git", rev = "abc" }' 'dev-dependencies.evil uses git'
dep build-dependencies 'evil = { path = "C:/evil" }' 'build-dependencies.evil path must stay inside'
dep 'target.x86_64-unknown-linux-gnu.dependencies' 'evil = { git = "https://example.invalid/evil.git" }' 'target.x86_64-unknown-linux-gnu.dependencies.evil uses git'
dep 'patch.crates-io' 'serde = { path = "../serde" }' '\[patch\] is not allowed'
dep replace '"serde:1.0.0" = { path = "../serde" }' '\[replace\] is not allowed'
printf '[workspace]\nmembers = ["crates/yamlprims-core", "crates/yamlprims-json"]\n\n[workspace.dependencies]\nevil = { git = "https://example.invalid/evil.git" }\n' >"$repo/Cargo.toml"
fixture_commit "$repo" workspace-git
refuse 'workspace.dependencies.evil uses git'
printf '[package]\nname = "yamlprims-core"\nworkspace = "../../elsewhere"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" package-workspace
refuse 'package.workspace is not allowed'
# A path dependency on a sibling crate in the tree is fine.
printf "$core"'\n[dependencies]\nyamlprims-json = { path = "../yamlprims-json", version = "1" }\nserde = "1"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" sibling-path
check >/dev/null
git -C "$repo" reset -q --hard "$base"
# crate-type as a string is a procedural macro too.
printf '[package]\nname = "yamlprims-core"\n\n[lib]\ncrate-type = "proc-macro"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" crate-type-string
refuse 'procedural macro'
# The legacy [project] alias is refused, whatever it carries.
printf '[project]\nname = "yamlprims-core"\nbuild = "payload.rs"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" legacy-project
refuse 'legacy \[project\] table'
printf '[project]\nname = "yamlprims-core"\nworkspace = "/elsewhere"\nreadme = "/etc/hosts"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" legacy-project-paths
refuse 'legacy \[project\] table'
git -C "$repo" update-index --add --cacheinfo "160000,$base,vendor/sub"
git -C "$repo" commit -qm submodule
refuse 'submodules'
check >/dev/null

# A checkout whose bytes differ from the tagged blobs (an ident attribute and a
# smudge filter) is refused; a faithful checkout passes. Windows drive paths in
# manifests are not "inside the crate".
tree="$fixture_dir/checkout"
git -C "$repo" worktree add -q --detach "$tree" "$base"
"$fixture_root/scripts/release-source-check.py" "$base" --repo "$repo" --worktree "$tree" >/dev/null
printf '$Id$\n' >>"$tree/crates/yamlprims-core/README.md"
expect_fail_with 'checked-out bytes differ from the tagged blobs: crates/yamlprims-core/README.md' \
	"$fixture_root/scripts/release-source-check.py" "$base" --repo "$repo" --worktree "$tree"
git -C "$repo" worktree remove --force "$tree"
printf '*.md filter=synthetic\n' >"$repo/.gitattributes"
fixture_commit "$repo" attributes
attributed="$(git -C "$repo" rev-parse HEAD)"
git -C "$repo" config filter.synthetic.smudge "sed s/synthetic/rewritten/"
git -C "$repo" worktree add -q --detach "$tree" "$attributed"
grep -q rewritten "$tree/crates/yamlprims-core/README.md"
expect_fail_with 'checked-out bytes differ from the tagged blobs' \
	"$fixture_root/scripts/release-source-check.py" "$attributed" --repo "$repo" --worktree "$tree"
git -C "$repo" worktree remove --force "$tree"
git -C "$repo" config --unset filter.synthetic.smudge
git -C "$repo" reset -q --hard "$base"
printf '[package]\nname = "yamlprims-core"\nreadme = "C:/outside/secret.md"\n' >"$repo/crates/yamlprims-core/Cargo.toml"
fixture_commit "$repo" drive-path
refuse 'package.readme must stay inside'

# The registry Cargo runner: a cleared environment and a pinned toolchain. A
# fake cargo on PATH reports what it received.
mkdir "$fixture_dir/bin"
cat >"$fixture_dir/bin/cargo" <<'SH'
#!/usr/bin/env bash
env | LC_ALL=C sort
SH
chmod +x "$fixture_dir/bin/cargo"
isolated="$fixture_dir/isolated"
mkdir "$isolated"
seen="$(
	PATH="$fixture_dir/bin:$PATH" RUSTFLAGS='-C linker=/payload' CARGO_ENCODED_RUSTFLAGS=x RUSTC_WRAPPER=/payload \
		CARGO_BUILD_RUSTC=/payload CARGO_REGISTRY_TOKEN=synthetic-token YAMLPRIMS_RELEASE_TAG=v1.2.3 \
		CARGO_HOME="$HOME/.cargo" \
		bash -c 'source "$1/scripts/release-common.sh"; release_cargo 1.88.0 "$2" metadata' _ "$fixture_root" "$isolated"
)"
grep -qx 'RUSTUP_TOOLCHAIN=1.88.0' <<<"$seen"
# The operator's Cargo home and HOME are never used: both are inside the
# isolated directory, so operator config, credentials and proxies are unseen.
grep -qx "CARGO_HOME=$isolated/cargo" <<<"$seen"
grep -qx "HOME=$isolated/home" <<<"$seen"
# (A negated command never trips set -e, so refusals fail explicitly.)
if grep -Eq '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|CARGO_BUILD_RUSTC|CARGO_REGISTRY_TOKEN|YAMLPRIMS_RELEASE_TAG)=' <<<"$seen"; then
	echo 'error: release_cargo passed through an unsafe or unrelated variable' >&2
	exit 1
fi
with_token="$(
	PATH="$fixture_dir/bin:$PATH" CARGO_REGISTRY_TOKEN=synthetic-token RUSTFLAGS=x \
		bash -c 'source "$1/scripts/release-common.sh"; release_cargo 1.88.0 "$2" --with-token publish' _ "$fixture_root" "$isolated"
)"
expect_fail_with 'CARGO_REGISTRY_TOKEN is required' env -u CARGO_REGISTRY_TOKEN PATH="$fixture_dir/bin:$PATH" \
	bash -c 'source "$1/scripts/release-common.sh"; release_cargo 1.88.0 "$2" --with-token publish' _ "$fixture_root" "$isolated"
expect_fail_with 'isolated absolute directory' env PATH="$fixture_dir/bin:$PATH" \
	bash -c 'source "$1/scripts/release-common.sh"; release_cargo 1.88.0 relative metadata' _ "$fixture_root"
grep -qx 'CARGO_REGISTRY_TOKEN=synthetic-token' <<<"$with_token"
if grep -q '^RUSTFLAGS=' <<<"$with_token"; then
	echo 'error: release_cargo --with-token passed RUSTFLAGS through' >&2
	exit 1
fi
echo '[ok] release source and registry Cargo environment controls passed'
