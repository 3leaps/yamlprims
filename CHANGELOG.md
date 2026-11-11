# Changelog

All notable changes to the `yamlprims` code and package releases are documented in this file.
Specification-tag release notes are maintained in [RELEASE_NOTES.md](RELEASE_NOTES.md), and per-version code release notes live under `docs/releases/v<VERSION>.md`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and package releases adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- Conformance qualification against `conformance/pins/pin-c8b7421.json` (`structcorpus` `c8b7421b9e2473a9a7f37d04e3acfd5f552a2f99`, 19 profile rows across `spec-v1.0` through `spec-v1.4`, `61,360` cases / `90,982` expectations) and 154 in-repo differential conformance vectors (`conformance/vectors/cases.jsonl`).
- Dual-track signed release, packaging, and provenance verification workflows and scripts (`.github/workflows/release.yml`, `.github/workflows/crates-verify.yml`, `.github/workflows/registry-publish.yml`, `scripts/release-*.sh`, `scripts/release-*.py`, `scripts/check-packages.sh`, `scripts/deps-check.sh`, `scripts/doctor.py`) supporting OpenPGP + Minisign release signing, zero-compile `crates.io` publication, and OIDC Trusted Publishing with PyPI PEP 740 and npm Sigstore provenance verification.

[Unreleased]: https://github.com/3leaps/yamlprims/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/3leaps/yamlprims/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/3leaps/yamlprims/releases/tag/v0.1.0
