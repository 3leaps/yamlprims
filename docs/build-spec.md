# yamlprims build specification

Status: proposed implementation plan. Current release scope: specification only.
The [behavioral specification](spec/yamlprims-spec.md) defines semantics; this
document defines package boundaries, implementation order and acceptance.

## Workspace and dependency direction

Use a virtual Cargo workspace when implementation begins. Start with two crates;
add the native boundary crates when their milestones begin.

| Package          | Path                     | Responsibility                                                                | First milestone |
| ---------------- | ------------------------ | ----------------------------------------------------------------------------- | --------------- |
| `yamlprims`      | `crates/yamlprims/`      | Owned model, event adapter, resolution, budgets, Serde conversion, wire codec | M1              |
| `yamlprims-cli`  | `crates/yamlprims-cli/`  | Binary `yamlprims`; file/stdin handling, diagnostics, `check` and `dump`      | M1              |
| `yamlprims-ffi`  | `crates/yamlprims-ffi/`  | C ABI, versioned header, buffer ownership and panic boundary                  | M2              |
| `yamlprims-node` | `crates/yamlprims-node/` | Node-API addon and conversion boundary; native npm artifacts                  | M2              |

`yamlprims-cli`, `yamlprims-ffi` and `yamlprims-node` depend on `yamlprims`.
The Go module in `bindings/go/` consumes the C ABI. The TypeScript package in
`bindings/typescript/` consumes Node-API. Neither binding independently parses
YAML or reimplements scalar/merge policy. The Node addon does not need to pass
through the C ABI merely to call the Rust core.

Keep `Value`, `Node`, `Integer`, `Mapping`, `Options`, `Error` and codecs in the
core package initially. Modules provide internal boundaries; extra published
model, engine and wire crates would impose additional compatibility surfaces
without an established consumer. The production graph has one selected backend,
not a runtime backend-plugin system. Alternative engines belong in qualification
tools, outside consumer dependency closures.

Planned tree (directories are created only when they contain working material):

```text
Cargo.toml / Cargo.lock / rust-toolchain.toml
crates/yamlprims/src/
  lib.rs                 public facade and operation entrypoints
  value.rs               owned model and checked mutation
  options.rs             validated policies and limits
  error.rs               error kinds, locations and provenance
  engine/                only module importing granit-parser
  resolve.rs             Core scalar/tag rules
  build.rs               owned-tree construction, anchors and merge
  budget.rs              checked counters and preflight expansion costs
  de.rs                  shared-tree Serde conversion
  wire.rs                canonical wire/error encoding and plain projection
crates/yamlprims-cli/src/main.rs
crates/yamlprims-ffi/     later: include/yamlprims.h, native tests
crates/yamlprims-node/    later: Node-API boundary
bindings/go/             later: module, typed model and tests
bindings/typescript/     later: package, typed model and tests
tests/                   small public synthetic contract regressions
conformance/             corpus/profile lock and consumer driver
fuzz/                    bounded explicit fuzz targets
docs/spec/               versioned behavioral contract
docs/decisions/          architecture, data, security and process records
```

`src/build.rs` above is an ordinary module, not a Cargo build script. The core
must not require native compilation, code generation or network access at build
time. The docs-only release creates no Cargo workspace or placeholder crates.

## Rust and dependency policy

- Proposed MSRV: Rust 1.88, edition 2021, workspace resolver 2. Confirm with an
  actual locked build/test on 1.88; current stable is a separate test cell.
- Initial engine qualification pin: `granit-parser =2.0.0`, defaults off,
  `std,error_messages` on. Inspect the shipped archive and complete dependency
  closure before the first code release. A pin is not qualification evidence.
- Rust core and CLI forbid unsafe code. Native boundaries, generated glue and
  their dependency closures have their own review obligations.
- Commit the workspace lockfile for CI/CLI reproducibility. Cargo consumers do
  not inherit that lockfile: audit supported resolved configurations too.
- Start with Serde owned deserialization as a supported core capability. Feature
  flags must not silently change scalar resolution, equality, defaults or wire
  semantics. Defer a no-Serde/minimal build until there is a tested requirement.
- Pin all shipped sibling package versions consistently. Use both `path` and
  registry `version` for workspace dependencies when publishing real crates.
- No library filesystem/network/environment access. CLI I/O is explicit and
  bounded before handing bytes to the core.

