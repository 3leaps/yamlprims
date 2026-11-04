# Open design items before implementation freeze

Status: open design work. Specification revision 1.4 is tagged at `spec-v1.4`;
revision 1.3 and earlier revisions remain available at their immutable tags.
Neither specification text
nor specification publication establishes implementation qualification.

## M1 contract freeze

### Selected in revision 1.1

ADR-0001 accepts the merge default and ADR-0003 accepts strict-only parsing;
other decision records remain proposed. Implementation qualification and a new immutable
profile are still required; revision 1.0 expectations stay bound to revision 1.0.

| Contract           | Normative resolution                                                                                                                                                  | Detecting evidence still required                                                                                                                                                                        |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source spans       | §7.2 fixes block-scalar ends and omitted-child positions; containment exceptions apply only at insertion boundaries.                                                  | Exact start/end coordinates and unchanged values across chomping, blank lines, trailing spaces, explicit empty keys/values, BOM, CRLF/lone CR and multibyte text; check inside alias/merge subtrees too. |
| Empty streams      | §5.5 distinguishes zero-document streams from explicit empty documents; §10 adds `load_all_bytes` and CLI stream behavior.                                            | String/byte cardinality and EOF/boundary positions, explicit document metadata, CLI empty success envelope and all-or-nothing failure.                                                                   |
| Error ownership    | §7.1 selects boxed Error with public non-exhaustive ErrorDetail, Deref/DerefMut, owned extraction and bounded constructors; §10 specifies the fallible error encoder. | Positive borrowed/owned/custom-error compile probes, negative struct-pattern and field-move probes, pointer-sized Error assertion, bounded formatting and too-small encoder budget controls.             |
| Strict indentation | §9 and accepted ADR-0003 require a strict-only public API and `true` in every parsing pass.                                                                           | Compare both settings on an identical pinned suite and migration inventory; malformed indentation must fail as syntax, not an unrelated error.                                                           |

### Selected in tagged revision 1.2

- **Owned descriptions (`1.2/E-1`).** `Type` uses owned `String` fields;
  descriptive fields have a 1,024-byte UTF-8-safe bound. Compile dynamic Serde
  descriptions and test multibyte boundaries at construction and encoding.
- **Encoder preflight (`1.2/E-2`).** Revalidate path length, each key's logical
  weight and collection nesting before writing; preserve structured identity
  or reject with the specified limit and non-source site. Test changed options,
  caller mutation, root weight at 65/66 bytes, direct bounded escaping, unchanged
  borrowed fields and output length at/one byte below the boundary. Instrument
  early rejection and absence of key clones or unbounded temporaries.
- **Empty-block anchors (`1.2/E-3`).** Pin the engine-derived examples in §7.2,
  zero-width normalization floors, values and containment. Independently test
  BOM, line breaks, tags/anchors and sibling-versus-EOF behavior. An event-span
  convention starting at an indicator is not an exact node-span oracle.

### Selected in tagged revision 1.3

- **Nested scalar anchors (`1.3/E-1`).** Normalize zero-width omitted/empty
  scalar spans over current-line ASCII-space indentation and preceding breaks,
  preserving every floor. Scalar tag wrappers and their inner nodes move
  together. Zero-width collections, including tagged collections, stay in place.
  Pin the §7.2 examples and line-break/BOM variants; test recursive containment
  and a mutation removing only the tagged-collection guard.
- **UTF-8 input scope (`1.3/E-2`).** No UTF-16/32 detection or transcoding. Test
  invalid UTF-8 at bytes 0 and 2 and valid UTF-8 containing raw NUL at byte 1;
  retain option validation and raw-size gates. This clarification does not
  resolve the separate general error-precedence item below.
- **f32 conversion (`1.3/E-3`).** Convert binary64 Value to binary32 through
  both typed paths; test the explicit double-rounding example and retain the
  separate Int exactness rule.
- **EOF keep chomping (`1.3/E-4`).** Pin the three exact §7.2 decoded-value
  examples, with grammar-derived expectations distinguishing a final physical
  break from trailing spaces at EOF. Do not generalize a suite row to other
  inputs or use parser agreement as acceptance of an expectation.
- **Logical weights.** Pin the 133/192/218 worked examples without changing
  the formula or asserting a physical-memory bound.

These selections do not close the remaining gates below. A syntax code's shape
check is not acceptance of a closed vocabulary; a successful corpus comparison
does not promote provisional expectations. Before Q0 closes, require a
fail-closed driver with exact inventory/profile-revision validation and the
negative controls in the build specification. Existing test results remain
bound to their original source, specification and profile pins.

