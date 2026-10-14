---
id: DDR-0002
title: Boxed errors with explicit owned detail extraction
status: proposed
date: 2026-10-14
---

# DDR-0002: Boxed errors with explicit owned detail extraction

## Decision

Revision 1.1 proposes an opaque `Error(Box<ErrorDetail>)` and public,
non-exhaustive `ErrorDetail`. Borrowed field access and mutation use
`Deref`/`DerefMut`; `into_detail()` transfers ownership. The Error handle is one
pointer wide, without a layout promise for arbitrary `Result<T, Error>` or a C ABI.

The [specification §7.1](../spec/yamlprims-spec.md#71-types) defines construction,
bounded message formatting, source attribution, trait support and ownership.
The public error encoder borrows the error, validates bounds and returns a Rust
error if the wire envelope cannot fit. Native-boundary failure protocols remain
separate M2 work.

## Migration

Revision 1.0's `Error { kind, .. }` struct patterns are replaced with field
inspection through the handle. Borrow a field with `&error.kind`; consume it with
`error.into_detail().kind`. Direct moves of non-Copy fields such as
`error.message` through Deref are unsupported. Taking owned detail does not
require cloning its strings or vectors.

Mutable detail is caller-controlled data. Error encoders must not trust its
lengths or metadata merely because the original error was crate-created. They
must revalidate the current wire kind/detail pairing against §11.1, with no stale
detail from a previous kind. An inconsistent pair returns a Rust `Internal`
error at `Site::NotSource { last_known: None }` and no JSON. Full
diagnostic allocation bounds and ErrorKind payload completeness remain explicit
freeze items, not consequences of boxing alone.

## Acceptance

Compile positive probes for borrowed fields, mutation, owned extraction, public
construction and custom Serde errors. Compile-negative probes must reject old
struct patterns and non-Copy field moves. Check source attribution and exact
wire fields independently of the allocation representation. Test that bounded
constructor formatting and output-budget rejection occur without an unbounded
intermediate formatted message or recursive error encoding.
Mutate the kind and verify the exact matching wire detail; reject inconsistent
pairings wherever the representation allows constructing them.
