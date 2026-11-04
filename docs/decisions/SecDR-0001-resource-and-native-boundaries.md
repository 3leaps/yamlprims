---
id: SecDR-0001
title: Resource accounting and native boundary controls
status: proposed
superseded_in_part_by: ADR-0004
date: 2026-10-14
---

# SecDR-0001: Resource accounting and native boundary controls

> **Note:** The C ABI (`yamlprims-ffi`) and Node-API (`yamlprims-node`) boundary
> mechanisms in this record are superseded by accepted
> [ADR-0004](ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md) (`Profile B`
> pure-Go twin `CGO_ENABLED=0`, isolated `wasm32-unknown-unknown` TypeScript WASM
> bridge with pre-copy `options_reserve` / `input_reserve` linear-memory bounds,
> and isolated `PyO3/maturin` `abi3-py310` Python workspace). All resource
> accounting, zero-I/O, and sensitive-local-data requirements remain in effect.

## Decision

Core and CLI Rust code forbid unsafe code. Native FFI and generated Node-API glue
are isolated boundaries with explicit pointer, ownership, failure and concurrency
contracts. Audit their complete selected dependency closures.

Every input has finite validated limits. Check input size before UTF-8 processing;
charge alias and merge copies before materializing them; use checked arithmetic.
Bound diagnostics and output separately. Do not interpret a logical byte budget
as a memory ceiling or `catch_unwind` as protection from invalid pointers or
allocator aborts.

Core operations do not read files, expand environment variables, fetch includes
or access the network. CLI and binding I/O remains explicit. Hostile qualification
inputs run under verified external process limits where required; a killed
process is not a passing parser-limit error.

## Data and public fixtures

Conform to the [3 Leaps Sensitive Local Data Policy](https://github.com/3leaps/oss-policies/blob/main/SENSITIVE-LOCAL-DATA.md).
Private inputs and operating notes remain outside the repository, including
ignored paths. Public regressions are independently constructed synthetic cases
or reviewed redistributable upstream material. Never export renamed production
configuration as a public fixture.

## Acceptance

Require decisive limit-boundary and disabled-guard controls, independent native
ownership tests, selected-closure advisory/license review, and evidence of actual
executed platforms. Unmeasured overhead and skipped hazardous runs stay visible.
