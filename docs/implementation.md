# Rust, Go and binding implementation status

The workspace contains a working Rust `yamlprims` core, a `Profile B` pure-Go
twin (`yaml/` in root `go.mod`, `github.com/3leaps/yamlprims/yaml`,
`CGO_ENABLED=0`, standard library only) with its conformance and crosscheck
driver (`cmd/structcorpus-driver`), isolated TypeScript WASM
([`bindings/typescript/`](../bindings/typescript/README.md), `@3leaps/yamlprims`)
and Python `PyO3/maturin` ([`bindings/python/`](../bindings/python/README.md),
`yamlprims`) bindings, and a thin `yamlprims-cli` consumer. The Rust crates and
language bindings have not been published to package registries yet, and their
public surfaces remain under development until release qualification is complete.
Package versions, specification tags and wire versions are separate namespaces.

Available operations include single-document and stream loading from UTF-8,
owned values with exact integers and arbitrary keys, explicit merge policy,
source spans, typed Serde conversion, canonical wire/error encoding and plain
JSON projection. The core forbids unsafe code and performs no filesystem,
environment or network access. Granit-parser 2.0.0 is the internal engine.

The engine adapter translates backend events, styles, tags, locations and
scanner/parser syntax errors (`ErrorKind::Syntax(SyntaxKind)`, `SyntaxKind::ALL`,
81 closed syntax codes mapped explicitly without `Debug`-name reflection) into
owned types. The iterative
builder owns tree construction, anchor lifecycle, merges and node-span
normalization; a separate budget module owns cumulative logical charges and copy
metrics. Raw input admission remains ahead of parsing. Production parsing is
strict-only in both passes. Compatibility experiments are outside the package;
no lenient feature is exported.

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

Anchor-cache registration charges the completed subtree's full logical weight
before cloning, without additional materialized or expanded nodes. Focused
controls cover the seven normative totals, registration-specific boundaries,
nested and redefined bindings, retained/core tags, document-local names and
whole-call charges, original-byte sites and earlier failures. Test-only crate
unit witnesses count actual registration clone attempts, including admitted
copies before a later refusal; they do not measure physical allocation or RSS.

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

## Revision 1.4 conformance integration and remaining gaps

The working specification is at revision 1.4 (`spec-v1.4`). Historical
specification tags (`spec-v1.0` through `spec-v1.4`) and conformance pins
retain their exact bindings. The pinned conformance baseline at
`c8b7421b9e2473a9a7f37d04e3acfd5f552a2f99` (`conformance/pins/pin-c8b7421.json`)
adds the 12 explicit-limit revision 1.4 records (`yamlprims-core-depth128` r1
and `yamlprims-core-entries2` r1) and synthetic driver controls
(`conformance/scripts/structcorpus_probes_v14.py`) for the four revision 4
policy profiles (`yamlprims-core`, `yamlprims-core-stream`,
`yamlprims-core-merge-apply`, `yamlprims-core-merge-as-key` r4), which carry
zero shipped corpus records in `c8b7421`:

| Revision 1.4 rule | Conformance and unit coverage |
| --- | --- |
| Bytes-only anchor registration | Focused unit/integration controls plus `yaml.limits.anchor-registration-bytes-not-nodes` (`yamlprims-core-entries2` r1) |
| Combined source depth and bounded validation precedence | Focused unit/integration controls plus the six `yamlprims-core-depth128` r1 depth-boundary and alias-subtree records (`depth-127`, `depth-128`, `depth-129`, `alias-125`, `alias-126`, `alias-127`) |
| Merged-entry capacity before copy admission | Focused unit/integration controls plus `yaml.limits.merge-shadowed-fits-entries2` and `yaml.limits.merge-partial-entries-before-copy` (`yamlprims-core-entries2` r1) |
| Ordinary-entry, copy-tie and completed-key rules | Focused unit/integration controls plus `yaml.limits.ordinary-entry-before-value-nodes` and the two completed-key NaN precedence records (`nan-key-before-value-nodes`, `nan-key-after-key-owned-bytes`) under `yamlprims-core-entries2` r1 |

Passing the pinned conformance gate establishes bounded corpus transfer for
these 12 records and the historical 61,348-case baseline; it does not close the
broader M1 release gates below.

## Remaining gates before API freeze

- Acceptance and qualification of proposed loader depth/accounting/precedence
  rules; separately, safety of arbitrary hand-built trees across clone, drop,
  equality, formatting and conversion. New caller-tree guarantees are deferred.
- Every open M1 item in [the design review](design-review.md): error precedence,
  construction-time error bounds, anchor-cache accounting, checked construction,
  wire decoding, metadata ranges, enum source-style handling and nested provenance
  (the closed 81-code `SyntaxKind` parser vocabulary is implemented and unit-tested
  in `crates/yamlprims`).
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
documentation CI is not runtime qualification. The `Profile B` pure-Go twin (`yaml/` in root `go.mod`, `CGO_ENABLED=0`,
standard library only) and its conformance driver (`cmd/structcorpus-driver`)
are qualified against all 19 manifest rows of `conformance/pins/pin-c8b7421.json`
(`conformance/requalify.sh`) and verified for byte-for-byte wire, `DecodeWire`
round-trip, and plain-JSON parity with the Rust core via `make crosscheck`. The
isolated M2 language bindings (`bindings/typescript/` zero-import WASM module
and `bindings/python/` `PyO3/maturin` `abi3-py310` wheel) are verified in-tree
against `conformance/vectors/cases.jsonl` and `structcorpus --crosscheck-jsonl`
via `make ts-check` and `make python-check`, and remain marked `planned` in
`.fulmen/repo.yaml` until package publication ([build specification](build-spec.md),
[ADR-0004](decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md)).

The working specification is a new draft; tagged specifications remain at their
immutable baselines. Historical draft-status prose inside those tags does not
describe current package availability; this page and the repository README
describe current implementation status.
