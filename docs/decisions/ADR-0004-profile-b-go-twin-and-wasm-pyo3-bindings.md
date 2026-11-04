---
id: ADR-0004
title: Profile B pure-Go twin and isolated WASM/PyO3 bindings
status: accepted
supersedes_in_part: [ADR-0002, SecDR-0001]
date: 2026-11-04
---

# ADR-0004: Profile B pure-Go twin and isolated WASM/PyO3 bindings

## Decision

Adopt the `Profile B` repository architecture (`.fulmen/repo.yaml`) with locked
parallel Rust (`crates/yamlprims`, `crates/yamlprims-cli`) and pure-Go
(`github.com/3leaps/yamlprims/yaml`) implementations, plus an isolated
`wasm32-unknown-unknown` TypeScript binding (`bindings/typescript/`,
`@3leaps/yamlprims`) and an isolated `PyO3/maturin` (`abi3-py310`) Python
binding (`bindings/python/`, `yamlprims` on PyPI).

Retire the earlier M2 plan for a C ABI crate (`crates/yamlprims-ffi` consumed by
Go via `cgo`) and a native Node-API addon (`crates/yamlprims-node`). The signed
`spec-v1.4` specification file (`docs/spec/yamlprims-spec.md`) remains immutable
at its tagged digest (`97acb274a18fb9d30a15fe05ce856ce4ba2e169c187ef12db8e824a8a925af0b`).
Implementations and documentation claim conformance to the applicable `spec-v1.4`
core and wire contracts together with this record:

| Contract Area | `spec-v1.4` Status | `ADR-0004` Resolution |
| --- | --- | --- |
| Core YAML 1.2.2 semantics, §3–§10 loader/accounting/precedence/coordinates, §11.1 `yamlprims-value/1` wire format | Normative (`spec-v1.4`) | **Unchanged** — both Rust (`crates/yamlprims`) and the pure-Go twin (`github.com/3leaps/yamlprims/yaml`) implement the exact `spec-v1.4` semantics, 12-parameter `Limits` accounting, 5-phase error precedence, and `yamlprims-value/1` wire grammar |
| Closed `SyntaxKind` vocabulary (`spec-v1.4 §7.1`, `§11.1`) | Left open in `spec-v1.4` as an M1 freeze item (`§7.1`, `§11.1` specify only `[a-z0-9_]{1,64}` code shape and `Internal` rejection on ill-shaped codes) | **M1 Freeze** — `ADR-0004` freezes the closed syntax vocabulary at the 81 codes in `SyntaxKind::ALL` (`crates/yamlprims/src/error.rs`); the pure-Go twin (`yaml/`) and `yamlprims-value/1` wire consumers must emit/accept those exact 81 code strings, and `spec-v1.4 §11.1`'s out-of-shape-is-`Internal` rule is satisfied by construction because `ErrorKind::Syntax(SyntaxKind)` carries the closed enum |
| M2 Go delivery (`spec-v1.4 §11.2`, `§11.3`, `§11.5` C ABI + `cgo`) | Proposed / unfrozen in `§11.5` | **Superseded** — `crates/yamlprims-ffi` (C ABI) is retired; Go ships in M1 (`v0.1.0`) as a `Profile B` pure-Go twin (`github.com/3leaps/yamlprims/yaml`, `CGO_ENABLED=0`, standard library only) preserving `§11.2–§11.4` host value, `ToPlain` (`int64`-or-error), and wire round-trip contracts |
| M2 TypeScript delivery (`crates/yamlprims-node` Node-API) | Proposed / unfrozen in [ADR-0002](ADR-0002-library-first-workspace.md) | **Superseded** — `crates/yamlprims-node` is retired; TypeScript (`@3leaps/yamlprims`) ships in M2 (`v0.2.0`) via an isolated `wasm32-unknown-unknown` module (`bindings/typescript/`) over the `yamlprims-value/1` wire bridge with pre-copy memory bounds |
| M2 Python delivery (`bindings/python/` — `yamlprims` on PyPI) | Not specified in `spec-v1.4 §11.2–§11.3` | **New Host Contract** — isolated `PyO3/maturin` (`abi3-py310`) workspace (`bindings/python/`) with an ordered `Node`/`Mapping` host representation preserving distinct `bool`/`int`/`float` keys and an explicit `to_plain()` loss matrix |

