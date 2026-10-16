---
id: ADR-0001
title: Explicit merge policy
status: accepted
date: 2026-10-14
accepted: 2026-10-16
---

# ADR-0001: Explicit merge policy

## Context

Merge keys are an extension rather than part of YAML 1.2 Core. Implicitly applying
them changes data, while silently discarding them can hide a configuration error.
Quoted `<<` and explicitly string-tagged `<<` are ordinary keys.

## Decision

Default to `MergeKeys::Reject`. A recognized directive raises a located
`MergeRejected`. Callers may explicitly choose `Apply` or `AsKey`.

The builder recognizes merge directives before scalar resolution. Apply uses
explicit-key precedence, then earlier-source precedence, with deterministic
entry order. Each unshadowed merged-in entry is a charged logical copy even if
the implementation moves storage. The normative rules are
[specification §5.3](../spec/yamlprims-spec.md#53-merge-keys).

## Consequences and acceptance

Migration from a permissive library may require an explicit option. Generic,
typed, CLI and binding paths must use the same policy. Implementation qualification
requires every recognition row, source shape, precedence and ordering case, plus
inline/aliased and nested-merge accounting controls. An implementation convenience cannot alter
the chosen default or omit logical-copy charges.

Accepted on 2026-10-16 for the merge policy already specified in revision 1.3,
including the initial Rust core. This records the policy decision without
changing the specification or accepting a later revision's depth contract.

The initial core's detecting checks are
the merge recognition, ordering and expansion-budget regressions in
`crates/yamlprims/tests/contract.rs`, the merge wire fixtures and the pinned
`yamlprims-core`/merge-profile rows documented in `conformance/README.md`.
This decision does not close the remaining M1 contract or release gates.
