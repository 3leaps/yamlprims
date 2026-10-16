# Contributing

This first release is specification-first. Useful contributions include concrete
semantic examples, API questions, independent expected results and improvements
to the build plan.

Start with the [specification](docs/spec/yamlprims-spec.md) and
[open design items](docs/design-review.md). Cite the affected section and explain
the expected behavior. Proposed changes need maintainer review; publishing a
draft does not freeze every interface.

Use public synthetic inputs or redistributable upstream material with provenance.
Do not submit private configuration, secrets or redacted production examples.
Follow the [Sensitive Local Data Policy](https://github.com/3leaps/oss-policies/blob/main/SENSITIVE-LOCAL-DATA.md).

Run `make check` before submitting. This checks documentation integrity only.
The checker requires `SPEC_VERSION`, the normative revision heading and the
first release-note heading to agree. Historical release sections are retained.
Run `make test-docs` for regression tests using temporary synthetic documents.
CI runs documentation checks and these tests on Linux x64/ARM64, macOS ARM64 and Windows
x64/ARM64. Fork pull requests run the three standard runner cells; the two
custom ARM64 cells run for same-repository changes. A separate Linux cell tests
the minimum supported Python 3.9. Workflow lint and synthetic
signed-tag controls run on Linux. These checks do not establish parser support
or runtime conformance. The signing-control test requires Unix and uses only
disposable keys. Run it locally with `make test-release-tag-controls`.

The documentation check requires Python 3.9+ and Make; select another Python
executable with `make check PYTHON=python`. Run `make lint-workflows` after workflow edits.
For release-script changes, run `make lint-shell` (Bash and ShellCheck required)
and `make test-release-tag-controls`. CI runs ShellCheck 0.11.0 before the
synthetic signing tests. YAML configuration uses two-space indentation, enforced by `.yamlfmt` and
`.yamllint` and mirrored in `.goneat/assess.yaml`. Run `make fmt-yaml` to format
configuration and `make lint-yaml` to check it (yamlfmt and yamllint required).
These commands cover root YAML tool configuration, `.github` and `.goneat`.
`make lint` combines documentation, shell and YAML checks;
`make check` remains independent of shell-lint tools.

Run `make prepush` before pushing. This runs the pinned-tool check, documentation
validation and regression tests, shell/YAML/workflow lint, then the synthetic
signed-tag controls. Full prepush requires Linux or macOS: the signing tests use
Python's Unix-only `pty` module. Windows contributors can run `make check test-docs`
and the individual lint targets; full prepush still needs a Unix host or CI.

Install Python 3.9+, Make, Bash, Git, GnuPG, actionlint, ShellCheck, yamlfmt,
yamllint and **goneat v0.6.4** before full prepush. `.goneat-version` records the
exact goneat version; `make check-tooling` rejects other versions. Obtain goneat
from its [release](https://github.com/fulmenhq/goneat/releases/tag/v0.6.4).
The workflow-lint container is the digest-pinned fulmen-toolbox **v0.5.10**
`goneat-tools-runner-glibc` image and verifies this same goneat pin.
These targets do not install or upgrade tools. Executable paths can be selected
with `PYTHON`, `ACTIONLINT`, `GONEAT`, `SHELLCHECK`, `YAMLFMT` and `YAMLLINT` Make
variables. `make check` and `make test-docs` do not require goneat or lint tools.

Implementation contributions begin after the qualification gate described in
the [build specification](docs/build-spec.md).

Code and general documentation contributions use MIT OR Apache-2.0; normative
specification text under `docs/spec/` uses CC0-1.0. Preserve upstream notices.
AI-assisted work is welcome under human supervision and the attribution rules
in [AGENTS.md](AGENTS.md).