### Proposed in revision 1.4

| Contract | Proposed resolution | Review and detecting evidence required |
| --- | --- | --- |
| Loader depth (`1.4/E-1`) | Collection levels, root position p=0, scalar H=0, empty collection H=1, transparent tags; final post-merge metrics and p+H alias admission | Independent source/alias/mixed/key/tag/merge examples, 127/128/129 and 254/255/256 boundaries, stale-metric and pre-copy controls |
| Anchor accounting (`1.4/E-2`) | Full bytes-only registration charge; no extra cache nodes; no refunds; nested/redefined bindings and whole-call counters | Independently derived totals, registration-only boundaries, wrapper charges, redefinition/document reset, final metrics versus cumulative work |
| Precedence and admission (`1.4/E-3`) | Options/size/UTF-8 before bounded source validation in yielded event/error order; terminal engine exceptions and native diagnostic sites; then ordered construction and unchanged local admission rules | Independent review of changed paired-failure expectations, native opener sites, finite lookahead headroom, exact configured values and copy/insertion/path-clone controls |
| Error-key metric (`1.4/E-4`) | Name existing child-edge nesting P separately from loader H | Preserve prior key preflight goldens and path-segment limits; include empty/nonempty collection distinctions |

The String-only scalar-text formula is unchanged; correcting an implementation
of that formula is an existing-contract repair, not a new 1.4 policy. These
proposals leave the general caller-tree API/support domain explicitly deferred.
No new constructors, foreign-Serde budget seed, recursive Debug bound or
all-finite-trees stack-safety promise is accepted by this draft.

### Remaining freeze and qualification items

The unmodified-engine proposal leaves the source-level boundedness audit and
independent review of changed expectations open. Application migration coverage
must compare inputs, loaded values and typed conversions, explicitly covering
schema, strict indentation, duplicates and merge policies as described in the
build specification. Diagnostic differences alone do not qualify compatibility.

| Item                       | Required resolution and detecting evidence                                                                                                                                                                                                                                                                        |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Error precedence           | Review and accept the 1.4 proposal; qualify engine error translation, combined source depth and bounded syntax precedence against earlier policy/materialization failures. A source safety stop must not be relabeled as malformed YAML or bypassed to seek later syntax. |
| Accounting scope           | Review and accept the 1.4 cache/depth proposal; qualify registration, final post-merge metrics and monotonic source/copy charges independently. Existing green corpus results do not establish the new boundaries. |
| Error storage and encoding | Complete the remaining ErrorKind payloads and bound `engine_detail`, related spans and structured paths before constructing large errors. Revision 1.2 specifies owned Type descriptions and encoder revalidation; construction-time allocation bounds and detecting evidence remain required. |
| Public operations          | Complete the plain encoder and checked constructors/accessors needed to create Document/Tagged/Tag values without exposing invariant-bearing fields. Compile the selected Error constructors, error encoder and `load_all_bytes` API in the candidate driver; illustrative signatures alone are not API evidence. |
| Syntax error vocabulary    | `ErrorKind::Syntax(SyntaxKind)` (`SyntaxKind::ALL`, 81 codes) is defined in `crates/yamlprims/src/error.rs` and mapped explicitly without backend `Debug`-name reflection in `crates/yamlprims/src/engine/mod.rs`; keep the pure-Go twin (`yaml/`) and `yamlprims-value/1` wire consumers locked to this closed set. |
| Wire decoding              | Specify integer range, location/order validation, duplicate JSON members, unknown fields, canonical NaN bits and whether noncanonical JSON is rejected or normalized. Byte-identical round trip applies to canonical producer output.                                                                             |
| Metadata integer ranges    | Rust limits use usize and wire metadata uses at most 2^53−1. Define rejection for out-of-range options/coordinates, including 32-bit hosts; never truncate.                                                                                                                                                       |
| Synthetic trees            | Clarify the guarantee for arbitrarily deep hand-built trees and foreign deserializers. Private invariant fields alone do not bound recursion, destructor stack use or caller allocations.                                                                                                                         |
| Local-tag enum conversion  | Pin raw-versus-resolved payload and style metadata through `from_node`; wire round trips must not infer source style that the wire does not carry.                                                                                                                                                                |
| Origin composition         | Specify how nested aliases and merges retain useful provenance when `Via` has only one slot; include descendant budget and conversion errors.                                                                                                                                                                     |

