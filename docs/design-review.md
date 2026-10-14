# Open design items before implementation freeze

Status: proposed. The specification is publishable as a draft while these items
remain visible. They must not silently become implementation choices or claims
of completed qualification.

## M1 contract freeze

### Selected in the revision 1.1 draft

The strict-only decision in ADR-0003 is accepted for revision 1.1; the other
selections remain proposals. Implementation qualification and a new immutable
profile are still required; revision 1.0 expectations stay bound to revision 1.0.

| Contract           | Normative resolution                                                                                                                                                  | Detecting evidence still required                                                                                                                                                                        |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source spans       | §7.2 fixes block-scalar ends and omitted-child positions; containment exceptions apply only at insertion boundaries.                                                  | Exact start/end coordinates and unchanged values across chomping, blank lines, trailing spaces, explicit empty keys/values, BOM, CRLF/lone CR and multibyte text; check inside alias/merge subtrees too. |
| Empty streams      | §5.5 distinguishes zero-document streams from explicit empty documents; §10 adds `load_all_bytes` and CLI stream behavior.                                            | String/byte cardinality and EOF/boundary positions, explicit document metadata, CLI empty success envelope and all-or-nothing failure.                                                                   |
| Error ownership    | §7.1 selects boxed Error with public non-exhaustive ErrorDetail, Deref/DerefMut, owned extraction and bounded constructors; §10 specifies the fallible error encoder. | Positive borrowed/owned/custom-error compile probes, negative struct-pattern and field-move probes, pointer-sized Error assertion, bounded formatting and too-small encoder budget controls.             |
| Strict indentation | §9 and accepted ADR-0003 require a strict-only public API and `true` in every parsing pass.                                                                           | Compare both settings on an identical pinned suite and migration inventory; malformed indentation must fail as syntax, not an unrelated error.                                                           |

### Remaining open items

| Item                       | Required resolution and detecting evidence                                                                                                                                                                                                                                                                        |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Error precedence           | Define `InvalidOptions` ordering, and distinguish engine safety limits from syntax errors during a syntax-only pre-pass. An over-depth event stream must not accidentally masquerade as malformed YAML or bypass a resource guard to search for a later syntax error.                                             |
| Accounting scope           | Specify whether anchor-cache copies charge nodes, bytes or both; define wrapper depth, root depth and alias insertion depth mathematically. Cached metrics must describe the final post-merge subtree. Test nested merge-plus-anchor-plus-alias paths with one decisive limit.                                    |
| Error storage and encoding | Complete ErrorKind payloads and bound `engine_detail`, related spans and structured paths before constructing large errors, including caller-mutated ErrorDetail. The boxed shape and Rust error-encoder budget failure are specified; diagnostic allocation and validation still need qualification.             |
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
pin is asserted by this draft. In particular, the existing profile citing
`spec-v1.0` must not be silently relabeled to cover revision 1.1.

The new corpus revisions are `yamlprims-core` r2 and `scalar-alias` r2, pinned
after `spec-v1.1` is tagged as described in the build specification. Qualify the
consecutive explicit-empty document coordinates in §5.5 and the mutated error
kind/detail validation and rejection in §11.1. The strict-indentation review must
report every suite ID whose verdict changes, keeping format-level syntax truth
separate from revision 1.0's leniency differences.

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
