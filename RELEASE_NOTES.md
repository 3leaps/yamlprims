# spec-v1.7 — unreleased draft specification

Revision 1.7 tightens loading to the YAML 1.2.2 tag rules. Specification
publication does not claim runtime implementation or qualification.

- `1.7/E-1`: bounded source validation refuses a resolved node tag identity,
  other than the non-specific `!`, that is neither a local tag nor an identity
  starting with an RFC 3986 scheme and `:` (for example `!<$:?>` or a `%TAG`
  expansion without a scheme), as `Syntax(InvalidGlobalTagCharacter)` at the
  tag, before construction and the `unknown_tags` policy. The documented
  meaning of `invalid_global_tag_character` is extended; the 81-code
  vocabulary and wire version 1 are unchanged.
- `1.7/E-2`: a `%TAG` prefix must be separated from its handle by a space or
  tab; otherwise `Syntax(ExpectedWhitespace)` at the first prefix character,
  whether or not the handle is used.

## spec-v1.6 — tagged draft specification

Revision 1.6 specifies proposed `v0.3.0` contracts for bounded stream and
ecosystem-familiar ergonomics across all four `Profile B` bindings, the YAML 1.1
to YAML 1.2.2 Core migration lint and its `yamlprims-lint/1` wire format, and
the frozen `yamlprims-cli` command-line contract. Specification publication
does not claim runtime implementation or qualification; `v0.3.0` implementation
and qualification are tracked separately.

- `1.6/E-1`: zero-friction reader/writer, byte-slice, and plain-host ergonomics
  across Rust (`from_slice`, `from_slice_with`, `from_reader`, `from_reader_with`,
  `load_reader`, `load_reader_with`, `load_all_reader`, `load_all_reader_with`,
  `to_writer`, `to_writer_with`, `emit_writer`, `emit_writer_with`,
  `emit_all_writer`, `emit_all_writer_with`), Go (`LoadReader*`, `LoadAllReader*`,
  `Unmarshal*`, `UnmarshalReader*`, `UnmarshalNode`, `MarshalWriter*`,
  `EmitWriter*`, `EmitAllWriter*`), TypeScript (`parse`, `parseAll`, `stringify`,
  `stringifyAll`), and Python single-pass native `PyO3` (`safe_load`,
  `safe_load_all`, `safe_dump`, `safe_dump_all`).
- `1.6/E-2`: YAML 1.1 to YAML 1.2.2 Core migration lint (`lint_yaml11*` in Rust,
  `LintYAML11*` in Go, `lintYaml11` in TypeScript, `lint_yaml11` in Python, and
  CLI `yamlprims check --lint-yaml11 [--format=text|json]`), structured
  `LintCode` (`yaml11_boolean`, `yaml11_number`, `yaml11_sexagesimal`,
  `yaml11_merge_key`), `LintDiagnostic`, `LintResult`, `Limits::max_warnings`
  (`1,000`) truncation and diagnostic path/string memory accounting, and the
  canonical `yamlprims-lint/1` JSON wire format (`encode_lint` / `EncodeLint*` /
  `encodeLint`).
- `1.6/E-3`: frozen `yamlprims-cli` command-line contract (§10.4) covering
  `check [--lint-yaml11] [--format=text|json]`, `dump [--plain|--ijson]`,
  `emit [--from-wire|--from-plain]`, `--version`, `--help`, `-` (`stdin`), `--`,
  single non-empty document requirement for `dump --plain`, bounded `max_input_bytes + 1`
  reads, whole-payload buffered `stdout`, and exit codes `0`, `1`, and `2`.
- `1.6/E-4`: specification milestone, stack-safety, and decision-record alignment
  for the `v0.3.0` (`spec-v1.6`) documentation targets.

The signed `spec-v1.5` tag and earlier tags (`spec-v1.0` through `spec-v1.4`)
remain immutable. The `yamlprims-value/1` wire grammar remains version 1,
`§11.7` introduces the separate `yamlprims-lint/1` lint wire grammar, and the
§6 subtree-weight formula is unchanged. Code releases use `v<VERSION>` tags and
per-version release notes under `docs/releases/v<VERSION>.md`.

## spec-v1.5 — tagged draft specification

Revision 1.5 is available at the signed `spec-v1.5` tag.