The reviewed tag formula counts `uri`, `handle` and `suffix` once, on the retained
wrapper. For `!long {}`, an empty mapping weighs 64 and its wrapper weighs 74:
total 138 bytes of logical weight and two nodes. This arithmetic is a semantic
golden, not a measurement of heap usage.

The sequence-span convention distinguishes ordinary and indentless block
sequences. A corpus profile must bind that convention explicitly; a broad
"first content token" assertion is not sufficient.

The qualification record must identify the implementing profile and detecting
test for each selected contract before closing its evidence gate. No new profile
pin is asserted by this draft. Profiles citing `spec-v1.0`, `spec-v1.1` or
`spec-v1.2` must not be silently relabeled to cover revision 1.3. Preserve the
1.2 anchor inventory independently from new 1.3 normalization expectations.

The revision 1.1 corpus migration uses `yamlprims-core` r2 and `scalar-alias` r2,
with immutable pins as described in the build specification. Qualify the
consecutive explicit-empty document coordinates in §5.5 and the mutated error
kind/detail validation and rejection in §11.1. The strict-indentation review must
report every suite ID whose verdict changes, keeping format-level syntax truth
separate from revision 1.0's leniency differences.

Revision 1.2 needs separately pinned profiles and matching operation tests.
Error-path collection nesting is defined for encoder preflight in §11.1;
revision 1.4 names that metric P and proposes separate loader height H.
Review/qualification of the loader rules and arbitrary synthetic-tree stack
safety remain distinct gates. Specification acceptance does not establish those
runtime guarantees.

Revision 1.3 additionally needs profile-bound node selectors and matching
schema/validator/driver support, separate from event locations. Limit-error
configured values need a schema extension; input-size errors already use the
corpus `input` phase. Test that mapping independently from other load limits.
Keep stream, output and M3-warning operations distinct. Fixed-message error
operation goldens require an operation schema/executor or API tests, rather than
freezing loader prose. Independent oracle promotion and a fail-closed run of
the exact new inventory remain separate qualification requirements.

## M2 boundary freeze

Per [ADR-0002](decisions/ADR-0002-library-first-workspace.md),
[ADR-0004](decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md) and
[SecDR-0001](decisions/SecDR-0001-resource-and-native-boundaries.md), `yamlprims`
replaces the earlier C ABI (`crates/yamlprims-ffi` + `cgo`) and native Node-API
(`crates/yamlprims-node`) plan with a `Profile B` pure-Go twin (`yaml/` in root
`go.mod`, `CGO_ENABLED=0`, standard library only), a zero-import
`wasm32-unknown-unknown` TypeScript binding (`bindings/typescript/`,
`@3leaps/yamlprims`), and a standalone `PyO3/maturin` (`abi3-py310`,
`#![forbid(unsafe_code)]`) Python binding (`bindings/python/`, `yamlprims`):

- Enforce `CGO_ENABLED=0` and zero third-party Go module dependencies in `yaml/`
  (`scripts/deps-check.sh`), locking the M1 pure-Go twin to the 19-row `structcorpus`
  conformance pin and differential `make crosscheck` against the Rust core.
- Define owned-reserve linear-memory buffer admission (`options_reserve`,
  `input_reserve`, `load_wire`, `load_all_wire`, `encode_plain`, `output_ptr`,
  `output_len`, `dispose` / `release`), pre-copy option/limit validation,
  reserve-failure state invalidation (`STATE.input`, `STATE.options`,
  `STATE.output`), single-owner non-reentrant execution, `panic=abort`
  trapped-instance replacement, and zero WASM host imports for the
  `wasm32-unknown-unknown` module in `bindings/typescript/`.
- Resolve output-budget exhaustion (`max_output_bytes`) separately from WASM
  linear-memory capacity, returning status `2` with a static non-recursive
  fallback when even the `output_limit_exceeded` error envelope cannot fit in
  `max_output_bytes`.
- Keep synthetic locations absent (`nil` / `null` / `None`) in Go, TypeScript,
  and Python host models rather than fabricating zero coordinates. Compute
  TypeScript `utf16Offset` values in the JS/TS host wrapper from the caller's
  input string or UTF-8 byte buffer; leave `utf16Offset` unset when decoding
  standalone wire JSON without source bytes.
- Isolate `bindings/python/` (`PyO3/maturin`, `abi3-py310`, `#![forbid(unsafe_code)]`)
  in its own Cargo workspace (`CARGO_TARGET_DIR=target/python`) with independent
  `cargo deny` and wheel verification (`make python-check`).

Updates resolve these items in the specification and a matching test/profile
revision. Acceptance belongs to the maintainers; proposed records do not imply
that implementation or package publication has occurred.
