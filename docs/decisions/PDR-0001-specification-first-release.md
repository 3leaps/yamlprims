---
id: PDR-0001
title: Specification-first release and qualification before transfer
status: proposed
date: 2026-10-14
---

# PDR-0001: Specification-first release and qualification before transfer

## Decision

The initial repository release publishes the specification, implementation plan
and proposed decisions. It ships no parser, CLI or binding and makes no runtime
conformance claim. Users can discuss the contract while implementation is being
qualified.

Before production implementation begins, a candidate must exercise a pinned
structcorpus/profile through a real consumer driver, with independently reviewed
expectations and explicit gap reporting. Acceptance of that transfer gate is
separate from full M1 and binding qualification. Repeat the same expectations
after porting or reorganizing code.

Expected values are reviewed independently of the implementation author. Fixing
an implementation cannot redefine its oracle; correcting a specification creates
a new identifiable expectation revision.

## Versions and packages

Use `spec-v1.0` for the initial repository release and revision `1.0` for its
draft specification. The first implemented release can use `v0.1.0`, with Rust
package versions `0.1.0` in their Cargo manifests. Git tags do not determine
crates.io versions. Wire and ABI versions evolve independently.

Package publication follows real implemented surfaces and package validation.
No empty name-reservation crates are required. Crate availability is checked
again at publication; a repository release does not reserve registry names.

## Release evidence

Each release states what is actually included, what was checked and what remains
proposed or unrun. A specification revision is not an assertion that an existing
implementation passes it. Maintainers accept decisions and releases explicitly.
