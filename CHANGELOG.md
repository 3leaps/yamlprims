# Changelog

All notable changes to the `yamlprims` code and package releases are documented in this file.
Specification-tag release notes are maintained in [RELEASE_NOTES.md](RELEASE_NOTES.md), and per-version code release notes live under `docs/releases/v<VERSION>.md`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and package releases adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Pinned `conformance/pins/pin-d04cd32.json` against the signed `structcorpus` `v0.1.0` release (tag object `d9d37e9d937041cebc41c31380dd02381f34dc23`, commit `d04cd325262717b2fcec1884536c6f72398e3b63`; `61,366` cases / `90,988` expectations, `20` result rows, unchanged from `2f055c1` and `b52fcd0`); it is the `conformance/requalify.sh` default. A pin's optional `corpus_tag` binds the source checkout's exact `refs/tags/<name>` ref to the pinned annotated tag object and commit in `conformance/scripts/structcorpus_gate.py`, checked before anything from the checkout runs, with a moved-tag control in `structcorpus_gate_probes.py`. Git runs in the gate and its probes without inherited `GIT_*` variables and without optional index writes. The probes alter a copy of the checkout with its own git refs and index, refuse to write through a symlink, require the unaltered copy to pass, and require each corruption to fail for its own reason. `pin-2f055c1.json` (the `structcorpus` format-registry layout) and earlier locks are unchanged.
- The `Dependency audit` workflow runs `cargo-deny` on every Cargo manifest and `govulncheck` on the Go module on each push and pull request and weekly; `make deny` and `make vulncheck` run the same checks locally. Public CI also replays the committed fuzz seeds without mutation (`make fuzz-replay`).

### Changed

