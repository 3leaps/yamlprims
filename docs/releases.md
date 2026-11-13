# Specification and code releases

Specification tags use `spec-v<SPEC_VERSION>` (governed by `SPEC_VERSION` and [RELEASE_NOTES.md](../RELEASE_NOTES.md)). Code and CLI releases use `v<VERSION>` (governed by `VERSION` and `docs/releases/v<VERSION>.md`).
Tags `spec-v1.0` through `spec-v1.5` exist (`SPEC_VERSION` is `1.5`). The current code release line is `0.2.2` (`VERSION` is `0.2.2`), shipping the Rust core crate (`yamlprims` on crates.io), the pure-Go twin (`github.com/3leaps/yamlprims/yaml`), the TypeScript WebAssembly package (`@3leaps/yamlprims` on npm), the Python `abi3` wheels (`yamlprims` on PyPI), and the multi-platform CLI (`yamlprims`); see [docs/releases/v0.2.2.md](releases/v0.2.2.md) and [the build specification](build-spec.md).

The immutable `spec-v1.1`, `spec-v1.2`, `spec-v1.3` and `spec-v1.4` specifications retain their original
"Unreleased draft" wording inside the tagged specification file. Status updates on `main` do not replace those
tagged bytes or their corpus-pinned digests. Cite the tag when an exact
specification snapshot is needed.

## Signed tags

Both specification tags (`spec-v<SPEC_VERSION>`) and code release tags (`v<VERSION>`) are annotated OpenPGP-signed objects. Signing uses the release
identity `3 Leaps Infosec Team <infosec@3leaps.net>` and an exact signing subkey
under the primary fingerprint in [the public anchor](../keys/expected-fingerprints.txt).
The keyring and private signing material remain outside Git repositories.

Existing `spec-v1.0` through `spec-v1.4` and any published `v<VERSION>` tags must not be recreated; creation commands reject existing tags.
For a new specification revision, update `SPEC_VERSION` and select `spec-v<SPEC_VERSION>`. For a new code release, update `VERSION`, `Cargo.toml` and `docs/releases/v<VERSION>.md` and select `v<VERSION>`.

The maintainer configures these environment variables in their signing terminal:

| Variable                            | Required value                                                   |
| ----------------------------------- | ---------------------------------------------------------------- |
| `YAMLPRIMS_RELEASE_TAG`             | Unused `v<VERSION>` or `spec-v<SPEC_VERSION>` tag                |
| `YAMLPRIMS_RELEASE_COMMIT`          | Reviewed full lowercase commit SHA                               |
| `YAMLPRIMS_GPG_HOMEDIR`             | Existing external GPG home                                       |
| `YAMLPRIMS_GPG_SIGNING_FINGERPRINT` | Full uppercase authorized primary fingerprint                    |
| `YAMLPRIMS_PGP_KEY_ID`              | Full uppercase signing-subkey fingerprint followed by `!`        |
| `YAMLPRIMS_TAGGER_NAME`             | `3 Leaps Infosec Team`                                           |
| `YAMLPRIMS_TAGGER_EMAIL`            | `infosec@3leaps.net`                                             |
| `YAMLPRIMS_MINISIGN_KEY`            | External Minisign secret key path (for `v<VERSION>` asset signs) |
| `YAMLPRIMS_MINISIGN_PUB`            | Repository Minisign public key (`docs/security/yamlprims-minisign.pub`) |
| `YAMLPRIMS_YAML_BIN`                | Absolute path to the yamlprims CLI from a release archive pinned in `config/yamlprims-cli.txt` (the workflow checkers run it; any path works, the digest is checked) |
| `YAMLPRIMS_DECERNOR_BIN`            | Absolute path to decernor >= 0.1.8 (fingerprint anchors)          |
| `YAMLPRIMS_CARGO_TOKEN_FILE`        | File holding the crates.io token (crate publish only; `CARGO_REGISTRY_TOKEN` must be unset) |

Per-release values set during the ceremony (`YAMLPRIMS_EXPECTED_TAG_OBJECT`, `YAMLPRIMS_EXPECTED_COMMIT`, `YAMLPRIMS_RELEASE_ANCHOR_FILE`, `YAMLPRIMS_RELEASE_RUN_ID`, `YAMLPRIMS_RELEASE_RUN_ATTEMPT`) belong to one tag only; start each release from a shell without them.

Prerequisites: Bash, Git, GnuPG, Minisign, Python 3.11+, the pinned yamlprims release CLI for the workflow checkers (`YAMLPRIMS_YAML_BIN`, the absolute path to the binary from a release archive listed in `config/yamlprims-cli.txt`) and Make. GitHub verification and release asset staging also
require authenticated `gh` and `jq`. Use the intended repository's `origin`.
The checkout must be clean on `main`; HEAD and freshly fetched `origin/main`
must both equal the explicit release commit.

```sh
make check
make release-tooling-test
make release-github-preflight
make release-tag                 # interactive GPG pinentry; local tag only
make release-verify-tag
make release-push-tag            # explicit, single-tag push without force
make release-verify-remote-tag   # matching object and GitHub verified status; prints the anchor exports
make release-record-anchor       # needs YAMLPRIMS_EXPECTED_TAG_OBJECT, YAMLPRIMS_EXPECTED_COMMIT, YAMLPRIMS_RELEASE_ANCHOR_FILE
```

The controls reject existing tags, lightweight tags, unexpected tag messages,
incorrect commit targets, wrong taggers, unpinned primary keys and signatures
made by a different subkey. Code release tags use subject `Release <VERSION>` with body from `docs/releases/v<VERSION>.md`; specification tags use subject `spec-v<SPEC_VERSION>` with body from `RELEASE_NOTES.md`. The tests
use disposable synthetic keys and a local bare remote; they do not sign or
publish an actual release. GitHub responses in those tests are simulated.

For a `v<VERSION>` code release, pushing the verified signed tag triggers `.github/workflows/release.yml` to build the multi-platform CLI archives, source archive, crate verification bundle, and CycloneDX SBOM. The maintainer then fetches the CI artifacts, stages public key anchors and release notes, generates `SHA256SUMS` and `SHA512SUMS`, dual-signs both checksum manifests with Minisign and the OpenPGP signing subkey, verifies the staged bundle, uploads the release assets, and verifies the published GitHub release (`RELEASE_CHECKLIST.md`).
