# Open design items before implementation freeze

Status: open design work. Specification revision 1.3 is a tagged draft; revision
1.2 and earlier revisions remain available at their immutable tags. These items
remain open. They must not silently become implementation choices or claims of
completed qualification.

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

### Remaining open items

| Item                       | Required resolution and detecting evidence                                                                                                                                                                                                                                                                        |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Error precedence           | Define `InvalidOptions` ordering, and distinguish engine safety limits from syntax errors during a syntax-only pre-pass. An over-depth event stream must not accidentally masquerade as malformed YAML or bypass a resource guard to search for a later syntax error.                                             |
| Accounting scope           | Specify whether anchor-cache copies charge nodes, bytes or both; define wrapper depth, root depth and alias insertion depth mathematically. Cached metrics must describe the final post-merge subtree. Test nested merge-plus-anchor-plus-alias paths with one decisive limit.                                    |
| Error storage and encoding | Complete the remaining ErrorKind payloads and bound `engine_detail`, related spans and structured paths before constructing large errors. Revision 1.2 specifies owned Type descriptions and encoder revalidation; construction-time allocation bounds and detecting evidence remain required. |
| Public operations          | Complete the plain encoder and checked constructors/accessors needed to create Document/Tagged/Tag values without exposing invariant-bearing fields. Compile the selected Error constructors, error encoder and `load_all_bytes` API in the candidate driver; illustrative signatures alone are not API evidence. |
| Syntax error vocabulary    | Name and test the closed `SyntaxKind` set before wire version 1 is frozen; a backend's message text is not a stable taxonomy.                                                                                                                                                                                     |
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
general parser/materialized root and wrapper depth, and arbitrary synthetic-tree
stack safety, remain distinct open items. Specification acceptance does not
establish those runtime guarantees.

Revision 1.3 additionally needs profile-bound node selectors and matching
schema/validator/driver support, separate from event locations. Limit-error
configured values need a schema extension; input-size errors already use the
corpus `input` phase. Test that mapping independently from other load limits.
Keep stream, output and M3-warning operations distinct. Fixed-message error
operation goldens require an operation schema/executor or API tests, rather than
freezing loader prose. Independent oracle promotion and a fail-closed run of
the exact new inventory remain separate qualification requirements.

## M2 boundary freeze

- Define pointer validity, lifetime, alignment and non-overlap for input, options
  and output. Require a fresh/zeroed output buffer and never overwrite a live
  allocation without an explicit ownership operation.
- Resolve output-budget exhaustion separately from allocation failure. The draft
  promise of a wire envelope on every ordinary failure is incompatible with an
  arbitrary one-byte output budget unless the protocol defines a fallback.
- Define behavior when `out` is null or options bytes are malformed UTF-8, and
  what status can be returned without writing through an invalid pointer.
- State allocation-failure limits honestly: `catch_unwind` does not contain
  allocator aborts, invalid foreign memory or stack overflow.
- Keep synthetic locations absent in Go/TypeScript, rather than fabricating zero
  coordinates. Define how UTF-16 offsets remain computable when a host receives
  only wire output and not the original input; require source context where needed.
- Qualify generated Node-API glue and native dependencies as part of the unsafe
  boundary. A safe core does not imply an entirely unsafe-free native distribution.

Updates resolve these items in the specification and a matching test/profile
revision. Acceptance belongs to the maintainers; proposed records do not imply
that implementation or package publication has occurred.
