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

## Revision 1.4 proposal and implementation gaps

The working specification proposes revision 1.4; these source packages are not
qualified against it. Historical specification tags and conformance pins retain
their exact bindings. The following gaps require separately reviewed source
changes and detecting tests:

| Proposed or existing rule | Current gap / required qualification |
| --- | --- |
| Final post-merge count, weight and H | Cached metrics still include source/directive structure; qualify corrected final-tree metrics independently of cumulative charges |
| Existing String-only scalar text weight | Scalar construction charges decoded text even for non-String values; repair against the existing formula and retain detecting owned/key-byte controls |
| Bytes-only anchor registration | Existing code charges cache bytes, not nodes; new normative totals need independent arithmetic review and exact boundary tests |
| Combined source depth and bounded validation precedence | The syntax pass relies on separate engine guards. Terminal unknown-anchor/version failures fit the revised phase contract; combined event-depth enforcement, finite lookahead headroom and native diagnostic sites still require implementation review and qualification |
| Merged-entry capacity before copy admission | Current merge code checks final capacity after merging; move admission before prohibited insertion/copy and test competing limits |
| Ordinary-entry, copy-tie and completed-key rules | NaN key rejection currently follows value construction; the draft makes the existing key-built requirement explicit before value construction. Review all admission timing and add isolated/competing-limit tests |

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
