# yamlprims specification

**Revision 1.4** · Status: Unreleased draft · License: CC0-1.0 (specification text)

The tagged draft baseline is [revision 1.3](https://github.com/3leaps/yamlprims/blob/spec-v1.3/docs/spec/yamlprims-spec.md).
This draft contains the changes and decision status listed in §16. Existing
profiles pinned to revisions 1.0 through 1.3 keep their original expectations;
they do not adopt this draft implicitly.

**Specification release.** This is a proposed target contract, not a claim that
the source tree conforms to every requirement. The repository contains a working
Rust core and an initial CLI; no package or binding is published by this draft.
See [implementation status](../implementation.md), the
[build specification](../build-spec.md) and [open design items](../design-review.md).
Revision 1.4 proposes loader depth, accounting, precedence and admission rules.
New caller-built-tree APIs and general stack-safety guarantees are deferred;
their existing open design gates remain prerequisites to M1 freeze.

This document is the normative contract for yamlprims: an owned YAML 1.2 data
layer over a maintained, safe-Rust event parser. Its key words MUST, MUST NOT,
SHOULD, and MAY are to be read as described in RFC 2119 and RFC 8174. Default
values marked _proposed_ are starting points that measurement may change.
Changes after publication are recorded as numbered errata (§16) or a new
revision.

## 1. Purpose and scope

The Rust YAML ecosystem's long-standing default stack depends on
`unsafe-libyaml`, a machine translation of libyaml into unsafe Rust, which is
unmaintained and archived
([RUSTSEC-2024-0449](https://rustsec.org/advisories/RUSTSEC-2024-0449.html);
see also the historical, patched alignment issue
[RUSTSEC-2023-0075](https://rustsec.org/advisories/RUSTSEC-2023-0075.html)
and, for `yaml-rust`,
[RUSTSEC-2024-0320](https://rustsec.org/advisories/RUSTSEC-2024-0320.html)).
yamlprims replaces that dependency with a library whose YAML **data contract**
is owned and specified here, using
[`granit-parser`](https://crates.io/crates/granit-parser) only as an event
source.

RUSTSEC-2024-0449 is an informational maintenance advisory, not a specific
exploitable-vulnerability finding. RUSTSEC-2023-0075 was fixed in
`unsafe-libyaml` 0.2.10; it is not a claim that 0.2.11 retains that defect.

yamlprims is library-first. Its contract is: bytes in; an owned value tree or a
located error out; with explicitly chosen semantics and enforced resource
limits.

**Input encoding.** yamlprims interprets input as UTF-8, with an optional initial
UTF-8 BOM. It does not detect or transcode UTF-16 or UTF-32. Invalid UTF-8 is
reported at the first invalid original-byte position; valid UTF-8 proceeds to
YAML syntax validation. Option validation (§6) and the raw-input-size gate still
apply. This is a UTF-8 input surface, not support for every YAML character encoding.

| Milestone                                | Contents                                                                                                               |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **M1**                                   | Rust core: `load`, `load_all`, `from_str`, owned `Value`/`Node`/`Error`, limits, wire format (§11), CLI `check`/`dump` |
| **M2**                                   | C ABI with a Go binding; Node-API with a TypeScript binding; cross-language conformance (§11–13)                       |
| M3 (optional, each requiring a consumer) | in-memory document iterator, migration lint, Failsafe schema, emitter                                                  |

**Out of scope:** a new YAML parser; source-preserving editing; `!include` or
environment expansion; a YAML 1.1 schema; borrowed deserialization; incremental
byte-stream input (all input is a complete in-memory buffer).

## 2. Layering

```
&[u8] / &str ──► input gate: max_input_bytes on raw length, BOM strip, UTF-8 validation
                 ▼
granit-parser =2.0.0 (default-features = false, features = ["std","error_messages"])
  engine options pinned (§9)                                          ← grammar / terminal engine checks
                 ▼
event pump      engine marker → Location; anchors; alias events       ← only module importing the engine
                 ▼
resolver        scalar and tag resolution (§4)                        ← owned policy
                 ▼
builder         nodes, keys, duplicates, merge, budgets (§5–6)        ← owned policy
                 ▼
Document / Node tree ─► typed conversion (§8) ─► wire encoder (§11) ─► CLI, Go, TS
```

- The generic (`load`) and typed (`from_str`) paths share one resolver and
  builder. §8 states exactly what that guarantees for typed conversion.
- The core and CLI crates are `#![forbid(unsafe_code)]`. Unsafe boundary code,
  including binding-generator output, is isolated in the C ABI and Node-API
  crates and reviewed with their dependency closures.
- Proposed MSRV: 1.88. The engine declares 1.81; its dependency closure is
  `arraydeque` and `smallvec`.

## 3. Value model

```rust
pub struct Node { /* private */ }
impl Node {
    pub fn value(&self) -> &Value;
    pub fn span(&self) -> &NodeSpan;
    pub fn via(&self) -> Option<&Via>;          // set on the root of an alias or merge copy
    pub fn tag_start(&self) -> Option<Location>; // explicit tag token, if any
    pub fn synthetic(value: Value) -> Node;      // hand-built trees: NodeSpan::Synthetic
}

pub enum NodeSpan { Source { start: Location, end: Location }, Synthetic }
pub enum Via { Alias { use_site: Span }, Merge { directive: Span } }

#[non_exhaustive]
pub enum Value {
    Null, Bool(bool), Int(Integer), Float(f64), String(String),
    Sequence(Vec<Node>), Mapping(Mapping), Tagged(Box<Tagged>),
}

pub struct Integer(/* private: canonical sign/magnitude */);
impl Integer {
    pub fn from_u128(v: u128) -> Self; pub fn from_i128(v: i128) -> Self;
    pub fn to_u128(&self) -> Option<u128>; pub fn to_i128(&self) -> Option<i128>;
    pub fn to_i64(&self) -> Option<i64>;  pub fn to_u64(&self) -> Option<u64>;
}
// Range: i128::MIN ..= u128::MAX. Exactly one representation per number.

pub struct Mapping { /* private: ordered entries + key index */ }
pub struct Tagged { tag: Tag, value: Node }               // accessors only
pub struct Tag { uri: String, handle: String, suffix: String }
pub struct Document { root: Node, yaml_version: Option<(u32, u32)>, explicit_start: bool }
```

- **Invariant-bearing fields are private.** Values are built through checked
  constructors, so conversion and encoding never see a non-canonical
  `Integer`, a mapping with duplicate keys, or a stale key index.
- **`Tag`.** `uri` is the resolved tag (`tag:example.com,2026:thing`,
  `!custom`). `handle` and `suffix` reconstruct the source spelling; they are
  not a promise of the exact source lexeme. Tag equality uses `uri` only.
- **Retained tags.** Non-core tags are preserved at every depth (§4.4).
  Explicit **core** tags (`!!str`, `!!int`, …) direct resolution and are not
  retained in `Value`; only their `tag_start` survives. The guarantee is
  "non-core tags preserved", not "all tags preserved".
- **Mapping mutation.** `insert_new(k, v) -> Result<(), InsertError>` with
  `InsertError::{Duplicate { existing }, InvalidKey}`; `get_mut` (values only),
  `remove`, and `replace_value`. Keys are never mutable in place, so the index
  stays valid. A key containing NaN at any depth is `InvalidKey`, so
  hand-built mappings keep the parser's invariants.
- **No panicking `Index`.** Mapping lookup returns `Option`.

These value invariants do not establish a supported depth or stack bound for
arbitrary caller-built trees. The support domain of construction, mutation,
Clone, Drop, equality/hashing, formatting and foreign deserialization remains
an M1 freeze item. Revision 1.4 adds no constructor or universal caller-tree
guarantee; loader depth alone is not such a guarantee.

## 4. Scalar resolution

### 4.1 Core schema (the only schema in this revision)

A plain, untagged scalar is matched **in this order**. Each pattern must match
the whole scalar text; the first match wins.

| #   | Result        | Full-match pattern ([YAML 1.2.2 §10.3.2](https://yaml.org/spec/1.2.2/#1032-tag-resolution)) |
| --- | ------------- | ------------------------------------------------------------------------------------------- |
| 1   | Null          | `~` \| `null` \| `Null` \| `NULL` \| empty                                                  |
| 2   | Bool          | `true` \| `True` \| `TRUE` \| `false` \| `False` \| `FALSE`                                 |
| 3   | Int (decimal) | `[-+]?[0-9]+`                                                                               |
| 4   | Int (octal)   | `0o[0-7]+`                                                                                  |
| 5   | Int (hex)     | `0x[0-9a-fA-F]+`                                                                            |
| 6   | Float         | `[-+]?(\.[0-9]+\|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`                                      |
| 7   | Float ±inf    | `[-+]?(\.inf\|\.Inf\|\.INF)`                                                                |
| 8   | Float NaN     | `\.nan\|\.NaN\|\.NAN`                                                                       |
| 9   | String        | anything else                                                                               |

- Patterns 3 and 6 overlap; integers are matched first, so `1` is Int and
  `1.0` and `1e3` are Float.
- An integer outside `i128::MIN..=u128::MAX` is an `IntegerOutOfRange` error,
  never a Float.
- A pattern-6 literal whose correctly rounded binary64 value is ±∞ (e.g.
  `1e9999`) is a `FloatOutOfRange` error. Infinity comes only from pattern 7.
- Underflow rounds to nearest per IEEE 754 and may produce a subnormal or ±0.0.
- Quoted, literal, and folded scalars are String unless explicitly tagged.
- Bool matches exactly the six spellings in row 2. `tRUE`, `yes`, `on`, `y`,
  and similar words are Strings. There is no YAML 1.1 mode.

### 4.2 `%YAML` directive

`%YAML 1.x` is accepted and recorded in `Document::yaml_version`; it does not
affect resolution. A major version other than 1 is an
`UnsupportedYamlVersion` error.

### 4.3 Explicit core tags (any scalar style)

| Tag                                               | Content must match | Result       | Otherwise                                |
| ------------------------------------------------- | ------------------ | ------------ | ---------------------------------------- |
| `!!str`                                           | anything           | String(text) | —                                        |
| `!!null`                                          | row 1              | Null         | `TagValueMismatch`                       |
| `!!bool`                                          | row 2              | Bool         | `TagValueMismatch`                       |
| `!!int`                                           | rows 3–5           | Int          | `TagValueMismatch` / `IntegerOutOfRange` |
| `!!float`                                         | rows 3–8           | Float        | `TagValueMismatch` / `FloatOutOfRange`   |
| `!!seq`, `!!map`                                  | matching node kind | no-op        | `TagKindMismatch`                        |
| a core scalar tag on a collection, or the reverse | —                  | —            | `TagKindMismatch`                        |

- `!!int "42"` is Int 42, `!!bool "TRUE"` is true, and `!!int "abc"` is an
  error located at `tag_start`.
- **`!!float` with an integer literal (rows 3–5).** The literal is not bound by
  the Int range. Its exact integer value is converted, and it is accepted only
  if exactly representable as binary64:
  `!!float 340282366920938463463374607431768211456` is 2^128. Because the integer value
  is converted, `!!float -0` is +0.0.

### 4.4 Other tags

- **Non-specific `!`.** The node resolves by kind only: a scalar becomes
  String(text) whatever it looks like; a collection stays a plain collection.
  It is never `Tagged`.
- **Non-core tags** — including the YAML 1.1 types `!!binary`, `!!timestamp`,
  `!!set`, `!!omap`, local `!x` tags, verbatim `!<…>` tags, and `%TAG`
  expansions — produce `Tagged { tag, value }`. For a scalar, the inner value
  is **String(raw text) without resolution**: `!custom 42` holds String "42".
  For a collection, the children resolve normally. Nothing is decoded.
- `unknown_tags: Preserve (default) | Reject`. `Reject` raises `UnknownTag` at
  `tag_start` for any non-core, non-`!` tag.

## 5. Structure policies

### 5.1 Key equality and hashing

`key_eq(a, b)` is recursive over `Value` and **ignores spans, `via`,
`tag_start`, `Tag.handle`, and `Tag.suffix`**.

| Kind            | Equal when                                                                                             |
| --------------- | ------------------------------------------------------------------------------------------------------ |
| Null            | always                                                                                                 |
| Bool            | same value                                                                                             |
| Int             | same integer (`1`, `+1`, `0x1`, `0o1` are equal)                                                       |
| Float           | same bits after mapping `-0.0` to `+0.0`                                                               |
| String          | identical bytes (no Unicode normalization)                                                             |
| Sequence        | same length and pairwise `key_eq`, in order                                                            |
| Mapping         | same length and, for each key in `a`, an equal key in `b` with `key_eq` values (**order-insensitive**) |
| Tagged          | equal `uri` and `key_eq` values                                                                        |
| different kinds | never: `Int(1)` ≠ `Float(1.0)` ≠ `String("1")` ≠ `Tagged(!x, "1")`                                     |

- NaN anywhere inside a key, at any depth, is an `InvalidKey` error, raised
  when the key is built. The completed-key check order is specified in §6;
  mapping mutation retains §3's invariant.
- `key_hash` is consistent with `key_eq`. Mapping hashes combine entry hashes
  order-independently. Hashing uses randomized SipHash (`RandomState`);
  correctness never depends on the absence of collisions.
- Lookup costs O(size of the probe key) to hash plus expected O(1) buckets.
  Key size is bounded by `max_key_bytes` (§6).

### 5.2 Duplicate keys

There is **one policy: error**. `DuplicateKey` reports the second key's
location and carries the first key's location as a related span. Duplicate
detection and `Mapping::get` use the same `key_eq` and `key_hash`, so they
cannot disagree.

**Ordinary-entry admission.** Check key validity at key completion (§5.1, §6).
Once its value has also been constructed and the source entry overhead charged,
check capacity, then duplicate identity. A NaN-bearing key is `InvalidKey`
before value construction or capacity admission. If the mapping already has
`max_mapping_entries` ordinary entries,
the next entry is `LimitExceeded(max_mapping_entries)`, even if its key would
be a duplicate. Otherwise an equal existing key is `DuplicateKey`. A rejected
entry is not inserted or counted as a stored entry; prior construction charges
are not refunded. Key/value construction and key-admission errors can occur
earlier. The error site is the candidate key's source start, with alias-related
provenance under §7.2. Syntax/depth validation still has §7.1 precedence.

With `max_mapping_entries = 2` and other budgets noncompeting:

| Input | Result |
| --- | --- |
| `{a: 0, b: 0}` | Success |
| `{a: 0, a: 1}` | `DuplicateKey` at the second `a` |
| `{a: 0, b: 0, a: 1}` | `LimitExceeded(max_mapping_entries)`, configured 2 |
| `{a: 0, b: 0, c: 1}` | Same limit error |
| `{a: 0, b: 0, .nan: 1}` | `InvalidKey` |
| `{.nan: !!int nope}` | `InvalidKey` before resolving the value |

Recognized `Apply` directives and merged entries follow §5.3 instead; `AsKey`
entries use this ordinary-entry rule after recognition.

### 5.3 Merge keys

Merge keys are not part of YAML 1.2. yamlprims recognizes them only under an
explicit policy.

**Recognition** is decided by the builder from event style and tag, before
resolution.

| Key as written                                     | Merge directive?                                   |
| -------------------------------------------------- | -------------------------------------------------- |
| plain `<<`, untagged                               | yes                                                |
| `!!merge <<` (any style, including `!!merge "<<"`) | yes; the explicit tag outranks the quoted-key rule |
| `!!merge x` where x ≠ `<<`                         | `TagValueMismatch`                                 |
| `"<<"`, `'<<'`, untagged                           | no; ordinary String key                            |
| `!!str <<`                                         | no; ordinary String key                            |
| `! <<`                                             | no; ordinary String key (non-specific tag)         |

**Policy:** `merge_keys: Reject (default) | Apply | AsKey`.

- `Reject`: a merge directive is a `MergeRejected` error at the key.
- `AsKey`: the directive is kept as the String key `<<`.
- `Apply` follows these rules:
  1. At most one directive per mapping; a second is `DuplicateKey`.
  2. The directive's value MUST be a mapping or a sequence of mappings.
     Aliased and inline forms are both accepted. A mapping may be untagged or
     `!!map`. Anything else, including custom-tagged mappings, is a
     `MergeSource` error.
  3. Merges apply bottom-up: a source mapping's own merges are complete before
     it is merged elsewhere.
  4. Explicit keys win wherever they appear relative to the directive. Among
     sources, the earlier source wins. Collisions between sources are not
     errors.
  5. Order is deterministic. Explicit entries keep their source order. The
     directive's slot is replaced by the unshadowed merged-in entries, in
     source order, with sources taken in sequence order.
  6. **Each unshadowed merged-in entry is a logical copy**, charged before it
     joins the result: its key and value node counts to `max_expanded_nodes`
     and `max_nodes`, and their weight plus one 24-byte entry overhead to
     `max_owned_bytes`. This holds even when an implementation moves rather
     than copies the nodes, for inline and aliased sources alike, and again at
      each level of nested merges. Shadowed entries incur no merge-copy charge. Each
      merged-in entry carries `Via::Merge { directive }`.

Under `Apply`, the directive key, source containers and their children are
constructed and charged as written, including one source-entry overhead for the
directive. They consume source collection levels even when the directive or a
shadowed value disappears from the final mapping. This applies to inline and
aliased source mappings and source sequences. The directive is not a stored
ordinary entry. A second directive is `DuplicateKey` after its value construction
and source-entry charge, before merge-source validation at mapping completion.

At mapping completion, validate the source kind and every source-sequence item
before admitting merged entries. Traverse sources in their defined order;
discard entries shadowed by any explicit key or an earlier source before copy
admission. For each remaining entry, check that explicit entries plus already
admitted merged entries leave capacity below `max_mapping_entries`, then check
the copy budgets in §5.4. Capacity and copy failures use the directive's start
as their source site and include its `merge_directive` related span. The capacity
check MUST precede the logical copy or insertion; it is not a final oversized-
mapping check. Shadowed entries consume no result capacity or merge-copy charge.

Moving admitted source entries into the receiving mapping cannot increase their
collection nesting. Source checks are not refunded, but completed mapping metrics
MUST describe only the final post-merge tree, including its retained tag wrapper.
They exclude removed directives and shadowed entries. Historical work charges
and completed-subtree metrics are different quantities; neither may substitute
for the other in a later anchor registration or alias-copy preflight.

### 5.4 Anchors and aliases

- An alias resolves to the most recent definition of its anchor.
- When an anchored node completes, the builder caches its node count,
  owned-byte weight, and collection height H (§6), all from the final post-merge
  subtree using checked arithmetic. The count includes every retained wrapper.
- Before copying, all of the following MUST hold; otherwise `LimitExceeded`
  is raised **before** the copy. Check them in this order:

  `p + H(anchor) ≤ max_depth`
  `nodes + count ≤ max_nodes`
  `expanded_nodes + count ≤ max_expanded_nodes`
  `owned_bytes + weight ≤ max_owned_bytes`

  Here `p` counts collection ancestors at the alias position (§6). A violation
  uses the alias use site's start, the violated limit and its configured value.
  Checked arithmetic overflow fails that bound at the same priority. No copy
  may begin until every applicable check succeeds. The same node, expanded-node,
  owned-byte order applies to merged-entry copies after capacity admission;
  their placement is already bounded by source admission (§5.3).

- At an alias event, first admit its increment against `max_alias_events`, then
  validate the target, then perform copy preflight. A still-open most-recent
  definition is `RecursiveAlias`; it MUST NOT fall back to an older definition
  of the same name. An undefined anchor is `UnknownAnchor`; the pinned engine
  reports it during source validation, before construction admission (§7.1).
  `RecursiveAlias` remains a construction error subject to that phase order.

- The copy's root carries `Via::Alias { use_site }`. Nodes inside the copy
  keep their **definition** spans (§7.2).
- Anchor definitions are document-local. Starting a new definition shadows the
  previous definition of that name; completing it registers that definition's
  subtree without changing which definition is most recent. Name binding follows
  definition-start order, not completion order: completion of an older enclosing
  definition MUST NOT restore it over a newer nested definition of the same name.
  Document boundaries discard anchor bindings, not whole-call budget charges.

For example, `[&a [&a 1], *a]` produces `[[1], 1]`: the alias selects the inner
scalar, not the later-completing outer sequence. Both completed definitions
still incur their registration charges (§6).

**Copy-tie example.** In `[&a [0], [[*a]]]`, immediately before the alias,
construction has charged 5 nodes, 0 expanded nodes and 448 owned bytes including
128 registration bytes. Its target has count 2, weight 128 and H=1; use-site p=3.
Successful copying requires depth 4, nodes 7, expanded nodes 2 and owned bytes
576. With limits `(max_depth, max_nodes, max_expanded_nodes, max_owned_bytes)`
set to `(3, 6, 1, 575)`, all four copy checks would fail and `max_depth` wins at
the alias start. Set scalar/key limits to 64 and keep other limits noncompeting
so the options and prefix are admissible. Raising each of the first three
limits in order to 4, 7 and 2 exposes nodes, expanded nodes and owned bytes
respectively; finally raising owned bytes to 576 permits the copy.

### 5.5 Documents

- `load`, `load_with`, and `load_bytes` return one document. A second document
  is a `MultipleDocuments` error at its start. Typed string entrypoints use
  these single-document semantics before applying §8 conversion rules.
- `load_all`, `load_all_with`, and `load_all_bytes` return the documents actually
  present in the stream. They are all-or-nothing: the first error ends the call
  and nothing partial is returned. Exceeding `max_documents` is `LimitExceeded`,
  not `MultipleDocuments`, even when that configured limit is one.

| Input                               | Single-document entrypoints                        | Multi-document entrypoints                 |
| ----------------------------------- | -------------------------------------------------- | ------------------------------------------ |
| Empty input, including a BOM alone  | One Null document with an empty source span at EOF | Zero documents                             |
| Only comments and blank lines       | One Null document with an empty source span at EOF | Zero documents                             |
| Bare `...` without a document       | One Null document with an empty source span at EOF | Zero documents                             |
| Explicit `---` with no node content | One Null document, `explicit_start = true`         | One Null document, `explicit_start = true` |

For a stream with no documents, the single-document result has
`yaml_version = None`, `explicit_start = false`, and a root with no tag or
alias/merge provenance. Its empty span is `NodeSpan::Source` at the original
input's EOF, **not** `Synthetic`; line and column follow §7.2.

An explicit empty document instead has its root's empty source span at the
boundary ending that document: the first byte of `...` or the following `---`,
or EOF when neither follows. Comments and blank lines before that boundary do
not create a node. Thus `---\n` has a Null root at byte 4, line 2, column 1;
`---\n...\n` has its Null root at the same point, before the `...`. Document
metadata records any YAML directive and the explicit start. This root rule is
separate from the omitted-child placement rule in §7.2.

For `---\n...\n---\n`, multi-document entrypoints with `max_documents >= 2`
return two explicit empty documents. Both have `yaml_version = None`,
`explicit_start = true`, and a Null root. The first root has an empty source
span at byte 4, line 2, column 1 (the `...`); the second is at byte 12, line 4,
column 1 (EOF). Single-document entrypoints instead report `MultipleDocuments`
at the second `---`, byte 8, line 3, column 1.

- M3: a document iterator over an **in-memory** input, fused after the first
  error.

## 6. Resource accounting

```rust
#[non_exhaustive]
pub struct Limits {            // defaults are proposed, not yet measured
    pub max_input_bytes: usize,       //   8 MiB   raw length, checked before UTF-8 validation
    pub max_depth: usize,             //   128     source/copy collection levels; ≤ 255
    pub max_nodes: usize,             //   250_000 materialized nodes, whole call
    pub max_expanded_nodes: usize,    //   100_000 nodes created by alias or merge copies
    pub max_alias_events: usize,      //   10_000
    pub max_owned_bytes: usize,       //   64 MiB  cumulative logical charges (below)
    pub max_scalar_bytes: usize,      //   1 MiB   checked on the event, before owning it
    pub max_key_bytes: usize,         //   64 KiB  weight of one key subtree
    pub max_mapping_entries: usize,   //   100_000 per mapping
    pub max_documents: usize,         //   1_024
    pub max_output_bytes: usize,      //   64 MiB  wire encoding (§11)
    pub max_warnings: usize,          //   1_000   (M3 lint), then one truncation marker
}
```

**Loader collection depth.** Let `p` be the number of collection ancestors at
a processing position. The root position has `p = 0`. Descending from a Sequence
or Mapping to an item, key or value increments `p`; a retained tag does not.
A collection consumes level `p + 1` and MUST satisfy `p + 1 <= max_depth` before
its contents are constructed. Scalars consume no additional collection level.
Source validation checks all collections as written, including empty collections,
mapping keys and merge-source containers, with mixed block/flow nesting counted
together. It does not expand aliases. Each document resets this nesting context.
This is not a promise that a scanner reads no lookahead bytes (§7.1, §9).

Define completed-subtree collection height H independently of source history:

- Scalar: `H = 0`.
- Tagged: `H = H(inner)`.
- Sequence or Mapping: `H = 1 + max(H(children))`, with `max(empty) = 0`.
  Mapping children include both keys and values.

Thus an empty collection has height 1. Core tags add no wrapper; retained tags
are transparent to H but still consume nodes and bytes. An alias copy at position
`p` requires `p + H(anchor) <= max_depth`. Apply uses final post-merge H for later
copies, while the original source checks and work charges remain in force.
There is no constant bound on the difference between source depth and final H:
explicit keys can shadow arbitrarily deeper source values within the input limit.

With other budgets noncompeting, these are the minimum legal `max_depth` values:

| Input | Merge mode | Minimum | Reason |
| --- | --- | --- | --- |
| `0` | Reject | 1 | H is 0, but configured limits must be positive |
| `[]` or `!t [0]` | Reject | 1 | Root collection, transparent tag |
| `[[]]` | Reject | 2 | Empty inner collection counts |
| `{? []: 0}` | Reject | 2 | Collection key counts |
| `{a: &a {x: 0}, b: &b {<<: *a}, c: [*b]}` | Apply | 3 | Both aliases have p=2 and completed target H=1 |
| `{a: &a {x: 0}, b: &b {<<: [*a]}, c: [*b]}` | Apply | 4 | Source-list alias has p=3; later alias still needs only 3 |
| `{m: {x: 0, <<: {x: [[[1]]]}}}` | Apply | 6 | Shadowed source still consumes levels; final H is 2 |

For boundary recipes, let `S_n(Y)` be `n` opening `[` bytes, the YAML fragment
`Y`, then `n` closing `]` bytes. At `max_depth = 128`, `S_127(0)` and `S_128(0)`
succeed; `S_129(0)` fails source-depth validation at the 129th `[`.
In `{a: &a [], b: S_k(*a)}`, substitute that construction for `S_k(*a)`:
`k = 125, 126, 127` gives alias insertion levels 127, 128, 129 respectively.
The first two succeed; the third has source depth 128 but fails at the alias
use. Likewise test levels 254/255/256 at legal configured depth 255. These are
recipes for exact input bytes, not a claim that the notation `S_k` is YAML.

These loader definitions do not redefine error-path key nesting (§11.1) or
establish stack safety for arbitrary caller-built trees and foreign deserializers.

**Owned-byte weight** is a deterministic accounting quantity, not measured
memory and not an allocator bound:

`weight(node) = 64 + string_bytes(node) + tag_bytes(node) + 24 × mapping_entries(node) + Σ weight(children)`

`string_bytes` is the UTF-8 byte length only for a String node. `tag_bytes` is
the sum of UTF-8 lengths of `uri`, `handle`, and `suffix` only for a Tagged
wrapper; it is zero for every other node. `mapping_entries` is nonzero only for
a Mapping; its children include both keys and values. A Tagged wrapper has one
child. Thus a retained tag is counted once, on its wrapper, not on its payload.

Worked subtree weights (logical bytes, not allocator or resident-memory sizes):

| Input | Weight | Charges |
| ----- | ------ | ------- |
| `!t x` | 133 | 64 wrapper + 4 tag bytes + 64 inner String + 1 text byte |
| `[~, ~]` | 192 | Three nodes at 64 each |
| `{a: b}` | 218 | Three nodes at 64 + one entry at 24 + two text bytes |

For `!t x`, the tag charge sums URI `!t`, handle `!` and suffix `t`: 2 + 1 + 1.

- **A retained tag is a wrapper node.** A `Tagged` node and its inner node are
  two nodes. The wrapper weighs 64 plus its tag bytes (`uri`, `handle`,
  `suffix`). This applies to scalars and collections. A collection's tag is
  checked and charged at its start event, so tag errors precede errors inside
  the collection. Core tags are not retained and add no wrapper. Anchor
  storage uses the same subtree weight, including wrappers; its counter scope
  is specified below. Alias copies count both wrapper and inner nodes.
- **Charging is monotonic within a call.** Weight is charged when a node is
  created — by parsing, alias copy, or merge copy — and never refunded.
- The weight and expansion counters are the **normative gates**. A rejected
  expansion MUST fail before it copies.
- Counters use checked 64-bit arithmetic; overflow raises `LimitExceeded`.
- Each completed source mapping entry, including an Apply directive, charges
  its 24-byte overhead after key/value construction and before entry admission.
  An entry discarded by a later policy error is not a refund.
- `max_key_bytes` compares the completed key subtree's logical weight before
  cloning a diagnostic path key, constructing its associated value or inserting
  the entry. Nothing is serialized to measure it. Earlier incremental node,
  owned-byte, scalar, depth and alias-copy limits remain active while building
  the key; this check does not authorize unlimited intermediate construction.
  An oversized key fails at its source start, with alias-related provenance
  under §7.2. A competing earlier construction failure wins. Source syntax/depth
  validation still precedes this construction phase (§7.1).
- After the completed-key weight check, reject NaN anywhere in an ordinary key
  as `InvalidKey` before cloning its diagnostic path or constructing its value.
  Thus an oversized NaN-bearing key fails `max_key_bytes`; an in-budget NaN key
  fails `InvalidKey` even if its associated value would have a resolution error.

**Anchor-cache accounting.** Registration of each completed anchored subtree
charges its full logical weight to `max_owned_bytes` before cache copying. It
charges **no additional `max_nodes` or `max_expanded_nodes`**. This is a logical
registration charge even if the implementation shares or moves storage. It uses
the final post-merge subtree weight, including every retained wrapper; anchor
names and physical cache/hash-table overhead are not part of this formula.
Nested registrations each charge their own complete subtree. Redefinition and
document reset release bindings without refunding earlier charges. Every later
alias copy additionally charges nodes, expanded nodes and bytes under §5.4.
Registration failure is `LimitExceeded(max_owned_bytes)` with the configured
limit and the completed anchored node's source start; it is not attributed to
a hypothetical later alias.

The following whole-call totals assume successful loading with noncompeting
limits; scalar text for Null, Bool, Int and Float contributes zero as before.
Escaped `\n` in the stream row denotes an actual line break.

| Input | Materialized nodes | Expanded nodes | Owned bytes, including registration |
| --- | --- | --- | --- |
| `[&a 1]` | 2 | 0 | 192 = 128 construction + 64 cache |
| `[&a 1, *a]` | 3 | 1 | 256 = 192 construction/copy + 64 cache |
| `[&a 1, &a 2, *a]` | 4 | 1 | 384 = 256 construction/copy + 128 registrations |
| `[&a [&b 1], *a]` | 5 | 2 | 512 = 320 construction/copy + 64 inner cache + 128 outer cache |
| `[&a [&a 1], *a]` | 4 | 1 | 448 = 256 construction/copy + 64 inner cache + 128 outer cache |
| `[&a !t x, *a]` | 5 | 2 | 463 = 64 sequence + 2 × 133 tagged subtree + 133 cache |
| `--- &a 1\n--- &a 2\n` | 2 | 0 | 256 = 128 construction + 128 registrations |

These are normative arithmetic examples for the proposed accounting policy,
not measurements or implementation observations. Zero expanded nodes is an
actual counter value, not a legal configured expansion limit; limits remain
positive. Test below/at/above each
total with other limits noncompeting and valid options. In particular, small
owned-byte budgets require correspondingly small scalar/key limits.

Example: with a 1 MiB anchored scalar and 10,000 aliases, the call is
rejected by `max_owned_bytes` at roughly the 63rd copy.

**Counter scope.** Cumulative node, expanded-node, owned-byte, alias-event and
document counters cover the whole `load`/`load_all` call, including documents
already built and discarded merge-source structure. Depth resets per document;
mapping-entry limits apply per mapping and scalar/key limits per scalar/key.
For the M3 iterator, tree counters reset
per document; input, document, and alias-event counters do not. Memory a
caller retains after a call returns is the caller's.

**Validation.** `Options::validate()` runs before processing input on every call:
every limit MUST be ≥ 1; `max_depth` MUST be ≤ 255;
`max_scalar_bytes` and `max_key_bytes` MUST be ≤ `max_owned_bytes`. An invalid
configuration is `InvalidOptions`. There is no unlimited setting.

**Allocation categories.** Memory falls into four categories: engine (scanner
and parser state), retained tree, anchor storage, and temporary adapter work
(key hashing, merge assembly, encoding buffers). Only the weight counters are
guaranteed. Peak allocation and resident memory are measured and reported
per category, not asserted against the weight budget.

The 64-byte node charge does not change when an implementation's `Node` layout
changes. Retained-tree estimates and a total peak measurement do not establish
per-category peak attribution or prove that rejected expansions allocate no
copies. Those require separate measurements and instrumentation. A hard process
memory ceiling requires external enforcement, not a multiplier on logical weight.

**Engine allocation.** The engine buffers a scalar while scanning it, before
the adapter sees the event, so the adapter cannot cap that allocation at
`max_scalar_bytes`. Its bound is `max_input_bytes` plus engine overhead; this
is an assumption to be measured, not a guarantee.

**Work and latency.** There is no cancellation in this revision. Work is
bounded by input size plus the expansion budgets. Latency is measured per
default profile, not promised.

**Bounded diagnostics.** Error messages are ≤ 4 KiB. Human-readable key
renderings inside messages are truncated to 256 bytes with a `…` marker. A
structured error path carries full keys (§11.1); a path has at most
`max_depth` segments, each key is bounded by `max_key_bytes`, and the whole
error envelope is bounded by `max_output_bytes`.

Type descriptions (`expected`, `found`) and field-name descriptions are each
bounded by 1,024 UTF-8 bytes. Descriptive bounds include the `…` truncation
marker and apply before JSON escaping; the escaped envelope must independently
fit `max_output_bytes`. They never authorize truncating structured path keys.
At encoding, recheck path length, each key's logical weight (including its
64-byte root charge), and collection nesting against the supplied options,
even for errors previously constructed under different limits. Reject oversized
identity before writing; truncate only descriptive text while writing (§11.1).

## 7. Errors and locations

### 7.1 Types

```rust
pub struct Location { pub byte_offset: usize, pub line: usize, pub column: usize }
pub struct Span { pub start: Location, pub end: Location }

pub enum Site {
    Source(Location),                                  // input-attributable error on a parsed node
    NotSource { last_known: Option<Location> },       // Internal, InvalidOptions, OutputLimitExceeded, synthetic nodes
}

pub struct Error(Box<ErrorDetail>);   // opaque, one owning pointer

#[non_exhaustive]
pub struct ErrorDetail {
    pub kind: ErrorKind,
    pub site: Site,
    pub related: Vec<Related>,       // e.g. first duplicate, alias use site, merge directive
    pub path: Vec<PathSegment>,
    pub message: String,             // human-readable, bounded, not a contract
    pub engine_detail: Option<String>,
}
impl Error {
    pub fn new(kind: ErrorKind, site: Site, message: impl std::fmt::Display) -> Self;
    pub fn into_detail(self) -> ErrorDetail;
    pub fn location(&self) -> Option<Location>;
}
impl std::ops::Deref for Error {
    type Target = ErrorDetail;
    fn deref(&self) -> &ErrorDetail;
}
impl std::ops::DerefMut for Error {
    fn deref_mut(&mut self) -> &mut ErrorDetail;
}
pub struct Related { pub role: RelatedRole, pub span: Span }
pub enum PathSegment { Key(Box<Value>), Index(usize) }

#[non_exhaustive]
pub enum ErrorKind {
    InvalidUtf8 { invalid_len: Option<usize> }, Syntax(SyntaxKind), UnsupportedYamlVersion,
    LimitExceeded(LimitKind), InvalidOptions,
    DuplicateKey, InvalidKey, UnknownAnchor, RecursiveAlias,
    MergeRejected, MergeSource, UnknownTag, TagKindMismatch, TagValueMismatch,
    IntegerOutOfRange, FloatOutOfRange, MultipleDocuments,
    Type { expected: String, found: String }, MissingField, UnknownField,
    Custom, OutputLimitExceeded, Internal,
}
```

**Ownership and construction.** `Error` is a boxed, pointer-sized owning handle;
this is not a C ABI layout promise and does not imply that every
`Result<T, Error>` is pointer-sized. `ErrorDetail` owns the diagnostic fields and
may have additional private bookkeeping. `Error` implements `Debug`, `Display`,
`std::error::Error`, and `serde::de::Error`.

`Type` owns its `expected` and `found` descriptions; they need not have static
lifetimes. Crate-produced descriptions, including dynamic Serde descriptions,
are formatted directly into buffers of at most 1,024 UTF-8 bytes each, truncating
at a character boundary with `…` included in that bound. Field-name descriptions
use the same bound. Encoders enforce these bounds again on caller-supplied or
mutated fields. The complete Rust payload shapes for other kinds remain a freeze
item; their required wire members are specified in §11.1.

`Error::new` initializes empty `related` and `path` vectors and no
`engine_detail`. It formats the message into a UTF-8-safe buffer bounded by §6,
truncating with `…` when necessary; it MUST NOT first allocate an unbounded
formatted message. The supplied kind and site are retained. `location()` returns
`Some(location)` only for `Site::Source`, never `last_known` as the offending
location.

Custom deserializers construct errors through
`<Error as serde::de::Error>::custom(message)` and the other Serde error trait
constructors. Before attribution, these use
`Site::NotSource { last_known: None }`; yamlprims attaches the current node's
provenance and path when propagating them through its deserializer. They use the
same bounded formatting rule. The related/path/engine-detail allocation limits
and error-kind payload completeness remain M1 freeze items in
[the design review](../design-review.md).

**Migration from revision 1.0's field-bearing `Error`.** Borrowed field access
continues through `Deref`, and explicit mutation through `DerefMut` is supported.
Struct patterns such as `Error { kind, .. }` no longer compile. Moving a non-Copy
field through dereferencing also fails: use `&error.kind` to borrow and
`error.into_detail().kind` to take ownership. Likewise, move the message using
`error.into_detail().message`, not `error.message`. Matching without moving a
payload may still compile; that does not make all field moves valid.

Caller mutation can invalidate diagnostic bounds. Encoders MUST enforce the
applicable diagnostic and output bounds again rather than trusting mutable
fields, and MUST revalidate the wire `kind`/`detail` pairing (§11.1).
This includes the structured-identity preflight and descriptive truncation rules
in that section. Encoding borrows the error without modifying it; successful
truncation of its encoded descriptions does not rewrite its owned fields.
Caller-owned allocations are outside the crate's allocation guarantee.

**Site depends on provenance, not on kind alone.** An input-attributable
error on parsed input MUST use `Site::Source`. An error on a synthetic node
(hand-built, `from_node`) uses `Site::NotSource { last_known }`, as do
`Internal`, `InvalidOptions`, and `OutputLimitExceeded`. `last_known` is the
nearest real source ancestor; it is context only and is never presented as the
offending position.

**Error precedence.**

YAML-loading entrypoints, including the loading phase of typed string calls,
use the following logical phase order to select their result. This does not
prescribe a number of physical parser passes. Encoder and `from_node` preflights
remain their own operations; this is not a global ordering of every `ErrorKind`.

1. Validate options. `InvalidOptions` precedes any examination of input and has
   `Site::NotSource { last_known: None }`.
2. Check raw input byte length, including any BOM, against `max_input_bytes`.
   A size failure wins over invalid UTF-8 and all later phases.
3. Strip only the optional initial UTF-8 BOM and validate UTF-8, preserving
   original-byte coordinates. The first invalid byte is `InvalidUtf8` (§7.2).
4. Perform **bounded source validation**: validate YAML grammar and source
   collection depth in yielded event/error order. Alias expansion and owned-tree construction
   budgets do not participate in this validation. At a valid collection-start
   event, test the §6 level before
   descending further. An excess is `LimitExceeded(max_depth)` at that
   collection's source start with the configured limit. If the engine yields an
   error before the next valid event, report that error with the translation
   below. Stop at the first yielded engine error or source-depth rejection;
   do not resume validation to search for later syntax. Scanner lookahead may
   yield an error before an earlier collection start becomes available; this
   is not a globally earliest-byte ordering. A source-depth safety stop MUST
   NOT be relabeled as malformed YAML.
5. Only if source validation succeeds may resolution, structure, tag, document
   and materialization-budget failures determine the result. The first failing
   construction operation in event order wins, using the operation-specific admission order
   in §§5–6. Errors that depend on a completed key, value, collection or merge
   source occur at that operation, not retroactively at the reported source byte.

The pinned engine's terminal `UnknownAnchor` and `UnsupportedYamlMajorVersion`
failures select `UnknownAnchor` and `UnsupportedYamlVersion`, respectively,
at their native diagnostic sites. Its `RecursionLimitExceeded` selects
`LimitExceeded(max_depth)` with the caller's configured depth and native site,
never generic syntax. These are the non-syntax terminal engine exceptions for
the string-input path. Other grammar/profile failures retain `Syntax(kind)`.
Later syntax beyond a terminal failure is not searched or promised. A syntax
failure reached by source validation precedes earlier construction-only errors,
including duplicates, rejected tags, recursive aliases and materialization
limits. Source validation does not charge materialized-node, expansion
or cache budgets; it is bounded by raw input, source depth and the engine's
fixed scanner guards (§9). The fixed non-depth guards listed there retain
syntax-classified failures in this draft; exceeding a profile restriction is
not independent evidence of malformed YAML. The closed syntax-error vocabulary
remains an M1 freeze item.

A source-validation pre-pass MAY implement phase 4; its cost is part of measured parse
cost. A combined implementation MUST preserve the same observable order without
constructing or copying past an applicable safety bound. After a construction
failure it may continue only bounded source validation to determine precedence,
not prohibited construction or expansion. Scanner lookahead needed
to produce the next event is not an alias expansion or permission to continue
source validation after a reported depth failure. For malformed text that cannot
produce a valid next event, do not infer a hypothetical deeper tree from its bytes.
Validation MUST use original input without per-failure recovery reparsing. A
separate validation pass consumes and discards events without constructing an
owned tree or expanding aliases. The engine may retain its own scanner/parser
storage; this is not a constant-total-memory guarantee.

Examples below assume other options are valid and noncompeting:

| Competing failures | Selected result |
| --- | --- |
| Invalid options and invalid UTF-8 | `InvalidOptions` |
| Oversized raw input and invalid UTF-8 | `LimitExceeded(max_input_bytes)` |
| Source-depth excess, followed by a later syntax failure | First source-depth failure |
| Syntax failure before a valid over-depth collection event | First syntax failure |
| Unknown anchor yielded before later syntax | `UnknownAnchor` at the native alias site |
| Unsupported YAML major yielded before later syntax | `UnsupportedYamlVersion` at the native directive site |
| Duplicate key or rejected tag, followed by syntax failure within source-depth bounds | Syntax failure |
| Materialized alias depth exceeds the limit, followed by syntax failure within source-depth bounds | Syntax failure |
| Oversized completed key followed by a resolution-invalid value, with valid source syntax/depth | `LimitExceeded(max_key_bytes)` |

Concrete paired-failure inputs use escaped line breaks and original byte offsets,
with other limits noncompeting:

| Input | Selected result and byte offset |
| --- | --- |
| `a: 1\na: 2\nb: [\n` | `Syntax(unclosed_flow_collection)`, opener 13 |
| `a: *missing\nb: [\n` | `UnknownAnchor`, alias 3 |
| `%YAML 2.0\n---\n[\n` | `UnsupportedYamlVersion`, directive 0 |
| `a: &a []\nb: [[*a]]\nc: [\n`, max_depth=3 | `Syntax(unclosed_flow_collection)`, opener 22 |
| `? ab\n: [\n`, max_key_bytes=1 | `Syntax(unclosed_flow_collection)`, opener 7 |
| `a: !t 1\nb: [\n`, unknown_tags=Reject | `Syntax(unclosed_flow_collection)`, opener 11 |
| `&a [*a]\n---\n[\n` | `Syntax(unclosed_flow_collection)`, opener 12 |

These are proposed expectations for this revision, requiring independent review
under §13; they do not rebind historical profiles.

During construction, alias-event admission precedes target validation and copy checks (§5.4).
Copy ties use depth, nodes, expanded nodes, then owned bytes; ordinary keys
check weight then validity before their values, followed by entry capacity and
duplicate identity after value construction/overhead;
merge admission uses source validity, shadow filtering, capacity, then copy
budgets. These local orders do not reorder failures across validation phases.

### 7.2 Coordinates

- **`byte_offset`** indexes the **original input bytes, including any BOM**.
  A UTF-8 BOM is stripped before parsing and its length added to every
  offset.
- **`line`** is 1-based. A line break is LF, CRLF, or a lone CR; CRLF counts
  as one break. U+0085, U+2028, and U+2029 are content, not breaks.
- **`column`** is 1-based, in Unicode scalar values from the line start. On
  line 1 the BOM is not a column. A tab counts as one column.
- If the engine cannot supply a byte offset, the result is `Internal` with
  `Site::NotSource`, never a fabricated zero.

| Thing                            | Location rule                                                                                                                                                                                                                                                                                                                                      |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scalar / collection node         | `start` = first content byte (after tag and anchor properties); `end` = exclusive end of content                                                                                                                                                                                                                                                   |
| Ordinary block sequence start    | the first `-`: this includes a sequence indented under its parent, a top-level sequence, or a sequence nested in a sequence item. `k:⏎  - x` starts at `-`; `k:⏎  - - x` at the outer `-`; `- - x` at the first `-`.                                                                                                                               |
| Indentless block sequence start  | a mapping value at its key's own indentation starts at the first token after its first `-`: `k:⏎- x` at `x`; `k:⏎- !t x` at `!t`; `k:⏎- &a x` at `&a`; `k:⏎- *a` at `*a`; `k:⏎- - x` at the inner `-`. An omitted first item (`k:⏎-⏎- x`) starts at the line break after the `-`; `k:⏎- # c⏎  x` starts at the line break ending the comment line. |
| Flow sequence start              | `[`; a sequence's own properties remain in `tag_start`.                                                                                                                                                                                                                                                                                            |
| Block collection end             | exclusive end of its last content line, excluding the trailing line break                                                                                                                                                                                                                                                                          |
| Omitted value (`a:`), empty node | `start == end` at the empty node's position                                                                                                                                                                                                                                                                                                        |
| Empty or comment-only input      | `start == end == input.len()`                                                                                                                                                                                                                                                                                                                      |
| Tag                              | `tag_start` at the tag token                                                                                                                                                                                                                                                                                                                       |
| Alias-copied subtree             | each node keeps its **definition** span; the copy's root has `Via::Alias { use_site }`; diagnostics inside a copy report the definition in `site` and the use site in `related`                                                                                                                                                                    |
| Merged-in entry                  | definition span plus `Via::Merge { directive }`, also in `related` on errors                                                                                                                                                                                                                                                                       |
| Engine diagnostic               | native diagnostic marker, translated to original-byte coordinates; missing closure or a mismatched delimiter may point at its unmatched opener. Use EOF only when the engine supplies EOF, with that point's line and column |
| Invalid UTF-8                    | `byte_offset` = length of the valid prefix; line and column over the valid prefix; `invalid_len` = length of the invalid sequence, or None when truncated at end                                                                                                                                                                                   |
| Typed conversion error (§8)      | span of the node being converted, with `path` to it                                                                                                                                                                                                                                                                                                |

**Encoding examples.** With valid options and an input within `max_input_bytes`,
the bytes `FF FE` fail UTF-8 validation at byte 0; `00 00 FE FF` fail at byte 2.
The first two NUL bytes in the latter are valid UTF-8. BOM-less UTF-16LE bytes for
`k: v\n` (`6B 00 3A 00 20 00 76 00 0A 00`) are valid UTF-8 containing raw NULs:
they pass the input gate and fail YAML syntax validation at byte 1. An encoding's
intended meaning does not change how the UTF-8 input surface interprets its bytes.

**Block scalar ends.** A literal or folded scalar ends at the exclusive end of
its last source content line containing a byte other than ASCII space or tab,
before the line break. Trailing spaces and tabs on that line remain inside the
span. Kept blank lines, trailing breaks and the next line's indentation are
outside it. If there is no such content line, the end is the end of the line
containing the scalar's start. LF, CRLF and lone CR are breaks as defined above;
Unicode content MUST NOT be trimmed as if it were ASCII whitespace.

For a block scalar with a source content line, this changes neither the scalar's
start convention nor its decoded value.
Chomping can preserve line breaks in a value that its source span excludes.
For `k: |+\n  é  \n\n`, the scalar's byte span is `[8, 12)` and its value is
`"é  \n\n"`. For `k: >-\n  a\n  b\n`, the span is `[8, 13)` and the value is
`"a b"`. A source slice is not a reconstruction of a decoded scalar.

**Empty block scalar anchors.** A literal or folded scalar with no source content
line containing a byte other than ASCII space or tab keeps the engine-derived
start marker. If the engine span is zero-width, apply the omitted-child
line-break normalization below to that point, retaining `start == end` and the
same sibling/key/collection floor. Otherwise keep the start and apply the block
scalar end rule above. Do not recover the `|` or `>` indicator by scanning
backward. These are source anchors, not token ranges: adding a following sibling
can move an empty scalar's anchor. The scalar remains a String with its style,
not an omitted Null; chomping still determines its decoded value. Empty document
roots instead follow §5.5.

The following escaped input strings pin the first mapping value or sequence
item's byte span and decoded String value. `\n` and `\r` denote actual input
line-break bytes; the surrounding quotes are not part of the input.

| Input | Byte span | Decoded value |
| ----- | --------- | ------------- |
| `"k: \|\n  \n  \nx: 1\n"` | `[10, 10)` | `""` |
| `"k: \|\n\nx: 1\n"` | `[4, 4)` | `""` |
| `"k: \|\n\n"` | `[3, 4)` | `""` |
| `"k: \|\n"` | `[3, 4)` | `""` |
| `"k: >-\n\nx: 1\n"` | `[5, 5)` | `""` |
| `"k: \|+\n\n\nx: 1\n"` | `[5, 5)` | `"\n\n"` |
| `"k: \|\r\n\r\nx: 1\r\n"` | `[4, 4)` | `""` |
| `"k: \| # c\n\nx: 1\n"` | `[8, 8)` | `""` |
| `"- \|\n\n- a\n"` | `[3, 3)` | `""` |

**Omitted scalar children and zero-width empty scalars.** Normalize a zero-width
omitted or empty scalar node when its engine marker directly follows line breaks,
or follows only ASCII-space indentation on its current line after line breaks.
Step over that current-line indentation first, then place the empty span before
the immediately preceding line breaks. Spaces on an earlier blank line stop this
normalization; do not trim arbitrary whitespace. The point MUST NOT move before
the end of its preceding sibling, the end of its key (for a mapping value), or
its collection's start.

This includes omitted scalar explicit keys and values and zero-width empty block
scalars. A retained tag wrapper and its inner scalar move together when they
share the empty span. A zero-width collection, including one wrapped in a retained
tag, MUST NOT move. Alias-copy definition spans retain their existing exception;
explicit empty-document roots use §5.5. For `? a\n? b\n`, the omitted values remain
at byte offsets 3 and 7 respectively.

Revision 1.3 adds the following exact anchors. Each quoted input is an escaped
string as in the preceding table; `\u{feff}` denotes a three-byte UTF-8 BOM.
The first nine empty-block rows above are unchanged. The variant rows below are
explicit examples, not a general offset-shift formula for arbitrary YAML edits.

| Input | Selected node | Byte span | Value |
| ----- | ------------- | --------- | ----- |
| `"\u{feff}k: \|\n\nx: 1\n"` | Value of `k` | `[7, 7)` | String `""` |
| `"k: \|\r\rx: 1\r"` | Value of `k` | `[4, 4)` | String `""` |
| `"k: !t \|\n\nx: 1\n"` | Value of `k`: wrapper and inner | `[7, 7)` | Tagged `!t`, inner String `""` |
| `"k: !!str \|\n\nx: 1\n"` | Value of `k` | `[10, 10)` | String `""` |
| `"k: &a \|\n\nx: 1\n"` | Value of `k` | `[7, 7)` | String `""` |
| `"a: 0\nk: \|\n\nx: 1\n"` | Value of `k` | `[9, 9)` | String `""` |
| `"p:\n  ? a\n  ? b\n"` | First omitted value under `p` | `[8, 8)` | Null |
| `"p:\n  k: \|\n\n  x: 1\n"` | Value of `k` under `p` | `[9, 9)` | String `""` |
| `"?\n\n :\n"` | Root mapping's key mapping | `[4, 4)` | Mapping |
| `"?\n !m\n :\n"` | Root mapping's tagged key: wrapper and inner mapping | `[7, 7)` | Tagged `!m`, inner Mapping |

**Keep chomping at EOF.** The following exact inputs pin decoded values, not
additional source-span conventions:

| Input | Decoded value of `k` |
| ----- | ------------------- |
| `"k: \|+\n\n   "` | `"\n"` |
| `"k: \|+\n\n   \n"` | `"\n\n"` |
| `"k: \|+\r\n\r\n   "` | `"\n"` |

The header break is non-content. Under
[YAML 1.2.2 §8.1.1.2](https://yaml.org/spec/1.2.2/#8112-block-chomping-indicator),
`l-keep-empty` repeats `l-empty`, each requiring a physical break via
`b-as-line-feed`. The unterminated spaces-only tail in these examples contributes
no additional break. Trailing non-content whitespace can end at EOF under §6.6;
an EOF match does not synthesize a line feed. CRLF body breaks normalize to LF.

**Containment.** Every parsed child's span MUST lie inside its parent's span.
The exception is the link at an alias-copy or merged-entry insertion boundary,
where the inserted root keeps its definition span. That exception does not
disable containment checks inside the copied subtree: its descendants still
fit their definition parents. Bounds and UTF-8 coordinate validity apply even
at an insertion boundary.

Spans are data spans. Lexical spans for source editing are out of scope; no
backward scan for indicators is performed. The line-boundary normalization above
does not change the sequence-start conventions.

The sequence rules above are explicit exceptions to a uniform content-token
start convention. They follow the selected engine's event markers; changing
this convention requires a specification revision and new pinned expectations.

**Host offsets.** The TypeScript binding adds `utf16Offset`, computed over the
valid prefix and defined for `InvalidUtf8` too. Go uses byte offsets as they
are.

## 8. Typed deserialization

```rust
pub fn from_str<T: DeserializeOwned>(input: &str) -> Result<T, Error>;
pub fn from_str_with<T: DeserializeOwned>(input: &str, opts: &Options) -> Result<T, Error>;
pub fn from_node<T: DeserializeOwned>(node: &Node, opts: &Options) -> Result<T, Error>;
pub struct Spanned<T> { pub value: T, pub span: Option<Span> }   // None for synthetic nodes
pub struct TaggedValue<T> { pub tag: Tag, pub value: T }
```

**Guarantee.** Resolver and builder policy is identical for `load` and
`from_str`. Built-in conversions follow the table below. Conversions written
by callers — custom `Deserialize` impls, `deserialize_with`, normalizing map
key types — are outside this guarantee: they may coerce, panic, or allocate,
and two distinct YAML keys that a caller's key type normalizes to one Rust key
can collide inside that type's own visitor.

| Target                       | Accepts                                                                                         | Rejects (`Type` error at the node unless noted)                                      |
| ---------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `u8`…`u128`, `i8`…`i128`     | untagged Int in range                                                                           | String (even `"8080"`), Float (even `1.0`), Bool; out of range → `IntegerOutOfRange` |
| `f64`                        | Float; Int if exactly representable                                                             | String; Int that would round                                                         |
| `f32`                        | Float's binary64 Value rounded to binary32, nearest with ties to even (NaN, ±inf, ±0 kept); Int if exactly representable | a finite input whose rounded result is infinite → `FloatOutOfRange` |
| `bool`                       | Bool                                                                                            | String (`"true"`, `"yes"`), Int                                                      |
| `String`                     | String                                                                                          | Int, Bool, Null, Float                                                               |
| `char`                       | a String of one Unicode scalar value                                                            | otherwise                                                                            |
| `Option<T>`                  | **untagged** Null → None; otherwise `T` rules                                                   | —                                                                                    |
| `()` / unit struct           | untagged Null                                                                                   | otherwise                                                                            |
| struct                       | Mapping with String keys; `deny_unknown_fields` honored; `MissingField`/`UnknownField` located  | non-mapping                                                                          |
| maps (`HashMap`, `BTreeMap`) | Mapping; keys converted by these same rules, never stringified                                  | key type mismatch                                                                    |
| `Vec`, tuples                | Sequence (tuple length must match)                                                              | otherwise                                                                            |
| enum                         | unit variant as String; single-key Mapping `{Variant: value}`; **local** tag `!Variant payload` | global tags never select variants; anything else                                     |
| `TaggedValue<T>`             | Tagged (inner value by `T` rules)                                                               | untagged                                                                             |
| `Spanned<T>`                 | anything (by `T` rules)                                                                         | —                                                                                    |
| `Value`, `Node`              | anything, losslessly                                                                            | —                                                                                    |

**f32 conversion path.** A Float literal is first resolved to the binary64
`Value`; conversion rounds that value to binary32 without reparsing its source
spelling. The same conversion applies through `from_str` and `load` plus
`from_node`. Decimal Float resolution followed by this conversion can double
round: `1.00000005960464477539062500000000000001` yields `1.0f32`, whereas direct
decimal-to-binary32 rounding yields the next f32 above 1.0. The separate Int-to-f32
rule still requires exact representability.

**f32 boundary.** `3.4028235e38` exceeds `f32::MAX` but rounds to it, so it is
accepted. The overflow threshold is the midpoint `2^128 − 2^103`
(`3.4028235677973366e38`): it ties to even, rounds to infinity, and is
rejected. `3.4028235677973362e38` is accepted. Negative values are symmetric.

- **Tags into ordinary targets.** A Tagged value converted into anything
  other than `TaggedValue<T>`, `Value`, `Node`, or an enum is a `Type` error.
  Tags are never silently stripped.
- **Wrappers are transparent.** `Spanned<T>` and `Option<T>` pass the same
  node, tagged or not, to `T`, and `T` must accept tags or fail. A tagged Null
  into `Option<T>` is therefore `Some(T from the tagged Null)`.
- **Tagged empty scalars.** A non-core tag's inner value is the raw String
  (§4.4), so `!n` with empty content is `TaggedValue { tag: !n, value: "" }`:
  `Option<TaggedValue<String>>` accepts it; `()` rejects `""`.
- **Enum payloads under a local tag.** When a local tag selects an enum
  variant (`!Variant payload`), the payload text is Core-resolved before
  conversion into the variant's content type, exactly as if untagged: a plain
  scalar resolves by §4.1; a quoted or block scalar stays String. So
  `!Tuned 7` gives `Tuned(7u32)` and `!Tuned "7"` is a `Type` error for
  `Tuned(u32)`. This is the only re-resolution in the contract: the loaded
  `Value`, `TaggedValue<T>`, the wire format, and the bindings all see the raw
  String. Bindings MUST NOT advertise identical typed enum semantics.
- **Private protocols.** `Spanned`, `TaggedValue`, `Value`, and `Node` use
  reserved struct names so they are lossless through this crate's
  deserializer. Through a foreign deserializer: `Node` and `Value` are rebuilt
  as synthetic nodes from the data model (duplicate and NaN keys stay errors),
  `Spanned` gets `span: None`, and `TaggedValue` is an error.
- **`from_node` on hand-built trees.** Constructors keep the invariants, so
  the tree is well-formed. `from_node` applies `max_depth` and `max_nodes`.
- **Panics.** Crate-owned YAML-loading paths must handle input and invalid options
  without panicking. Caller conversions are excluded. Depth enforcement through
  crate-owned typed conversion remains required; the construction/cleanup support
  domain for arbitrary caller-built trees and foreign deserializers is deferred
  to the M1 freeze gate. Loader depth is not a bound on caller-owned Serde frames
  or every operation on a synthetic tree.

## 9. Engine configuration

The selected source is unmodified `granit-parser =2.0.0` with the features below.

| Engine option                                   | Value                                                                                    |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------- |
| features                                        | `std`, `error_messages` (comments off)                                                   |
| `strict_indentation`                            | `true` in this draft; required in every parsing pass, including any syntax-only pre-pass |
| flow and block nesting limits                   | bounded guards consistent with §6 source collection levels and §7.1 precedence             |
| simple-key lookahead                            | 1024 (engine default; the YAML simple-key rule)                                          |
| directive bytes / reserved directive parameters | 1024 / 16 (engine defaults)                                                              |

Changing the engine or its pinned version requires requalification against
the conformance suites (§13).

The normative depth is the combined block/flow collection count in §6, not an
engine's internal stack or separate flow/block counter. Keep finite engine
guards enabled in every pass. Their configuration and error translation MUST
admit the source shapes allowed by §6 and reject excess levels without making
them generic syntax failures. If the engine rejects a permitted source shape,
that is an implementation gap requiring correction and qualification, not a
new definition of `max_depth`. Qualify pure block, pure flow and mixed nesting,
including mapping keys and empty collections, at the configured boundaries.
Qualify scanner lookahead, implicit mappings and indentless sequences as well.
The finite native headroom needed to preserve combined event-depth enforcement
requires a source-level boundedness audit; passing boundary examples alone does
not establish it. No particular headroom formula is accepted by this draft.

Revision 1.0 pinned `strict_indentation = false`. Revision 1.1 requires strict
indentation as an accepted contract change under
[ADR-0003](../decisions/ADR-0003-strict-indentation.md), not as an unannounced
engine default update. There is no public compatibility toggle in `Options`.
The initial public API is intentionally strict-only. Lenient modes used by
qualification or migration tooling are outside this public contract; adding one
to the public API requires an established consumer need and a separate decision.
For example, this under-indented flow collection MUST be a syntax error:

```yaml
---
flow: [a,
b,
c]
```

This is a YAML conformance correction: the indentation requirement in
[YAML 1.2.2 §7.4](https://yaml.org/spec/1.2.2/#74-flow-collection-styles) makes
these flow entries invalid. The example is yaml-test-suite
[9C9N, “Wrong indented flow sequence”](https://github.com/yaml/yaml-test-suite/blob/v2022-01-17/src/9C9N.yaml),
marked `fail: true` upstream. Its format-level expectation is a syntax failure,
independently of yamlprims's profile. Revision 1.0's lenient acceptance must be
recorded as a difference from format truth, not an alternative valid YAML result.

Qualification compares both engine settings on the same pinned suite and
migration inputs; only the selected strict setting defines this revision's
profile, which MUST explicitly pin strict indentation in every parsing pass.
Report every suite ID whose verdict changes between settings. Preserve
revision 1.0 results as revision 1.0 results. A migration input
accepted only by the earlier leniency is a reported compatibility difference,
not grounds to rewrite a malformed-input expectation.

## 10. Public Rust API (M1)

```rust
#[non_exhaustive]
pub struct Options {
    pub merge_keys: MergeKeys,        // Reject (default) | Apply | AsKey
    pub unknown_tags: UnknownTags,    // Preserve (default) | Reject
    pub limits: Limits,
}
pub fn load(input: &str) -> Result<Document, Error>;
pub fn load_with(input: &str, opts: &Options) -> Result<Document, Error>;
pub fn load_bytes(input: &[u8], opts: &Options) -> Result<Document, Error>;
pub fn load_all(input: &str) -> Result<Vec<Document>, Error>;
pub fn load_all_with(input: &str, opts: &Options) -> Result<Vec<Document>, Error>;
pub fn load_all_bytes(input: &[u8], opts: &Options) -> Result<Vec<Document>, Error>;
pub fn from_str / from_str_with / from_node                          // §8
pub fn encode(docs: &[Document], opts: &Options) -> Result<String, Error>;   // §11
pub fn encode_error(error: &Error, opts: &Options) -> Result<String, Error>; // §11 error envelope
```

Library calls perform no filesystem access, environment interpolation,
network requests, or process-global configuration, and are safe to call
concurrently.

The byte entrypoints take explicit options; string entrypoints without `_with`
use `Options::default()`. All byte paths use the same raw-input limit, BOM and
UTF-8 gates. `load_all_bytes` has exactly the document semantics of `load_all_with`
and preserves original-byte coordinates.

`encode_error` borrows the error and writes one canonical error envelope. Like
`encode`, it validates options and returns no partial JSON on failure. It MUST
revalidate the current error kind and its associated detail against §11.1,
including after caller mutation, and preflight all structured path identity
before writing. Descriptive text is bounded while writing, without mutating the
borrowed error. Invalid identity is rejected rather than shortened. If even
the error envelope exceeds `max_output_bytes`, it returns a Rust
`OutputLimitExceeded` error with `Site::NotSource { last_known: None }`;
it MUST NOT recursively try to encode that error or bypass the budget.
The C ABI's fallback when an error
envelope cannot fit remains an M2 design gate (§11.5).

**CLI.** `yamlprims check FILE…` prints `file:line:col: kind: message` and
exits 1 on any error. `yamlprims dump FILE` writes the wire format.
`yamlprims dump --plain FILE` writes plain JSON or fails, naming the loss
(§11.3).

`check` and wire-format `dump` use multi-document semantics. An empty file is a
successful check; dumping it emits
`{"format":"yamlprims-value","version":1,"documents":[]}`. The plain-output
shape for zero or multiple documents remains part of the CLI freeze gate; it
MUST NOT silently synthesize a Null document under the single-document rules.

## 11. Wire format `yamlprims-value/1`

### 11.1 Grammar

The output is UTF-8 JSON with **canonical encoding**: no insignificant
whitespace; object members in exactly the order listed; strings escaped as in
RFC 8785 §3.2.2.2 (`"`, `\`, and controls only; short forms where they exist,
otherwise `\u00xx` in lowercase). **Numbers:** metadata numbers (`version`,
YAML version parts, byte offsets, lines, columns, indexes, counts, configured
limits) are canonical unsigned JSON integers in `0..=2^53−1`. YAML integers are
decimal **strings**, and floats are **bit strings**. No other number forms
appear.

```
Result    = {"format":"yamlprims-value","version":1,"documents":[Document*]}
          | {"format":"yamlprims-value","version":1,"error":Error}
Document  = {"yaml_version":null|[major,minor],"explicit_start":bool,"root":Node}
Node      = {"kind":K, <payload>, "span":Span|null, "via":Via|null, "tag_start":Loc|null}
  null      : (none)
  bool      : "value":true|false
  int       : "value":"<decimal>"           // ^(0|-?[1-9][0-9]*)$
  float     : "bits":"<16 lowercase hex>"   // IEEE 754 binary64
  string    : "value":"<string>"
  sequence  : "items":[Node*]
  mapping   : "entries":[{"key":Node,"value":Node}*]
  tagged    : "tag":{"uri":S,"handle":S,"suffix":S},"value":Node
Loc       = [byte,line,col]
Span      = [start_byte,start_line,start_col,end_byte,end_line,end_col]
Via       = {"alias":Span} | {"merge":Span}
PathKey   = Node with "span", "via" and "tag_start" null at every depth
Error     = {"kind":"<kind>","detail":Detail,
             "site":{"source":Loc} | {"not_source":{"last_known":Loc|null}},
             "related":[{"role":"<role>","span":Span}*],
             "path":[{"key":PathKey} | {"index":N}*],
             "message":S}
```

- **NaN.** The resolver produces only the canonical quiet NaN
  `7ff8000000000000`. The encoder canonicalizes any other NaN payload (for
  example from a synthetic node) to the same bits. NaN sign and payload are
  not part of the contract.
- **Paths are exact identities.** A `path` key segment carries the full
  structured key; display truncation (§6) never applies to it.

**Error vocabulary.** `kind` is the snake_case `ErrorKind` name. `detail` has
exactly these members:

| kind                             | detail                                                      |
| -------------------------------- | ----------------------------------------------------------- |
| `invalid_utf8`                   | `{"invalid_len":N\|null}`                                   |
| `syntax`                         | `{"syntax_kind":"<snake_case>"}` (closed set fixed with M1) |
| `limit_exceeded`                 | `{"limit":"<Limits field name>","configured":N}`            |
| `type`                           | `{"expected":S,"found":S}`                                  |
| `missing_field`, `unknown_field` | `{"field":S}`                                               |
| `output_limit_exceeded`          | `{"configured":N}`                                          |
| all others                       | `{}`                                                        |

**Error encoding validation.** `encode_error` MUST construct and validate the
wire `kind`/`detail` pair from the current `ErrorKind` and its associated payload.
It MUST NOT reuse stale detail from an earlier kind. Required members must be
present with the specified types; members belonging to another kind must not be
emitted. If the current error cannot form a valid pair, encoding returns a Rust
`Internal` error with `Site::NotSource { last_known: None }` and no JSON. It MUST
NOT fabricate a missing payload, emit a malformed envelope, or recursively
encode the rejection. A representation that makes invalid pairings unrepresentable
can enforce these rules by construction.

Before writing any JSON, `encode_error` MUST also preflight the current path
against the options supplied to this encoding call:

- More than `max_depth` path segments is `LimitExceeded` for `max_depth`.
- A key whose §6 logical subtree weight exceeds `max_key_bytes` is
  `LimitExceeded` for `max_key_bytes`. Count the root, both mapping keys and
  values, entry overheads and tag bytes/wrappers; never measure by serializing.
- A path key's collection nesting beyond `max_depth` is `LimitExceeded` for
  `max_depth`. For this preflight, the root starts at depth zero; descending
  from a Sequence or Mapping to a child adds one, while a Tagged wrapper's
  child retains its depth. Every wrapper still contributes its §6 weight.
  The path segment count and the key's nesting are separate checks.
- A `syntax_kind` must be an ASCII code of 1–64 bytes drawn only from lowercase
  letters, digits and `_`. A code outside that shape is an invalid kind/detail
  pair and is rejected as `Internal`. This shape check does not define or
  complete the closed `SyntaxKind` vocabulary, which remains an M1 freeze item.

Every such rejection has `Site::NotSource { last_known: None }`, returns no JSON
and is not recursively encoded. Limit errors identify the violated limit and
its configured value. Path segments and structured keys MUST NOT be dropped,
truncated or replaced with display strings to make an error encodable.

The preflight key walk MUST be iterative, visit borrowed values, charge nodes
before scheduling them and stop at the first bound. It MUST NOT first clone,
serialize or traverse the entire rejected subtree. Arithmetic must detect a
weight beyond the limit without wrapping. This bounds preflight work by the
path and key budgets, not by the size of a caller-created rejected tree.
Call this existing error-key nesting metric **P**: scalars and empty collections
have P=0; a nonempty Sequence or Mapping has `P = 1 + max(P(children))`; a Tagged
wrapper has its child's P. Mapping children include both keys and values.
This counts child edges, including edges to scalars, unlike loader collection
height H (§6). Revision 1.4 names the distinction without changing preflight
expectations or path-segment limits:

| Key tree | Loader H | Error-key P |
| --- | --- | --- |
| `[]` | 1 | 0 |
| `[0]` | 1 | 1 |
| `[[]]` | 2 | 1 |
| `[[0]]` | 2 | 2 |

There is no universal constant offset between H and P. Stack safety of arbitrary
caller-built trees, including destruction and operations without Options,
remains an explicit M1 gate in the design review.

While writing, truncate `message` to 4,096 UTF-8 bytes and each `expected`,
`found` and `field` description to 1,024 bytes, retaining a character-boundary
prefix plus `…` within the bound when truncation is necessary. Escape directly
into the bounded output; do not build an unbounded escaped temporary string.
Write path keys by reference without cloning their trees. `engine_detail` is
not in the wire grammar. Bounds on its storage and on `related` allocation
remain separate M1 gates; the entire encoded envelope, including `related`,
is always subject to `max_output_bytes`.

**`related` roles:** `first_definition` (DuplicateKey), `alias_use`,
`merge_directive`, `anchor_definition`.

**Decoders** reject unknown fields, unknown kinds, unknown versions,
non-canonical integers, and float bits that are not 16 hex digits. Adding a
field or kind requires a new `version`. Encoding is fallible: exceeding
`max_output_bytes` is `OutputLimitExceeded`, checked while writing.

**Round trip.** Rust → wire → host → wire is byte-identical, including
`tag_start`. Host decoders are lossless for every field, except that NaN
payloads are canonicalized.

### 11.2 Host types

- **Go.** `Value` is a sealed interface: `Null`, `Bool`, `Int{*big.Int}`,
  `Float{float64}` (built from bits), `String`, `Sequence`,
  `Mapping{[]Entry}`, `Tagged`. Every node carries a `Span`. Errors are
  `*yamlprims.Error`.
- **TypeScript.** A discriminated union on `kind`. `int.value` is a `bigint`;
  `float.value` is a `number` built from the bits. `mapping.entries` is
  `Array<[Node, Node]>`. Errors are a `YamlprimsError` with `byteOffset`,
  `utf16Offset`, `line`, `column`, `path`, and `related`.

### 11.3 Plain conversion loss matrix

`toPlain()` (TypeScript) and `ToPlain()` (Go) are explicitly lossy projections:
they drop source metadata and YAML kind distinctions that the host cannot
retain, such as Int versus Float in a TypeScript `number`. They **fail** on the
additional losses marked as errors below. Use the wire format for lossless data.

| Value                                       | TypeScript `toPlain`                                                                         | Go `ToPlain`                       |
| ------------------------------------------- | -------------------------------------------------------------------------------------------- | ---------------------------------- |
| Int within ±(2^53−1)                        | `number`                                                                                     | `int64`                            |
| Int outside that range                      | **error**                                                                                    | `int64` if it fits, else **error** |
| Float finite, −0.0, NaN, ±inf               | `number` (in memory)                                                                         | `float64`                          |
| Float in JSON text output (`dump --plain`)  | NaN/±inf → **error**; −0.0 written as `-0.0`                                                 | same                               |
| String key                                  | object member (object created with `Object.create(null)`, so `__proto__` is an ordinary key) | `map[string]any`                   |
| non-String key (Int, Bool, Null, composite) | **error**                                                                                    | **error**                          |
| Tagged                                      | **error**                                                                                    | **error**                          |
| spans, `via`, `tag_start`                   | dropped (documented)                                                                         | dropped                            |

### 11.4 Host-side conformance

For every corpus case, each binding decodes the wire bytes; asserts its host
values independently (bigint / `big.Int` equality, float bit patterns, tag
URIs, key kinds, spans); and re-encodes canonically and compares the bytes
with the Rust encoder's. The same suite covers error objects, paths, and the
loss matrix.

### 11.5 C ABI (M2)

**Proposed interface, not yet frozen.** The output-envelope budget/failure
protocol and pointer preconditions must be resolved under
[the M2 design gate](../design-review.md#m2-boundary-freeze) before an ABI release.

```c
uint32_t yp_abi_version(void);                         // 1
int32_t  yp_load(const uint8_t* ptr, size_t len, const char* opts_json, size_t opts_len,
                 yp_buffer* out);                      // 0 ok, 1 input error, 2 internal error
void     yp_buffer_free(yp_buffer* buf);               // frees and zeroes; zeroed buffer = no-op
typedef struct { uint8_t* ptr; size_t len; size_t cap; } yp_buffer;
```

- `ptr` may be NULL only when `len == 0`. `out` MUST be non-NULL.
- On every status, `out` holds one wire `Result` envelope. Status 0 carries
  `documents`; statuses 1 and 2 carry `error`, and status 2's kind is
  `internal`. The only exception: if allocating the envelope itself fails,
  `out` stays zeroed.
- `opts_json`: NULL with `opts_len == 0` means all defaults. Otherwise a JSON
  object `{"merge_keys":"reject"|"apply"|"as_key","unknown_tags":"preserve"|"reject","limits":{<Limits field>:N,…}}`.
  Every member is optional and defaults as in Rust. Unknown members, wrong
  types, and failed validation return status 1 with `invalid_options`. This
  schema is fixed with M1 and frozen at the M2 ABI freeze.
- The library owns `out`'s memory; the caller frees it exactly once with
  `yp_buffer_free`. Freeing a stale copy of a freed buffer is undefined
  behavior.
- Every entry point runs under `catch_unwind`, and the shared library is
  built with `panic = "unwind"`. This does not cover allocation-failure
  aborts or undefined behavior caused by invalid caller pointers.
- The Node-API binding passes no caller-supplied callbacks into Rust.

## 12. Design rationale

| Problem in existing stacks                                                                      | This specification                                                                      |
| ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| YAML 1.1 words (`yes`, `on`, `y`) silently become booleans, including in keys, and then collide | §4.1 Core schema only; §5.2 `DuplicateKey` with both locations                          |
| Typed deserialization coerces quoted strings (`"8080"` → `u16`)                                 | §4.1 quoted scalars are String; §8 no coercion                                          |
| Integers beyond 64 bits error or silently round to f64                                          | `Integer` covers `i128::MIN..=u128::MAX`; out of range is an error                      |
| Local and global tags are dropped                                                               | non-core tags kept at every depth (§3, §4.4); typed targets never strip them (§8)       |
| Typed maps keep the last duplicate silently                                                     | one `DuplicateKey` policy on every path                                                 |
| Merge keys skipped on one path, kept as a literal key on another                                | merge recognized by the builder under an explicit policy (§5.3)                         |
| Alias expansion and document count unbounded in bytes and nodes                                 | §6 aggregate budgets, checked before copying                                            |
| Multi-document iteration misbehaves after an error                                              | `load_all` all-or-nothing; iterator fused (M3)                                          |
| Duplicate-key and other structural errors reported without a location                           | `Site::Source` required for input errors (§7.1)                                         |
| String error paths are ambiguous                                                                | typed `PathSegment`s and exact wire path keys                                           |
| Dependence on an unmaintained machine-translated C parser                                       | a maintained safe-Rust event engine; no libyaml-lineage crate in the dependency closure |

## 13. Conformance

Every MUST in an implemented milestone MUST have an identified conformance
test before that milestone claims conformance. Expected results are produced
independently of the implementation, and no test is rewritten to match observed
behavior without a specification change. A specification-only release makes no
test-coverage or implementation-conformance claim.

### 13.1 Truth tables

Expected scalar and key results come from an **independent oracle**, written
from §4–§5 and not derived from the resolver, anchored by a hand-checked
golden subset. Oracle acceptance is separate from specification acceptance.

- **Booleans and nulls.** Every case variant of `true`, `false`, `null`, and
  the YAML 1.1 words `y`, `yes`, `n`, `no`, `on`, `off`, plus `~`, empty, `1`,
  and `0`. Positions: block mapping value, implicit key, explicit key (`? `),
  block sequence item, flow sequence item, flow mapping key, flow mapping
  value, top-level scalar. Styles: plain, single-quoted, double-quoted,
  literal, folded. Tags: none, `!!bool`, `!!str`, `!!null`, `!`, `!custom`.
- Rows are categorized as `value`, `resolution_error` (e.g. `!!bool "yes"` →
  `TagValueMismatch`), or `syntax_invalid` (e.g. a block scalar inside a flow
  collection or as an implicit key); syntax rows are reported separately.
  Explicit block-scalar keys (`? |`, `? >`) are valid. Expected block-scalar
  text applies chomping first (`|`, `|-`, and `|+` variants each).
- **Typed columns** (`bool`, `String`, `Option<bool>`) record the value or a
  located `Type` error.
- **Collision rows.** Sibling key pairs drawn from all words and styles
  assert `DuplicateKey` exactly when §5.1 equality holds.
- **Numbers.** Signs, the three bases, leading zeros; `u128::MAX`,
  `u128::MAX+1`, `i128::MIN`, `i128::MIN−1`; `-0`, `0.0`, `-0.0`; `1e9999`,
  `1e-400`; every `.inf`/`.nan` spelling; quoted and tagged forms of each,
  against every integer and float target, including both sides of the f32
  threshold for both signs.

### 13.2 Other suites

- **Public corpora.** The pinned [yaml-test-suite](https://github.com/yaml/yaml-test-suite)
  (events and error/no-error) and the
  [structcorpus](https://github.com/3leaps/structcorpus) cases for the
  profile that cites this specification.
- **Differential.** Comparison against other YAML stacks. Each difference
  cites a section of this document. A defect in another stack is an expected
  difference, not a target.
- **Resource.** With tiny configured budgets: a large scalar with many
  aliases; a shallow anchored subtree used at maximum depth; repeated and
  nested merges; counters at the edge of overflow; wire output expansion;
  cleanup on error paths. Expansion rejections MUST identify the limit and
  precede the prohibited copy, proven by copy instrumentation. Peak allocation
  is measured per category (§6), not asserted. Test load-limit boundaries with
  named configured values and valid options; test `max_documents` through stream
  APIs and `max_output_bytes` through encoders. Input-size failures belong to the
  input gate even though the Rust kind is `LimitExceeded`. M3 warning exhaustion
  uses its truncation marker, not a load-error pair.
- **Equality.** Reordered composite keys; equal URIs with different handles;
  the same value at different locations; nested NaN; ±0; Int vs Float; alias
  keys; mutation after indexing. `get` and duplicate detection agree on every
  case.
- **Merge.** Every recognition row, multiple sources, nested merges,
  overlapping sources, malformed sources, and the ordering rules.
- **Typed.** Every row of the §8 table, including tagged Null into `Option`,
  local and global tags into enums, tuple lengths, the f32 threshold and
  binary64-to-binary32 double-rounding example through both conversion paths,
  Int→float exactness, hand-built nodes, and a caller-defined normalizing key
  type (documented as outside the guarantee).
- **Locations.** Every node and error: `start ≤ end ≤ input.len()`; offsets on
  UTF-8 boundaries of the valid prefix; line and column agree with an
  independent scan; token-bearing nodes start at their token; empty nodes
  have `start == end` when their placement rule requires a zero-width span.
  Empty block scalars use the explicit anchor exceptions in §7.2, including
  its nonzero EOF examples. Fixtures include multi-byte text, a BOM, CRLF, a lone
  CR, tabs, empty values, comment-only input, end-of-input errors, and
  invalid UTF-8.
- **Revision 1.1 locations.** Literal/folded blocks with all chomping modes,
  blank-only blocks, trailing spaces, explicit empty keys/values, BOM and
  LF/CRLF/lone-CR variants with multibyte text. Assert exact offsets and decoded
  values together, plus parent containment with only the insertion-boundary
  exceptions in §7.2. Test descendants inside alias/merge copies too.
- **Revision 1.2 empty blocks.** Pin the nine original §7.2 empty-block rows'
  spans and decoded values together, with independent line/column checks. Include
  literal and folded styles, clip/strip/keep chomping, spaces-only lines, comments,
  sibling versus EOF anchors, sequence items, tags/anchors, BOM and
  LF/CRLF/lone-CR variants. Assert normalization floors and containment. Do not
  substitute an indicator-based event-span convention for these node spans.
- **Revision 1.3 anchors.** Pin the additional §7.2 rows, nested omitted values
  and tagged/untagged zero-width collections, including BOM and LF/CRLF/lone-CR
  variants. Assert unchanged values, every normalization floor, wrapper/inner
  agreement and recursive containment, including inside alias/merge copies.
  Removing only the tagged-collection exception MUST fail a detecting test.
  Keep these new normalization expectations separate from revision 1.2 profiles.
- **EOF keep chomping.** Assert all three exact §7.2 value examples, distinguishing
  a physical final break from an unterminated spaces-only tail. Record the
  grammar-derived expectation separately from observed parser or suite results.
- **UTF-8 input surface.** Exercise §7.2's invalid-byte positions 0 and 2 and the
  BOM-less UTF-16LE bytes that pass UTF-8 but fail YAML syntax at byte 1. Include
  raw NUL and valid escaped `\0`, the optional UTF-8 BOM, and input-size boundaries.
- **Worked weights.** Assert §6's 133/192/218 subtree weights independently of
  serialization or allocator layout, with wrapper/entry charges intact.
- **Revision 1.4 depth.** Derive expected H and p from §§5–6 independently of
  the implementation. Include root scalars, empty collections, mapping keys,
  retained/core tags, mixed block/flow nesting, inline/aliased merges, source
  lists, shadowing, merged anchors reused later and document reset. Exercise
  127/128/129 at limit 128 and 254/255/256 at limit 255, isolating source versus
  materialized rejection. Assert kind/configured/source site and alias/merge
  provenance. Removing empty-collection levels, counting tags, omitting keys,
  changing alias base or retaining pre-merge metrics must each be detected.
- **Revision 1.4 accounting.** Independently derive §6's registration totals;
  test below/at/above with valid options and other budgets noncompeting. Include
  nested/redefined anchors, including nested reuse of the same name where
  definition-start order differs from completion order, wrapper weights,
  stream reset and registration of a
  merged anchor before any later alias. Verify final count/weight separately
  from monotonic source/directive/copy charges. Scalar text weight remains the
  existing String-only rule; cover owned-byte and key-byte boundaries directly.
- **Revision 1.4 admission and precedence.** Exercise every §7.1 paired-failure
  row, including unknown aliases and unsupported YAML versions before later
  syntax, with their new terminal selections and native sites. Include lookahead
  errors before collection starts, missing/mismatched delimiter opener sites,
  original-byte BOM/Unicode/CRLF coordinates and detecting controls for relocated
  diagnostics or per-failure reparsing. Test ordinary-entry capacity versus duplicate/NaN, second Apply
  directives, malformed later merge sources, shadowed and unshadowed entries at
  capacity, all pairwise copy-limit ties and an all-four tie. Prove each competing
  limit independently before asserting the selected result. Test completed-key
  weight with composite keys, earlier incremental failures and a later invalid
  value. Instrument rejection before path cloning or prohibited copies. Keep
  error-key P and path-segment controls unchanged and distinct from load depth.
- **Document cardinality.** Every §5.5 row and the consecutive explicit-empty
  document example through string and byte entrypoints,
  with EOF coordinates for zero-document streams and boundary coordinates for
  explicit empty documents. Check CLI empty-stream output, multiple-document
  rejection in single-document calls, and all-or-nothing stream failure.
- **Error API compile probes.** Borrow fields, mutate through `DerefMut`, extract
  owned detail and non-Copy fields, construct custom Serde errors, and pass a
  borrowed error to `encode_error`. Negative compile probes reject old struct
  patterns and direct moves of non-Copy fields through `Deref`. Test bounded
  constructor formatting, source attribution and undersized error-output budgets.
  After changing an error kind, assert the exact corresponding wire detail and
  absence of stale members; reject inconsistent kind/detail pairs wherever the
  representation permits constructing them. Compile probes belong to the
  candidate's Rust API tests (and yamlprims's own tests after transfer), rather
  than corpus data.
- **Revision 1.2 error encoding.** Mutate messages, type/field descriptions and
  structured paths after construction; encode under tighter options and assert
  the borrowed error is unchanged. Test path-length and key-nesting boundaries,
  a two-byte String key at logical weight 66 (accept 66, reject 65), and a
  structured key longer than 1,024 bytes retained in full when within its key
  budget. Check UTF-8-safe truncation including the marker, every JSON control
  escape, quotes/backslashes, exact encoded-length acceptance and one-byte-less
  output rejection. Reject malformed syntax codes and kind/detail pairs.
  Assert exact rejection kind, limit/configured payload and non-source site,
  with no partial JSON or recursive fallback. Instrument the preflight and
  encoding to detect traversal past the first bound, key clones and unbounded
  escaped temporaries. Disable the root weight charge as a negative control;
  the 65-byte rejection test must detect the mutation.
- **Fuzzing** with a sanitizer on `load_all`, a typed configuration target,
  and `encode`, checking: no panic; `Site::Source` for input errors; span
  bounds; counters within budget on success.
- **Miri** on the corpus.
- **Mutation testing** of the duplicate check, each budget check, the alias
  height check, merge precedence, and (M3) the iterator fuse. Every mutant
  must fail the suite.
- **Measurements, not gates:** peak memory and latency per default profile,
  and the engine's scalar buffering bound (§6).
- **Validation items:** depth accounting at 127/128/129 for lexical and
  materialized trees, including alias insertion; block collection spans with
  block scalars, chomping, comments, and trailing breaks; hash collision
  handling and order-independent composite-key hashing; anchor lifecycle
  (per-document reset, redefinition, alias inside a later definition, nested
  alias and merge origin); MSRV built and tested, not inferred.

## 14. Open questions

1. **Emission.** None in M1/M2. If consumers need it, M3 adds a canonical
   emitter whose round-trip test uses §5.1 equality extended so NaN equals NaN
   by bits, ignoring spans and tag spelling.
2. **Limits.** The §6 defaults are proposals until measured.
3. **MSRV.** Proposed 1.88.

## 15. Decisions

Policy defaults in this specification are recorded as decision records in the
yamlprims repository under `docs/decisions/`. The merge-key default
(`Reject`) is the first.

## 16. Errata

Tagged revisions 1.0 through 1.3 remain preserved at their `spec-v1.x` tags.
Erratum numbering belongs to this public specification and restarts per revision;
cross-document references use revision-qualified identifiers such as `1.4/E-1`.
Specification publication and decision acceptance do not establish implementation
qualification.

### Revision 1.4 — unreleased draft

These proposed contract updates require maintainer acceptance and a separately
pinned qualification profile. Historical tags, profiles and observations remain
unchanged; successful historical comparisons do not qualify these new boundaries.

| ID | Sections | Change from revision 1.3 |
| --- | --- | --- |
| E-1 | §5.3–5.4, §6, §9, §13.2 | Define source collection levels, completed height H and alias placement; use final post-merge metrics while preserving source checks and work charges. |
| E-2 | §5.4, §6, §13.2 | Specify bytes-only anchor-registration charges, no extra cache-node charges, nested/redefined anchors and whole-call scope. |
| E-3 | §5.1–5.4, §6–7, §9, §13.2 | Specify bounded source-validation precedence with terminal engine exceptions and native diagnostic sites, ordinary-entry and merge-capacity admission, deterministic copy-limit ties and completed-key weight timing. |
| E-4 | §6, §11.1, §13.2 | Name the existing error-key nesting metric P separately from loader height H without changing encoder preflight expectations. |

The logical subtree-weight formula, 64-byte node charge and wire grammar version
1 are unchanged. This draft adds no caller-tree constructor, foreign-Serde seed,
general stack-safety guarantee, or CLI contract freeze. Those remain open M1
items. Implementation and package qualification are separate from this revision.

### Revision 1.3 — tagged draft specification

The following updates were incorporated in revision 1.3. Its immutable tag and
separately pinned expectations remain unchanged. New nested-anchor expectations
are not attributed to profiles claiming unchanged revision 1.2 behavior.

| ID | Sections | Change from revision 1.2 |
| --- | -------- | ------------------------ |
| E-1 | §7.2, §13.2 | Normalize zero-width omitted/empty scalars over current-line ASCII-space indentation and preceding breaks; preserve floors, move scalar tag wrappers together and retain zero-width collection anchors. |
| E-2 | §1, §7.2, §13.2 | Clarify the UTF-8-only input surface, original-byte invalid-UTF-8 positions and syntax validation of valid UTF-8 containing raw NULs. |
| E-3 | §8, §13.2 | Clarify binary64-Value-to-binary32 conversion through both typed paths, without reparsing Float literals; Int exactness is unchanged. |
| E-4 | §7.2, §13.2 | Pin keep-chomping values for exact spaces-only EOF examples with and without a final physical line break. |

Section 6 also adds worked logical-weight examples; the accounting formula and
64-byte node charge are unchanged. Wire grammar version 1 is unchanged.

### Revision 1.2 — tagged draft specification

The following updates were incorporated in revision 1.2. Its tag and separately
pinned expectations remain unchanged; tagging did not establish implementation
qualification or accept proposed decision records.

| ID | Sections | Change from revision 1.1 |
| --- | -------- | ------------------------ |
| E-1 | §6, §7.1, §11.1, §13.2 | Owned `String` type descriptions; 1,024-byte UTF-8-safe descriptive bound enforced at construction and encoding. |
| E-2 | §6, §7.1, §10–11, §13.2 | Preflight mutable structured error identity; exact rejection kind/site, bounded descriptive encoding and non-recursive failures. |
| E-3 | §7.2, §13.2 | Engine-derived empty-block scalar anchors, explicit zero-width normalization and pinned examples without indicator-recovery scanning. |

### Revision 1.1 — tagged draft specification

The following errata were incorporated in revision 1.1. The strict-only decision
in `1.1/E-4` is accepted under ADR-0003; tagging did not accept the other
proposed decision records or establish candidate conformance.

| ID  | Sections         | Change from revision 1.0                                                                                                                                                      |
| --- | ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| E-1 | §7.2, §13.2      | Precise block-scalar ends, omitted-child placement and parent containment; scalar values and ordinary/indentless sequence starts unchanged.                                   |
| E-2 | §5.5, §10, §13.2 | Zero-document streams stay empty for multi-document APIs and CLI; define explicit empty-root boundaries and `load_all_bytes`.                                                 |
| E-3 | §7.1, §10, §13.2 | Boxed `Error`/`ErrorDetail`, borrowed access, owned extraction and constructors; document both pattern and field-move migration. Add the fallible error encoder.              |
| E-4 | §9               | Accepted strict-only API with strict indentation in all parsing passes to align flow indentation with YAML 1.2.2; profile qualification and migration checks remain required. |