All four language surfaces (`rust`, `go`, `typescript`, `python`) remain marked
`planned` in `.fulmen/repo.yaml` (`profile_status: bootstrapping`) until each
surface's implementation, conformance, and differential parity receipts land and
packages are published.

## Surface contracts

At the time this record is accepted, only the Rust workspace (`crates/yamlprims`,
`crates/yamlprims-cli`, unpublished with `publish = false`) is present in the
repository tree. Sections 2–4 below specify the target contracts and acceptance
obligations that the pure-Go twin (`yaml/`), TypeScript WASM binding
(`bindings/typescript/`), and Python binding (`bindings/python/`) must satisfy
when they land and before `profile_status` flips from `bootstrapping` to `locked`
or any binding flips from `planned` to `shipped`.

### 1. Rust core and CLI (`crates/yamlprims`, `crates/yamlprims-cli`)

- `#![forbid(unsafe_code)]` is enforced on `crates/yamlprims` and
  `crates/yamlprims-cli` today and must also be enforced on `bindings/python`
  when that crate lands.
- `crates/yamlprims` uses safe-Rust `granit-parser =2.0.0` (`default-features = false`,
  `std,error_messages`) as an isolated private event source and maps its
  grammar/scanner errors directly to `ErrorKind::Syntax(SyntaxKind)` over the
  closed 81-code `SyntaxKind` enum without `Debug`-name reflection.

### 2. Pure-Go twin (`github.com/3leaps/yamlprims/yaml` in root `go.mod`)

- Must be standard-library-only (`CGO_ENABLED=0`, zero external `go.mod` or
  resolved package-graph dependencies enforced by `scripts/deps-check.sh`) with
  separate `CGO_ENABLED=0` build/test and `CGO_ENABLED=1 -race` test gates.
- `yaml/internal/engine` and root `NOTICE` must record upstream attribution and
  license provenance for scanner/parser logic ported from `granit-parser =2.0.0`
  (MIT / Apache-2.0, including its `saphyr-parser` / `yaml-rust` attribution chain).
- **Canonical 128-bit `Integer` (`i128::MIN..=u128::MAX`) with immutable value
  semantics (refinement of `spec-v1.4 §11.2`):** Rather than exposing a mutable
  `*big.Int` field on `Int` that callers could mutate in place and corrupt
  `Mapping` key hashes, the Go twin must represent integers with an immutable
  value struct storing unexported canonical sign and 128-bit magnitude
  (`hi, lo uint64`, `negative bool`, never `-0`), providing checked constructors
  (`IntegerFromInt64`, `IntegerFromUint64`, `IntegerFromBigInt`) and returning a
  fresh defensive `*big.Int` copy from `BigInt()`.
- **Immutable `Mapping` key invariants:** Ordered `(key Node, value Node)` entries
  with randomized hash index (`KeyEq`) and read-only accessors; never expose a
  mutable backing slice.
- **`ToPlain` (`spec-v1.4 §11.3`) and `EncodePlain`:** `ToPlain(node Node)`
  must convert `Int` to `int64` if it fits, else return a located `Type` loss
  error (never arbitrary `*big.Int`), and reject non-`String` mapping keys and
  `Tagged` values. `EncodePlain` writes plain JSON text bounded by
  `MaxOutputBytes`, rejecting non-finite floats, non-`String` keys, and `Tagged`
  values.
- **Public typed conversion helpers (`spec-v1.4 §8`):** Must expose checked
  conversions covering `bool`, `*bool`, `string`, narrow and wide unsigned
  integers (`uint8`..`uint64`, `u128`), narrow and wide signed integers
  (`int8`..`int64`, `i128`), `float32` (binary64->binary32 rounding with midpoint
  overflow check `FloatOutOfRange`), and `float64`, enforcing `MaxDepth`/`MaxNodes`
  preflight, `ErrorKindType` on non-matching kinds or tagged nodes, and
  `ErrorKindIntegerOutOfRange` on out-of-range `Int` values (including negative
  values for `u128` and values `> i128::MAX` for `i128`).
- **Conformance and parity:** Must be qualified against all 19 manifest rows of
  the pinned `structcorpus` lock (`conformance/pins/pin-c8b7421.json`) and a
  byte-for-byte `make crosscheck` (`Encode`, `EncodeError`, `DecodeWire` ->
  `Encode`, `EncodePlain`) against the Rust core before `profile_status` flips
  from `bootstrapping` to `locked`.