Revision 1.5 specifies the data-layer YAML 1.2.2 Core emitter, caller-constructed
tree bounds, symmetric wire decoding, and strict RFC 7493 I-JSON projection
recorded in [ADR-0005](docs/decisions/ADR-0005-data-layer-emitter-and-ijson-projection.md),
alongside the `Profile B` multi-language architecture accepted under
[ADR-0004](docs/decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md).

- `1.5/E-1`: checked caller-tree constructors (`Document::new`,
  `Document::with_metadata`, `Tag::try_new`, `Tag::with_spelling`, `Tagged::new`),
  iterative heap-stack `Drop`, `Clone`, `PartialEq`, and depth-bounded `Debug`
  for `Node`, `Value`, `Mapping`, `Tagged`, and `Document`, and symmetric
  `decode_wire` over `yamlprims-value/1`.
- `1.5/E-2`: deterministic 2-space block-first YAML 1.2.2 Core emitter (`emit`,
  `emit_all`, `EmitOptions`, `EmitLimits`), `CompatQuoting::Yaml11Safe` default
  scalar quoting, `serde::Serialize` (`to_node`, `to_string`, `to_string_with`),
  CLI `yamlprims emit`, and byte-for-byte `Profile B` emitter parity across Rust,
  Go, TypeScript WASM, and Python `PyO3`.
- `1.5/E-3`: strict RFC 7493 I-JSON projection (`encode_ijson`, `load_ijson`,
  `load_ijson_with`, `load_ijson_bytes`, CLI `yamlprims dump --ijson`, and host
  `EncodeIJSON*` / `LoadIJSON*` / `loadIjson` / `load_ijson`) enforcing
  single-document cardinality, `±(2^53 − 1)` safe integers, finite floats (`-0.0`
  as `0`), `String`-only mapping keys, `UnknownTags::Reject`, and rejection of
  all 66 Unicode noncharacters.
- `1.5/E-4`: alignment of §1, §2, §11.2, and §11.5 with `ADR-0004` (`Profile B`:
  pure-Go twin `github.com/3leaps/yamlprims/yaml`, zero-import
  `wasm32-unknown-unknown` TypeScript package `@3leaps/yamlprims`, and `PyO3`
  `abi3-py310` Python package `yamlprims`).

The signed `spec-v1.4` tag and earlier tags (`spec-v1.0` through `spec-v1.3`)
remain immutable. The `yamlprims-value/1` wire grammar remains version 1, and
the §6 subtree-weight formula is unchanged. Code releases use `v<VERSION>` tags
and per-version release notes under `docs/releases/v<VERSION>.md`.

## spec-v1.4 — tagged draft specification

Revision 1.4 is available at the signed `spec-v1.4` tag.

- `1.4/E-1`: source collection levels, completed-subtree height and alias
  placement, including final post-merge metrics and preserved source charges.
- `1.4/E-2`: bytes-only anchor-cache registration charges, nested/redefined
  anchors and whole-call counter scope.
- `1.4/E-3`: bounded source-validation precedence, ordinary and merged entry
  admission, copy-limit priority and completed-key weight timing; terminal engine
  exceptions and native diagnostic sites, including unmatched opener locations.
- `1.4/E-4`: the existing error-key nesting metric named separately from loader
  collection height, without changing encoder preflight expectations.

New caller-tree APIs and general stack-safety guarantees are deferred. CLI freeze
and the other open M1 items remain separate.

The signed `spec-v1.4` tag and earlier tags (`spec-v1.0` through `spec-v1.3`) remain immutable. New expectations
require their own specification/profile pins; historical depth observations and
event-only ladders are not promoted to normative load-boundary expectations.
The wire grammar remains version 1. The subtree-weight formula and String-only
scalar text charge are unchanged; correcting implementations of that formula
does not require new semantics.

The source tree includes a Rust core and CLI. A specification
tag identifies that source snapshot but does not publish a package or claim
runtime conformance. Code releases use `v<VERSION>` tags and per-version release
notes under `docs/releases/v<VERSION>.md`. See [implementation status](docs/implementation.md) for
current status. Specification acceptance, independent oracle review and qualification
of the actual implementation remain separate gates.

## spec-v1.3 — tagged draft specification

Revision 1.3 is available at the signed `spec-v1.3` tag.

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

That specification release contains documentation only. Specification acceptance, independent
oracle review and implementation qualification remain separate gates. No runtime
or package was published by that tag.

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
