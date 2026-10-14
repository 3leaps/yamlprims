# Open design items before implementation freeze

Status: proposed. The specification is publishable as a draft while these items
remain visible. They must not silently become implementation choices or claims
of completed qualification.

## M1 contract freeze

| Item                       | Required resolution and detecting evidence                                                                                                                                                                                                                                     |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Error precedence           | Define `InvalidOptions` ordering, and distinguish engine safety limits from syntax errors during a syntax-only pre-pass. An over-depth event stream must not accidentally masquerade as malformed YAML or bypass a resource guard to search for a later syntax error.          |
| Accounting scope           | Specify whether anchor-cache copies charge nodes, bytes or both; define wrapper depth, root depth and alias insertion depth mathematically. Cached metrics must describe the final post-merge subtree. Test nested merge-plus-anchor-plus-alias paths with one decisive limit. |
| Error storage and encoding | Stabilize public Error layout; bound `engine_detail`, related spans and structured paths before constructing large errors. Specify behavior when `max_output_bytes` is too small even for the error envelope.                                                                  |
| Public operations          | Complete the byte-input multi-document API, error/plain encoders and checked constructors/accessors needed to create Document/Tagged/Tag values without exposing invariant-bearing fields. Do not equate illustrative signatures with a complete compilable API.               |
| Syntax error vocabulary    | Name and test the closed `SyntaxKind` set before wire version 1 is frozen; a backend's message text is not a stable taxonomy.                                                                                                                                                  |
| Wire decoding              | Specify integer range, location/order validation, duplicate JSON members, unknown fields, canonical NaN bits and whether noncanonical JSON is rejected or normalized. Byte-identical round trip applies to canonical producer output.                                          |
| Metadata integer ranges    | Rust limits use usize and wire metadata uses at most 2^53−1. Define rejection for out-of-range options/coordinates, including 32-bit hosts; never truncate.                                                                                                                    |
| Synthetic trees            | Clarify the guarantee for arbitrarily deep hand-built trees and foreign deserializers. Private invariant fields alone do not bound recursion, destructor stack use or caller allocations.                                                                                      |
| Local-tag enum conversion  | Pin raw-versus-resolved payload and style metadata through `from_node`; wire round trips must not infer source style that the wire does not carry.                                                                                                                             |
| Empty streams              | Specify `load_all`'s empty/comment-only result independently of `load`'s single Null document.                                                                                                                                                                                 |
| Origin composition         | Specify how nested aliases and merges retain useful provenance when `Via` has only one slot; include descendant budget and conversion errors.                                                                                                                                  |

The reviewed tag formula counts `uri`, `handle` and `suffix` once, on the retained
wrapper. For `!long {}`, an empty mapping weighs 64 and its wrapper weighs 74:
total 138 bytes of logical weight and two nodes. This arithmetic is a semantic
golden, not a measurement of heap usage.

The sequence-span convention distinguishes ordinary and indentless block
sequences. A corpus profile must bind that convention explicitly; a broad
"first content token" assertion is not sufficient.

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
