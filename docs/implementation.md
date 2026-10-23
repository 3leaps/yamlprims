# Rust implementation status

The workspace contains a working `yamlprims` core and a thin `yamlprims-cli`
consumer. Both are initial packages with publication disabled and their public
surfaces still under development.
Package versions, specification tags and wire versions are separate namespaces.

Available operations include single-document and stream loading from UTF-8,
owned values with exact integers and arbitrary keys, explicit merge policy,
source spans, typed Serde conversion, canonical wire/error encoding and plain
JSON projection. The core forbids unsafe code and performs no filesystem,
environment or network access. Granit-parser 2.0.0 is the internal engine.

The engine adapter translates backend events, styles, tags and locations into
private internal types. The iterative builder owns tree construction, anchor
lifecycle, merges and node-span normalization; a separate budget module owns
cumulative logical charges and copy metrics. Raw input admission remains ahead
of parsing. Production parsing is strict-only in both passes. Compatibility
experiments are outside the package; no lenient feature is exported.

Included validation covers contract regressions, revision 1.2/1.3 additions,
truth tables, wire snapshots, Error compile probes and parser-created trees on
2 MiB threads. The tests include allocation instrumentation in separate test
binaries. Fixtures are package-local, retain exact bytes and are included in
extracted-package tests. Full pinned corpus checks are described in
[conformance](../conformance/README.md).

Scalar construction charges decoded text bytes only for String values under the
existing logical-weight formula. Focused regressions cover owned/key-byte
boundaries, core and retained tags, anchor registration, alias copies and stream
totals. The scalar-input byte limit remains independent of that logical weight.

Completed mapping metrics describe the final entries and retained tag after
merging. Anchor registration and later copies use those node counts, weights and
collection heights. Construction, directive/source work and logical copy charges
remain cumulative. Focused controls cover shadowing, source lists, empty/tagged
subtrees, mapping keys, repeated/nested anchors and streams.

Ordinary keys are checked for NaN after their completed logical weight is
admitted, before diagnostic-key cloning or associated value construction.
Focused controls cover competing value failures, earlier construction failures,
source syntax, nested keys and alias definition/use provenance, with allocation
and bounded parser-key stack checks.

Merge completion validates all source kinds before admitting entries. Explicit
keys and earlier sources shadow later entries before capacity admission. Each
remaining entry must fit the result's entry limit before logical copy charges
or insertion. Focused controls cover competing copy budgets, partial capacity,
inline and aliased sources, nested merges, exact directive provenance and
cumulative source/copy accounting. Test-only instrumentation observes logical
copy and insertion attempts; source entries move rather than physically clone.

Source validation counts yielded sequence and mapping starts together before
owned construction, including empty collections, keys, implicit mappings,
indentless sequences and merge sources. Tags add no collection level and each
document starts at level zero. The first yielded engine error or excess start
terminates validation at its original-input site. Both parser passes keep finite
native nesting guards with headroom for the pinned scanner's simple-key
lookahead. Focused controls cover depth boundaries, native and adapter error
precedence, coordinates, pass counts and the native resource fallback.

## Revision 1.4 proposal and implementation gaps

The working specification proposes revision 1.4; these source packages are not
qualified against it. Historical specification tags and conformance pins retain
their exact bindings. The following gaps require separately reviewed source
changes and detecting tests:

| Proposed or existing rule | Current gap / required qualification |
| --- | --- |
| Bytes-only anchor registration | Existing code charges cache bytes, not nodes; new normative totals need independent arithmetic review and exact boundary tests |
| Combined source depth and bounded validation precedence | Combined event-depth enforcement and finite native headroom have focused source/site, phase-order and instrumentation controls; integrated revision 1.4 qualification remains open |
| Merged-entry capacity before copy admission | Focused source-validation, shadowing, capacity/copy precedence and instrumentation controls are included; integrated revision 1.4 qualification remains open |
| Ordinary-entry, copy-tie and completed-key rules | Completed-key weight and NaN admission have focused coverage; review the remaining entry/copy admission timing with isolated and competing-limit tests |

This is a gap inventory, not a claim of executed revision 1.4 qualification.
The revision changes no Rust code, lock, fixture or conformance expectation.

## Remaining gates before API freeze

- Acceptance and qualification of proposed loader depth/accounting/precedence
  rules; separately, safety of arbitrary hand-built trees across clone, drop,
  equality, formatting and conversion. New caller-tree guarantees are deferred.
- Every open M1 item in [the design review](design-review.md): error precedence,
  construction-time error bounds, anchor-cache accounting, checked construction,
  closed syntax vocabulary, wire decoding, metadata ranges, enum source-style
  handling and nested provenance.
- A contract-to-test matrix for every implemented MUST and reviewed disposition
  of uncovered cases. Existing passing tests do not settle those design questions.
- Independent review and freeze of CLI behavior: stdin, cardinality, I/O limits,
  stream/error output and failure handling. The [initial CLI](../crates/yamlprims-cli/README.md)
  has bounded file/stdin reads, multi-document check/wire dump, exactly-one-document
  plain dump, and serialization before stdout. Operating-system write failures
  can still leave partial output. No binding placeholder is included.

Before a package release, the actual build still needs the complete native
five-target MSRV/stable matrix, contained stress with original resource budgets,
bounded fuzz/mutation/Miri, phase-specific memory/time measurements, dependency
and license audit, and package/downstream qualification. Logical weight limits
are not RSS limits. The current Rust workflow is baseline coverage on Linux;
documentation CI is not runtime qualification. Bindings have separate M2 gates.

The working specification is a new draft; tagged specifications remain at their
immutable baselines. Historical draft-status prose inside those tags does not
describe current package availability; this page and the repository README
describe current implementation status.
