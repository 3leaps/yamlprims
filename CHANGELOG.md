# Changelog

All notable changes to the `yamlprims` code and package releases are documented in this file.
Specification-tag release notes are maintained in [RELEASE_NOTES.md](RELEASE_NOTES.md), and per-version code release notes live under `docs/releases/v<VERSION>.md`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and package releases adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] - 2026-11-13

### Added

- `make release-env-check` names every per-release environment value that belongs to another release; `eval "$(make -s release-env-reset)"` clears them. The release checklist starts with both.

### Changed

- `make release-tag` refuses unless `HEAD`, fetched `origin/main` and `YAMLPRIMS_RELEASE_COMMIT` are the same commit, and `make release-push-tag` refuses a local tag naming another commit (`YAMLPRIMS_RELEASE_COMMIT` was documented but not checked).
- `release-verify-published-tag.sh` binds to the recorded ceremony anchor once `YAMLPRIMS_RELEASE_ANCHOR_FILE` names it, refusing a disagreeing exported `YAMLPRIMS_EXPECTED_*` or an anchor for another tag.
- Release guard errors print the compared values and the likely cause.
- The OIDC npm publish verifies with a 120 x 15 s poll budget by default.
- `check-tooling.py --prepush` preflights venv creation; the `make doctor` yamllint floor is 1.29.0 (the pinned CI toolbox).
- RELEASE_CHECKLIST: `release-create-draft` before signing, crate dry run moved out of section 4, crate inputs, npm validation delay, Homebrew tap and Scoop bucket step, release environment variables.

## [0.2.1] - 2026-11-12

### Added

- `yamlprims --version` / `-V` (sole argument) prints `yamlprims <version>` to stdout and exits 0.
- Bound the `structcorpus` revision 5 policy profiles (`yamlprims-core`, `yamlprims-core-stream`, `yamlprims-core-merge-apply`, `yamlprims-core-merge-as-key` r5, `spec-v1.5`) in the Rust and Go conformance drivers and pinned `conformance/pins/pin-b52fcd0.json` against `structcorpus` `b52fcd090d6879a0bd58ec5a5496c6a4de90504e` (`61,366` cases / `90,988` expectations, `20` result rows), with synthetic r5 binding controls in `conformance/scripts/structcorpus_probes_v14.py` (`--require-r5`).
- `make release-verify-remote-tag` prints the `YAMLPRIMS_EXPECTED_TAG_OBJECT` and `YAMLPRIMS_EXPECTED_COMMIT` exports for `make release-record-anchor`.

### Changed

- The workflow digest and permission checkers read and write workflow YAML with the published `yamlprims` v0.2.0 CLI (`scripts/yaml_json.py`, digest-pinned in `config/yamlprims-cli.txt`, installed in CI by `scripts/ci-install-yamlprims.sh`) instead of PyYAML; `scripts/ci-install-pyyaml.sh` and `config/ci-python-requirements.txt` are removed. A mutation the reader refuses now fails the mutation run.
- `scripts/release-verify-registries.py` validates its poll settings and names npm's post-upload validation queue on a final `404`.
- README, CLI README and Python README describe YAML writing and the I-JSON to JCS path, and cite `spec-v1.5`.

### Fixed

- `scripts/release-decernor.sh` no longer depends on `BASH_REMATCH`, so it resolves when sourced from zsh.

## [0.2.0] - 2026-11-10

### Added

- Deterministic 2-space block-first YAML 1.2.2 Core Schema emitter (`spec-v1.5 §10.2`, [ADR-0005](docs/decisions/ADR-0005-data-layer-emitter-and-ijson-projection.md)) with `CompatQuoting::Yaml11Safe` default quoting, `EmitLimits`, and `EmitOptions` across Rust (`emit`, `emit_with`, `emit_all`, `emit_all_with`), pure Go (`Emit`, `EmitWith`, `EmitAll`, `EmitAllWith`, `FromPlain`, `Marshal`, `MarshalWith`), TypeScript WASM (`dump`, `dumpAll`, `fromPlain`, `defaultEmitLimits`, `defaultEmitOptions`), Python `PyO3` (`dump`, `dump_all`, `from_plain`, `EmitLimits`, `EmitOptions`), and the CLI (`yamlprims emit` with `--from-wire`, `--from-plain`, `--compat-quoting`, `--merge`, and `--max-output-bytes`).
- `serde::Serialize` implementations in `crates/yamlprims` for `Value`, `Node`, `Document`, `Mapping`, and `Integer`, plus bounded `to_node`, `to_string`, and `to_string_with` routing arbitrary `T: serde::Serialize` values through `EmitLimits` and the Core emitter.
- Strict six-rule RFC 7493 I-JSON projection (`spec-v1.5 §11.6`) across Rust (`encode_ijson`, `load_ijson`, `load_ijson_with`, `load_ijson_bytes`, `Options::ijson_default`), pure Go (`EncodeIJSON`, `EncodeIJSONWith`, `LoadIJSON`, `LoadIJSONWith`, `LoadIJSONBytes`, `DefaultIJSONOptions`), TypeScript WASM (`loadIjson`), Python `PyO3` (`load_ijson`), and the CLI (`yamlprims dump --ijson`).
- Checked caller-tree constructors (`Document::new`, `Document::with_metadata`, `Tag::try_new`, `Tag::with_spelling`, `Tagged::new`, `Node::into_value` in Rust and `NewSimpleDocument`, `TryNewDocumentWithMetadata`, `TryNewTag`, `TryNewTagWithSpelling`, `TryNewTagged` in Go), symmetric `decode_wire(json: &str, opts: &Options) -> Result<WireEnvelope, Error>` in Rust (`spec-v1.5 §11.1`), and 16 `spec-v1.5` emitter and I-JSON conformance vectors in `conformance/vectors/cases.jsonl` (`176` total vectors verified across all four bindings).