- The Rust (`crates/yamlprims/examples/structcorpus.rs`) and Go (`cmd/structcorpus-driver`) conformance drivers, `conformance/scripts/structcorpus_gate.py`, the driver and node-selector controls, `scripts/crosscheck.sh` and `conformance/requalify.sh` read the `structcorpus` format registry (`manifests/formats.json`, `formats/v1`): the YAML case manifest, profiles and span conventions come from its `yaml` entry, with the single-format `manifests/` paths when a checkout has no registry. A malformed, dangling or unreadable registry, a missing `yaml` entry or an absolute, `.`/`..` or escaping path fails closed; only a missing `manifests/formats.json` selects the single-format paths, and `scripts/crosscheck.sh` treats a dangling manifest link as present. Only `"format": "yaml"` case records are run; records of other formats are skipped, and a record without `format` is an error for a registry checkout. For a registry checkout the gate also pins `manifests/formats.json`.
- `SECURITY.md` states the supported versions, the private reporting address and response aim, how advisories are published, and that the resource limits are not a time limit.
- The libyaml migration guide gives the correct default limits (`max_input_bytes` 8 MiB, `max_expanded_nodes` 100,000, `max_owned_bytes` 64 MiB) and recommends a caller deadline for untrusted input. The documentation check now compares the guide's limit values with the Rust and Go defaults, and the Go defaults with the Rust defaults.
- Rust, Python and TypeScript plain JSON output (`encode_plain`, `dump --plain`) now spells a float that lies exactly halfway between two shortest decimals with the even final digit, as Go already did (spec revision 1.7 §11.3). The printed digits can change for such ties; the value read back is unchanged.
- `decode_wire` refusals of a malformed wire payload now carry the message `invalid yamlprims wire payload` in Rust and Go (and in `decode_lint_wire` for JSON-level refusals); the specific reason is in `engine_detail` (Go `EngineDetail()`), which is not serialized. Error kinds, limits and options checks are unchanged, and a decoded error envelope keeps its own message.
- Python and TypeScript `decode_wire` / `decodeWire` refuse a malformed wire payload with the same message, and their wire decoding limit errors use the same wording. Their errors gain an optional read-only `engine_detail` (Python) / non-enumerable `engineDetail` (TypeScript) with the bounded reason (at most 4,096 UTF-8 bytes); it is not part of the wire format and not a stable parsing API.
- TypeScript `decodeWire` accepts a document `yaml_version` anywhere in the u32 range, as the other bindings do; the Document constructor and emitter limits (major 1, minor at most 999,999,999) no longer apply while decoding.
- `encode_error` in Rust, Python and TypeScript now refuses, as Go already did, an error whose `configured` limit is 0 or above 2^53 - 1, whose `invalid_len` is above 2^53 - 1, or that carries a path index, location or span value above 2^53 - 1, with an `Internal` error and no output. Previously Rust could write such an envelope, which `decode_wire` then refused.
- Wire decoding limit errors read `wire payload exceeds <limit> (<configured>)` in Go as in Rust, and the Go `Tagged` and `Document` constructors, the TypeScript `createTag`, `createTaggedNode` and `createDocument`, the Python `Document` constructor, and the Python emitter's unsupported-version error use the same messages as the Rust core.
- The error message for a tag URI that is invalid or over the emit tag limit is now worded the same in all four bindings (`Tagged node contains invalid or oversized tag URI`, also on the Rust serde `TaggedValue` path); the error kind and site are unchanged.
- Loading refuses a resolved node tag that is neither a local tag nor a global tag URI with an RFC 3986 scheme (for example `!<$:?>` or a `%TAG` expansion without a scheme), as `Syntax(InvalidGlobalTagCharacter)` at the tag, before construction and the `unknown_tags` policy. Such documents previously loaded with the tag retained, and `emit*` and `decode_wire` then refused them (`spec-v1.7 §4.4`, `§7.1`).
- Loading refuses a verbatim `!<!>` tag the same way. It was previously treated as the non-specific `!` and its tag was dropped; a bare `!` is unchanged (`spec-v1.7 §4.4`).
- A `%TAG` directive whose named handle uses a character outside `[0-9A-Za-z-]` (such as `_`) is refused as `Syntax(ExpectedTagDirectiveBang)` at the directive, used or not, and a tag using such a handle as `Syntax(InvalidTagTerminator)` at the tag. Underscores in primary local tags, suffixes and URIs are unchanged (`spec-v1.7 §4.4`, `§7.1`).
- A `%TAG` directive whose prefix immediately follows its handle is refused as `Syntax(ExpectedWhitespace)` at the first prefix character, whether or not the handle is used; a space or tab separator is required (`spec-v1.7 §7.1`).

### Security

