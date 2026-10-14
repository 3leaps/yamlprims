# Specification releases

The specification-only release uses `spec-v<SPEC_VERSION>`, initially
`spec-v1.0`. It publishes the repository documentation and no crates or runtime
artifacts. The proposed first Rust packages are `yamlprims` and `yamlprims-cli`
at `0.1.0`, after their implementation and qualification gates. Native bindings
follow their own milestone. See [the build specification](build-spec.md).

## Signed tags

Release tags are annotated OpenPGP-signed objects. Signing uses the release
identity `3 Leaps Infosec Team <infosec@3leaps.net>` and an exact signing subkey
under the primary fingerprint in [the public anchor](../keys/expected-fingerprints.txt).
The keyring and private signing material remain outside Git repositories.

The maintainer configures these environment variables in their signing terminal:

| Variable                            | Required value                                            |
| ----------------------------------- | --------------------------------------------------------- |
| `YAMLPRIMS_RELEASE_TAG`             | `spec-v1.0`, matching `SPEC_VERSION`                      |
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

After verification, create the GitHub release from the existing tag using
`RELEASE_NOTES.md`. Never create a replacement or lightweight tag through the
release UI. A ruleset matching `v*` does not cover `spec-v*`; the signing controls
still apply. Version-tag rules and package-publishing CI are configured with the
first implemented release. There is no tag-triggered package publication here.