### Changed

- Replaced compiler-generated recursive `Drop`, `Clone`, and `PartialEq` on `Node` and `Value` with explicit heap-worklist implementations and bounded `Debug` formatting to container depth `<= 255`, ensuring caller-constructed trees cannot overflow call stacks on 2 MiB threads (`spec-v1.5 §3`, `§6`).
- Aligned Go `DecodeWire`, Python `decode_wire`, and TypeScript `decodeWire` with the `spec-v1.5 §11.1` and `§11.4` non-Core tag URI syntax, inner-kind, and `max_scalar_bytes` rules, and extended `crates/yamlprims/examples/structcorpus.rs` and `cmd/structcorpus-driver` to verify `load -> emit_all_with -> load` semantic round-trip across all positive value expectations in `structcorpus`.

## [0.1.1] - 2026-11-09

### Added

- Bound `yamlprims-core-depth255` r1 in both the Rust (`crates/yamlprims/examples/structcorpus.rs`) and Go (`cmd/structcorpus-driver`) conformance drivers and pinned `conformance/pins/pin-6b1266e.json` against `structcorpus` `6b1266e16676dc456328eb64c0eed028060845c3` (`20` profile rows, `61,366` cases / `90,988` expectations, plus `51` synthetic revision 1.4 boundary probes across both drivers in `conformance/requalify.sh`).
- Added the six 255-level sequence and alias-expansion ceiling vectors (`limit.max_depth.ceiling_*`) to `conformance/vectors/cases.jsonl` (`160` differential vectors verified across Rust, Go, TypeScript WASM, and Python `PyO3`).

### Changed

- Raised the conformance driver deep-JSON expectation container nesting cap from `512` to `1024` so 255-deep `yaml-value/1` expectations load without hitting container-depth rejection, and updated the paired positive/negative transport boundary controls in `conformance/scripts/structcorpus_probes_v14.py`.
- Added `/dist/` to root `.gitignore`.

### Fixed

- Replaced recursive JSON container parsing in Python `decode_wire` (`bindings/python/python/yamlprims/__init__.py`) with an iterative state machine and unified `_write_node` so `max_depth = 255` YAML containers load and round-trip without hitting Python's recursion limit.

## [0.1.0] - 2026-11-08

### Added

- Safe-Rust `yamlprims` core crate (`crates/yamlprims`, `#![forbid(unsafe_code)]`, MSRV `1.88.0`) and `yamlprims-cli` command (`crates/yamlprims-cli`, `check`, `dump`, `dump --plain`) implementing the `spec-v1.4` YAML 1.2.2 Core Schema data layer (`Value`, `Node`, `Integer`, `Mapping`, `Options`, `Limits`, `Error`, `SyntaxKind`, typed `serde::Deserialize`, canonical `yamlprims-value/1` wire encoding, and plain JSON projection).
- Pure-Go `Profile B` twin (`github.com/3leaps/yamlprims/yaml`, `CGO_ENABLED=0`, standard library only), Go `structcorpus` conformance driver (`cmd/structcorpus-driver`), and automated Rust/Go differential parity gate (`make crosscheck`) ([ADR-0004](docs/decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md)).
- Isolated TypeScript WebAssembly binding (`@3leaps/yamlprims`, `bindings/typescript/`, zero-import `wasm32-unknown-unknown` module with 128 MiB linear-memory ceiling, pre-copy reservation bounds, status-`2` `OutputLimitExceeded` fallback, and 16 MiB high-water instance recycling) and Python `PyO3/maturin` `cp310-abi3` binding (`yamlprims`, `bindings/python/`, `#![forbid(unsafe_code)]`, GIL release and `catch_unwind` panic containment).
- Conformance qualification against `conformance/pins/pin-c8b7421.json` (`structcorpus` `c8b7421b9e2473a9a7f37d04e3acfd5f552a2f99`, 19 profile rows across `spec-v1.0` through `spec-v1.4`, `61,360` corpus cases / `90,982` expectations) and 154 in-repo differential conformance vectors (`conformance/vectors/cases.jsonl`).
- Dual-track signed release, packaging, and provenance verification workflows and scripts (`.github/workflows/release.yml`, `.github/workflows/crates-verify.yml`, `.github/workflows/registry-publish.yml`, `scripts/release-*.sh`, `scripts/release-*.py`, `scripts/check-packages.sh`, `scripts/deps-check.sh`, `scripts/doctor.py`) supporting OpenPGP + Minisign release signing, zero-compile `crates.io` publication, and OIDC Trusted Publishing with PyPI PEP 740 and npm Sigstore provenance verification.

[Unreleased]: https://github.com/3leaps/yamlprims/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/3leaps/yamlprims/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/3leaps/yamlprims/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/3leaps/yamlprims/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/3leaps/yamlprims/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/3leaps/yamlprims/releases/tag/v0.1.0
