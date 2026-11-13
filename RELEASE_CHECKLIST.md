# Release Checklist

Use this checklist for `yamlprims` releases. Specification tags (`spec-v<SPEC_VERSION>`) and code releases (`v<VERSION>`) use separate version files and tag namespaces; see [docs/releases.md](docs/releases.md).

## 0. Start from a clean release shell

Per-release values (`YAMLPRIMS_RELEASE_TAG`, `YAMLPRIMS_RELEASE_COMMIT`, `YAMLPRIMS_EXPECTED_*`, `YAMLPRIMS_RELEASE_ANCHOR_FILE`, `YAMLPRIMS_RELEASE_RUN_ID`/`_ATTEMPT`, `YAMLPRIMS_CRATE_RUN_ID`) belong to one tag. Clear them before a new release, then set this release's tag and reviewed commit:

```sh
eval "$(make -s release-env-reset)"
export YAMLPRIMS_RELEASE_TAG=v<VERSION>          # or spec-v<SPEC_VERSION>
export YAMLPRIMS_RELEASE_COMMIT=<reviewed 40-hex commit on main>
make release-env-check
```

Run `make release-env-check` again whenever a step fails unexpectedly or the shell has been reused: it names every per-release value that belongs to another release.

## 1. Version and release-note alignment

- For a code release (`v<VERSION>`):
  - Confirm `VERSION`, `Cargo.toml` (`[workspace.package].version`), and `crates/yamlprims-cli/Cargo.toml` (`yamlprims` dependency version) match `<VERSION>`.
  - Write `docs/releases/v<VERSION>.md` starting with heading `## yamlprims v<VERSION>` and update [CHANGELOG.md](CHANGELOG.md).
- For a specification release (`spec-v<SPEC_VERSION>`):
  - Confirm `SPEC_VERSION`, the `**Revision <SPEC_VERSION>**` line in [docs/spec/yamlprims-spec.md](docs/spec/yamlprims-spec.md), and the first `# spec-v<SPEC_VERSION>` heading in [RELEASE_NOTES.md](RELEASE_NOTES.md) match.

## 2. Local quality, conformance, and release-tooling gates

```sh
make prepush
make release-tooling-test
make check-packages
make surface-scan
```

When qualifying a code release or adapter change, also run the pinned `structcorpus` qualification gate (`conformance/requalify.sh`) as documented in [conformance/README.md](conformance/README.md).

## 3. GitHub preflight and signed tag creation

Source the maintainer's external release environment outside the repository and set `YAMLPRIMS_RELEASE_TAG` (`v<VERSION>` or `spec-v<SPEC_VERSION>`) and `YAMLPRIMS_RELEASE_COMMIT` (the reviewed full lowercase 40-hex commit on `main`; `release-tag` refuses unless `HEAD`, fetched `origin/main` and this value are the same commit, and `release-push-tag` refuses a local tag that names any other commit); see [docs/releases.md](docs/releases.md#signed-tags) for the required environment variables:

```sh
make release-github-preflight
make release-tag
make release-verify-tag
make release-push-tag
make release-verify-remote-tag
```

`make release-verify-remote-tag` ends by printing `export YAMLPRIMS_EXPECTED_TAG_OBJECT=…` and `export YAMLPRIMS_EXPECTED_COMMIT=…` for the pushed tag. Compare both values with the approval record, export them, and record the original ceremony anchor at an absent absolute path outside the checkout:

```sh
export YAMLPRIMS_EXPECTED_TAG_OBJECT=<40-hex tag object>
export YAMLPRIMS_EXPECTED_COMMIT=<40-hex commit>
export YAMLPRIMS_RELEASE_ANCHOR_FILE=/absolute/path/outside/checkout/<tag>.anchor
make release-record-anchor
unset YAMLPRIMS_EXPECTED_TAG_OBJECT YAMLPRIMS_EXPECTED_COMMIT
```

From here on the recorded anchor file is the only binding: every later step reads the tag object and commit from `YAMLPRIMS_RELEASE_ANCHOR_FILE`, and `release-verify-remote-tag` refuses an exported `YAMLPRIMS_EXPECTED_*` that disagrees with it.

## 4. Code release artifact assembly, signing, and verification (`v<VERSION>`)

After the `Release` workflow succeeds on the pushed `v<VERSION>` tag:

```sh
make release-fetch-ci-artifacts
make release-stage-public
make release-checksums
make release-create-draft     # before signing: the draft holds the checksummed set
make release-sign
make release-verify
make release-upload           # adds the four signature files to the draft
make release-publish
make release-verify-published
```

`release-create-draft` validates the exact checksummed inventory, so it must run before `release-sign`. If the set is already signed, move the four `SHA*SUMS.{minisig,asc}` files aside, run `release-create-draft`, move them back, then `release-upload` (it re-verifies both signature sets).

## 5. Package registry publication (`v<VERSION>`)

The registries receive **exact signed files from the published GitHub release**; nothing is rebuilt.

### 5a. crates.io (`yamlprims`)

For each crate in `make release-crates-list` order (`yamlprims`; `yamlprims-cli` remains `publish = false` and ships as signed release binaries):

- Inputs: `YAMLPRIMS_CARGO_TOKEN_FILE` (a file holding the crates.io token, read only by the isolated credential provider `scripts/release-cargo-provider.py`); `CARGO_REGISTRY_TOKEN` must be unset. `release-crates-fetch` creates `dist/crates`, which the dry run and publish read.
- Fetch, verify, and publish:

```sh
make release-crates-fetch CRATE=<crate>
make release-crate-dry-run CRATE=<crate>
make release-crate-publish CRATE=<crate>
```

### 5b. PyPI (`yamlprims`) and npm (`@3leaps/yamlprims`) via OIDC Trusted Publishing

Once the project exists on PyPI (`yamlprims` in the `3leaps` organization) and npm (`@3leaps/yamlprims`), and GitHub Environments `publish-pypi` and `publish-npm` (required reviewer `@3leapsdave`, deployment restricted to `v*` tags) are configured as Trusted Publishers on PyPI and npm:

- Stage and verify in dry-run mode on the release tag:
  `gh workflow run registry-publish.yml --ref v<VERSION> -f tag=v<VERSION> -f registry=all -f dry_run=true`
- Dispatch the OIDC publication on the release tag and approve the `publish-pypi` and `publish-npm` environment deployments:
  `gh workflow run registry-publish.yml --ref v<VERSION> -f tag=v<VERSION> -f registry=all -f dry_run=false`
  - `publish-pypi` re-verifies the 5 staged `cp310-abi3` wheel digests against the verified `SHA256SUMS` and `SHA512SUMS`, uploads via `pypa/gh-action-pypi-publish` with PEP 740 provenance attestations, and runs `scripts/release-verify-registries.py --expect-provenance`.
  - `publish-npm` re-verifies `3leaps-yamlprims-<VERSION>.tgz` against the verified `SHA256SUMS` and `SHA512SUMS`, publishes with OIDC provenance attestation (`npm publish ... --provenance --access public`), and runs `scripts/release-verify-registries.py --expect-provenance`.
- Verify locally from the maintainer checkout:

```sh
YAMLPRIMS_RELEASE_TAG=v<VERSION> make release-stage-published release-verify-registries
```

- **npm post-upload validation:** after `npm publish` succeeds, npmjs.com can keep the version in its validation queue for many minutes (about 18 minutes for v0.2.0), so the workflow's `release-verify-registries.py` step may fail with HTTP 404 even though the publish succeeded (the error says so). Wait it out, then re-run `YAMLPRIMS_RELEASE_TAG=v<VERSION> make release-verify-registries` locally, raising `YAMLPRIMS_REGISTRY_POLL_ATTEMPTS` if needed.
- **Post-publish verification failure recovery & Sigstore pin currency:**
  - `registry-publish.yml` runs `scripts/release-verify-registries.py --expect-provenance` immediately after upload. If verification fails due to transient registry indexing lag, re-run `YAMLPRIMS_RELEASE_TAG=v<VERSION> make release-verify-registries` locally once the registry metadata settles.
  - If verification fails because Sigstore rotated the Fulcio intermediate CA or Rekor transparency log key, refresh the pinned trust material in `scripts/release-verify-registries.py` from the live Sigstore public-good trust root and re-run `make release-verify-registries`.
  - If a published artifact digest or provenance attestation genuinely fails verification, immediately yank the affected PyPI release or deprecate/unpublish the affected npm version before cutting a patch release.

### 5c. Homebrew tap and Scoop bucket (CLI)

After the registries verify, update the shared `3leaps/homebrew-tap` and `3leaps/scoop-bucket` from the published release (hashes come only from the minisign-verified `SHA256SUMS`; nothing is rebuilt), each in its own branch and PR:

```sh
make -C ../homebrew-tap update-yamlprims TAG=v<VERSION>
make -C ../homebrew-tap style audit test APP=yamlprims   # brew test runs `yamlprims --version`
make -C ../homebrew-tap clean APP=yamlprims              # uninstall after the smoke: no two versions on PATH

make -C ../scoop-bucket update-yamlprims VERSION=<VERSION>
make -C ../scoop-bucket check
```

Open one PR in each repository; merge after CI is green.

### First publication only (before the package exists on PyPI or npm)

Because npm Trusted Publishing can only be configured on an existing package and a first PyPI upload creates the project:

- `YAMLPRIMS_RELEASE_TAG=v<VERSION> make release-stage-published`
- PyPI: `uvx twine upload dist/published/yamlprims-<VERSION>-cp310-abi3-*.whl` (and transfer ownership to the `3leaps` organization on PyPI if uploaded with a user token).
- npm: `npm publish dist/published/3leaps-yamlprims-<VERSION>.tgz --access public`
  - Register OIDC Trusted Publishing (`npm >= 11.5.1`):
    `npm trust github @3leaps/yamlprims --file registry-publish.yml --repo 3leaps/yamlprims --env publish-npm --allow-publish -y`
  - One-time defensive bare-name redirector (`yamlprims` -> `@3leaps/yamlprims`): run `scripts/pack-npm-redirector.sh dist/npm-redirector <VERSION>`, publish `dist/npm-redirector/yamlprims-<VERSION>.tgz`, transfer `yamlprims` to the `3leaps` npm org, and deprecate it immediately:
    `npm deprecate yamlprims "Canonical package is @3leaps/yamlprims — install @3leaps/yamlprims instead."`
- Verify live registry digests against the signed release manifest:
  `YAMLPRIMS_RELEASE_TAG=v<VERSION> scripts/release-verify-registries.sh --bootstrap-no-provenance`