- [GHSA-5v8m-gxw9-c82j](https://github.com/3leaps/yamlprims/security/advisories/GHSA-5v8m-gxw9-c82j) covers the Go block scalar indentation fix released in 0.3.1 (Go module `github.com/3leaps/yamlprims`, versions 0.1.0 through 0.3.0). It names the affected functions, which include the `Unmarshal` and `LintYAML11` families as well as the `Load` functions.

## [0.3.1] - 2026-11-22

The library contract is still `spec-v1.6`; the in-tree specification text is the unreleased revision 1.7 draft, which this release does not implement.

### Fixed

- Pure Go: loading a block scalar whose content is indented 14 or more columns no longer fails to terminate when the input ends inside that indentation (all Go load entrypoints, v0.1.0 through v0.3.0). Six conformance vectors cover the indentation boundary and chomping (`194` in-repo vectors).

### Changed

- The workflow digest and permission checkers run the published `yamlprims` v0.3.0 CLI: `config/yamlprims-cli.txt` pins the v0.3.0 release archives and binary digests, and the minimum version is now 0.3.0.

## [0.3.0] - 2026-11-18

### Added

- Bounded byte-slice, reader, and writer entrypoints across Rust (`load_reader*`, `load_all_reader*`, `from_slice*`, `from_reader*`, `to_writer*`, `emit_writer*`, `emit_all_writer*`) and pure Go (`LoadBytesWith`, `LoadAllBytesWith`, `LoadReader*`, `LoadAllReader*`, `EmitWriter*`, `EmitAllWriter*`, `MarshalWriter*`, and typed `Unmarshal*` with `math/big.Int` support, default unknown-struct-field rejection, iterative finite pointer-chain unwrapping, and pointer-cycle rejection) (`spec-v1.6 §9`, `§10.2`, `§11.2`).
- Single-pass native `PyO3` `safe_load`, `safe_load_all`, and buffered `safe_dump`, `safe_dump_all` in Python (`bindings/python`, enforcing pre-copy chunk byte caps on file-like streams and materializing plain Python values up to 128-bit integers without a JSON round-trip), plus `parse`, `parseAll`, `stringify`, and `stringifyAll` plain-value helpers in TypeScript (`bindings/typescript`) (`spec-v1.6 §11.2`, `§11.5`).
- YAML 1.1 → YAML 1.2.2 Core migration lint (`spec-v1.6 §10.3`, `§11.7`) across Rust (`LintCode`, `LintDiagnostic`, `LintResult`, `lint_yaml11*`, `encode_lint`), pure Go (`LintCode`, `LintDiagnostic`, `LintResult`, `LintYAML11*`, `EncodeLint*`), TypeScript (`LintCode`, `lintYaml11`, `encodeLint`), Python (`LINT_CODES`, `lint_yaml11`, `encode_lint`), and the CLI (`yamlprims check --lint-yaml11 [--format=text|json]`), with `max_warnings` (`1,000`) pre-formatting retention admission, fixed-buffer streaming diagnostic formatting (`1,024` / `4,096` bytes), and zero-clone error-path transfer on budget abort.
- 12 `spec-v1.6` YAML 1.1 migration lint conformance vectors in `conformance/vectors/cases.jsonl` (`188` total in-repo vectors verified byte-identically across Rust, Go, TypeScript, and Python), and the [libyaml migration guide](docs/migrating-from-libyaml.md).

### Changed

- Normatively froze the `yamlprims-cli` command-line contract (`spec-v1.6 §10.4`): `check`, `dump`, and `emit` with global `--merge=reject|apply|as_key`, emit-only `--compat-quoting=yaml11-safe|core-only`, `--max-output-bytes` on `dump`, `emit`, and `check --lint-yaml11 --format=json`, whole-payload buffered `stdout`, and exit codes `0`, `1`, `2`.

## [0.2.2] - 2026-11-13

### Added

- `make release-env-check` names every per-release environment value that belongs to another release; `eval "$(make -s release-env-reset)"` clears them. The release checklist starts with both.

### Changed

- `make release-tag` refuses unless `HEAD`, fetched `origin/main` and `YAMLPRIMS_RELEASE_COMMIT` are the same commit and signs that commit; `make release-push-tag` pushes the verified annotated object by id after checking it names that commit and did not move, then requires the remote to hold it (`YAMLPRIMS_RELEASE_COMMIT` was documented but not checked).
- `release-verify-published-tag.sh` binds to the recorded ceremony anchor once `YAMLPRIMS_RELEASE_ANCHOR_FILE` names it, refusing a disagreeing exported `YAMLPRIMS_EXPECTED_*`, an anchor for another tag, or a configured anchor that is not a regular file; `spec-vX.Y` anchors are accepted.
- Release guard errors print the compared values and the likely cause.
- The OIDC npm publish verifies with a 120 x 15 s poll budget by default; the `publish-npm` job timeout is 45 minutes.
- `check-tooling.py --prepush` preflights venv creation; the `make doctor` yamllint floor is 1.29.0, the Debian bookworm yamllint that the digest-pinned CI toolbox image installs (without a version pin).
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

[Unreleased]: https://github.com/3leaps/yamlprims/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/3leaps/yamlprims/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/3leaps/yamlprims/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/3leaps/yamlprims/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/3leaps/yamlprims/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/3leaps/yamlprims/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/3leaps/yamlprims/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/3leaps/yamlprims/releases/tag/v0.1.0
