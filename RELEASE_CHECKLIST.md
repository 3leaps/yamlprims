# Release Checklist

Use this checklist for `yamlprims` releases. Specification tags (`spec-v<SPEC_VERSION>`) and code releases (`v<VERSION>`) use separate version files and tag namespaces; see [docs/releases.md](docs/releases.md).

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

Source the maintainer's external release environment outside the repository and set `YAMLPRIMS_RELEASE_TAG` (`v<VERSION>` or `spec-v<SPEC_VERSION>`) and `YAMLPRIMS_RELEASE_COMMIT` (full 40-hex commit SHA on `main`); see [docs/releases.md](docs/releases.md#signed-tags) for the required environment variables:

```sh
make release-github-preflight
make release-tag
make release-verify-tag
make release-push-tag
make release-verify-remote-tag
```

## 4. Code release artifact assembly, signing, and verification (`v<VERSION>`)

After the `Release` workflow succeeds on the pushed `v<VERSION>` tag:

```sh
make release-fetch-ci-artifacts
make release-stage-public
make release-checksums
make release-sign
make release-verify
make release-upload
make release-verify-published
make release-crate-publish-dry-run
```
