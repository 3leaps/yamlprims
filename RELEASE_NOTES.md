# spec-v1.0 — specification-first release

The initial yamlprims repository release contains:

- Draft specification revision 1.0 for an owned YAML 1.2 data layer.
- A library-first workspace plan for Rust, a CLI, and later Go/TypeScript bindings.
- Proposed architecture, data, security and release-process decisions.
- Explicit qualification gates and open design items.

This is a documentation release. It contains no parser implementation, installable
CLI, Cargo package, C library or language binding. Documentation checks do not
establish runtime conformance. Crate publication follows implemented surfaces.

Specification, package, wire and ABI versions are separate. Future specification
changes retain identifiable revisions so consumers can pin exact expectations.
