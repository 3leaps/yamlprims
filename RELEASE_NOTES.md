# spec-v1.1 — tagged draft specification

Revision 1.1 is available at the signed `spec-v1.1` tag. No GitHub release has
been published as of October 14, 2026. The tag contains documentation only.

- `1.1/E-1`: precise block-scalar ends, omitted-child positions and span containment.
- `1.1/E-2`: distinct single-document and empty-stream behavior, including byte APIs
  and CLI, with exact boundaries for consecutive explicit empty documents.
- `1.1/E-3`: boxed Rust errors with borrowed access, owned extraction, explicit
  constructors and a fallible error encoder that revalidates kind/detail pairing.
- `1.1/E-4`: accepted flow-indentation correction for YAML 1.2.2 conformance,
  with an intentionally strict-only initial public API.
- Stronger qualification-driver controls and verdict-change reporting.

The revision 1.0 tag and its profile expectations remain unchanged. Revision 1.1
requires a separately pinned specification/profile. ADR-0003 is accepted for
revision 1.1; the other proposed decisions await maintainer acceptance.
The wire grammar version remains 1; source positions and
document counts are profile-dependent semantic changes, not new JSON fields.

This remains a specification-only project. Documentation validation is not runtime
qualification, and no crate or binding is published with this draft.
This is an updated specification, not a claim that a candidate satisfies every
additional revision 1.1 requirement. Decision acceptance and implementation
qualification remain separate.

## spec-v1.0 — specification-first release

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
