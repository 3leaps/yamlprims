---
id: ADR-0002
title: Library-first workspace with an internal event engine
status: proposed
date: 2026-10-14
---

# ADR-0002: Library-first workspace with an internal event engine

## Decision

One Rust core owns values, policies, resource accounting, errors, Serde conversion
and wire encoding. Use `granit-parser` as an isolated event source; backend types
remain private. Engine/version changes require requalification.

Start with `yamlprims` and `yamlprims-cli`. Add `yamlprims-ffi` and
`yamlprims-node` for actual native-boundary work. The CLI is a consumer of the
library. Go consumes the C ABI; TypeScript consumes Node-API. There is no second
resolver in either language. The [build specification](../build-spec.md) defines
the package graph and proposed platform matrix.

## Context and tradeoffs

Owning semantic behavior lets consumers depend on a stable contract without
requiring a new scanner/parser engine. Engine risk remains and must be measured.
A safe core does not establish the safety of generated native glue.

Keep model and wire modules in the core until a real independent consumer needs
another crate. Extra package boundaries impose versioning, release-order and
public-API obligations; modules already provide implementation separation.

## Acceptance

No backend re-exports, no I/O in core entrypoints, and all shipped paths exercise
the same resolver/builder. Check actual selected feature/dependency closures and
preserve licenses. The docs-only release contains the plan, not empty packages.