## Delivery gates

### S0 — public specification release

Publish the README, behavioral spec, this build spec, open items, governance,
licenses and decision drafts. Documentation validation runs without Rust or a
parser. Release notes say exactly that no runtime/package is shipped.

Repository release `spec-v1.0`, specification revision `1.0`, future Cargo package
versions, wire version and C ABI version are separate namespaces. The first
implemented release can use tag `v0.1.0` and Cargo package version `0.1.0`.
crates.io reads package versions from Cargo manifests, independently of Git tags.
Never move the earlier specification tag. Corpus profiles
pin immutable specification bytes and the exact revision they implement.

The working draft is revision `1.3` (`SPEC_VERSION`), targeting `spec-v1.3`.
Existing tags `spec-v1.0`, `spec-v1.1` and `spec-v1.2` and their pinned expectations
remain immutable. Revision 1.3 adds nested empty-scalar normalization and the
zero-width collection exception, and clarifies UTF-8 input scope, f32 conversion
and exact keep-chomping EOF values. Its new anchor rules require a new profile
citing the final specification commit/file digest and independently reviewed
expectations before transfer is authorized.

The revision 1.1 corpus migration specifies `yamlprims-core` r2 and `scalar-alias`
span convention r2 revisions citing `spec-v1.1`. Pin the tag's specification
digest and the new profile/convention digests before qualification. The profile
must capture the `1.1/E-2` document split and `1.1/E-4` strict-indentation change.
`yamlprims-core` r2 must explicitly pin strict indentation in every parsing pass;
candidate compatibility modes and historical lenient results stay outside that
profile. Qualifying a migration mode does not add it to the production API.
The existing `scalar-alias` r1 convention ends block scalars after the last
content line break; `1.1/E-1` ends them before it. These are distinct conventions:
retain r1 bytes and expectations, and bind new spans to r2. These migration
requirements do not establish acceptance of any corpus artifact or oracle.

Revision 1.2 adds owned type descriptions, error-encoder preflight and empty-block
anchors. It does not silently rebind those r2 artifacts. Assign new immutable
profile/convention revisions where their contracts change, with the exact
specification and artifact digests. The corpus's indicator-based block-scalar
event spans are not yamlprims content-based node spans, even when their end rules
agree. Qualify the empty-block node anchors in §7.2 independently, and preserve
prior event-span conventions. Error mutation, bounded formatting and encoder
rejection probes belong in Rust API/consumer-operation tests; an input/value
corpus alone cannot exercise them.

Revision 1.3 does not retroactively extend revision 1.2 profiles with nested
normalization or the collection exception. Preserve every historical profile's
actual binding: some r1 profiles cite revision 1.0, while `yamlprims-core` r2 and
`yamlprims-core-limits` r1 cite revision 1.1. Profile revision numbers are not
specification version numbers. Add 1.2 and 1.3 profiles beside historical ones;
do not relabel recipes or treat earlier runs as coverage of a later inventory.

