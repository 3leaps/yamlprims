# spec-v1.3 — unreleased specification draft

- `1.3/E-1`: nested zero-width scalar normalization, normalization floors,
  paired scalar tag anchors and a zero-width collection exception.
- `1.3/E-2`: explicit UTF-8 input scope and original-byte encoding examples.
- `1.3/E-3`: binary64-Value-to-binary32 conversion through both typed paths,
  preserving the separate exact-integer rule.
- `1.3/E-4`: exact keep-chomping EOF values with and without a final line break.
- Worked logical weights and explicit span variants with matching qualification
  requirements.

The signed `spec-v1.2` tag and earlier specification tags remain unchanged.
New nested-anchor behavior requires a separately pinned revision 1.3 profile;
existing profiles keep their individual specification bindings. The wire grammar
version remains 1, and the logical-weight formula is unchanged.

This draft contains documentation only. Specification acceptance, independent
oracle review and implementation qualification remain separate gates. No runtime
or package is published by this draft.

## spec-v1.2 — tagged draft specification

Revision 1.2 is available at the signed `spec-v1.2` tag.

- `1.2/E-1`: owned type descriptions and 1,024-byte UTF-8-safe descriptive bounds.
- `1.2/E-2`: structured error-path preflight, precise rejection kinds/sites,
  bounded descriptive encoding and non-recursive encoding failures.
- `1.2/E-3`: empty-block scalar source anchors with explicit zero-width
  normalization and pinned examples.

The signed `spec-v1.1` tag and its expectations remain unchanged. Revision 1.2
requires new immutable profile pins; no older profile implicitly adopts these
changes. The wire grammar version remains 1. These changes refine validation,
descriptive bounds and source anchors without adding wire members.

This specification publishes no runtime or package and makes no candidate-conformance
claim. Independent expectations and fail-closed qualification tooling remain
required by Q0 before implementation transfer. The remaining design gates retain
their milestone requirements.

## spec-v1.1 — tagged draft specification

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
qualification, and no crate or binding is published with this specification release.
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
