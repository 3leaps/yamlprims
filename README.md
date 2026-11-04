# yamlprims

**A specified YAML data layer for Rust and language bindings. Built in public,
starting with the contract.**

The repository contains the [specification](docs/spec/yamlprims-spec.md),
[build specification](docs/build-spec.md), [decision records](docs/decisions/README.md)
and a working Rust core with an initial [CLI](crates/yamlprims-cli/README.md). No library package, CLI or binding has been
released. See [implementation status](docs/implementation.md) for current coverage
and remaining work.

## What we are building

- **Strict YAML 1.2.2 Core Schema data layer:** Owned values with exact 128-bit integers (`i128::MIN..=u128::MAX`), IEEE 754 `f64` floats, arbitrary mapping keys (`Value::key_eq`), retained non-core tags (`UnknownTags::Preserve` by default or `UnknownTags::Reject`), and strict flow/block indentation ([ADR-0003](docs/decisions/ADR-0003-strict-indentation.md)). YAML 1.1 boolean words (`yes`, `no`, `on`, `off`, `y`, `n`) resolve as plain strings.
- **Explicit duplicate-key, `NaN`-key and merge policies:** Duplicate mapping keys are always rejected (`ErrorKind::DuplicateKey` with `RelatedRole::FirstDefinition`), `NaN` keys are rejected (`ErrorKind::InvalidKey`), and `<<` merge directives default to `MergeKeys::Reject` with explicit opt-in `MergeKeys::Apply` or `MergeKeys::AsKey` ([ADR-0001](docs/decisions/ADR-0001-explicit-merge-policy.md)).
- **Source locations, provenance and closed syntax taxonomy:** Byte/line/column spans, `Via::Alias` and `Via::Merge` provenance, boxed `Error` ownership ([DDR-0002](docs/decisions/DDR-0002-boxed-error-ownership.md)), and a closed 81-code `SyntaxKind` vocabulary shared across generic loading, typed `serde::Deserialize` (`from_str`, `from_str_with`, `from_node`), and canonical `yamlprims-value/1` wire output.
- **Finite resource budgets before materialization:** 12 validated resource limits (`Limits`), including combined source collection depth, bytes-only anchor registration, capacity-before-copy merge admission, and output-byte preflight ([SecDR-0001](docs/decisions/SecDR-0001-resource-and-native-boundaries.md)).
- **Memory-safe `Profile B` implementations and planned bindings ([ADR-0002](docs/decisions/ADR-0002-library-first-workspace.md), [ADR-0004](docs/decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md)):**
  - **Rust (`crates/yamlprims`, `crates/yamlprims-cli`):** `#![forbid(unsafe_code)]` library and thin CLI (`check`, `dump`), backed by safe-Rust `granit-parser =2.0.0`.
  - **Go (`yaml/` in root `go.mod`, `github.com/3leaps/yamlprims/yaml`, planned M1):** Pure-Go twin (`CGO_ENABLED=0`, standard library only, zero `cgo`) locked to the same `yamlprims-value/1` wire contract and `structcorpus` conformance pin.
  - **TypeScript (`bindings/typescript/`, `@3leaps/yamlprims`, planned M2):** Isolated `wasm32-unknown-unknown` WebAssembly module with zero WASM imports and pre-copy linear-memory bounds (no native Node-API addons).
  - **Python (`bindings/python/`, `yamlprims`, planned M2):** Standalone `PyO3/maturin` (`abi3-py310`, `#![forbid(unsafe_code)]` in crate, crossing CPython's C-extension ABI via `PyO3`) binding isolated from the root Cargo workspace.

The Rust implementation uses `granit-parser` as its internal event source; the planned pure-Go twin implements its own standard-library scanner/parser and resolver against the applicable `spec-v1.4` semantics and [ADR-0004](docs/decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md). `yamlprims` owns the value, policy and error contract; it does not expose backend types or promise drop-in compatibility with YAML 1.1 or `unsafe-libyaml`-backed loaders. Current M1/M2 scope covers loading, validation, typed deserialization, canonical `yamlprims-value/1` wire encoding/decoding and plain JSON projection (`encode_plain` / `to_plain`); YAML text emission and the fused M3 iterator/linter remain M3 scope. It performs no includes, environment interpolation or network access.

## Status and participation

Current tagged specification: [**spec-v1.4**](https://github.com/3leaps/yamlprims/tree/spec-v1.4),
a draft contract. [Revision 1.4](docs/spec/yamlprims-spec.md)
defines loader depth, anchor-cache accounting, bounded error precedence and
entry/copy/key admission. Caller-tree guarantees remain deferred.
Earlier specification tags (`spec-v1.0` through `spec-v1.3`) and their file digests remain immutable, including
historical draft-status wording. A specification tag may include in-development
Rust source; it does not imply a crate release, API freeze or conformance of
that source to every requirement.
Existing corpus profiles remain bound to their cited specification revision.
GitHub release publication is separate from specification tagging.
See [the roadmap and acceptance gates](docs/build-spec.md#delivery-gates) and
[open design items](docs/design-review.md).
The [release procedure](docs/releases.md) covers signed specification and code release tags and
package-publication timing.

Feedback on examples, semantic boundaries and acceptance criteria is welcome.
Please state the specification section and expected behavior when proposing a
change. We will qualify the implementation against pinned, independently reviewed
test material, including [structcorpus](https://github.com/3leaps/structcorpus).

```sh
make check    # checks the specification release; no Rust toolchain required
make test-rust  # runs the included core, truth, wire and stack regressions
```

## License

Code and general documentation: [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE) (with [NOTICE](NOTICE)). The normative specification under `docs/spec/` is
[CC0-1.0](docs/spec/LICENSE). Upstream material retains its own notices.