### 3. TypeScript WASM binding (`bindings/typescript/`, `@3leaps/yamlprims`)

- Isolated Cargo workspace compiling `crates/yamlprims` to a single
  `wasm32-unknown-unknown` module with zero WASM host imports and an owned-reserve
  export surface (`options_reserve`, `input_reserve`, `load_wire`,
  `load_all_wire`, `encode_plain`, `output_ptr`, `output_len`, `dispose` / `release`).
- **Pre-copy admission, single-owner execution, and linear-memory ceiling:**
  Configured `Options`/`Limits` are validated before input reservation.
  `options_reserve` and `input_reserve` enforce configured `max_input_bytes` and
  hard WASM ceilings before allocating; any reserve failure clears/invalidates
  `STATE.input`, `STATE.options`, and `STATE.output` so stale state cannot be
  reused. Each instance enforces single-owner, non-reentrant execution.
- **Non-recursive status-2 fallback:** Status `0` indicates a `documents` wire
  envelope (or plain JSON text), status `1` indicates an `error` wire envelope,
  and status `2` indicates that encoding the error envelope itself failed (such
  as `OutputLimitExceeded` on a tiny `max_output_bytes`), returning a static
  fallback without recursively calling `encode_error`.
- **View reacquisition and `panic=abort` trap lifecycle:** The TypeScript host
  wrapper reacquires `Uint8Array` views after any WASM call that may grow linear
  memory, and treats any `WebAssembly.RuntimeError` trap under `panic=abort` as
  poisoning the instance (discarding and recreating a fresh `WebAssembly.Instance`
  without calling `release()` in `finally` on the trapped instance). Negative
  controls must cover absolute memory ceiling, output exhaustion, partial reserve
  failure, and post-failure reuse.
- **Strict wire decoder and O(N) `utf16Offset` index:** Must validate
  `yamlprims-value/1` strictly (rejecting duplicate JSON keys, unknown fields,
  non-canonical integer strings, and malformed float `bits`) and compute
  `utf16Offset` in a single O(N) pass over the valid UTF-8 prefix.

### 4. Python `PyO3/maturin` binding (`bindings/python/`, `yamlprims` on PyPI)

- Standalone Cargo workspace (`abi3-py310`, `#![forbid(unsafe_code)]`,
  `CARGO_TARGET_DIR=target/python`) outside the root Cargo workspace so `pyo3`
  never enters root `Cargo.lock` or the `crates.io` source-archive verification
  closure; crosses CPython's C-extension ABI via `PyO3`.
- **Lossless ordered `Mapping`:** Because Python's built-in `dict` conflates
  `True == 1 == 1.0` and `False == 0 == 0.0`, the lossless `Document`/`Node`
  tree exposes an immutable ordered `Mapping` backed by `(key_node, value_node)`
  pairs and `yamlprims::key_eq` lookup.
- **Explicit Python `to_plain()` loss matrix:** Converts a `Node` to plain
  Python `dict` / `list` / `str` / `int` / `float` / `bool` / `None` and fails
  with `YamlprimsError` (`kind="type"`) on non-`String` mapping keys (preventing
  `bool`/`int`/`float` key collisions) and `Tagged` values; `encode_plain`
  additionally rejects non-finite floats (`NaN`, `±Inf`).

## Context and tradeoffs

Replacing `cgo` + `yamlprims-ffi` with a pure-Go twin (`CGO_ENABLED=0`)
eliminates C ABI pointer/lifetime hazards and keeps `go test ./...` and static Go
builds self-contained, at the cost of maintaining a second implementation locked
by the 61,360-case `structcorpus` pin and `make crosscheck`. Replacing
`yamlprims-node` with a zero-import `wasm32-unknown-unknown` module eliminates
multi-platform native Node-API binary matrices while executing `crates/yamlprims`
directly.

## Acceptance

- `docs/spec/yamlprims-spec.md` remains byte-identical to `spec-v1.4`.
- `scripts/deps-check.sh` enforces leaf dependencies in Cargo and zero external
  Go dependencies (`CGO_ENABLED=0 go list -deps`).
- Rust, Go, TypeScript, and Python pass the pinned `structcorpus` conformance
  suite and differential `yamlprims-value/1` wire parity checks before each
  surface is marked `shipped`.
