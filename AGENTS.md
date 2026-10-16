# yamlprims contributor agent guide

Read `README.md`, `docs/spec/yamlprims-spec.md`, `docs/build-spec.md`,
`docs/design-review.md` and `docs/decisions/README.md` before changing contracts.
Read local guidance when present.

## Current phase

Rust core baseline under development. Read `docs/implementation.md` and
`conformance/README.md` for the implemented surface, pinned checks and open gates.
Preserve the baseline behavior through structural changes and rerun the same
expectations. Do not add placeholder crates or regenerate expected results to
match implementation output. Proposed decision records remain proposed until
maintainers accept them.

## Public content

Follow the [Sensitive Local Data Policy](https://github.com/3leaps/oss-policies/blob/main/SENSITIVE-LOCAL-DATA.md).
Private inputs, planning, identifiers and operating notes stay outside the working
tree, including ignored paths. Use synthetic examples. Treat commit messages,
branches and PR text as permanent public content.

## Checks

Run `make check`. It validates the documentation release, not YAML behavior.
Run `make check-rust test-rust test-conformance-tools` for runtime changes and
`make prepush` before publication. Run the pinned conformance gate for adapter
changes. Keep test outputs and hydrated corpora outside the working tree.
Keep format semantics, adapter policy, implementation observations and test
expectations distinct. Never change expectations solely to make an implementation
pass. Do not claim tests, platforms or package releases that were not executed.

Use the public attribution footer on commits and PR bodies:

```text
Role: <bare-role>
Co-authored-by: <model> <noreply@3leaps.net>
Committer-of-Record: @3leapsdave
```