Node-span expectations need profile-bound selectors for document roots, sequence
items, mapping keys/values, tag wrappers/inners and nodes inside alias/merge
copies. Event indexes do not identify those nodes. Define schema, validator and
consumer support before adding node records. Limit-error `configured` expectations
likewise require an explicit corpus schema extension. Under structcorpus's
[phase convention](https://github.com/3leaps/structcorpus/blob/main/docs/decisions/DDR-0002-format-events-and-profile-expectations.md),
input-size errors belong to `input`; the driver must map `MaxInputBytes` to that
phase rather than the general `limit` phase and test that mapping at the boundary.

Keep operation coverage distinct: ordinary load limits, stream document limits,
encoder output limits and M3 warning truncation. Use fixed-message constructed
errors for exact error-envelope/output-length goldens; loader prose is not a
normative oracle. Such operations may become declarative corpus records with a
versioned operation schema and executor. Until then, retain consumer API tests.
Promote corpus records to `reviewed` only through independent semantic review
with reviewer and method recorded, never as a consequence of candidate passes.

No crate-name reservation is required for S0. Prefer publishing useful packages
over empty placeholders. A first crate may expose a deliberately smaller,
documented working surface, but must not contain parser APIs that panic, return
fabricated empty results, or imply unavailable conformance.

### Q0 — qualification before production implementation

Before transferring candidate implementation into this workspace, exercise it
through the actual consumer driver against a pinned structcorpus release or
commit and named profile. Keep this qualification separate from implementing
the production workspace. Publishable evidence describes observable behavior;
it need not expose development-environment details.

The gate requires:

1. Exact candidate commit, specification commit/file digest, corpus commit,
   profile path/digest, generator revision, seed/parameters, generated hashes and
   toolchain pins. Verify imported bytes and the exact case inventory. Never
   substitute a moving branch or an assumed corpus/profile pin.
   Dispatch or validate the exact `(profile, revision)` and its settings, not
   just the profile name. An unknown requested profile/revision is a failure;
   exclusions must be explicit and counted. Historical results run under current
   settings must be labeled as such, not claimed as qualification of the older
   profile's configuration.
2. Independent acceptance of expected values, error categories and source
   positions. An implementation's observed output is not its oracle.
3. Smoke and available conformance runs through real generic, typed and wire
   APIs, with pass, mismatch, expected policy difference, provisional, skipped
   and unrun outcomes kept separate.
4. No unresolved mismatch against the agreed transfer profile. Older expectations
   remain bound to older specification revisions; do not rewrite history.
5. Negative controls proving the driver catches missing, extra and duplicate IDs,
   wrong values, wrong error categories, wrong source offsets and disabled
   expansion guards. A result set containing only expected policy exceptions
   MUST fail, as MUST malformed inputs relabeled `invalid_options` instead of
   syntax errors. Controls must fail for the intended reason, not an unrelated
   build or setup failure. Use bounded resource execution where required.
   Mismatches and malformed/unsupported requested expectations MUST cause a
   nonzero process exit. Empty, missing or partially hydrated inventories MUST
   fail. Retain wrong-value, wrong-error, empty-inventory and unknown-revision
   probes as tests of the driver itself; a printed summary or exit zero alone
   is not qualification evidence.
6. A reviewed gap inventory for coverage not yet present in the corpus. Corpus
   publication by itself does not establish candidate conformance.
   Include exact node spans and explicit-empty-document boundaries, the
   revision 1.2 empty-block examples and encoder mutation/boundary probes,
   and revision 1.3 nested anchors, collection exceptions, encoding examples,
   double rounding and exact keep-chomping EOF values. Pin each to its matching
   specification revision; a driver control must detect input-size errors
   misclassified as ordinary limit errors.
   Provisional expectations remain provisional after a successful comparison;
   independent semantic acceptance is a separate requirement.

Q0 authorizes a controlled port, not production adoption or binding qualification.
S0 may publish while Q0 runs. The port must not begin merely because the spec
release is public.

Run generic, typed and wire APIs independently. Compare exact integers without
coercion through f64, and compare float values/bit patterns rather than just event
kinds. Keep scalar truth-oracle acceptance separate from suite-event comparison.
Hydration executes only the pinned, reviewed corpus generators; test payloads
must not fetch scripts or contact external services. Fixture provenance, licenses
and classification must be checked before admission.

For a YAML test-suite run, record the exact suite revision, byte inventory and
valid/malformed classification. One available baseline is `data-2022-01-17` at
`6e6c296ae9c9d2d5c4134b4b64d01b29ac19ff6f` (402 unique cases: 308 valid,
94 malformed). These counts describe that pin, not every suite revision and not
a claim that yamlprims has executed it. Explicit duplicate-key policy exceptions
remain distinct from syntax failures. Qualify both indentation settings on the
same inventory and migration fixtures as part of qualification of the new strict profile.
Report every suite ID whose verdict changes. Format-level syntax expectations,
including 9C9N, remain independent of any historical lenient profile.

### M1 — Rust implementation and thin CLI

Port the qualified behavior in reviewable slices:

1. Owned types, canonical integers, ordered mappings, equality/hashing, checked
   mutation, options, located errors and source/synthetic provenance.
2. Input gate and isolated engine adapter; original-byte coordinates and explicit
   source-span convention. No backend aliases in public signatures.
3. Shared resolver/builder, tag wrappers, anchor lifecycle, alias/merge preflight,
   monotonic accounting and duplicate rejection.
4. Serde conversion through that same tree, including source-aware failures,
   direct foreign scalar/collection visits, wrapper nesting and reentrancy.
5. Canonical wire and error codecs, explicit plain projection, output budgets;
   then CLI over the same entrypoints.

Keep any metadata needed for the documented local-enum re-resolution exception
private; it must not invent behavior when reconstructing a tree from wire data.
Porting and package separation must rerun Q0's identical inputs and expectations.
Preserve the independently accepted behavior rather than merely reproducing
the prior module organization.

M1 acceptance adds:

- Every implemented MUST mapped to a test and every open M1 item in
  [the design review](design-review.md) resolved or explicitly revised.
- Exact boundary controls for tags, merges, aliases, keys, depth, document count
  and output size; instrumentation proves rejection precedes prohibited copies.
- Source/provenance assertions for parsing and `from_node`, including inherited
  alias/merge context. Foreign Serde integer widths and wrappers tested directly.
- Exact wire goldens for success and failure; differential errors retain their
  categories and locations rather than collapsing to a generic failure.
- Pinned yaml-test-suite coverage with reviewed exclusions, plus the selected
  structcorpus profile. Small regressions run offline from a pinned local subset.
- Tests on MSRV and stable; bounded fuzz/Miri/mutation evidence with exact scope,
  time, skips and toolchains. Actual fuzz duration includes overhead reporting.
- Per-category memory and latency measurements. Logical weights are not a
  maximum RSS or allocation guarantee. A retained-tree model plus total peak
  allocation does not establish separate anchor-cache or temporary-work peaks,
  nor replace preventive-copy instrumentation. Keep the 64-byte logical node
  charge independent of any measured struct layout.
- Package dry-runs, extracted-package tests and downstream smoke builds. Audit
  the actual production feature/dependency graph, not a mixed comparison harness.

The proposed M1 core qualification matrix uses native execution on five targets:

| Platform       | Rust target                 |
| -------------- | --------------------------- |
| Linux x86_64   | `x86_64-unknown-linux-gnu`  |
| Linux ARM64    | `aarch64-unknown-linux-gnu` |
| macOS ARM64    | `aarch64-apple-darwin`      |
| Windows x86_64 | `x86_64-pc-windows-msvc`    |
| Windows ARM64  | `aarch64-pc-windows-msvc`   |

Run the core tests on both the proposed MSRV and stable Rust for each target,
recording exact toolchain versions and executed coverage. Report compile-only
checks separately; cross-compilation does not establish native conformance.
These are planned runtime qualification cells, not current support claims.
The existing five-platform CI validates documentation only and does not satisfy
Q0 or M1 acceptance. Runtime transfer still requires Q0 before implementation.

Name and qualify any additional target separately. Test a 32-bit target before
claiming it, especially for usize/metadata and numeric boundaries. This core
matrix does not qualify the M2 native boundaries or language packages.

CLI contract before its package release: define stdout/stderr, exit codes for
input/usage/I/O/internal errors, stdin, multiple files, byte limits and atomic
output behavior. Never leave partial JSON presented as a successful wire result.
Wire `dump` and `check` use the specification's multi-document semantics. The
plain-output representation for zero or multiple documents still needs an
explicit CLI contract.

### M2 — native boundaries and language packages

Freeze the C pointer/buffer/failure protocol separately from the wire grammar.
Add ABI ownership, alignment, length, zero-input, repeat-free-on-zero, failure and
unwind tests. Invalid foreign pointers are caller violations, not recoverable
input errors. Decide how an unavailable or too-large error envelope is reported.

Implement Go and TypeScript lossless host models, exact integers, float bits,
ordered arbitrary-key entries, tags, optional source locations and structured
paths. Run each language's independent assertions before canonical re-encoding.
Document that Rust typed enum re-resolution is not a binding guarantee.

Distribute only platform/architecture combinations with executed native tests.
Record shared-library ABI, Node-API floor, Go/Node floors, artifact checksums and
runtime loading behavior. No implicit download on library import. Package Rust
libraries before their dependents; npm/Go publication follows native artifacts
and binding tests. `yamlprims-node` can remain `publish = false` on crates.io if
the supported distribution is solely npm; it need not occupy a crate name.

### M3 — demand-driven additions

Consider the in-memory document iterator, migration lint, Failsafe schema and
emitter only for concrete consumers. They are not hidden prerequisites for M1.
Streaming input, source-preserving editing and a new parser engine require new
scope and decision records.
