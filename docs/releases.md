# Specification releases

Specification tags use `spec-v<SPEC_VERSION>`. Both `spec-v1.0` and
`spec-v1.1` exist; the current `SPEC_VERSION` is `1.1`. Revision 1.1 remains a
draft contract with open qualification gates. No GitHub release has been published
as of October 14, 2026; publishing a release entry is separate from creating a tag.
Existing tags are never moved. Specification tags contain repository documentation
and no crates or runtime artifacts. The proposed first Rust packages are
`yamlprims` and `yamlprims-cli` at `0.1.0`, after their implementation and
qualification gates. Native bindings follow their own milestone. See [the build specification](build-spec.md).

The immutable `spec-v1.1` specification retains its original "Unreleased draft"
wording. Status corrections on `main` do not replace those tagged bytes or their
corpus-pinned digest. Cite the tag when an exact specification snapshot is needed.

## Signed tags

Release tags are annotated OpenPGP-signed objects. Signing uses the release
identity `3 Leaps Infosec Team <infosec@3leaps.net>` and an exact signing subkey
under the primary fingerprint in [the public anchor](../keys/expected-fingerprints.txt).
The keyring and private signing material remain outside Git repositories.

The procedure below applies to a new specification tag. Existing `spec-v1.0`
and `spec-v1.1` tags must not be recreated; creation commands reject them.
For the next revision, update `SPEC_VERSION` and select its matching unused tag.

The maintainer configures these environment variables in their signing terminal:

| Variable                            | Required value                                            |
| ----------------------------------- | --------------------------------------------------------- |
| `YAMLPRIMS_RELEASE_TAG`             | Unused `spec-v<SPEC_VERSION>` tag                      |
| `YAMLPRIMS_RELEASE_COMMIT`          | Reviewed full lowercase commit SHA                        |
| `YAMLPRIMS_GPG_HOMEDIR`             | Existing external GPG home                                |
| `YAMLPRIMS_GPG_SIGNING_FINGERPRINT` | Full uppercase authorized primary fingerprint             |
| `YAMLPRIMS_PGP_KEY_ID`              | Full uppercase signing-subkey fingerprint followed by `!` |
| `YAMLPRIMS_TAGGER_NAME`             | `3 Leaps Infosec Team`                                    |
| `YAMLPRIMS_TAGGER_EMAIL`            | `infosec@3leaps.net`                                      |

Prerequisites: Bash, Git, GnuPG, Python 3.9+ and Make. GitHub verification also
requires authenticated `gh` and `jq`. Use the intended repository's `origin`.
The checkout must be clean on `main`; HEAD and freshly fetched `origin/main`
must both equal the explicit release commit.

```sh
make check
make test-release-tag-controls
make release-tag                 # interactive GPG pinentry; local tag only
make release-verify-tag
make release-push-tag            # explicit, single-tag push without force
make release-verify-remote-tag   # matching object and GitHub verified status
```

The controls reject existing tags, lightweight tags, unexpected tag messages,
incorrect commit targets, wrong taggers, unpinned primary keys and signatures
made by a different subkey. Tag text is fixed to `yamlprims <tag>`. The tests
use disposable synthetic keys and a local bare remote; they do not sign or
publish an actual release. GitHub responses in those tests are simulated.

For a new specification release, after verifying its newly created signed tag,
create the GitHub release from that tag using `RELEASE_NOTES.md`. This procedure
does not schedule release publication for the existing `spec-v1.0` or `spec-v1.1`
tags. Never create a replacement or lightweight tag through the release UI.
A ruleset matching `v*` does not cover `spec-v*`; the signing controls still apply. Version-tag rules and package-publishing CI are configured with the
first implemented release. There is no tag-triggered package publication here.
