# Contributing

The released artifacts are specifications; a Rust core baseline is under development.
Useful contributions include concrete
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
the minimum supported Python 3.11. Workflow lint and synthetic
signed-tag controls run on Linux. These checks do not establish parser support
or runtime conformance. The signing-control test requires Unix and uses only
disposable keys. Run it locally with `make test-release-tag-controls`.

The documentation check requires Python 3.11+ (standard library only; no third-party
packages) and Make; select another Python executable with `make check PYTHON=python3.11`
(for example on macOS hosts where `/usr/bin/python3` from Xcode Command Line Tools is
Python 3.9). Install Python 3.11+ via a system package manager (`brew`, `apt`, `dnf`,
`winget`, `scoop`), `uv`, `pyenv` or python.org. Run `make lint-workflows` after workflow edits.
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

Install Python 3.11+, Make, Bash, Git, GnuPG, actionlint, ShellCheck, yamlfmt,
yamllint and **goneat v0.6.4** (or newer) before full prepush. Repository scripts
use only the Python 3.11+ standard library. The release-workflow checkers
(`make workflow-pin`, `make prepush` and `make release-tooling-test`) read YAML
through a published yamlprims release CLI, never this working tree's build: set
`YAMLPRIMS_YAML_BIN` to the absolute path of the binary from a release archive
listed in `config/yamlprims-cli.txt`. `scripts/yaml_json.py` runs it only if its
SHA-256 matches the committed digest for your platform. `.goneat-version` records the
minimum goneat version; `make check-tooling` rejects older versions. Obtain goneat
from its [release](https://github.com/fulmenhq/goneat/releases/tag/v0.6.4).
The workflow-lint container is the digest-pinned fulmen-toolbox **v0.5.10**
`goneat-tools-runner-glibc` image and verifies this same goneat pin.
These targets do not install or upgrade tools automatically. Executable paths can be selected
with `PYTHON`, `ACTIONLINT`, `GONEAT`, `SHELLCHECK`, `YAMLFMT` and `YAMLLINT` Make
variables. `make check` and `make test-docs` do not require goneat, the yamlprims release CLI or lint tools.

Full prepush also checks Rust formatting, all targets, the core tests and
qualification-script failure propagation. Install a Rust toolchain meeting the
`rust-version = "1.88.0"` MSRV in `Cargo.toml`; MSRV validation uses Rust 1.88.0
separately. Run `make check-rust test-rust test-conformance-tools` for runtime
changes. The truth, wire and allocation-instrumented suites run serially. Golden
blessing is disabled. Package-local fixtures are included in extracted-package tests.

The Rust baseline workflow exercises Linux on Rust 1.88.0 and 1.89.0 in debug
and release. It does not replace the full five-target qualification plan.
Adapter changes also need the exact pinned [conformance checks](conformance/README.md).
See [implementation status](docs/implementation.md) and the
[build specification](docs/build-spec.md) for remaining contract and release gates.

Code and general documentation contributions use MIT OR Apache-2.0; normative
specification text under `docs/spec/` uses CC0-1.0. Preserve upstream notices.
AI-assisted work is welcome under human supervision and the attribution rules
in [AGENTS.md](AGENTS.md).
