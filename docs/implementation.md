# Rust implementation status

The workspace contains a working `yamlprims` core package. It is an initial
baseline, with publication disabled and the public API still under development.
Package versions, specification tags and wire versions are separate namespaces.

Available operations include single-document and stream loading from UTF-8,
owned values with exact integers and arbitrary keys, explicit merge policy,
source spans, typed Serde conversion, canonical wire/error encoding and plain
JSON projection. The core forbids unsafe code and performs no filesystem,
environment or network access. Granit-parser 2.0.0 is the internal engine.

The baseline preserves the iterative loader and existing resolver/builder
organization. Engine-type isolation and separate builder/budget modules remain
structural work. Production parsing is strict-only in both passes. Compatibility
experiments are outside the package; no lenient feature is exported.

Included validation covers contract regressions, revision 1.2/1.3 additions,
truth tables, wire snapshots, Error compile probes and parser-created trees on
2 MiB threads. The tests include allocation instrumentation in separate test
binaries. Fixtures are package-local, retain exact bytes and are included in
extracted-package tests. Full pinned corpus checks are described in
[conformance](../conformance/README.md).

Remaining gates before API freeze:

- Mathematical root/wrapper/alias/merge depth rules and safety of arbitrary
  hand-built trees across clone, drop, equality, formatting and conversion.
- Every open M1 item in [the design review](design-review.md): error precedence,
  construction-time error bounds, anchor-cache accounting, checked construction,
  closed syntax vocabulary, wire decoding, metadata ranges, enum source-style
  handling and nested provenance.
- A contract-to-test matrix for every implemented MUST and reviewed disposition
  of uncovered cases. Existing passing tests do not settle those design questions.
- A thin CLI with explicit stdin, cardinality, I/O limits, atomic output, stream
  and error behavior. No CLI crate or binding placeholder is included yet.

Before a package release, the actual build still needs the complete native
five-target MSRV/stable matrix, contained stress with original resource budgets,
bounded fuzz/mutation/Miri, phase-specific memory/time measurements, dependency
and license audit, and package/downstream qualification. Logical weight limits
are not RSS limits. The current Rust workflow is baseline coverage on Linux;
documentation CI is not runtime qualification. Bindings have separate M2 gates.

The tagged specification remains byte-identical to its immutable baseline.
Historical draft-status prose inside it does not describe package availability;
this page and the repository README describe current implementation status.
