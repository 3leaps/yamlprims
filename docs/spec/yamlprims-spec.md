# yamlprims specification

**Revision 1.6** · Status: Unreleased draft · License: CC0-1.0 (specification text)

The tagged draft baseline is [revision 1.5](https://github.com/3leaps/yamlprims/blob/spec-v1.5/docs/spec/yamlprims-spec.md).
This draft contains the changes and decision status listed in §16. Existing
profiles pinned to revisions 1.0 through 1.5 keep their original expectations;
they do not adopt this draft implicitly.

**Specification release.** This is a proposed target contract, not a claim that
every source snapshot conforms to every requirement before qualification. See
[implementation status](../implementation.md), the
[build specification](../build-spec.md), and [open design items](../design-review.md).
Revision 1.5 incorporated the `Profile B` multi-language architecture
([ADR-0004](../decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md))
and defined checked caller-tree constructors and stack-safe cleanup (§3), the
deterministic YAML 1.2.2 Core emitter and typed serialization (§8.2, §10.2),
symmetric wire decoding (§11.1), and strict RFC 7493 I-JSON projection (§11.6)
([ADR-0005](../decisions/ADR-0005-data-layer-emitter-and-ijson-projection.md)).
Revision 1.6 adds bounded reader/writer, byte-slice, and plain-host ergonomics
across all four bindings (§8.1–8.2, §10.1–10.2, §11.2–11.3, §11.5), the YAML 1.1
to YAML 1.2.2 Core migration lint and `yamlprims-lint/1` wire format (§10.3,
§11.7), and the frozen `yamlprims-cli` command-line contract (§10.4).

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

| Milestone                                | Contents                                                                                                                                                                            |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **M1**                                   | Rust core: `load`, `load_all`, `from_str`, owned `Value`/`Node`/`Error`, limits, wire format (§11), CLI `check`/`dump`                                                              |
| **M2**                                   | `Profile B` multi-language parity ([ADR-0004](../decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md)): pure-Go twin, zero-import WASM TypeScript, `PyO3` Python (§11–13) |
| **M3**                                   | Checked caller-tree constructors & stack-safe `Drop` (§3), Core emitter & `serde::Serialize` (§8.2, §10.2), symmetric `decode_wire` (§11.1), strict RFC 7493 `encode_ijson` (§11.6) |
| **M4** (demand-driven)                   | Bounded reader/writer & plain-host ergonomics (§8, §10.1–10.2, §11.2–11.5), YAML 1.1 migration lint & `yamlprims-lint/1` (§10.3, §11.7), frozen CLI (§10.4); optional in-memory document iterator & Failsafe schema (deferred until consumer need) |

**Out of scope:** a new YAML parser; source-preserving CST editing (comment,
 trivia, or anchor/alias graph retention); `!include` or environment expansion;
a YAML 1.1 resolution schema; borrowed deserialization; incremental unbounded
byte-stream input (all input is bounded by `max_input_bytes` and validated as a
complete in-memory buffer; `Read` / `io.Reader` entrypoints read at most
`max_input_bytes + 1` bytes into memory).

## 2. Layering

```
&[u8] / &str / Read (bounded by max_input_bytes + 1)
                 │
                 ▼
input gate: max_input_bytes on raw length, BOM strip, UTF-8 validation
                 ▼
granit-parser =2.0.0 (default-features = false, features = ["std","error_messages"])
  engine options pinned (§9)                                          ← grammar / terminal engine checks
                 ▼
event pump      engine marker → Location; anchors; alias events       ← only module importing the engine
                 ▼
resolver        scalar and tag resolution (§4) + YAML 1.1 lint (§10.3)← owned policy
                 ▼
builder         nodes, keys, duplicates, merge, budgets (§5–6)        ← owned policy
                 ▼
Document / Node tree ─► typed conversion (§8.1) / wire encoder (§11.1) / I-JSON projection (§11.6)
        ▲            ├► YAML 1.1 migration lint (§10.3) ──► LintResult / yamlprims-lint/1 (§11.7)
        │            └► Core emitter (§10.2) ──► UTF-8 YAML 1.2 Core text / buffered Write
        │
checked constructors (§3) / serde::Serialize (§8.2) / decode_wire (§11.1)
```

- The generic (`load`) and typed (`from_str`) paths share one resolver and
  builder. §8 states exactly what that guarantees for typed conversion and
  serialization.
- Under `Profile B` ([ADR-0004](../decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md)),
  the Rust core, Rust CLI, and `PyO3` Python extension are `#![forbid(unsafe_code)]`;
  the Go package (`github.com/3leaps/yamlprims/yaml`) is pure Go (`CGO_ENABLED=0`,
  standard library only); and the TypeScript package (`@3leaps/yamlprims`) wraps
  a zero-import `wasm32-unknown-unknown` module whose `#[allow(unsafe_code)]`
  annotations are confined strictly to exported `#[no_mangle]` ABI entrypoints.
- MSRV: 1.88. The engine declares 1.81; its dependency closure is
  `arraydeque` and `smallvec`.

## 3. Value model

```rust
pub struct Node { /* private */ }
impl Node {
    pub fn value(&self) -> &Value;
    pub fn into_value(self) -> Value;            // moves Value out safely under iterative Drop
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
pub struct Tagged { tag: Tag, value: Node }
impl Tagged {
    pub fn new(tag: Tag, value: Node) -> Result<Tagged, Error>; // validates tag and §4.4 inner kind
    pub fn tag(&self) -> &Tag;
    pub fn value(&self) -> &Node;
    pub fn into_parts(self) -> (Tag, Node);
}
pub struct Tag { uri: String, handle: String, suffix: String }
impl Tag {
    pub fn new(uri: impl Into<String>) -> Tag;
    pub fn try_new(uri: impl Into<String>) -> Result<Tag, Error>;
    pub fn with_spelling(
        uri: impl Into<String>,
        handle: impl Into<String>,
        suffix: impl Into<String>,
    ) -> Result<Tag, Error>;
}
pub struct Document { root: Node, yaml_version: Option<(u32, u32)>, explicit_start: bool }
impl Document {
    pub fn new(root: Node) -> Document; // yaml_version: None, explicit_start: false
    pub fn with_metadata(
        root: Node,
        yaml_version: Option<(u32, u32)>,
        explicit_start: bool,
    ) -> Result<Document, Error>;
}
```

- **Invariant-bearing fields are private.** Values are built through checked
  constructors, so conversion and encoding never see a non-canonical
  `Integer`, a mapping with duplicate keys, or a stale key index.
- **`Tag` and `Tagged` constructors.** `uri` is the resolved decoded tag
  identity (`tag:example.com,2026:thing`, `!custom`), matching the engine's
  UTF-8 percent-decoded tag output. `handle` and `suffix` reconstruct the
  source spelling; they are not a promise of the exact source lexeme. Tag
  equality uses `uri` only. Existing `Tag::new` is retained for source
  compatibility; `Tag::try_new` and `Tag::with_spelling` validate syntax and
  structural invariants without imposing a fixed byte-length cap (byte lengths
  are bounded by `Limits::max_owned_bytes` / `Limits::max_key_bytes` on load
  and wire decode, and by `EmitLimits::max_tag_bytes` during emission): `uri`
  MUST be either a **local tag** starting with `'!'` (`uri.len() >= 2`,
  rejecting non-specific `"!"`) or a **global tag URI** starting with an RFC
  3986 §3.1 scheme `[A-Za-z][A-Za-z0-9+.-]*:` followed by zero or more
  characters (permitting an empty scheme-specific part such as `"x:"` per RFC
  3986 §3 `path-empty` while rejecting scheme-less verbatim strings such as
  `"$:?"` per YAML 1.2.2 §6.9.1 Example 6.25; any UTF-8 bytes outside
  `ns-tag-char \ {'%'}` for shorthand `!`/`!!` tags or outside
  `ns-uri-char \ {'%', '[', ']', '#'}` for verbatim `!<…>` tags in decoded `uri`
  are percent-encoded as `%XX` during emission per §10.2), and MUST NOT be one
  of the seven Core Schema tag URIs
  (`tag:yaml.org,2002:{null,bool,int,float,str,seq,map}`), returning
  `InvalidOptions` (`Site::NotSource { last_known: None }`; `InvalidOptions` is
  used for caller-supplied metadata/configuration violations so the
  `yamlprims-value/1` version `1` `ErrorKind` vocabulary remains closed at 23
  variants). `Tag::with_spelling` additionally requires `handle` to be `""`
  (`uri == suffix`), `"!"`, `"!!"`, or a named handle `!<ns-word-char>+!`
  (`[0-9A-Za-z-]+`), where any non-empty `handle` requires a non-empty `suffix`
  and a non-empty prefix
  (`uri.strip_suffix(&suffix).is_some_and(|prefix| !prefix.is_empty())`,
  supporting both default `!`/`!!` handles and `%TAG`-redefined `!`, `!!`, and
  named handles whose expanded `uri` is a valid local tag or global URI).
  `Tagged::new` validates `tag` by those same rules (`InvalidOptions`) and
  enforces §4.4 inner-kind compatibility: `value.value()` MUST be
  `Value::String`, `Value::Sequence`, or `Value::Mapping` (and MUST be
  `Value::String` when `tag.uri() == "tag:yaml.org,2002:merge"`), returning
  `TagKindMismatch` (`Site::NotSource { last_known: None }`) on `Null`, `Bool`,
  `Int`, `Float`, nested `Tagged`, or `!!merge` collections. Wire decoding
  (`decode_wire`, §11.1) enforces these §3 `Tagged` inner-kind and
  non-empty/non-`"!"`/non-Core/handle-suffix `Tag` invariants while accounting
  `tag_bytes(tag)` against `Limits::max_owned_bytes` and `Limits::max_key_bytes`
  (returning `Internal` on malformed wire input), and `emit*` (§10.2)
  revalidates `yaml_version`, `Tag`, `max_tag_bytes`, and `Tagged` invariants
  before emitting (while `encode_ijson` rejects any `Value::Tagged` node per
  §11.6 Rule 5).
- **`Document` constructors.** `Document::new(root)` sets `yaml_version = None`
  and `explicit_start = false`. `Document::with_metadata` accepts `None` or
  `Some((1, minor))` with `minor <= 999_999_999` (matching the §9 nine-digit
  scanner limit) and rejects any other `(major, minor)` pair with
  `UnsupportedYamlVersion` (`Site::NotSource { last_known: None }`).
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
- **Caller-tree bounds and stack-safe cleanup.** `Drop`, `Clone`, and
  `PartialEq` for `Node`, `Value`, `Mapping`, `Tagged`, and `Document` MUST
  traverse nested `Sequence`, `Mapping`, and `Tagged` subtrees iteratively on
  explicit heap worklists (and `fmt::Debug` MUST bound collection formatting at
  `MAX_DEPTH_CEILING = 255`) so dropping, cloning, comparing, or debug-formatting
  arbitrarily deep caller-constructed trees never overflows the call stack, even
  when no `Options` are supplied; `Node::into_value` moves `Value` out without
  triggering E0509 destructuring restrictions. Crate-owned internal traversals
  (`key_eq`, `contains_nan`, `hash_into`, `budget::metrics`) also use iterative
  heap stacks, and crate/binding operations parameterized by `Options` or
  `EmitOptions` (`from_node`, `to_node`, `to_string*`, `to_writer*`, `encode`,
  `encode_plain`, `encode_ijson`, `encode_lint`, `decode_wire`, `emit*`,
  `emit_writer*`, `emit_all_writer*`, Go `Emit*`/`FromPlain`/`Marshal*`/`Unmarshal*`/`EncodeLint*`,
  TypeScript `fromPlain`/`toPlain`/`parse*`/`dump*`/`stringify*`/`encodeLint`,
  and Python `from_plain`/`to_plain`/`safe_load*`/`dump*`/`safe_dump*`/`encode_lint`)
  enforce configured `max_depth` (`<= 255`), `max_nodes`, and `max_output_bytes`
  before descending into child collections or via explicit heap worklists,
  turning over-deep or cyclic host graphs into `LimitExceeded(MaxDepth)` before
  call-stack exhaustion.

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
  `!!set`, `!!omap` (and `!!merge` when appearing on a scalar value rather than
  a mapping key; as in revisions 1.0–1.4, `!!merge` on a collection is
  `TagKindMismatch` and `!!merge` on a mapping key follows §5.3), local `!x`
  tags, verbatim `!<…>` tags, and `%TAG` expansions — produce
  `Tagged { tag, value }`. For a scalar, the inner value is **String(raw text)
  without resolution**: `!custom 42` holds String "42". For a collection, the
  children resolve normally. Nothing is decoded. Note that while the scanner's
  lexical grammar (`[86] ns-global-tag-prefix`, `[91] c-verbatim-tag`) accepts
  any `ns-uri-char+` sequence when `unknown_tags = Preserve`, YAML 1.2.2 §6.8.2
  and §6.9.1 require non-local tags to be global tag URIs (`scheme:…`);
  `Tag::try_new` and `emit*` (§10.2) enforce that YAML 1.2.2 §6.8.2 / §6.9.1
  URI rule so every loaded document whose retained tags are valid YAML 1.2.2
  local (`!…`) or global (`scheme:…`) tags (within `EmitLimits`) is guaranteed
  to succeed under `emit*`, while scheme-less non-local verbatim tags (such as
  YAML 1.2.2 Example 6.25 `!<$:?>`) are rejected by `emit*` with `InvalidOptions`.
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

- `load`, `load_with`, `load_bytes`, `load_reader`, and `load_reader_with` (plus
  Go `Load*`/`LoadReader*`, TypeScript `load`/`parse`, and Python `load*`/`safe_load`)
  return one document. A second document is a `MultipleDocuments` error at its
  start. Typed entrypoints (`from_str*`, `from_slice*`, `from_reader*`, Go
  `Unmarshal*`) use these single-document semantics before applying §8 / §11.3
  conversion rules.
- `load_all`, `load_all_with`, `load_all_bytes`, `load_all_reader`, and
  `load_all_reader_with` (plus Go `LoadAll*`/`LoadAllReader*`, TypeScript
  `loadAll`/`parseAll`, and Python `load_all*`/`safe_load_all`) return the
  documents actually present in the stream. They are all-or-nothing: the first
  error ends the call and nothing partial is returned. Exceeding `max_documents`
  is `LimitExceeded`, not `MultipleDocuments`, even when that configured limit
  is one.

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

- M4: a document iterator over an **in-memory** input, fused after the first
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
    pub max_output_bytes: usize,      //   64 MiB  wire/lint/plain encoding (§11)
    pub max_warnings: usize,          //   1_000   YAML 1.1 migration lint (§10.3), then truncated = true
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
For the M4 iterator, tree counters reset
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

YAML-loading and migration-lint entrypoints (`load*`, `load_all*`, `load_reader*`,
`load_all_reader*`, `lint_yaml11*`, and the loading phase of `from_str*`,
`from_slice*`, `from_reader*`, `load_ijson*`, and Go `Unmarshal*`) use the
following logical phase order to select their result. This does not prescribe a
number of physical parser passes. Encoder and `from_node` / `UnmarshalNode`
preflights remain their own operations; this is not a global ordering of every
`ErrorKind`.

1. Validate options. `InvalidOptions` precedes any examination of input (and
   precedes reading any bytes from `std::io::Read`, `io.Reader`, or a Python
   stream) and has `Site::NotSource { last_known: None }`.
2. Check raw input byte length, including any BOM, against `max_input_bytes`.
   Reader entrypoints (`from_reader*`, `load_reader*`, `load_all_reader*`,
   `lint_yaml11_reader*`, Go `LoadReader*`/`LoadAllReader*`/`UnmarshalReader*`/`LintYAML11Reader*`,
   and Python `safe_load*`/`lint_yaml11` on file-like objects) accumulate chunks
   until EOF or `max_input_bytes.saturating_add(1)` UTF-8 bytes (`Read::take` /
   `io.LimitReader` / looped Python `.read(min(chunk_cap, remaining))` per §11.5):
   any underlying stream I/O failure returns `Internal` with
   `Site::NotSource { last_known: None }`; reaching `max_input_bytes + 1` UTF-8
   bytes stops reading immediately and raises `LimitExceeded(max_input_bytes)` at
   byte 0 (`Site::Source(Location { byte_offset: 0, line: 1, column: 1 })`),
   identical to slice and string entrypoints. A size failure wins over invalid
   UTF-8 and all later phases.
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
not independent evidence of malformed YAML. The closed 81-code `SyntaxKind`
vocabulary is defined in §11.1 (`ADR-0004`).

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

## 8. Typed conversion

### 8.1 Typed deserialization

```rust
pub fn from_str<T: DeserializeOwned>(input: &str) -> Result<T, Error>;
pub fn from_str_with<T: DeserializeOwned>(input: &str, opts: &Options) -> Result<T, Error>;
pub fn from_slice<T: DeserializeOwned>(input: &[u8]) -> Result<T, Error>;
pub fn from_slice_with<T: DeserializeOwned>(input: &[u8], opts: &Options) -> Result<T, Error>;
pub fn from_reader<R: std::io::Read, T: DeserializeOwned>(reader: R) -> Result<T, Error>;
pub fn from_reader_with<R: std::io::Read, T: DeserializeOwned>(reader: R, opts: &Options) -> Result<T, Error>;
pub fn from_node<T: DeserializeOwned>(node: &Node, opts: &Options) -> Result<T, Error>;
pub struct Spanned<T> { pub value: T, pub span: Option<Span> }   // None for synthetic nodes
pub struct TaggedValue<T> { pub tag: Tag, pub value: T }
```

**Guarantee.** Resolver and builder policy is identical for `load*` and
`from_str*` / `from_slice*` / `from_reader*`. Entrypoints without `_with` use
`Options::default()`. `from_slice_with` runs `load_bytes(input, opts)` followed
by typed conversion of the single document's root (`from_node` with source
plain-scalar enum metadata preserved). `from_reader_with` validates `opts`
first (`InvalidOptions`), reads at most
`opts.limits.max_input_bytes.saturating_add(1)` bytes via `std::io::Read::take`
(mapping any `std::io::Error` to `ErrorKind::Internal` with
`Site::NotSource { last_known: None }`), and delegates to
`from_slice_with(&buf, opts)`. Built-in conversions follow the table below. Conversions written
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
- **Panics.** Crate-owned YAML-loading, serialization, and emission paths must
  handle input and invalid options without panicking. Caller conversions are
  excluded. Depth and node budgets apply across `from_node`, `to_node`, `emit`,
  and encoders; `Drop` for `Node` is iterative (§3). Loader depth is not a bound
  on caller-owned Serde visitor or `Serialize` frames.

### 8.2 Typed serialization

```rust
pub fn to_node<T: Serialize>(value: &T, opts: &EmitOptions) -> Result<Node, Error>;
pub fn to_string<T: Serialize>(value: &T) -> Result<String, Error>;
pub fn to_string_with<T: Serialize>(value: &T, opts: &EmitOptions) -> Result<String, Error>;
pub fn to_writer<W: std::io::Write, T: Serialize>(writer: W, value: &T) -> Result<(), Error>;
pub fn to_writer_with<W: std::io::Write, T: Serialize>(writer: W, value: &T, opts: &EmitOptions) -> Result<(), Error>;
```

`to_node` converts a `serde::Serialize` value into a synthetic `Node`
(`NodeSpan::Synthetic`) under `EmitOptions::validate()`, enforcing `max_depth`,
`max_nodes`, `max_scalar_bytes`, `max_key_bytes`, and `max_tag_bytes` while
constructing the tree. `to_string` and `to_writer` use `EmitOptions::default()`,
and `to_string_with` runs `to_node(value, opts)` followed by
`emit_with(&node, opts)`. `to_writer_with` buffers and validates the complete
emitted YAML string via `to_string_with(value, opts)` before calling
`writer.write_all(yaml.as_bytes())`, so option, structural, or `max_output_bytes`
failures never write partial YAML bytes to `writer`; any `std::io::Error` from
`write_all` returns `ErrorKind::Internal` with `Site::NotSource { last_known: None }`.

| Rust `Serialize` input                 | Resulting `Value`                                                                                  | Rejects (`Site::NotSource { last_known: None }`)                                      |
| -------------------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `()`, unit struct, `Option::None`      | `Null`                                                                                             | —                                                                                     |
| `bool`                                 | `Bool`                                                                                             | —                                                                                     |
| `u8`…`u128`, `i8`…`i128`               | `Int(Integer)`                                                                                     | —                                                                                     |
| `f32` (`f64::from`), `f64`             | `Float(f64)` (any NaN normalized to canonical quiet NaN `0x7ff8_0000_0000_0000`)                   | NaN anywhere inside a mapping key → `InvalidKey`                                      |
| `char`, `&str`, `String`               | `String`                                                                                           | UTF-8 length > `max_scalar_bytes` → `LimitExceeded(MaxScalarBytes)`                   |
| bytes (`serialize_bytes`)              | `Sequence` of `Int(0..=255)`                                                                       | exceeds `max_depth` or `max_nodes` → `LimitExceeded`                                  |
| `Option::Some(v)`, newtype struct      | `v` converted by these rules                                                                       | —                                                                                     |
| sequence, tuple, tuple struct          | `Sequence`                                                                                         | exceeds `max_depth` or `max_nodes` → `LimitExceeded`                                  |
| struct, map                            | `Mapping` in serialization order                                                                   | duplicate §5.1 key → `DuplicateKey`; NaN key → `InvalidKey`; key weight > `max_key_bytes` → `LimitExceeded(MaxKeyBytes)` |
| unit enum variant                      | `String(variant)`                                                                                  | —                                                                                     |
| newtype / tuple / struct enum variant  | single-entry `Mapping { String(variant): payload }`                                                | —                                                                                     |
| `Spanned<T>`                           | `value` converted by `T` rules (`span` ignored on output)                                          | —                                                                                     |
| `TaggedValue<T>`                       | `Tagged` constructed via `Tagged::new` (§3) with `value` converted by `T` rules                    | invalid/core/oversized tag → `InvalidOptions`; `Null`, `Bool`, `Int`, `Float`, nested `Tagged`, or `!!merge` collection → `TagKindMismatch` (rejected immediately by `to_node`) |
| `Value`, `Node`                        | lossless synthetic copy (preserving `Integer`, `Float`, `Mapping`, and `Tagged`)                   | budget violations → `LimitExceeded`                                                   |

**Serde typed round-trip scope.** For a Rust value `v: T` where `T: Serialize + DeserializeOwned` uses the built-in mappings in §8.1 and §8.2, `from_str_with::<T>(&to_string_with(&v, &emit_opts)?, &reload_opts)` reconstructs `v` whenever `T`'s §8.2 serialized representation is injective and Accepted by §8.1. The following built-in shapes are intentionally non-invertible or excluded from round-trip:
1. `Option<()>` (`Some(())` and `None` both serialize to `Value::Null`, which deserializes as `None`) and nested `Option<Option<U>>` (`Some(None)` and `None` both serialize to `Value::Null`, which deserializes as outer `None`).
2. `TaggedValue<U>` where `U` serializes to `Null`, `Bool`, `Int`, `Float`, or nested `TaggedValue` (rejected by `to_node` with `TagKindMismatch` because §4.4 `Tagged` wraps only `String`, `Sequence`, or `Mapping`), and §8.1's local-tag enum variant syntax (`!Variant payload`, since `Serialize` emits enum variants as strings or single-entry mappings `{Variant: payload}`).
3. `Spanned<U>` (source `span` is ignored by `Serialize` and populated from the emitted YAML coordinates on `Deserialize`).
4. Caller-defined Serde attributes or custom impls that are non-injective or asymmetric (`#[serde(untagged)]` with overlapping variant shapes, `#[serde(flatten)]` producing duplicate keys, or `#[serde(skip_serializing_if = "...")]` without a matching `#[serde(default)]`).

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

## 10. Public Rust API, Core emitter, migration lint, and CLI

### 10.1 Loading, typed conversion, wire, and projection APIs

```rust
#[non_exhaustive]
pub struct Options {
    pub merge_keys: MergeKeys,        // Reject (default) | Apply | AsKey
    pub unknown_tags: UnknownTags,    // Preserve (default) | Reject
    pub limits: Limits,
}
impl Options {
    pub fn ijson_default() -> Options; // Options::default() with unknown_tags = Reject and max_output_bytes = 16 MiB
}
pub enum WireEnvelope {
    Documents(Vec<Document>),
    Error(Error),
}
pub fn load(input: &str) -> Result<Document, Error>;
pub fn load_with(input: &str, opts: &Options) -> Result<Document, Error>;
pub fn load_bytes(input: &[u8], opts: &Options) -> Result<Document, Error>;
pub fn load_reader<R: std::io::Read>(reader: R) -> Result<Document, Error>;
pub fn load_reader_with<R: std::io::Read>(reader: R, opts: &Options) -> Result<Document, Error>;
pub fn load_all(input: &str) -> Result<Vec<Document>, Error>;
pub fn load_all_with(input: &str, opts: &Options) -> Result<Vec<Document>, Error>;
pub fn load_all_bytes(input: &[u8], opts: &Options) -> Result<Vec<Document>, Error>;
pub fn load_all_reader<R: std::io::Read>(reader: R) -> Result<Vec<Document>, Error>;
pub fn load_all_reader_with<R: std::io::Read>(reader: R, opts: &Options) -> Result<Vec<Document>, Error>;
pub fn from_str / from_str_with / from_slice / from_slice_with / from_reader / from_reader_with / from_node // §8.1
pub fn to_node / to_string / to_string_with / to_writer / to_writer_with                                    // §8.2
pub fn encode(docs: &[Document], opts: &Options) -> Result<String, Error>;   // §11.1
pub fn encode_error(error: &Error, opts: &Options) -> Result<String, Error>; // §11.1 error envelope
pub fn decode_wire(wire_json: &str, opts: &Options) -> Result<WireEnvelope, Error>; // §11.1
pub fn encode_plain(node: &Node, opts: &Options) -> Result<String, Error>;   // §11.3
pub fn encode_ijson(node: &Node, opts: &Options) -> Result<String, Error>;   // §11.6
pub fn load_ijson(input: &str) -> Result<String, Error>;                     // §11.6 (uses Options::ijson_default())
pub fn load_ijson_with(input: &str, opts: &Options) -> Result<String, Error>;// §11.6
pub fn load_ijson_bytes(input: &[u8], opts: &Options) -> Result<String, Error>; // §11.6
pub fn lint_yaml11(input: &str) -> Result<LintResult, Error>;                // §10.3
pub fn lint_yaml11_with(input: &str, opts: &Options) -> Result<LintResult, Error>; // §10.3
pub fn lint_yaml11_bytes(input: &[u8], opts: &Options) -> Result<LintResult, Error>; // §10.3
pub fn lint_yaml11_reader<R: std::io::Read>(reader: R) -> Result<LintResult, Error>; // §10.3
pub fn lint_yaml11_reader_with<R: std::io::Read>(reader: R, opts: &Options) -> Result<LintResult, Error>; // §10.3
pub fn encode_lint(result: &LintResult, opts: &Options) -> Result<String, Error>; // §11.7
```

Library calls perform no filesystem path resolution, environment interpolation,
network requests, or process-global configuration, and are safe to call
concurrently. Stream helpers (`*_reader*` and `*_writer*`) operate solely on the
caller-supplied `std::io::Read` or `std::io::Write` handle.

The byte entrypoints take explicit options; string and reader entrypoints without
`_with` use `Options::default()`. All byte and reader paths use the same
raw-input limit, BOM, and UTF-8 gates. Reader entrypoints validate `opts` before
reading any bytes, read at most `opts.limits.max_input_bytes.saturating_add(1)`
bytes via `std::io::Read::take`, map any `std::io::Error` to
`ErrorKind::Internal` with `Site::NotSource { last_known: None }`, and delegate
to the corresponding byte entrypoint (`load_bytes`, `load_all_bytes`,
`from_slice_with`, `lint_yaml11_bytes`). `load_all_bytes` and
`load_all_reader_with` have the exact document semantics of `load_all_with` and
preserve original-byte coordinates.

`encode_error` borrows the error and writes one canonical error envelope. Like
`encode`, it validates options and returns no partial JSON on failure. It MUST
revalidate the current error kind and its associated detail against §11.1,
including after caller mutation, and preflight all structured path identity
before writing. Descriptive text is bounded while writing, without mutating the
borrowed error. Invalid identity is rejected rather than shortened. If even
the error envelope exceeds `max_output_bytes`, it returns a Rust
`OutputLimitExceeded` error with `Site::NotSource { last_known: None }`;
it MUST NOT recursively try to encode that error or bypass the budget.

### 10.2 Data-layer Core emitter

```rust
#[non_exhaustive]
pub enum CompatQuoting {
    Yaml11Safe, // default: quote YAML 1.1 scalar hazards in addition to §4.1 Core non-strings
    CoreOnly,   // quote only §4.1 Core non-strings and YAML 1.2.2 lexical hazards
}

#[non_exhaustive]
pub struct EmitLimits {
    pub max_depth: usize,           // 128     collection levels; 1..=255
    pub max_nodes: usize,           // 250_000 visited nodes, whole call
    pub max_scalar_bytes: usize,    // 1 MiB   UTF-8 bytes per String node
    pub max_key_bytes: usize,       // 64 KiB  logical subtree weight per mapping key
    pub max_mapping_entries: usize, // 100_000 entries per Mapping
    pub max_tag_bytes: usize,       // 1_024   UTF-8 bytes per Tag::uri
    pub max_documents: usize,       // 1_024
    pub max_output_bytes: usize,    // 8 MiB   emitted YAML UTF-8 bytes (matches Limits::default().max_input_bytes)
}

#[non_exhaustive]
pub struct EmitOptions {
    pub compat_quoting: CompatQuoting,
    pub limits: EmitLimits,
}

pub fn emit(node: &Node) -> Result<String, Error>;
pub fn emit_with(node: &Node, opts: &EmitOptions) -> Result<String, Error>;
pub fn emit_writer<W: std::io::Write>(writer: W, node: &Node) -> Result<(), Error>;
pub fn emit_writer_with<W: std::io::Write>(writer: W, node: &Node, opts: &EmitOptions) -> Result<(), Error>;
pub fn emit_all(docs: &[Document]) -> Result<String, Error>;
pub fn emit_all_with(docs: &[Document], opts: &EmitOptions) -> Result<String, Error>;
pub fn emit_all_writer<W: std::io::Write>(writer: W, docs: &[Document]) -> Result<(), Error>;
pub fn emit_all_writer_with<W: std::io::Write>(writer: W, docs: &[Document], opts: &EmitOptions) -> Result<(), Error>;
```

`EmitOptions::validate()` runs before writing any output: every field of
`EmitLimits` MUST be `>= 1`, `max_depth` MUST be `<= 255`, and both
`max_scalar_bytes` and `max_key_bytes` MUST be `<= max_output_bytes`; violations
return `InvalidOptions` with `Site::NotSource { last_known: None }`. Exceeding
`max_depth`, `max_nodes`, `max_scalar_bytes`, `max_key_bytes`,
`max_mapping_entries`, or `max_documents` returns `LimitExceeded`
(`max_tag_bytes` violations return `InvalidOptions` because `yamlprims-value/1`
`LimitKind` is frozen at wire version `1`); exceeding `max_output_bytes` returns
`OutputLimitExceeded` with no partial output. `emit_writer*` and
`emit_all_writer*` validate and buffer the entire emitted UTF-8 YAML payload via
`emit_with` / `emit_all_with` before calling `writer.write_all(yaml.as_bytes())`
(so option, structural, or `max_output_bytes` errors never write partial bytes
to `writer`); any `std::io::Error` from `write_all` returns `ErrorKind::Internal`
with `Site::NotSource { last_known: None }`.

**Deterministic diagnostic order (all emitter errors use `Site::NotSource { last_known: None }`):**
1. `EmitOptions::validate()` (`InvalidOptions`), then `docs.len() > max_documents`
   (`LimitExceeded(MaxDocuments)`).
2. Complete a bounded iterative pre-order structural and `EmitLimits` preflight
   over `0..docs.len()` **before** writing output bytes: for each document,
   validate `doc.yaml_version` (`UnsupportedYamlVersion` when
   `major != 1 || minor > 999_999_999`), then traverse `doc.root` in iterative
   pre-order DFS (`Sequence` items `0..len`; `Mapping` entries `0..len`,
   visiting each entry's `key` subtree in full before its `value` subtree).
3. At each visited `Node` during preflight: charge `max_nodes`
   (`LimitExceeded(MaxNodes)`), check mapping-key subtree weight once at each
   mapping entry's key root before descending into the key's children
   (`LimitExceeded(MaxKeyBytes)`, using the §6 subtree-weight formula with
   `NODE_OVERHEAD = 64` per `Node`, `ENTRY_OVERHEAD = 24` per nested mapping
   entry, `s.len()` per `Value::String`, and `NODE_OVERHEAD + 2 * tag.uri.len()`
   per `Tagged` wrapper so key admission depends only on canonical `tag.uri` and
   upper-bounds the reloaded `uri + handle + suffix` §6 weight), then validate
   the `Value` variant (`LimitExceeded(MaxScalarBytes)` on `String`;
   `InvalidKey` on `Float(NaN)` at any depth within a mapping-key subtree and
   `DuplicateKey` on duplicate mapping keys; `InvalidOptions` on invalid or
   oversized `Tag::uri`, then `TagKindMismatch` on `Tagged`;
   `LimitExceeded(MaxDepth)` when a `Sequence` or `Mapping`, empty or non-empty,
   exceeds `max_depth` with `Tagged` depth-transparent, followed by
   `LimitExceeded(MaxMappingEntries)` when `mapping.len() > max_mapping_entries`).
4. Emit UTF-8 YAML bytes, checking `max_output_bytes` incrementally as bytes are
   written (`OutputLimitExceeded` with no partial output). Running preflight
   before output writing guarantees that structural and input-limit diagnostics
   always precede `OutputLimitExceeded` across all four bindings.

**Deterministic layout ([ADR-0005](../decisions/ADR-0005-data-layer-emitter-and-ijson-projection.md)):**

- **Encoding and stream boundaries (YAML 1.2.2 §9.2).** Output is UTF-8 without
  BOM using `\n` line endings; every non-empty emitted stream ends with `\n`.
  `emit_all(&[])` returns `""`. Any `doc.yaml_version` with `major != 1` or
  `minor > 999_999_999` is rejected with `UnsupportedYamlVersion`. For document
  index `i == 0`: emit `%YAML 1.<minor>\n---\n` when
  `doc.yaml_version == Some((1, minor))`, else emit `---\n` when
  `doc.explicit_start` is `true`, else emit the root node directly. For document
  index `i >= 1`: emit `...\n%YAML 1.<minor>\n---\n` when
  `doc.yaml_version == Some((1, minor))` (explicitly terminating document `i − 1`
  before directives per YAML 1.2.2 §9.2), else emit `---\n` before the root node.
- **Expanded block collection placement.** Indentation is 2 spaces per collection
  level (`pad = " ".repeat(indent)`); nested block collections always use
  expanded block form (never compact `- - x` or `- k: v`), and block sequences
  under mapping keys are always indented `indent + 2` (never indentless). Empty
  sequences and mappings always emit inline as `[]` and `{}`.
  - **Sequence item at `indent`:** `{pad}- {rendered}\n` for single-line scalars
    and empty `[]`/`{}`; `{pad}- {header}\n` + body lines at `indent + 2` for
    literal block strings (empty body lines emit bare `\n` with zero spaces);
    `{pad}-\n` + child collection at `indent + 2` for non-empty `Sequence` or
    `Mapping`; `{pad}- {tag} {rendered}\n`, `{pad}- {tag} {header}\n` + body
    lines at `indent + 2`, or `{pad}- {tag}\n` + child collection at `indent + 2`
    for `Tagged`.
  - **Mapping entry at `indent` (preserving `Mapping` entry order; every `Value`
    key variant is assigned exhaustively):** Untagged `Null` (`null`), `Bool`
    (`true`/`false`), `Int` (canonical decimal), finite or infinite `Float`
    (`0.0`, `-0.0`, shortest decimal, `.inf`, `-.inf`), and single-line
    `Value::String` keys (`!s.contains('\n')`) whose rendered single-line UTF-8
    length is `<= 1,024` bytes emit as **implicit** keys (`{pad}{key}: {val}\n`,
    `{pad}{key}: {header}\n` + body at `indent + 2`, `{pad}{key}:\n` + child
    collection at `indent + 2`, or `{pad}{key}: {tag} …` matching the
    sequence-item `Tagged` forms — for example `{Int(1): String("x")}` emits
    `"1: x\n"` and `{Null: Bool(true)}` emits `"null: true\n"`). `Sequence`,
    `Mapping`, `Tagged`, and `String` keys that contain `\n` (which always
    select double-quoted `"..."` per step 1 below) or whose rendered single-line
    form exceeds `1,024` UTF-8 bytes emit with **explicit** key syntax honoring
    §9's 1,024-byte simple-key lookahead rule: key prefix `{pad}? {rendered}\n`,
    `{pad}?\n` + collection at `indent + 2`, `{pad}? {tag} {rendered}\n`, or
    `{pad}? {tag}\n` + collection at `indent + 2`, followed immediately by
    `{pad}: {rendered}\n`, `{pad}: [tag ]{header}\n` + body at `indent + 2`,
    `{pad}:\n` + collection at `indent + 2`, or `{pad}: {tag}\n` + collection at
    `indent + 2` (for example `{Sequence([Int(1)]): String("v")}` emits
    `"?\n  - 1\n: v\n"`).
  - **Root `Tagged` at `indent = 0`:** `{tag} {rendered}\n` (single-line `String`
    or empty `[]`/`{}`), `{tag} {header}\n` + body at `2` spaces (literal block
    when no explicit indentation digit is needed), or `{tag}\n` + root collection
    at `indent = 0` (non-empty collection).
- **Scalars.** `Null` emits as `null`; `Bool` as `true` or `false`; `Int` as
  canonical base-10 decimal (`i128::MIN..=u128::MAX`); `Float` as `.nan`,
  `.inf`, `-.inf`, `-0.0`, `0.0`, or IEEE 754 shortest round-trip decimal with
  round-to-even tie-breaking (`|f| = 0.<digits> × 10^k`, `n = digits.len()`,
  `sci_exp = k − 1`): fixed-point when `-6 <= sci_exp < 21` (appending `.0` when
  `k >= n` so integral floats never resolve as `Int`), and lowercase scientific
  with an explicit decimal point (`d_0.0` when `n == 1`, `d_0.<d_1..>` when
  `n > 1`) and explicit exponent sign `e+`/`e-` without leading exponent zeros
  otherwise (e.g. `1.0e+21`, `1.5e-7`). `String` uses a deterministic 4-style
  state machine:
  1. **Double-quoted (`"..."`)** when the string contains any C0 control
     character (`0x00..=0x1F` other than `\n`, explicitly including `\t` `0x09`
     and `\r` `0x0D` so tabs never appear unescaped in plain, single-quoted, or
     block scalars), `0x7F`, C1 controls (`U+0080..=U+009F`, including `U+0085`
     NEL), `U+2028`, `U+2029`, `U+FEFF` (BOM), `U+FFFE`, or `U+FFFF` — or
     consists solely of `\n` bytes (`"\n"`, `"\n\n"`, …), or is a mapping key
     (whether an untagged `String` key or the inner `String` of a `Tagged` key)
     containing `\n`, or is a root-level scalar (`enclosing_collections == 0`)
     containing `\n` whose first byte is `' '` or `'\n'` (avoiding YAML 1.2.2
     `n = −1` vs libyaml `n = 0` root indentation-indicator ambiguity). Exact
     escape mapping: `"` (`0x22`), `\` (`0x5C`), `\b` (`0x08`), `\t` (`0x09`),
     `\n` (`0x0A`), `\f` (`0x0C`), and `\r` (`0x0D`) emit as the 2-byte short
     escapes `\"`, `\\`, `\b`, `\t`, `\n`, `\f`, and `\r`; all remaining C0
     controls (`0x00..=0x07`, `0x0B`, `0x0E..=0x1F`), `0x7F`, C1 controls
     (`U+0080..=U+009F`), `U+2028`, `U+2029`, `U+FEFF`, `U+FFFE`, and `U+FFFF`
     emit as 6-byte lowercase `\uXXXX` (`\u0000`..`\u001f`, `\u007f`..`\u009f`,
     `\u2028`, `\u2029`, `\ufeff`, `\ufffe`, `\uffff`); all other valid UTF-8
     scalar values (including `U+FDD0..=U+FDEF` and non-BMP characters) are
     written directly as UTF-8.
  2. **Literal block (`|`, `|-`, `|+`)** in non-key positions when the string
     contains `\n`, has at least one non-`\n` byte, and was not selected by
     step 1 (`|-` without trailing `\n`, `|+` ending in `\n\n`, `|` otherwise;
     inside enclosing collections, `|2`/`|2-`/`|2+` when starting with `' '` or
     `'\n'`; non-empty lines indented `indent + 2` spaces, empty lines bare `\n`).
  3. **Plain** when single-line (`!s.contains('\n')`), non-empty, `<= 1,024`
     UTF-8 bytes, has no leading or trailing whitespace (`' '`, `'\t'`, or
     Unicode `White_Space`), does not start with
     `, [ ] { } # & * ! | > ' " % @ \``, does not start with `-`, `?`, or `:`
     followed by end-of-string or ASCII whitespace (`' '` or `'\t'`), does not
     equal or start with `---` or `...` followed by end-of-string or whitespace,
     does not contain `: `, `:\t`, ` #`, or `\t#`, does not end with `:`, is not
     `<<` or `=`, matches none of §4.1 rows 1–8 as a full-string match, and —
     under `CompatQuoting::Yaml11Safe` (the default) — matches none of the four
     YAML 1.1 scalar hazard classes (all evaluated as full-string `\A…\z`
     matches):
     - Case-insensitive YAML 1.1 booleans/nulls: `y`, `n`, `yes`, `no`, `on`,
       `off`, `true`, `false`, `null`.
     - Underscore- or comma-grouped, multi-dot, or YAML 1.1 radix/float
       literals: `[+-]?0[0-9_]+`, `[+-]?0b[01_]+`, `[+-]?0x[0-9a-fA-F_]+`,
       `[+-]?([0-9_]+\.[0-9_.]*|\.[0-9_][0-9_.]*)([eE][+-]?[0-9_]+)?`
       (requiring at least one digit, including multi-dot tokens such as
       `1.2.3`), `[+-]?[0-9][0-9_,]*([eE][+-]?[0-9_]+)?` (including
       comma-grouped integers such as `1,000`), and ASCII case-insensitive
       `[+-]?\.inf` / `\.nan`.
     - Sexagesimal int and float literals (including underscored leading groups):
       `[+-]?[0-9][0-9_]*(:[0-5]?[0-9])+([.][0-9_]*)?` (such as `10:20`,
       `1_0:20`, or `1:05:30.5_0`).
     - YAML 1.1 `!!timestamp` date/time prefixes:
       `[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}([Tt \t].*)?` (such as `2026-11-09`).
  4. **Single-quoted (`'...'`)** when single-line printable without `'`, else
     **double-quoted (`"..."`)**.
- **Tags (YAML 1.2.2 §5.6, §6.9.1; RFC 3986 §3.3).** Every `Value::Tagged` is
  validated against §3's `Tag` rules (local `!…` with `len >= 2` or global RFC
  3986 `[A-Za-z][A-Za-z0-9+.-]*:…` non-core URI with zero or more scheme-specific
  characters, permitting `"x:"` per RFC 3986 §3 `path-empty` while rejecting
  scheme-less verbatim strings such as `"$:?"` per YAML 1.2.2 §6.9.1 Example
  6.25) and `max_tag_bytes` (`InvalidOptions`). `tag:yaml.org,2002:merge` on a
  mapping key or collection and any `Value::Tagged` wrapping `Null`, `Bool`,
  `Int`, `Float`, or nested `Tagged` are rejected with `TagKindMismatch`
  (`Site::NotSource { last_known: None }`), preserving §4.4's non-core scalar
  rule. Canonical tag rendering percent-encodes UTF-8 bytes outside
  `ns-tag-char \ {'%'}` (`[0-9A-Za-z#;/?:@&=+$_.~*'()-]`) for shorthand tags and
  outside `ns-uri-char \ {'%', '[', ']', '#'}` (`[0-9A-Za-z;/?:@&=+$_.~*'()-,!]`,
  matching RFC 3986 §3.3 `pchar / "/" / "?"` so URI path/query characters never
  contain unescaped `[`, `]`, or `#`) for verbatim global URIs as uppercase
  `%XX`: local tags (`uri` starting with `'!'`, `len >= 2`) emit `'!'` +
  `percent_encode(&uri[1..], ns-tag-char \ {'%'})`; secondary
  `tag:yaml.org,2002:` non-core tags (`&uri[18..]` non-empty) emit `"!!"` +
  `percent_encode(&uri[18..], ns-tag-char \ {'%'})`; all other valid global
  `uri` values emit `"!<"` + `percent_encode(&uri, ns-uri-char \ {'%', '[', ']',
  '#'})` + `">"` (preserving valid commas such as `!<tag:example.com,2026:v>`,
  encoding `Tag::try_new("x:[")` as `!<x:%5B>`, and round-tripping through
  `scanVerbatimTag`).
- **Self-round-trip invariant.** Because `EmitLimits::default()` matches
  `Limits::default()` on `max_depth` (`128`), `max_nodes` (`250,000`),
  `max_scalar_bytes` (`1 MiB`), `max_key_bytes` (`64 KiB`, charging
  `NODE_OVERHEAD + 2 * tag.uri.len()` per `Tagged` wrapper),
  `max_mapping_entries` (`100,000`), `max_documents` (`1,024`), and
  `max_output_bytes == max_input_bytes` (`8 MiB`), every document slice `docs`
  accepted by default `emit_all(docs)` (`EmitOptions::default()`) reloads
  unconditionally under default `load_all(&emitted)` (`Options::default()`).
  More generally, for any `emit_opts` accepted by `emit_all_with(docs, &emit_opts)`
  (within `EmitLimits` and satisfying §3's `yaml_version`, `Tag`, and `Tagged`
  invariants — which includes every document produced by `load*` whose retained
  non-core tags are valid YAML 1.2.2 §6.8.2 / §6.9.1 local `!…` or global
  `scheme:…` tags within `EmitLimits`), loading the emitted UTF-8 text under
  `reload_opts` with `UnknownTags::Preserve`, `MergeKeys::Reject`, and
  `reload_opts.limits` satisfying
  `max_input_bytes >= emitted.len()`,
  `max_depth >= emit_opts.limits.max_depth`,
  `max_nodes >= emit_opts.limits.max_nodes`,
  `max_documents >= emit_opts.limits.max_documents`,
  `max_mapping_entries >= emit_opts.limits.max_mapping_entries`,
  `max_scalar_bytes >= max(40, emit_opts.limits.max_scalar_bytes)` (since the
  loader's `MaxScalarBytes` check compares decoded scalar event text length,
  which is `s.len()` for `String` and at most `40` bytes for `i128::MIN` among
  non-`String` scalars),
  `max_key_bytes >= emit_opts.limits.max_key_bytes` (since `EmitLimits` already
  charges `2 * tag.uri.len()` per `Tagged` wrapper in mapping keys), and
  `max_owned_bytes >= 65_536 + 128 * emit_opts.limits.max_nodes + 4 * emitted.len()`
  returns documents of equal length where each document has identical
  `yaml_version` and root `Value` under §5.1 `key_eq` extended so `NaN` equals
  `NaN` and `-0.0` preserves its sign bit (`explicit_start || yaml_version.is_some()`
  preserved on document `0`; documents `i >= 1` load with `explicit_start == true`
  and `yaml_version == doc.yaml_version`, so a document `i >= 1` with
  `yaml_version == None` after an earlier `%YAML` document reloads with
  `yaml_version == None`). Because `Tagged` scalar payloads are `Value::String`
  and source-only scalar style (`inner_plain`) is not part of `Value` equality
  or `yamlprims-value/1`, source-dependent `!LocalTag 7` plain-scalar enum
  coercion (§8.1) is not preserved through data-layer `emit`, whereas Serde
  typed round-trip (`from_str_with::<T>(&to_string_with(&v, &emit_opts)?, &reload_opts)`)
  holds for all `Serialize + DeserializeOwned` types within the invertible scope
  defined in §8.2.

### 10.3 YAML 1.1 migration lint

```rust
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LintCode {
    Yaml11Boolean,     // wire code: "yaml11_boolean"
    Yaml11Number,      // wire code: "yaml11_number"
    Yaml11Sexagesimal, // wire code: "yaml11_sexagesimal"
    Yaml11MergeKey,    // wire code: "yaml11_merge_key"
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintDiagnostic {
    pub code: LintCode,
    pub span: Span,
    pub document_index: usize,
    pub path: Vec<PathSegment>,
    pub raw: String,
    pub yaml12_resolution: String,
    pub yaml11_note: String,
    pub message: String,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintResult {
    pub diagnostics: Vec<LintDiagnostic>,
    pub warning_count: usize,
    pub truncated: bool,
}
```

`lint_yaml11*` (Rust), `LintYAML11*` (Go), `lintYaml11` (TypeScript),
`lint_yaml11` (Python), and `yamlprims check --lint-yaml11` (CLI) inspect a
YAML stream for unquoted constructs that match the normative
[YAML 1.1 Type Repository](https://yaml.org/type/index.html) (`!!bool`,
`!!int`, `!!float`, `!!merge`) definitions (with the explicit decimal-float
underscore policy in item 3 below) and whose YAML 1.2.2 Core resolution
differs, without changing `yamlprims`'s strict YAML 1.2.2 Core resolution
semantics. (Individual legacy loaders such as `PyYAML`, `ruamel.yaml`,
`gopkg.in/yaml.v2`, `gopkg.in/yaml.v3`, and `js-yaml` implement subsets or
variants of these YAML 1.1 type definitions; the normative baseline for
`lint_yaml11*` is the explicit YAML 1.1 rule set below. Tokens that were plain
strings in YAML 1.1 (`!!str`) but resolve to numbers in YAML 1.2.2 Core — such
as `0o10` or `08`/`09` under §4.1 rows 3–4 — are outside `lint_yaml11*`'s
YAML-1.1-literal scope, so zero lint warnings does not guarantee that a file
containing `0o10` or `08`/`09` produced identical types under a YAML 1.1
loader.)

- **Validation, precedence, and stream semantics.** `lint_yaml11*` uses
  multi-document stream semantics (`load_all` cardinality: zero-document streams
  succeed with `warning_count == 0`, `truncated == false`, and `diagnostics == []`)
  and follows §7.1 error precedence:
  1. `Options::validate()` (`InvalidOptions`, `Site::NotSource { last_known: None }`).
  2. Raw input byte length against `max_input_bytes` (`LimitExceeded(MaxInputBytes)`
     at `Site::Source(Location { byte_offset: 0, line: 1, column: 1 })` before
     UTF-8 validation; reader entrypoints accumulate up to
     `max_input_bytes.saturating_add(1)` UTF-8 bytes until EOF and map
     `std::io::Error` / `io.Reader` / Python stream errors to
     `ErrorKind::Internal` with `Site::NotSource { last_known: None }`).
  3. UTF-8 BOM stripping and validation (`InvalidUtf8`).
  4. Bounded source syntax and `max_depth` validation across the stream (§7.1).
  5. Stream construction and migration-hazard collection. Any hard YAML,
     structure, tag, or materialization-budget failure (`DuplicateKey`,
     `InvalidKey`, `UnknownAnchor`, `RecursiveAlias`, `MergeSource`,
     `UnknownTag`, `TagKindMismatch`, `TagValueMismatch`, `IntegerOutOfRange`,
     `FloatOutOfRange`, `LimitExceeded`) aborts the call and returns `Err(Error)`.
- **Source-definition-only recording (no alias or merge amplification).**
  Diagnostics are recorded strictly at source events in document and source-byte
  order. Expanding an `*alias` (§5.4) or applying a `<<` merge (§5.3) MUST NOT
  re-emit diagnostics for nodes inside the anchored or merged source subtree.
- **Merge-key hazard (`LintCode::Yaml11MergeKey`, `"yaml11_merge_key"`, [YAML 1.1 `!!merge`](https://yaml.org/type/merge.html)).**
  Whenever the builder recognizes a merge directive in mapping-key position per
  the §5.3 recognition table (an untagged plain `<<` scalar or an explicitly
  tagged `!!merge <<` scalar of any style; quoted `"<<"` / `'<<'`, `!!str <<`,
  and `! <<` are ordinary string keys and do not trigger `Yaml11MergeKey`, while
  `!!merge x` with `x != "<<"` remains a hard `TagValueMismatch` error):
  - Record a `LintDiagnostic` with `code = LintCode::Yaml11MergeKey`, `span` equal
    to the directive key node's source `Span`, `document_index` equal to the
    0-based index of the current document, `path` equal to the path of the
    enclosing `Mapping`, `raw` equal to `"<<"`, `yaml11_note` equal to
    `"YAML 1.1 merge key specification (https://yaml.org/type/merge.html)"`,
    and `yaml12_resolution` determined by `opts.merge_keys`:
    - `MergeKeys::Reject`: `"rejected (MergeKeys::Reject default; literal string \"<<\" under MergeKeys::AsKey)"`
    - `MergeKeys::Apply`: `"merge applied (MergeKeys::Apply opt-in; not part of YAML 1.2.2 Core)"`
    - `MergeKeys::AsKey`: `"string (\"<<\" under MergeKeys::AsKey)"`
  - When `opts.merge_keys == MergeKeys::Reject` (the default), `lint_yaml11*`
    admits the `<<` key using `MergeKeys::AsKey` ordinary-entry rules for the
    remainder of the lint pass instead of aborting at the first `<<` with
    `ErrorKind::MergeRejected`, so callers linting a YAML 1.1 stream under
    default options receive all `yaml11_merge_key` and scalar diagnostics in a
    single pass (while a second unquoted `<<` in the same mapping still fails
    with `ErrorKind::DuplicateKey`). When `opts.merge_keys == MergeKeys::Apply`,
    §5.3 merge validation and expansion run normally (so invalid merge sources
    still fail with `ErrorKind::MergeSource` and duplicate directives with
    `ErrorKind::DuplicateKey`).
- **Untagged plain scalar hazards (`Yaml11Boolean`, `Yaml11Sexagesimal`, `Yaml11Number`).**
  Inspected **only** on untagged plain scalars (`ScalarStyle::Plain` with no
  explicit tag and no non-specific `!` tag) that were not already recognized as
  a §5.3 merge directive key. Single-quoted (`'...'`), double-quoted (`"..."`),
  literal (`|`), folded (`>`), and tagged (`!!str`, `!`, `!custom`, …) scalars
  never produce scalar lint diagnostics. At most one diagnostic is recorded per
  scalar, tested in the following priority order against the full scalar text
  `raw`:
  1. **`LintCode::Yaml11Boolean` (`"yaml11_boolean"`, [YAML 1.1 `!!bool`](https://yaml.org/type/bool.html)):**
     `raw` is one of the 16 exact YAML 1.1 boolean spellings that resolve to
     `Value::String` in YAML 1.2.2 Core:
     - Truthy (8 spellings): `"y"`, `"Y"`, `"yes"`, `"Yes"`, `"YES"`, `"on"`,
       `"On"`, `"ON"`.
     - Falsity (8 spellings): `"n"`, `"N"`, `"no"`, `"No"`, `"NO"`, `"off"`,
       `"Off"`, `"OFF"`.
     (The six YAML 1.2.2 Core boolean spellings `true`/`True`/`TRUE`/
     `false`/`False`/`FALSE` in §4.1 row 2 resolve to `Bool` in both YAML 1.1
     and YAML 1.2.2 Core and are not warned; mixed-case spellings outside the
     YAML 1.1 `!!bool` regular expression such as `"yEs"` or `"oN"` resolve to
     `String` in both YAML 1.1 and YAML 1.2.2 Core and are not warned.)
     - `yaml12_resolution`: `string (<JSON-escaped raw>)` (e.g. `"string (\"yes\")"`).
     - `yaml11_note`: `"YAML 1.1 boolean literal (true)"` for the 8 truthy
       spellings; `"YAML 1.1 boolean literal (false)"` for the 8 falsity
       spellings.
  2. **`LintCode::Yaml11Sexagesimal` (`"yaml11_sexagesimal"`, [YAML 1.1 `!!int`](https://yaml.org/type/int.html) / [`!!float`](https://yaml.org/type/float.html)):**
     `raw` matches the YAML 1.1 sexagesimal integer or sexagesimal float regular
     expression:
     - Sexagesimal integer (`!!int`, non-zero leading digit group):
       `\A[+-]?[1-9][0-9_]*(:[0-5]?[0-9])+\z` (such as `1:30`, `10:20:30`, or
       `1_0:20`; zero-leading integer tokens such as `0:10` or `00:30` are
       excluded by the YAML 1.1 `!!int` `[1-9][0-9_]*` leading-group rule and
       resolve to `String` in both YAML 1.1 and YAML 1.2.2 Core, so they do not
       warn).
     - Sexagesimal float (`!!float`, any decimal leading digit group and no
       exponent suffix): `\A[+-]?[0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*\z` (such
       as `10:20.5`, `0:10.5`, `00:30.0`, or `1:05:30.5_0`; exponent-suffixed
       tokens such as `1:30.5e+2` are excluded by the YAML 1.1 `!!float`
       sexagesimal production and resolve to `String` in both YAML 1.1 and
       YAML 1.2.2 Core, so they do not warn).
     - `yaml12_resolution`: `string (<JSON-escaped raw>)` (e.g. `"string (\"10:20\")"`).
     - `yaml11_note`: `"YAML 1.1 sexagesimal float literal"` when `raw` contains
       `'.'`, else `"YAML 1.1 sexagesimal integer literal"`.
  3. **`LintCode::Yaml11Number` (`"yaml11_number"`, [YAML 1.1 `!!int`](https://yaml.org/type/int.html) / [`!!float`](https://yaml.org/type/float.html)):**
     `raw` was not matched by (1) or (2) and matches one of the following
     YAML 1.1 `!!int` or `!!float` forms whose YAML 1.2.2 Core resolution
     differs:
     - **Legacy leading-zero octal integer without underscores
       (`\A[+-]?0[0-7]*[1-7][0-7]+\z`, requiring at least one non-zero octal
       digit `[1-7]` somewhere before the final digit, such as `010`, `0755`,
       `0100`, `0101`, `0200`, `-010`, `-0100`, `0010`):** In YAML 1.2.2 Core
       (§4.1 row 3), this resolves to a **base-10 `Value::Int`** (`10`, `755`,
       `100`, `101`, `200`, `-10`, `-100`, `10`), whereas in YAML 1.1 `!!int` it
       resolved to a **base-8 integer** (`8`, `493`, `64`, `65`, `128`, `-8`,
       `-64`, `8`). All-zero spellings (`0`, `+0`, `-0`, `00`, `000`) and
       single-nonzero-final-digit spellings (`01`..`07`, `007`, `0001`, `0007`,
       `-007`, `+07`) denote identical integer values (`0` or `±1..=±7`) in base
       8 and base 10 and are not warned.
       - `yaml12_resolution`: `int (<canonical base-10 decimal>)` (e.g.
         `"int (10)"` for `010`, `"int (-10)"` for `-010`).
       - `yaml11_note`: `"YAML 1.1 base-8 (octal) integer literal; use 0o prefix for octal in YAML 1.2 or quote as string"`.
     - **Base-2 binary integer (`\A[+-]?0b[0-1_]+\z` containing at least one
       binary digit `[01]`, such as `0b1010`, `-0b11_00`):** Matches YAML 1.1
       `!!int` base-2 and resolves to `Value::String` in YAML 1.2.2 Core.
       - `yaml12_resolution`: `string (<JSON-escaped raw>)`.
       - `yaml11_note`: `"YAML 1.1 base-2 (binary) integer literal"`.
     - **Signed hexadecimal integer (`\A[+-]0x[0-9a-fA-F_]+\z` containing at
       least one hex digit `[0-9a-fA-F]`, such as `-0x10`, `+0x10`, `-0x1_0`):**
       Matches signed YAML 1.1 `!!int` base-16 and resolves to `Value::String`
       in YAML 1.2.2 Core (where §4.1 row 5 `0x[0-9a-fA-F]+` is unsigned).
       - `yaml12_resolution`: `string (<JSON-escaped raw>)`.
       - `yaml11_note`: `"YAML 1.1 signed hexadecimal integer literal"`.
     - **Underscore-grouped YAML 1.1 integer or float literal (`raw` contains
       `'_'` and matches one of the following patterns that §4.1 Core resolves
       as `Value::String`):**
       - Legacy octal with `'_'`: `\A[+-]?0[0-7_]+\z` containing `'_'` and at
         least one octal digit `[0-7]` (e.g. `07_55`, `0_0`, `0_7`);
       - Decimal integer with `'_'`: `\A[+-]?[1-9][0-9_]*\z` containing `'_'`
         (e.g. `1_000`, `-1_000_000`);
       - Unsigned hexadecimal integer with `'_'`: `\A0x[0-9a-fA-F_]+\z`
         containing `'_'` and at least one hex digit `[0-9a-fA-F]` (e.g.
         `0x1_0`, `0xdead_beef`);
       - Decimal float with `'_'` (this profile's explicit decimal-float
         policy — requiring a decimal point `.`, at most one `.`, a digit
         `[0-9]` immediately after a leading `.` when there is no integer part,
         and a signed digit-only exponent `[eE][+-][0-9]+` when an exponent is
         present, correcting the YAML 1.1 `!!float` Type Repository page's
         permissive `[0-9.]` character-class typo while matching its
         `685.230_15e+03` example):
         `\A[+-]?([0-9][0-9_]*\.[0-9_]*|\.[0-9][0-9_]*)([eE][+-][0-9]+)?\z`,
         containing `'_'` (e.g. `1_000.5`, `1.5_0`, `1._5`, `.5_0`,
         `1_000.5e+2`, `685.230_15e+03`). (Underscore-free `1.e+2` already
         resolves as `Value::Float(100.0)` under YAML 1.2.2 Core §4.1 row 6 and
         is not warned; dotless `1_0e+2`, signless-exponent `1_000.5e2`,
         underscore-exponent `1.5e+1_0`, and leading-underscore-fraction `._5`
         are outside the YAML 1.1 `!!float` production and resolve to `String`
         in both YAML 1.1 and YAML 1.2.2 Core, so they are not warned.)
       (Spellings outside this rule set — such as `0o7_7`, comma-grouped
       `1,000`, or multi-dot `1.2.3` — are not warned by `lint_yaml11*`; note
       that `CompatQuoting::Yaml11Safe` in §10.2 quotes a broader defensive
       superset on emission.)
       - `yaml12_resolution`: `string (<JSON-escaped raw>)`.
       - `yaml11_note`: `"YAML 1.1 underscore-grouped numeric literal"`.
- **Coordinates, paths, and messages.**
  - `span` is the scalar or merge-directive key node's exact source `Span` (§7.2).
  - `document_index` is the 0-based index of the document in the stream.
  - `path` is the structured `Vec<PathSegment>` to the diagnostic site: `[]` at
    the document root; `PathSegment::Index(i)` inside sequence item `i`; for a
    scalar that is itself a mapping key (or for `Yaml11MergeKey`), `path` is the
    path to the enclosing `Mapping` (plus any nested sequence/mapping segments
    when a composite mapping key contains nested collections); for a scalar
    inside a mapping entry's value, `path` includes `PathSegment::Key(Box::new(key_value))`
    for that entry's resolved key (with spans, `via`, and `tag_start` stripped
    at every depth, matching `ErrorDetail::path` in §11.1).
  - `raw`, `yaml12_resolution`, and `yaml11_note` are formatted directly into
    UTF-8-safe buffers bounded by 1,024 bytes each (truncating with `"…"` if
    needed), and `message` is formatted deterministically into a UTF-8-safe
    buffer bounded by 4,096 bytes (§6):
    `YAML 1.1 compatibility hazard (<code_str>): unquoted <JSON-escaped raw> resolves as <yaml12_resolution> in YAML 1.2 Core (<yaml11_note>)`.
- **Diagnostic memory accounting and warning budget (`Limits::max_warnings`).**
  - Every matched hazard increments `warning_count` by 1 using saturating
    arithmetic (`warning_count.saturating_add(1)`).
  - When `diagnostics.len() < opts.limits.max_warnings` (default `1,000`),
    **before** cloning any `PathSegment::Key(Node)` or allocating the retained
    `LintDiagnostic`, `lint_yaml11*` charges the retained diagnostic against the
    call's cumulative §6 `nodes` (`opts.limits.max_nodes`) and `owned_bytes`
    (`opts.limits.max_owned_bytes`) budgets:
    1. Each `PathSegment::Key(key_node)` in `path` is checked against
       `weight(key_node) <= opts.limits.max_key_bytes` (`LimitExceeded(MaxKeyBytes)`)
       and error-key nesting metric **P** `<= opts.limits.max_depth`
       (`LimitExceeded(MaxDepth)`), and charges `nodes(key_node)` against
       `opts.limits.max_nodes` (`LimitExceeded(MaxNodes)`) and
       `weight(key_node)` against `opts.limits.max_owned_bytes`
       (`LimitExceeded(MaxOwnedBytes)`, evaluated in the §6 tie order
       `max_nodes`, then `max_owned_bytes`).
    2. The retained `LintDiagnostic` itself charges `1` node against
       `opts.limits.max_nodes` and
       `64 + 8 * path.len() + raw.len() + yaml12_resolution.len() + yaml11_note.len() + message.len()`
       bytes against `opts.limits.max_owned_bytes`.
    If any of these checks fails, `lint_yaml11*` aborts before cloning the path
    or diagnostic strings and returns `Err(Error)` with `LimitExceeded` at the
    hazard's `Site::Source(span.start)`. Otherwise the `LintDiagnostic` is
    appended to `diagnostics`.
  - When `diagnostics.len() == opts.limits.max_warnings`, the `(max_warnings + 1)`-th
    and any subsequent hazards set `truncated = true`, increment `warning_count`,
    and are omitted from `diagnostics` without cloning `path` or strings (and
    without charging additional diagnostic `nodes` or `owned_bytes`), while
    parsing, validation, and `warning_count` accumulation continue to the end of
    the stream. Thus `truncated == false` implies
    `warning_count == diagnostics.len() <= opts.limits.max_warnings`, and
    `truncated == true` implies
    `diagnostics.len() == opts.limits.max_warnings && warning_count > diagnostics.len()`.

### 10.4 Command-line interface (`yamlprims-cli`)

The `yamlprims` binary is a thin `#![forbid(unsafe_code)]` command-line consumer
of the `yamlprims` library crate. Its synopsis, flags, stream/file behavior, and
exit codes are frozen in Revision 1.6:

```
yamlprims [--merge=reject|apply|as_key] check [--lint-yaml11] [--format=text|json] [--] FILE...
yamlprims [--merge=reject|apply|as_key] dump [--plain|--ijson] [--max-output-bytes=N] [--] FILE
yamlprims [--merge=reject|apply|as_key] emit [--from-wire|--from-plain] [--compat-quoting=yaml11-safe|core-only] [--max-output-bytes=N] [--] FILE
yamlprims --help|-h
yamlprims --version|-V
```

- **Global and subcommand flags.**
  - `--merge=reject|apply|as_key` selects `Options.merge_keys` (`reject` default)
    and may appear before the subcommand or interspersed with subcommand flags
    before `--`.
  - `--compat-quoting=yaml11-safe|yaml11_safe|core-only|core_only` (or
    `--compat-quoting <val>`) is accepted on `emit` (`yaml11-safe` default).
  - `--max-output-bytes=N` (or `--max-output-bytes N`, with `N >= 1`) is
    accepted on `dump`, `emit`, and `check --lint-yaml11 --format=json` to
    override `limits.max_output_bytes`.
  - `--lint-yaml11` and `--format=text|json` (or `--format text|json`, `text`
    default) are accepted only on `check`; `--format` requires `--lint-yaml11`,
    and `--format=json` requires exactly one `FILE` argument.
  - `--help` / `-h` (alone) writes usage to `stdout` and exits `0`. `--version`
    / `-V` (alone) writes `yamlprims <CARGO_PKG_VERSION>\n` to `stdout` and
    exits `0`. Unknown or repeated flags, combining `--help` or `--version` with
    any other argument, missing `FILE`, or passing multiple `FILE` arguments to
    `dump`, `emit`, or `check --lint-yaml11 --format=json` writes usage to
    `stderr`, writes nothing to `stdout`, and exits `2`.
- **File and `stdin` handling.**
  - `-` selects `stdin` and may appear at most once per invocation (a second `-`
    is a usage error, exit `2`). `--` stops option parsing so following
    arguments (other than bare `-`, which still selects `stdin`) are treated as
    literal file paths even if they begin with `-`.
  - Each input read stops at `opts.limits.max_input_bytes + 1` bytes (`8 MiB + 1`
    default) via `Read::take`, so oversized inputs are reported by the core's
    `LimitExceeded(MaxInputBytes)` gate before UTF-8 validation.
- **Subcommands and output buffering.**
  - `check FILE...`: validates every input using multi-document stream semantics
    (`load_all_bytes`, or `lint_yaml11_bytes` when `--lint-yaml11` is set). An
    empty stream is valid (`0` documents, `0` warnings).
    - Without `--lint-yaml11`: silent on success (exit `0`). On a YAML error or
      file open/read failure, writes `<file>:<line>:<col>: <kind>: <message>\n`
      (or `<file>: <kind>: <message>\n` for `Site::NotSource`, or
      `<file>: cannot read: <io_error>\n` for read failure) to `stderr`,
      continues through remaining `FILE` arguments, and exits `1`.
    - With `--lint-yaml11` and `--format=text` (default): for each `FILE`, runs
      `lint_yaml11_bytes`. On a hard YAML or read error, writes the error
      diagnostic to `stderr` and sets exit status to `1`. On `Ok(result)`, writes
      each warning in `result.diagnostics` to `stderr` as
      `<file>:<start_line>:<start_col>: warning[<code>]: <message>\n` (followed
      by `<file>: warning[max_warnings]: truncated after <max_warnings> warnings (<warning_count> total)\n`
      when `result.truncated` is `true`); if `result.warning_count > 0`, sets
      exit status to `1`. Continues through all `FILE` arguments and exits `0`
      only when every file succeeded with zero warnings.
    - With `--lint-yaml11 --format=json FILE`: requires a single `FILE`. On a
      file open/read error, writes `<file>: cannot read: <io_error>\n` to
      `stderr` and exits `2`. On a hard YAML error, writes the error diagnostic
      to `stderr`, writes nothing to `stdout`, and exits `1`. On `Ok(result)`,
      encodes `result` to a canonical `yamlprims-lint/1` JSON envelope (§11.7)
      via `encode_lint(&result, &opts)` before writing `stdout`: if encoding
      fails (`OutputLimitExceeded`), writes the diagnostic to `stderr` with no
      `stdout` and exits `1`; otherwise writes the JSON line to `stdout` and
      exits `0` when `result.warning_count == 0` or `1` when
      `result.warning_count > 0`.
  - `dump FILE`:
    - Default (wire format): runs `load_all_bytes` and writes one
      `yamlprims-value/1` `documents` envelope + `\n` to `stdout` (exit `0`; an
      empty file emits `{"format":"yamlprims-value","version":1,"documents":[]}`).
      On a YAML error, writes the diagnostic to `stderr` and writes one
      `yamlprims-value/1` `error` envelope + `\n` to `stdout` (exit `1`; if
      `encode_error` itself fails with `OutputLimitExceeded`, writes that
      diagnostic to `stderr` with no `stdout` and exits `1`).
    - `--plain`: requires **exactly one document** (`docs.len() == 1` from
      `load_all_bytes`; zero-document empty streams and multi-document streams
      fail with `<file>: --plain needs exactly one document` on `stderr`, no
      `stdout`, and exit `1`) and writes `encode_plain(doc.root(), &opts)` + `\n`
      to `stdout` (exit `0`, or diagnostic on `stderr` with no `stdout` and exit
      `1` on loss/error per §11.3).
    - `--ijson`: runs `load_ijson_bytes` (§11.6) and writes the RFC 7493 I-JSON
      payload + `\n` to `stdout` (exit `0`, or diagnostic on `stderr` with no
      `stdout` and exit `1` on empty stream, multi-document stream, or any §11.6
      rule violation).
  - `emit FILE`: reads a YAML stream (or a `yamlprims-value/1` `documents` wire
    envelope with `--from-wire`, or a single plain JSON value with
    `--from-plain`), emits canonical YAML 1.2.2 Core text via `emit_all_with`
    (§10.2; zero bytes on an empty `0`-document stream), and writes it to
    `stdout` (exit `0`, or diagnostic on `stderr` with no `stdout` and exit `1`
    on load/decode/emit failure).
  - **Whole-payload buffering and exit codes.** Every command completes
    serialization in memory before writing to `stdout`, so YAML, policy, or
    `OutputLimitExceeded` errors never write a partial payload to `stdout`. Exit
    codes are:
    - `0`: success (with zero lint warnings when `--lint-yaml11` is active).
    - `1`: YAML input error, policy/projection/emission error, lint warning(s)
      found under `check --lint-yaml11`, or unreadable file under `check`.
    - `2`: CLI usage error, unreadable input file under `dump`, `emit`, or
      `check --lint-yaml11 --format=json`, or an OS `stdout` write/flush failure.

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
| `syntax`                         | `{"syntax_kind":"<snake_case>"}` (closed 81-code `SyntaxKind::ALL` vocabulary frozen in [ADR-0004](../decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md)) |
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
- A `syntax_kind` must belong to the closed 81-member `SyntaxKind::ALL`
  snake_case vocabulary frozen in
  [ADR-0004](../decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md)
  (and in `decode_wire`, an unknown `syntax_kind` code outside those 81 codes is
  rejected as a malformed wire error envelope). An unrepresentable kind/detail
  pair in `encode_error` is rejected as `Internal`.

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

There is no universal constant offset between H and P. Iterative `Drop` for
`Node`, checked caller-tree constructors, and iterative crate-owned traversals
are selected in §3; runtime qualification across all four bindings is tracked in
§13.2.

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

**Decoders** (`decode_wire` in Rust and Python, `DecodeWire` in Go, `decodeWire`
in TypeScript) validate `Options` first (`InvalidOptions` with
`Site::NotSource { last_known: None }`), enforce configured `Limits`
(`max_input_bytes` on the wire JSON UTF-8 byte length, `max_documents`,
`max_depth` using loader collection height **H** on `documents[*].root` and
error-key nesting metric **P** on `error.path[*].key` trees so `[[]]` has
**H = 2** and **P = 1**, `max_nodes`, `max_scalar_bytes`, `max_key_bytes`,
`max_mapping_entries`, `max_owned_bytes` including §6 `tag_bytes(tag) = uri.len() + handle.len() + suffix.len()` with no separate fixed tag-byte cap) returning `LimitExceeded` with
`Site::NotSource { last_known: None }`, and reject all malformed wire JSON or
schema/invariant violations — trailing tokens, duplicate JSON object keys,
unknown or missing fields, wrong member types, unsupported `format` or
`version != 1`, out-of-range metadata integers or coordinates, non-canonical
integer strings, float `bits` that are not 16 lowercase hex digits, `tagged`
nodes wrapping `null`/`bool`/`int`/`float`/`tagged` or `tag:yaml.org,2002:merge`
on a collection (§3, §4.4), empty or non-specific `"!"` or Core Schema `tag.uri`
values, inconsistent `tag.handle`/`tag.suffix`, NaN or
duplicate mapping keys, unknown `ErrorKind`/`SyntaxKind`/`LimitKind`/`RelatedRole`,
or non-null locations on error `PathSegment::Key` nodes — with `Internal` and
`Site::NotSource { last_known: None }` (using `Internal` because the
`yamlprims-value/1` `ErrorKind` vocabulary is frozen at wire version `1` and
wire payloads are an internal/inter-binding transport contract). Standard RFC
8259 insignificant whitespace and valid string escapes are accepted on input;
any `NaN` bit pattern normalizes to `0x7ff8000000000000`. Adding a field or kind
requires a new `version`. Encoding is fallible: exceeding `max_output_bytes` is
`OutputLimitExceeded`, checked while writing.

**Round trip.** For document envelopes produced by `encode(&docs, &enc_opts)`,
`decode_wire` followed by `encode` is byte-identical (including `tag_start`,
with NaN payloads canonicalized) whenever `dec_opts.limits.max_input_bytes`
accommodates the wire JSON length and `dec_opts.limits` match or exceed the
document tree's §6 metrics. For error envelopes produced by
`encode_error(&err, &enc_opts)`, because `encode_error` checks `max_depth` (path
segment count and error-key metric **P**), `max_key_bytes`, and
`max_output_bytes` whereas `decode_wire` additionally charges `max_nodes`,
`max_owned_bytes`, `max_scalar_bytes`, and `max_mapping_entries` across decoded
path-key nodes, `decode_wire` followed by `encode_error` is byte-identical
whenever `dec_opts.limits.max_input_bytes` accommodates the wire JSON length and
`dec_opts.limits` (`max_nodes`, `max_owned_bytes`, `max_scalar_bytes`,
`max_mapping_entries`, `max_depth`, `max_key_bytes`) accommodate the nodes in
`err.path`.

### 11.2 Host types (`Profile B`)

- **Go (`github.com/3leaps/yamlprims/yaml`).** Pure-Go twin (`CGO_ENABLED=0`,
  standard library only) under
  [ADR-0004](../decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md).
  `Value` exposes kind-checked accessors over `Null`, `Bool`, `Integer`
  (128-bit), `Float` (`float64` preserving bits), `String`, `Sequence`,
  `Mapping`, and `Tagged`. Every `Node` carries its `NodeSpan`, `Via`, and
  `TagStart`. Errors are `*yaml.Error`. Exposes `Load*`, `LoadReader*`,
  `LoadAllReader*`, `Unmarshal*`, `UnmarshalReader*`, `UnmarshalNode`,
  `Encode*`, `DecodeWire`, `ToPlain`, `FromPlain`, `Marshal`, `MarshalWriter*`,
  `Emit*`, `EmitWriter*`, `EmitAllWriter*`, `EncodeIJSON*`, `LoadIJSON*`,
  `LintYAML11*`, and `EncodeLint*`.
- **TypeScript (`@3leaps/yamlprims`).** Zero-import `wasm32-unknown-unknown`
  package. A discriminated union on `kind`: `int.value` is a `bigint`;
  `float.value` is a `number` built from `bits`; `mapping.entries` is a
  `Mapping` of `[Node, Node]` pairs. Errors are `YamlprimsError` with
  `byteOffset`, `utf16Offset`, `line`, `column`, `path`, and `related`. Exposes
  `load*`, `encode*`, `decodeWire`, `toPlain`, `fromPlain`, `loadPlain`,
  `parse`, `parseAll`, `loadIjson`, `dump`, `dumpAll`, `stringify`,
  `stringifyAll`, `lintYaml11`, and `encodeLint`.
- **Python (`yamlprims`).** `#![forbid(unsafe_code)]` `PyO3` `cp310-abi3`
  extension. `Node` preserves 128-bit `int`, `float` + `bits`, lossless
  `Mapping` (keyed by §5.1 `key_eq`, never colliding `True`, `1`, and `1.0`),
  and `Tagged`. Errors raise `YamlprimsError`. Exposes `load*`, `encode*`,
  `decode_wire`, `to_plain`, `from_plain`, `load_plain`, `safe_load`,
  `safe_load_all`, `load_ijson`, `dump`, `dump_all`, `safe_dump`,
  `safe_dump_all`, `lint_yaml11`, and `encode_lint`.

### 11.3 Plain conversion loss matrix

`toPlain` / `parse` / `parseAll` (TypeScript), `to_plain` /
`safe_load` / `safe_load_all` (Python), and `ToPlain` (Go) are explicitly lossy
projections: they drop source metadata (`NodeSpan`, `Via`, `TagStart`) and
host-indistinguishable YAML kind distinctions, such as `Int` versus `Float` in
a TypeScript `number`. They **fail** on the additional losses marked as errors
below. Use the `Node`/`Document` model or the wire format for lossless data.

| Value | TypeScript `toPlain` / `parse*` | Python `to_plain` / `safe_load*` | Go `ToPlain` |
| --- | --- | --- | --- |
| Int within ±(2^53−1) | `number` | `int` (arbitrary precision; full `i128::MIN..=u128::MAX` preserved) | `int64` |
| Int outside ±(2^53−1) | **error** (`IntegerOutOfRange`) | `int` (within `i128::MIN..=u128::MAX`) | `int64` if within `math.MinInt64..=math.MaxInt64`, else **error** (`IntegerOutOfRange`) |
| Float finite, −0.0, NaN, ±inf | `number` (in memory) | `float` (in memory) | `float64` |
| Float in JSON text output (`dump --plain`) | NaN/±inf → **error** (`FloatOutOfRange`); −0.0 written as `-0.0` | same | same |
| String key | object member (object created with `Object.create(null)`, so `__proto__` is an ordinary key) | `dict[str, Any]` entry (insertion order) | `map[string]any` |
| non-String key (Int, Bool, Null, Float, composite) | **error** (`Type`) | **error** (`Type`) | **error** (`Type`) |
| Tagged | **error** (`UnknownTag`) | **error** (`UnknownTag`) | **error** (`UnknownTag`) |
| spans, `via`, `tag_start` | dropped (documented) | dropped (documented) | dropped (documented) |

**Go `Unmarshal*` / `UnmarshalNode` target conversion (`yaml.Unmarshal`, `yaml.UnmarshalWith`, `yaml.UnmarshalReader`, `yaml.UnmarshalReaderWith`, `yaml.UnmarshalNode`).**
The Go binding provides typed unmarshaling into a caller-supplied target `v any`
under the same no-coercion, fail-closed guarantees as §8.1:
- **Target pointer requirement.** `v` MUST be a non-nil pointer; a `nil`
  interface or non-pointer (or typed `nil` pointer) fails closed with `Type`
  (`Site::NotSource { last_known: None }`).
- **Stream vs `Node` entrypoints.** `Unmarshal(data, v)` and
  `UnmarshalWith(data, v, opts)` call `LoadBytesWith(data, opts)` first (so a
  zero-document stream yields an implicit `Null` root and `> 1` documents fail
  with `MultipleDocuments` after §7.1 Phases 1–4 validation) and then call
  `UnmarshalNode(doc.Root, v)`. `UnmarshalReader(r, v)` and
  `UnmarshalReaderWith(r, v, opts)` read from `r` up to `max_input_bytes + 1`
  bytes (§10.1) before delegating to `UnmarshalWith`.
- **Untagged-only target requirement.** Except when unmarshaling into `*Node`,
  `*Value`, `*Document`, or `*Tagged`, encountering a `Value::Tagged` node fails
  closed with `Type` (`expected: "<target type>", found: "tagged value"`,
  located at `node.Span.Start`).
- **Supported Go target types** (unmarshaling recursively tracks `PathSegment`s
  and reports located `*Error` values at each node's `Span.Start`):
  - `*Node`, `*Value`, `*Document`, `*Tagged`: lossless copy of the decoded
    node/value (`*Document` wraps `node` with default metadata when called from
    `UnmarshalNode`, or preserves loaded metadata from `Unmarshal*`; `*Tagged`
    accepts `Value::Tagged` only and fails closed with `Type` on untagged
    nodes).
  - `*any` (`*interface{}`): delegates to `ToPlain(node)` (`Null` → `nil`,
    `Bool` → `bool`, `Int` in `int64` → `int64`, `Float` → `float64`, `String`
    → `string`, `Sequence` → `[]any`, `Mapping` with `String` keys →
    `map[string]any`).
  - `*bool`: accepts `Value::Bool` only (`Type` otherwise).
  - `*int`, `*int8`, `*int16`, `*int32`, `*int64`, `*uint`, `*uint8`,
    `*uint16`, `*uint32`, `*uint64`, `*uintptr`, `*Integer`, `*big.Int`:
    accepts `Value::Int` only (`Type` on `Float`, `String`, `Bool`, etc.);
    values outside the target integer width fail with `IntegerOutOfRange`.
  - `*float32`, `*float64`: accepts `Value::Float` or exact `Value::Int` per §8.1
    (`Value::Int` values not exactly representable in the target IEEE 754 binary
    format fail with `IntegerOutOfRange`; finite `Value::Float` values whose
    `float32(f)` overflows to `±Inf` fail with `FloatOutOfRange`).
  - `*string`: accepts `Value::String` only (`!utf8.ValidString` on synthetic
    nodes fails with `InvalidUtf8`).
  - Pointer `*T`: `Value::Null` sets `*v = nil`; any non-`Null` value allocates
    `new(T)` if `*v == nil` and unmarshals into `**v`.
  - Slice `*[]E` and fixed array `*[N]E`: accepts `Value::Sequence` only
    (`Value::Null` sets a slice target to `nil`, whereas `Value::Null` into a
    fixed array fails with `Type`); fixed-array length mismatch fails with
    `Type` (`expected: "sequence of length N"`).
  - `*map[K]V` and `*Mapping`: accepts `Value::Mapping` only (`Value::Null`
    sets `*map[K]V` to `nil`). For `map[K]V`, each entry's key and value are
    unmarshaled into `K` and `V`; if two YAML keys that were distinct under §5.1
    map to the same Go comparable key `K`, unmarshaling fails with
    `DuplicateKey`.
  - `*struct`: accepts `Value::Mapping` only. Field matching honors `yaml:"name,..."`
    struct tags (`yaml:"-"` ignores the field; the first comma-separated token,
    if non-empty, sets the YAML field name; otherwise the exact case-sensitive
    exported Go field name is used). Every mapping key into a `struct` MUST be
    `Value::String` (`Type` otherwise); any key not matching a non-ignored
    exported struct field fails closed with `UnknownField` (`Site::Source` at
    the offending key's `Span.Start`), matching §8.1 `#[serde(deny_unknown_fields)]`.

**Normative host-to-`Node` conversion (`fromPlain` / `from_plain` / `FromPlain` / `Marshal`).**
All host input conversions construct synthetic `Node` trees (`NodeSpan::Synthetic`)
under `EmitOptions::validate()`, checking `max_depth` (`<= 255`), `max_nodes`,
and `max_mapping_entries` before descending into child collections (or via
iterative heap worklists) so cyclic or over-deep host values fail with
`LimitExceeded(MaxDepth)` (or `LimitExceeded(MaxNodes)` /
`LimitExceeded(MaxMappingEntries)`, with `Site::NotSource { last_known: None }`)
rather than exhausting the VM or goroutine call stack. Host strings (`String`
values, mapping keys, and `Tag` fields) are validated for well-formed UTF-8 /
Unicode scalar values before any `TextEncoder` replacement, `UnicodeEncodeError`,
or `max_scalar_bytes` check: ill-formed UTF-8 bytes in Go (`!utf8.ValidString`)
or unpaired UTF-16 surrogates (`U+D800..=U+DFFF`) in TypeScript and Python fail
closed with `InvalidUtf8` (`Site::NotSource { last_known: None }`):

| Host input | TypeScript `fromPlain` / `dump*` / `stringify*` | Python `from_plain` / `dump*` / `safe_dump*` | Go `FromPlain` / `Marshal*` |
| --- | --- | --- | --- |
| Null / absent | `null` → `Null`; `undefined`, `symbol`, `function` → `Type` | `None` → `Null` | `nil` interface or typed `nil` pointer → `Null` |
| Boolean | `boolean` → `Bool` | `bool` (checked before `int`) → `Bool` | `bool` → `Bool` |
| Exact integer | `bigint` in `i128::MIN..=u128::MAX` → `Int`; outside → `IntegerOutOfRange` | `int` (non-`bool`) in `i128::MIN..=u128::MAX` → `Int`; outside → `IntegerOutOfRange` | `int*`, `uint*`, `yaml.Integer` → `Int` |
| Floating-point / JS `number` | `number`: `-0` (`Object.is(n, -0)`), non-finite, fractional, or outside `±(2^53 − 1)` → `Float` (canonicalizing `NaN`); finite safe integers in `±(2^53 − 1)` other than `-0` → `Int` (pass explicit `float` `Node` when `1.0` float kind is required) | `float` → `Float` (preserving `-0.0`, canonicalizing `NaN`) | `float32` (`float64(v)`), `float64` → `Float` (preserving `-0.0`, canonicalizing `NaN`) |
| String | well-formed `string` → `String` (lone surrogate `U+D800..=U+DFFF` → `InvalidUtf8`; `> max_scalar_bytes` → `LimitExceeded(MaxScalarBytes)`) | well-formed `str` → `String` (lone surrogate `U+D800..=U+DFFF` → `InvalidUtf8`; `> max_scalar_bytes` → `LimitExceeded(MaxScalarBytes)`) | valid UTF-8 `string` → `String` (`!utf8.ValidString` → `InvalidUtf8`; `> MaxScalarBytes` → `LimitExceeded(MaxScalarBytes)`) |
| Sequence | `Array` → `Sequence` | `list`, `tuple` → `Sequence` | slice (including `nil` slice → `[]`), array → `Sequence` |
| Mapping / struct | `Mapping`, `Map` (insertion order, keys converted recursively), or plain object (`Object.prototype` / `null` prototype, own enumerable string keys) → `Mapping` | `Mapping` or `dict` (insertion order, keys converted recursively) → `Mapping` | `yaml.Mapping` (entry order), `struct` (exported fields in declaration order honoring `yaml:"name,omitempty,-"` tags), `map[K]V` (`nil` map → `{}`, keys converted recursively and sorted deterministically by `bytes.Compare` of each key's `CompatQuoting::CoreOnly` emitted text) → `Mapping` |
| Host `Node` / `Tagged` / other | lossless synthetic copy validated per §3; non-plain objects (`Date`, `RegExp`, `Set`, …) → `Type` | lossless synthetic copy validated per §3; `bytes`, `set`, arbitrary objects → `Type` | `yaml.Node`, `yaml.Value`, `yaml.Document` validated per §3; `chan`, `func`, unsafe pointers → `Type` |

### 11.4 Host-side conformance

For every corpus case, each binding decodes the wire bytes; asserts its host
values independently (bigint / `big.Int` equality, float bit patterns, tag
URIs, key kinds, spans); and re-encodes canonically and compares the bytes
with the Rust encoder's. The same suite covers error objects, paths, and the
loss matrix.

### 11.5 WASM and `PyO3` native bridges (`Profile B`)

Under [ADR-0004](../decisions/ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md),
`yamlprims` ships a pure-Go twin rather than a `cgo` C ABI, a zero-import
`wasm32-unknown-unknown` module for TypeScript, and a `#![forbid(unsafe_code)]`
`PyO3` `abi3-py310` extension for Python:

- **Zero-import WASM ABI (`bindings/typescript`).** Linear memory is bounded;
  `options_reserve` and `input_reserve` enforce input and option ceilings before
  copying bytes into linear memory. Entry points return status `0` (`documents`
  wire envelope, `yamlprims-lint/1` JSON envelope, or UTF-8 payload), `1`
  (`error` wire envelope), `2` (compact `output_limit_exceeded` / `internal`
  fallback when even the error envelope exceeds `max_output_bytes`), or trap
  recovery via fresh instance recreation. `parse(input, opts?)` and
  `parseAll(input, opts?)` load single-document and multi-document streams and
  project each document root through `toPlain`; `stringify(value, opts?)` and
  `stringifyAll(values, opts?)` alias `dump` and `dumpAll`.
- **`PyO3` extension (`bindings/python`).** Releases the Python GIL during
  native parse/emit/projection/lint work and wraps native calls in
  `catch_unwind`, mapping status `2` to `OutputLimitExceeded` and status `3` to
  `Internal` (`YamlprimsError`).
- **Single-pass native `PyO3` `safe_load` / `safe_load_all` and `safe_dump` / `safe_dump_all` (`bindings/python`).**
  `safe_load(input, **opts) -> Any` and `safe_load_all(input, **opts) -> list[Any]`
  are distinct from `load_plain(input, **opts) -> str` (which continues to
  return plain JSON text): they accept `str`, `bytes` (`bytes`, `bytearray`,
  `memoryview`), or any file-like object with a `.read` method, parse the YAML
  stream in Rust with the GIL released, and construct Python `None`, `bool`,
  `int`, `float` (including `NaN` and `±inf`), `str`, `list`, and `dict` objects
  **directly from the in-memory `yamlprims::Node` tree inside `PyO3`** without
  intermediate `yamlprims-value/1` wire JSON serialization or Python `Node`
  wrapper allocation.
  - **Bounded stream reading until EOF.** When `input` is a file-like object
    with a `.read` method, option validation (`InvalidOptions`,
    `Site::NotSource { last_known: None }`) runs before any `.read(...)` call.
    Let `cap = opts.limits.max_input_bytes + 1` and `bytes_seen = 0`. While
    `bytes_seen < cap`, let `remaining = cap - bytes_seen` (`>= 1`). Before the
    first non-empty chunk establishes stream kind (and once established as a
    `str` stream, where Python `.read(n)` counts Unicode characters and 1
    character encodes to `1..=4` UTF-8 bytes), the binding calls
    `stream.read(min(16_384, remaining))`; once established as byte-like
    (`bytes`, `bytearray`, `memoryview`), subsequent calls use
    `stream.read(min(65_536, remaining))` (never requesting more than
    `remaining` bytes from a binary stream), looping until `.read(...)`
    returns an empty chunk (`""` or empty byte-like `b""`, EOF) or the
    cumulative UTF-8 byte count reaches `cap`:
    - All non-empty chunks from a stream MUST be uniformly `str` or uniformly
      byte-like (`bytes`, `bytearray`, `memoryview`). Each `str` chunk is
      validated for unpaired UTF-16 surrogates (`U+D800..=U+DFFF` →
      `InvalidUtf8` at `Site::NotSource { last_known: None }`) and measured in
      UTF-8 bytes (`chunk_bytes = len(chunk.encode("utf-8"))`) as it arrives so
      the `max_input_bytes` budget is enforced in UTF-8 bytes, not UTF-16 code
      units or Unicode scalar count.
    - If `bytes_seen + chunk_bytes >= cap` (i.e. `> opts.limits.max_input_bytes`,
      including when a non-conforming binary `.read(k)` returns more than `k`
      bytes or a multibyte `str` chunk pushes UTF-8 bytes past
      `opts.limits.max_input_bytes`), the binding stops reading immediately
      without buffering past `cap` bytes and fails closed with
      `LimitExceeded(MaxInputBytes)` at byte 0
      (`Site::Source(Location { byte_offset: 0, line: 1, column: 1 })`),
      matching §7.1 Phase 2. Thus well-behaved binary streams never consume
      more than `max_input_bytes + 1` bytes from the underlying stream, and text
      streams never read more than one bounded character chunk (`<= 16,384`
      characters, `<= 64 KiB` UTF-8) past the cap before failing closed at
      Phase 2.
    - A `.read(...)` call that raises an exception, returns `None` (such as a
      non-blocking `io.RawIOBase` would-block return), mixes `str` and
      byte-like chunks, or returns any other type fails closed with `Internal`
      (`Site::NotSource { last_known: None }`).
  - **Loss-matrix enforcement.** The native `Node` → Python object walk is
    iterative (heap-allocated frame stack, never recursing on the C stack) and
    enforces the exact §11.3 `to_plain` loss-matrix rules: any `Value::Tagged`
    node fails closed with `UnknownTag` (located at `node.span.start`), and any
    non-`String` mapping key fails closed with `Type`
    (`expected: "plain mapping string key"`, located at `key.span.start`).
  - **Buffered `safe_dump` / `safe_dump_all` stream delivery.**
    `safe_dump(value, stream=None, **opts)` and
    `safe_dump_all(values, stream=None, **opts)` validate and render the
    complete YAML output string `yaml_str` in memory before touching `stream`
    (so any `EmitLimits` or `OutputLimitExceeded` failure raises
    `YamlprimsError` with zero writes to `stream`). When `stream is None`, they
    return `yaml_str`. When `stream` is non-`None`, they deliver `yaml_str` via
    `stream.write` and return `None`: for a non-empty `yaml_str`, to guarantee
    linear $O(N)$ work even when a custom writer returns small positive
    character counts, the binding writes bounded slices
    `chunk = yaml_str[offset : offset + step]` (starting with
    `step = len(yaml_str)` on the first write call and clamping
    `step = min(1_024, len(yaml_str) - offset)` on all subsequent iterations
    after any short positive integer return `0 < n < len(chunk)`) in a loop
    until `offset == len(yaml_str)`:
    - If `n = stream.write(chunk)` returns `None`, `chunk` is treated as
      completely consumed (`offset += len(chunk)`).
    - If `n` is an `int` (and not `bool`): if `n <= 0` or `n > len(chunk)` (short
      zero-progress or out-of-bounds write), the call fails closed with
      `Internal` (`Site::NotSource { last_known: None }`); otherwise `offset`
      advances by `n` (clamping `step <= 1,024` characters if `n < len(chunk)`)
      and the loop continues until the entire `yaml_str` is written.
    - Any other return type or any exception raised by `stream.write(...)` fails
      closed with `Internal` (`Site::NotSource { last_known: None }`).
- **Host emission (`dump` / `dumpAll` / `stringify*` / `dump_all` / `safe_dump*`).**
  While Rust (`emit*`) and Go (`Emit*`) emit directly from in-memory `Document`
  / `Node` trees without wire serialization, TypeScript (`dump` / `dumpAll` /
  `stringify` / `stringifyAll`) and Python (`dump` / `dump_all` / `safe_dump` /
  `safe_dump_all`) run the §10.2 pre-order structural and `EmitLimits` validation
  walk on the host side first, strip source-only metadata on the emission wire
  envelope (`span: null`, `via: null`, `tag_start: null`,
  `tag: { uri, handle: "", suffix: uri }`), and verify
  `min_output_bytes = sum(string_utf8_bytes) + sum(rendered_tag_min_bytes(tag.uri)) <= max_output_bytes`
  (`OutputLimitExceeded`), where `rendered_tag_min_bytes(uri)` is
  `2 + (uri.len() - 18)` for non-empty `tag:yaml.org,2002:*` URIs and `uri.len()`
  otherwise — a strict lower bound on rendered YAML output that never rejects
  valid native-emitter inputs such as `!!foo ''\n`. Admitted envelopes pass
  through native `decode_wire` + `emit_all_with`. The bridge derives its
  internal `decode_wire` `Limits` from `Limits::default()` with the following
  fields overridden from `emit_opts.limits` (while `max_tag_bytes` is enforced
  by `emit_all_with` after wire decoding):
  `max_depth = emit_limits.max_depth`, `max_nodes = emit_limits.max_nodes`,
  `max_documents = emit_limits.max_documents`,
  `max_mapping_entries = emit_limits.max_mapping_entries`,
  `max_scalar_bytes = emit_limits.max_scalar_bytes`,
  `max_key_bytes = emit_limits.max_key_bytes`,
  `wire_cap = saturating_add(65_536, saturating_add(saturating_mul(max_nodes, 512), saturating_mul(max_output_bytes, 18)))`,
  and
  `owned_cap = saturating_add(65_536, saturating_add(saturating_mul(max_nodes, 128), saturating_mul(max_output_bytes, 4)))`.
  On the 64-bit native `PyO3` bridge (`Python`), `max_input_bytes` and
  `max_owned_bytes` use `wire_cap` and `owned_cap` directly so wire framing
  never narrows `EmitLimits`, guaranteeing byte-identical YAML output and
  diagnostic parity with Rust and Go across the full `EmitLimits` domain for
  equal `Document`/`Node` trees. On the 32-bit `wasm32` bridge (`TypeScript`),
  linear memory is bounded, so the supported WASM emission domain is
  `EmitLimits` intersected with a `64 MiB` (`67_108_864`-byte) canonical
  wire-envelope and `64 MiB` logical-owned-weight ceiling
  (`min(67_108_864, wire_cap)` and `min(67_108_864, owned_cap)`), mirroring
  `Limits::wasm_safe()` on load: inputs whose wire JSON and logical owned
  weight lie within that `64 MiB` WASM ceiling have byte-identical YAML output
  and diagnostic parity across all four bindings, while inputs exceeding the
  `64 MiB` WASM wire or owned ceiling fail deterministically with
  `LimitExceeded(MaxInputBytes, configured: 67_108_864)` or
  `LimitExceeded(MaxOwnedBytes, configured: 67_108_864)`
  (`Site::NotSource { last_known: None }`).
- **Host YAML 1.1 migration lint (`lintYaml11` / `lint_yaml11` / `LintYAML11*`).**
  Go implements `LintYAML11*` and `EncodeLint*` natively in pure Go; TypeScript
  (`lintYaml11`, `encodeLint`) and Python (`lint_yaml11`, `encode_lint`) invoke
  the native Rust `lint_yaml11` and `encode_lint` entrypoints across the WASM /
  `PyO3` bridge, decoding the canonical `yamlprims-lint/1` JSON envelope (§11.7)
  into host `LintResult` / `LintDiagnostic` structures (with TypeScript
  additionally computing `utf16Offset` on each diagnostic's `span.start` and
  `span.end` from the validated UTF-8 source).

### 11.6 Strict RFC 7493 I-JSON projection

`encode_ijson(node, opts)`, `load_ijson(input)`, and `load_ijson_with` /
`load_ijson_bytes` (Rust), `EncodeIJSON*` and `LoadIJSON*` (Go), `loadIjson`
(TypeScript), `load_ijson` (Python), and `yamlprims dump --ijson` (CLI) project
a YAML document to compact UTF-8 [RFC 7493](https://www.rfc-editor.org/rfc/rfc7493)
I-JSON text (no insignificant whitespace; [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785)
§3.2.2.2 string escaping and §3.2.2.3 number formatting; object members in
`Mapping` insertion order, leaving lexicographic UTF-16 key sorting to
downstream RFC 8785 JCS canonicalizers; see
[ADR-0005](../decisions/ADR-0005-data-layer-emitter-and-ijson-projection.md)).
Rust and Go expose both stream-level (`load_ijson*` / `LoadIJSON*`) and
in-memory `Node`-level (`encode_ijson` / `EncodeIJSON*`) entrypoints, whereas
TypeScript (`loadIjson`), Python (`load_ijson`), and the CLI (`dump --ijson`)
expose the stream-level entrypoint directly over source YAML text/bytes.

Because JSON has a single `number` type and no distinct `-0` token in RFC 8785
JCS, I-JSON projection is an intentional **many-to-one projection from the YAML
1.2 data model onto the RFC 7493 / RFC 8785 JSON value domain**: `Value::Int(1)`
and `Value::Float(1.0)` both project to JSON `"1"`, and `Value::Float(-0.0)` and
`Value::Float(0.0)` both project to JSON `"0"`. Unlike `encode_plain` (§11.3,
which remains unchanged and allows 128-bit JSON integers, `-0.0`, and implicit
empty-stream `null` via single-document `load`), I-JSON projection combines RFC
7493 interoperability constraints (§2.1 UTF-8 and noncharacter prohibition, §2.2
`binary64` finite numbers and `±(2^53 − 1)` exact integers, §2.3 unique string
member names) with fail-closed YAML-projection profile rules (single
non-empty-stream YAML document, explicit tag/merge refusal, and bounded output
bytes) so out-of-range `Value::Int` values, non-finite floats, non-`String`
mapping keys, and `Value::Tagged` wrappers fail closed rather than silently
rounding integers, stringifying keys, or stripping tags:

| # | Rule | Normative requirement | `ErrorKind` |
| --- | --- | --- | --- |
| 1 | **Single non-empty-stream document** | `load_ijson*` preserves §7.1 error precedence: Phases 1–4 (`InvalidOptions`, `max_input_bytes`, `InvalidUtf8`, and §7.1 bounded source syntax and `max_depth` validation across the whole input buffer) run before Phase 5 document-cardinality checks (`MultipleDocuments` at the start of any second document `Site::Source`, independent of `max_documents`, and empty-stream refusal at EOF `Site::Source` on zero-document streams such as `""`, comments/whitespace only, or bare `...` instead of synthesizing `null`). Thus `null\n---\n[\n` fails with `Syntax(unclosed_flow_collection)` and `null\n---\n[[[]]]\n` (`max_depth=2`) fails with `LimitExceeded(max_depth)`, while valid-syntax `null\n---\n1\n` fails with `MultipleDocuments`. Explicit `---\n` or `null`/`~` emits `"null"`; bare single documents (`a: 1`) are accepted. | `Type { expected: "single YAML document", found: "empty_stream" }` (`0` docs); `MultipleDocuments` (`> 1` docs) |
| 2 | **IEEE 754 safe integers** | `Value::Int` MUST lie within `-(2^53 − 1)..=+(2^53 − 1)` (`-9_007_199_254_740_991..=9_007_199_254_740_991`). `Value::Int` values outside that range (including `±2^53`, `i64::MIN`, `u64::MAX`, `2^64`) fail closed so distinct YAML integer literals never round-collide in `binary64`. | `IntegerOutOfRange` |
| 3 | **Finite `binary64` floats** | `Value::Float` MUST be finite. `.nan`, `.inf`, and `-.inf` fail closed. Finite `Value::Float` values (which are already IEEE 754 `binary64` floats in the YAML data model) format per RFC 8785 §3.2.2.3 (ECMAScript `Number::toString`, with `-0.0` → `"0"`, `1.0` → `"1"`, `1e20` → `"100000000000000000000"`, `1e21` → `"1e+21"`). | `FloatOutOfRange` |
| 4 | **String-only mapping keys** | Every `Mapping` key MUST be `Value::String`. `Bool`, `Int`, `Float`, `Null`, `Sequence`, and `Mapping` keys fail closed without stringification (`Value::Tagged` keys fail under Rule 5 `UnknownTag`, which takes precedence). Duplicate keys are already rejected during loading (§5.2) and checked on synthetic mappings. | `Type { expected: "I-JSON string key", found: "non_string_key" }` |
| 5 | **Tags and merge keys** | `load_ijson*` forces `UnknownTags::Reject` regardless of `opts.unknown_tags` (allowing callers to pass modified `Options::default()` structs without manually overriding the `UnknownTags::Preserve` loader default), and `encode_ijson` rejects any `Value::Tagged` node (keys and values alike) before key-kind checks. `MergeKeys::Reject` is the default; callers may opt into `MergeKeys::Apply` via `opts.merge_keys`; `MergeKeys::AsKey` (which is never a default) is rejected by `load_ijson*` with `InvalidOptions` so unquoted `<<` is never silently treated as a literal key across canonicalization boundaries. | `UnknownTag` / `MergeRejected` / `InvalidOptions` |
| 6 | **Unicode noncharacters & budgets** | Every `Value::String` (both mapping keys and string values) is validated for well-formed UTF-8 first (`InvalidUtf8`, including Go ill-formed UTF-8 strings on synthetic `Node` inputs and TypeScript/Python unpaired UTF-16 surrogates `U+D800..=U+DFFF` on source strings before `TextEncoder` replacement or `UnicodeEncodeError`) and then scanned left-to-right for the 66 Unicode noncharacters (`U+FDD0..=U+FDEF`, and code points on planes `0..=16` with `(cp & 0xFFFE) == 0xFFFE`); any match fails closed. `Options::ijson_default()` (Rust) and `DefaultIJSONOptions()` (Go) return `Options::default()` with `unknown_tags = UnknownTags::Reject` and `limits.max_output_bytes = 16 MiB` (`16_777_216`). Parameterless entrypoints (`load_ijson(input)` in Rust, `EncodeIJSON(node)` / `LoadIJSON(input)` in Go) and partial-option entrypoints when `max_output_bytes` is omitted (`loadIjson(input, opts?)` in TypeScript, `load_ijson(input, **opts)` in Python, and CLI `dump --ijson` without `--max-output-bytes`) merge onto `Options::ijson_default()` (`16 MiB` default output cap); in Rust and Go, explicit-struct entrypoints (`encode_ijson`, `load_ijson_with`, `load_ijson_bytes`, `EncodeIJSONWith`, `LoadIJSONWith`, `LoadIJSONBytes`) use `opts.limits.max_output_bytes` directly from the supplied struct (callers customizing `Options` while retaining the `16 MiB` I-JSON cap start from `Options::ijson_default()` / `DefaultIJSONOptions()`). | `Type { expected: "RFC 7493 I-JSON string", found: "noncharacter" }`, `InvalidUtf8`, `LimitExceeded`, `OutputLimitExceeded` |

**Traversal, normative `Type` strings, and `Site` parity.** The `Type.expected`
and `Type.found` strings in Rules 1, 4, and 6 above are normative across all
four bindings. Projection walks the root `Node` in iterative pre-order DFS
(`Sequence` items `0..len`; `Mapping` entries `0..len`, inspecting each entry's
`key` before its `value`) using a fresh per-call node counter (`1..=opts.limits.max_nodes`,
independent of any prior loader node counter). Each visited `Node` charges
`max_nodes` first (`LimitExceeded(MaxNodes)`), then checks `Value::Tagged`
(`UnknownTag`), then (in key position) non-`String` kind (`non_string_key`),
then scalar/collection budgets (`max_scalar_bytes`, `max_key_bytes`,
`max_depth`, `max_mapping_entries`) and scalar validity (`noncharacter`,
`IntegerOutOfRange`, `FloatOutOfRange`), reporting `Site::Source(span.start)` on
source-spanned nodes (using alias use-site provenance where present) and
`Site::NotSource { last_known: None }` on synthetic nodes, and checking
`max_output_bytes` incrementally as output bytes are written
(`OutputLimitExceeded` with `Site::NotSource { last_known: None }` and no
partial output).

### 11.7 YAML 1.1 migration lint wire format (`yamlprims-lint/1`)

`encode_lint(result, opts)` (Rust and Python), `EncodeLint(result)` /
`EncodeLintWith(result, opts)` (Go), `encodeLint(result, opts?)` (TypeScript),
and `yamlprims check --lint-yaml11 --format=json` (CLI) serialize a `LintResult`
(§10.3) as a single line of compact UTF-8 JSON (with a trailing `\n` when
written by CLI `check --lint-yaml11 --format=json`), using the same RFC 8785
string/number token rules and `PathKey` grammar as §11.1 (`Span`, `Position`,
and `PathKey` have the exact §11.1 definitions, with member order fixed as
written below):

```
LintEnvelope   = {"format":"yamlprims-lint","version":1,"warning_count":N,"truncated":bool,"diagnostics":[LintDiagnostic*]}
LintDiagnostic = {"code":LintCode,"span":Span,"document_index":N,"path":[PathStep*],"raw":S,"yaml12_resolution":S,"yaml11_note":S,"message":S}
PathStep       = {"key":PathKey} | {"index":N}
LintCode       = "yaml11_boolean" | "yaml11_number" | "yaml11_sexagesimal" | "yaml11_merge_key"
```

- **Invariants and preflight.** Before writing any output, `encode_lint`
  validates `opts` (`InvalidOptions`, `Site::NotSource { last_known: None }`)
  and validates the `LintResult` truncation, count, metadata-integer, `Span`,
  and path invariants:
  1. `result.diagnostics.len() <= opts.limits.max_warnings` (failing closed with
     `InvalidOptions` at `Site::NotSource { last_known: None }` when exceeded;
     `max_warnings` is a diagnostic-retention cap and not a `yamlprims-value/1`
     `LimitKind` variant).
  2. When `result.truncated == false`, `result.warning_count` MUST equal
     `result.diagnostics.len()`; when `result.truncated == true`,
     `result.warning_count` MUST be strictly greater than
     `result.diagnostics.len()` (and in results produced by `lint_yaml11*`,
     `result.diagnostics.len() == opts.limits.max_warnings`). Any caller-built
     `LintResult` violating this `(warning_count, truncated, diagnostics.len())`
     relationship fails closed with `InvalidOptions`
     (`Site::NotSource { last_known: None }`).
  3. Caller-built metadata integers, `LintCode`, and `Span` coordinates MUST be
     valid: `result.warning_count`, each diagnostic's `document_index`, each
     `PathStep::Index(i)`, and each `span.start.{byte_offset, line, column}` and
     `span.end.{byte_offset, line, column}` MUST be in `0..=9_007_199_254_740_991`
     (`2^53 - 1`, RFC 7493 safe integer range); each `code` MUST be one of the
     four closed `LintCode` values; and each `span` MUST have 1-based line and
     column numbers (`start.line >= 1`, `start.column >= 1`, `end.line >= 1`,
     `end.column >= 1`) and ordered endpoints
     (`start.byte_offset <= end.byte_offset` and
     `start.line < end.line || (start.line == end.line && start.column <= end.column)`).
     Any violation fails closed with `InvalidOptions`
     (`Site::NotSource { last_known: None }`) before writing any JSON.
  4. Each diagnostic's `path` is preflighted under the exact §11.1 error-path
     rules: `path.len() <= opts.limits.max_depth` (`LimitExceeded(MaxDepth)`),
     each `PathStep::Key(node)` has error-key nesting metric **P**
     `<= opts.limits.max_depth` (`LimitExceeded(MaxDepth)`) and logical subtree
     weight `weight(node) <= opts.limits.max_key_bytes`
     (`LimitExceeded(MaxKeyBytes)`), and all string fields are valid UTF-8
     (`InvalidUtf8`). Descriptive string fields are bounded on output with the
     same UTF-8-safe `"…"` (`U+2026`, 3 bytes) truncation marker as §11.1:
     `message` at 4,096 UTF-8 bytes and `raw`, `yaml12_resolution`, and
     `yaml11_note` at 1,024 UTF-8 bytes.
- **Output budget.** Total encoded JSON byte length is checked incrementally
  against `opts.limits.max_output_bytes`; exceeding `max_output_bytes` fails
  closed with `OutputLimitExceeded` (`Site::NotSource { last_known: None }`) and
  no partial output.

## 12. Design rationale

| Problem in existing stacks                                                                      | This specification                                                                      |
| ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| YAML 1.1 words (`yes`, `on`, `y`) silently become booleans, including in keys, and then collide | §4.1 Core schema only; §5.2 `DuplicateKey` with both locations; §10.3 migration lint    |
| Typed deserialization coerces quoted strings (`"8080"` → `u16`)                                 | §4.1 quoted scalars are String; §8 no coercion                                          |
| Integers beyond 64 bits error or silently round to f64                                          | `Integer` covers `i128::MIN..=u128::MAX`; out of range is an error                      |
| Local and global tags are dropped                                                               | non-core tags kept at every depth (§3, §4.4); typed targets never strip them (§8)       |
| Typed maps keep the last duplicate silently                                                     | one `DuplicateKey` policy on every path                                                 |
| Merge keys skipped on one path, kept as a literal key on another                                | merge recognized by the builder under an explicit policy (§5.3)                         |
| Alias expansion and document count unbounded in bytes and nodes                                 | §6 aggregate budgets, checked before copying                                            |
| Multi-document iteration misbehaves after an error                                              | `load_all` all-or-nothing; iterator fused (M4)                                          |
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
  input gate even though the Rust kind is `LimitExceeded`. Warning exhaustion
  (`max_warnings`) uses its `truncated: true` marker (§10.3), not a load-error
  pair.
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
  height check, merge precedence, and (M4) the iterator fuse. Every mutant
  must fail the suite.
- **Measurements, not gates:** peak memory and latency per default profile,
  and the engine's scalar buffering bound (§6).
- **Validation items:** depth accounting at 127/128/129 for lexical and
  materialized trees, including alias insertion; block collection spans with
  block scalars, chomping, comments, and trailing breaks; hash collision
  handling and order-independent composite-key hashing; anchor lifecycle
  (per-document reset, redefinition, alias inside a later definition, nested
  alias and merge origin); MSRV built and tested, not inferred.
- **Revision 1.5 caller-tree safety and wire decoding.** Drop, clone, compare,
  and debug-format a synthetic tree nested 10,000+ levels deep on a default
  thread stack without stack overflow. Exercise `Tag::try_new`,
  `Tag::with_spelling`, `Tagged::new`, and `Document::with_metadata` across
  valid local/global URIs (including empty scheme-specific `"x:"`, RFC 4151
  commas in `tag:example.com,2026:v`, `%TAG` handle expansions onto valid local
  or global URI prefixes, and percent-encoded control/delimiter/path bytes such
  as `x:[` → `!<x:%5B>`, `tag:example.com,2026:a#b` →
  `!<tag:example.com,2026:a%23b>`, `tag:example.com,2026:a|b`,
  `tag:example.com,2026:a\nb`, and `!a%` across all four bindings) and negative
  controls (empty URI, non-specific `!`, scheme-less `"$:?"`, Core Schema URIs,
  mismatched `handle`/`suffix`, non-`String` scalar or nested `Tagged` inner
  nodes, `!!merge` collections, major YAML version != 1, and minor YAML version
  > `999_999_999`). Verify `decode_wire` round-trip identity (including valid
  loaded tags exceeding 2 KiB within `max_owned_bytes`), `LimitExceeded`
  bounds (including shared error-envelope boundary vectors distinguishing
  error-key metric **P = 1** on `[[]]` from document height **H = 2**, and
  rejecting error path keys when `max_nodes` or `max_scalar_bytes` is exceeded),
  and `Internal` malformed-envelope controls (including `tagged` inner-kind /
  Core-tag violations and unknown `syntax_kind` rejection) across all four
  bindings (`Rust`, `Go`, `TypeScript`, `Python`) on the conformance vector
  suite, including 255-deep containers.
- **Revision 1.5 Core emitter.** Verify `emit` / `emit_all` self-round-trip
  (`load_all` ∘ `emit_all` == input modulo spans, `via`, `tag_start`, tag
  handle/suffix spelling, and §10.2 `explicit_start` normalization on directives
  and multi-document separators, preserving `yaml_version` including `None` on
  documents `i >= 1` following a `%YAML` document, `-0.0` sign bit, and
  canonical NaN under default `Options::default()` for `EmitOptions::default()`)
  across all valid `structcorpus` cases and in-repo conformance vectors, plus
  independently hand-authored canonical byte vectors pinning expanded nested
  collections, implicit non-`String` scalar keys (`null`, `bool`, `int`,
  `float`), `1,024`- vs `1,025`-byte simple-key boundary, mixed-metadata
  multi-document streams (`...\n` before later `%YAML 1.2`, and `---\n`
  resetting `yaml_version` to `None`), percent-encoded local/verbatim tags
  (`x:` → `!<x:>`, `x:[` → `!<x:%5B>`), `\ufffe`/`\uffff` double-quoted
  escaping, root-level leading-space multiline double-quoting, and
  shortest-decimal/exponent float formatting (`1.0e+21`, `1.5e-7`, `1.0`,
  subnormals, and `2^53` boundaries). Assert byte-for-byte output identity for
  structurally equal `Document` / `Node` trees across Rust (`emit_all_with`),
  Go (`EmitAllWith`), TypeScript (`dumpAll`), and Python (`dump_all`) within the
  `64 MiB` WASM wire/owned domain, including dense host-bridge sequences
  (`150,000`+ `null`/float nodes). Cover both `CompatQuoting::Yaml11Safe`
  (quoting `yes`/`no`/`on`/`off`/`y`/`n`, plain and underscored sexagesimals
  such as `10:20` and `1_0:20`, `0o`/`0x`/leading-zero octals, underscore- and
  comma-grouped numbers such as `1,000`, multi-dot numeric tokens such as
  `1.2.3`, and `YYYY-MM-DD`) and `CompatQuoting::CoreOnly`, multiline chomping
  (`|`, `|-`, `|+`, nested `|2`), explicit `? ` keys (collections, `Tagged`,
  keys > 1,024 bytes), `TagKindMismatch` on `Tagged` wrapping non-`String`
  scalars or `!!merge` keys/collections, `serde::Serialize` (`to_node` /
  `to_string` round-trip with `from_str` within §8.2's invertible scope), CLI
  `yamlprims emit`, and every `EmitLimits` rejection without partial output
  (including competing-failure precedence where `[{a: 1, b: 2}]` under
  `max_depth = 1, max_mapping_entries = 1` fails with
  `LimitExceeded(MaxDepth)`).
- **Revision 1.5 RFC 7493 I-JSON projection.** Exercise all six §11.6 rules,
  §7.1 source-validation precedence (including `null\n---\n[\n` →
  `Syntax(unclosed_flow_collection)` and `null\n---\n[[[]]]\n` at `max_depth=2`
  → `LimitExceeded(max_depth)` winning over `MultipleDocuments`, while valid
  `null\n---\n1\n` at `max_documents=10` → `MultipleDocuments`), pre-order
  diagnostic precedence (including `UnknownTag` winning on a `Tagged` mapping
  key), and exact `ErrorKind` values across Rust, Go, TypeScript, Python, and
  CLI `dump --ijson`: zero-document (`""`, `# comment\n`, `...\n`) vs
  explicit-null (`---\n`, `null`) vs multi-document streams; `±(2^53 − 1)` pass
  vs `±2^53` / `i64::MIN` / `u64::MAX` `IntegerOutOfRange`; finite float RFC
  8785 formatting (`-0.0` → `"0"`, `1.0` → `"1"`, `1e20`, `1e21`) vs
  `.nan`/`±.inf` `FloatOutOfRange`; `String` keys vs `Bool`/`Int`/`Null`/collection
  keys; unconditional `UnknownTags::Reject` vs opt-in `MergeKeys::Apply` and
  rejected `MergeKeys::AsKey`; all 66 Unicode noncharacters (`U+FDD0..=U+FDEF`
  and plane `0..=16` `0xFFFE`/`0xFFFF` code points) in both keys and values;
  and parameterless `16 MiB` vs explicit `Options` `max_output_bytes`
  enforcement.
- **Revision 1.6 reader/writer and host ergonomics.** Exercise Rust
  (`from_slice*`, `from_reader*`, `load_reader*`, `load_all_reader*`,
  `to_writer*`, `emit_writer*`, `emit_all_writer*`), Go (`LoadReader*`,
  `LoadAllReader*`, `Unmarshal*`, `UnmarshalReader*`, `UnmarshalNode`,
  `MarshalWriter*`, `EmitWriter*`, `EmitAllWriter*`), TypeScript (`parse`,
  `parseAll`, `stringify`, `stringifyAll`), and Python (`safe_load`,
  `safe_load_all`, `safe_dump`, `safe_dump_all`, including `bytes`/`bytearray`
  and file-like `.read()` / `.write()` streams). Verify bounded reader admission
  (`max_input_bytes + 1` via `io::Read::take` / `io.LimitReader` / looped
  `.read(min(chunk_cap, remaining))` until EOF failing with
  `LimitExceeded(MaxInputBytes)` at
  `Site::Source(Location { byte_offset: 0, line: 1, column: 1 })` without
  unbounded buffering, binary short-read streams near the cap never requesting
  more than `remaining = (max_input_bytes + 1) - bytes_seen` bytes, multibyte
  text streams enforced in UTF-8 bytes, and multi-chunk short-read Python
  streams reading to EOF rather than accepting a first-chunk prefix),
  reader/writer I/O error mapping to `Internal`
  (`Site::NotSource { last_known: None }`, including Python short/zero-progress
  `.write` rejection and `<= 1,024`-character windowed linear $O(N)$ delivery on
  1-character short writes), all-or-nothing writer buffering (no bytes written
  to `W` / `io.Writer` / `stream` when `EmitLimits` or `max_output_bytes`
  fails), Go `Unmarshal*` strict type checking (no string-to-number/bool
  coercion, `IntegerOutOfRange`, `FloatOutOfRange`, `UnknownField` on unknown
  struct fields, `DuplicateKey` on colliding map keys, `*Tagged` acceptance vs
  `Type` on `Tagged` into untagged targets), and Python single-pass native
  `safe_load*` parity with `to_plain` (while keeping `load_plain` returning JSON
  `str`, and verifying `UnknownTag` and non-`String` mapping key `Type`
  rejection with source spans).
- **Revision 1.6 YAML 1.1 migration lint (`yamlprims-lint/1`).** Exercise
  `lint_yaml11*` (Rust/Python), `LintYAML11*` (Go), `lintYaml11` (TypeScript),
  `encode_lint` / `EncodeLint*` / `encodeLint`, and CLI
  `yamlprims check --lint-yaml11 [--format=text|json]` on shared conformance
  vectors covering all four `LintCode` categories (`yaml11_boolean`,
  `yaml11_number`, `yaml11_sexagesimal`, `yaml11_merge_key`) in both key and
  value positions, multi-document streams (`document_index`), quoted/block/tagged
  and non-YAML-1.1 negative controls (zero warnings on `'yes'`, `"010"`, `|-`,
  `!!str on`, mixed-case `yEs`, zero-leading sexagesimal integer `0:10`,
  exponent-suffixed `1:30.5e+2`, underscore-free `1.e+2`, dotless `1_0e+2`,
  signless-exponent `1_000.5e2`, underscore-exponent `1.5e+1_0`,
  leading-underscore-fraction `._5`, single-nonzero-final-digit
  `01`..`07`/`0001`/`0007`/`-007`, `0o10`, `08`/`09` which resolve to Core
  `Int(8)`/`Int(9)` under §4.1, `0o7_7`, `1,000`, `1.2.3`, and YAML 1.2
  `true`/`false`/`0x10`/`.inf`/`.nan`), sexagesimal integers and floats (`1:30`,
  `10:20:30`, `10:20.5`, `0:10.5`, `00:30.0`), legacy octal (`010`, `0755`,
  `0100`, `0101`, `-010`, `-0100` → Core `int (10)`/`int (755)`/`int (100)`/`int (101)`/`int (-10)`/`int (-100)`
  vs YAML 1.1 `int (8)`/`int (493)`/`int (64)`/`int (65)`/`int (-8)`/`int (-64)`),
  signed hex (`-0x10`, `+0x10` → Core `str` vs YAML 1.1 `int`), underscore-grouped
  YAML 1.1 numbers (`1_000`, `07_55`, `0x1_0`, `1_000.5`, `1._5`, `.5_0`,
  `1_000.5e+2`, `685.230_15e+03` → Core `str` vs YAML 1.1 `int`/`float`), unquoted `<<` continued scanning under default
  `MergeKeys::Reject` vs clean admission under `MergeKeys::Apply`, alias/merge
  non-duplication (one warning at the anchor definition site), pre-clone
  diagnostic path/string `max_owned_bytes` and `max_nodes` admission (rejecting
  deep/large ancestor-key path multiplication with `LimitExceeded(MaxOwnedBytes)`
  before cloning), `max_warnings` boundary controls (exact-cap
  `warning_count == max_warnings` with `truncated: false` and
  `diagnostics.len() == max_warnings` vs cap-plus-one
  `warning_count == max_warnings + 1` with `truncated: true` and
  `diagnostics.len() == max_warnings`, plus `encode_lint` rejection with
  `InvalidOptions` at `Site::NotSource` of malformed caller-built `LintResult`
  values: `diagnostics.len() > max_warnings`, inconsistent
  `(warning_count, truncated, diagnostics.len())` triples, unknown `LintCode`,
  `line == 0` or `column == 0`, reversed `Span` endpoints, negative `< 0`
  `warning_count` / `document_index` / `PathStep::Index` / `Span` coordinates,
  and `warning_count` / `document_index` / `PathStep::Index` / `Span`
  coordinates above `2^53 - 1`), and `yamlprims-lint/1` wire
  byte parity across all four bindings.
- **Revision 1.6 CLI freeze.** Verify `yamlprims check` (including
  `--lint-yaml11` and `--format=text|json`), `yamlprims dump` (wire default,
  `--plain`, `--ijson`, and mutual exclusion of `--plain` + `--ijson`),
  `yamlprims emit`, `--version` / `-V`, `--help` / `-h`, `-` stdin and `--`
  end-of-flags handling, single non-empty document enforcement in `dump --plain`
  (exiting `1` on zero-document and multi-document streams), whole-payload
  buffered `stdout` writes, and exit codes `0` / `1` / `2`.

## 14. Open questions

1. **In-memory document iterator and Failsafe schema (M4).** YAML 1.1 migration
   lint is specified in §10.3 and §11.7 (`Revision 1.6`). Incremental
   in-memory document iteration and the YAML 1.2 Failsafe schema remain deferred
   until a concrete consumer requires them.
2. **Limits.** The §6 and §10.2 defaults remain subject to empirical peak-memory
   and latency measurement across target workloads.

## 15. Decisions

Policy and architecture decisions in this specification are recorded as decision
records in the yamlprims repository under `docs/decisions/` (`ADR-0001` through
`ADR-0005`).

## 16. Errata

Tagged revisions 1.0 through 1.5 remain preserved at their `spec-v1.x` tags.
Erratum numbering belongs to this public specification and restarts per revision;
cross-document references use revision-qualified identifiers such as `1.6/E-1`.
Specification publication and decision acceptance do not establish implementation
qualification.

### Revision 1.6 — unreleased draft

These proposed contract updates require maintainer acceptance and corresponding
conformance qualification. Historical tags (`spec-v1.0` through `spec-v1.5`) and
their pinned profiles remain unchanged.

| ID | Sections | Change from revision 1.5 |
| --- | --- | --- |
| E-1 | §1, §2, §8.1–8.2, §10.1–10.2, §11.2–11.3, §11.5, §13.2 | Specify bounded byte-slice, reader, and writer entrypoints in Rust (`from_slice*`, `from_reader*`, `load_reader*`, `load_all_reader*`, `to_writer*`, `emit_writer*`, `emit_all_writer*`) and Go (`LoadReader*`, `LoadAllReader*`, `Unmarshal*`, `UnmarshalReader*`, `UnmarshalNode`, `MarshalWriter*`, `EmitWriter*`, `EmitAllWriter*`), TypeScript ergonomic aliases (`parse`, `parseAll`, `stringify`, `stringifyAll`), and Python single-pass native `PyO3` `safe_load` / `safe_load_all` (constructing Python values directly from `yamlprims::Node` without wire JSON round-tripping, with looped `.read(...)` accumulation until EOF) and `safe_dump` / `safe_dump_all` (with looped `.write(...)` completion semantics). |
| E-2 | §1, §2, §6, §7.1, §10.1, §10.3, §11.2, §11.5, §11.7, §13.2, §14 | Specify the YAML 1.1 → YAML 1.2.2 Core migration lint surface (`lint_yaml11*`, `LintYAML11*`, `lintYaml11`, `encode_lint` / `EncodeLint*` / `encodeLint`, `LintCode`, `LintDiagnostic`, `LintResult`) anchored to the YAML 1.1 Type Repository regular expressions, pre-clone diagnostic path/string memory accounting under `max_nodes` and `max_owned_bytes`, `Limits.max_warnings` (`1,000`) truncation semantics, the `yamlprims-lint/1` JSON wire format, and CLI `yamlprims check --lint-yaml11 [--format=text\|json]`. |
| E-3 | §1, §5.5, §10.4, §13.2 | Freeze the `yamlprims-cli` command-line contract (`check`, `dump` wire/`--plain`/`--ijson`, `emit`, `--version`, `--help`, `-` stdin, `--` end-of-flags, single non-empty document requirement for `dump --plain`, whole-payload buffered `stdout`, and exit codes `0` / `1` / `2`). |
| E-4 | §1, §3, §15–16 | Reconcile milestone status and cross-document references for `v0.1.0`–`v0.3.0` (`ADR-0005` accepted, stack-safe `Drop`/`Clone`/`PartialEq`/`Debug` and crate traversals across all recursive walks). |

Wire grammar version 1 (`yamlprims-value/1`) and the §6 subtree-weight formula
are unchanged; §11.7 introduces the separate `yamlprims-lint/1` lint wire
grammar.

### Revision 1.5 — tagged draft specification

The following updates ([ADR-0005](../decisions/ADR-0005-data-layer-emitter-and-ijson-projection.md))
were incorporated in revision 1.5. Its immutable tag (`spec-v1.5`) and pinned
profiles remain unchanged.

| ID | Sections | Change from revision 1.4 |
| --- | --- | --- |
| E-1 | §3, §4.4, §7.1, §11.1, §13.2 | Add checked caller-tree constructors (`Document::new`, `Document::with_metadata` with 9-digit minor-version bound, `Tag::try_new` / `Tag::with_spelling` with RFC 3986 scheme / local-tag validation permitting empty scheme-specific `"x:"`, `Tagged::new` with §4.4 inner-kind enforcement), clarify §4.4's existing `!!merge` collection/scalar/key behavior without changing historical profile expectations, specify iterative heap-stack `Drop`/`Clone`/`PartialEq` and bounded `Debug` plus crate traversals, incorporate the closed 81-code `SyntaxKind` catalog (`ADR-0004`), and define symmetric `decode_wire` validation (including §3 `Tagged` inner-kind/tag checks, **H** vs **P** depth metrics, and error-envelope round-trip budget preconditions) across all four bindings. (`Internal` for malformed wire input and `InvalidOptions` for invalid tag metadata preserve the closed 23-variant `yamlprims-value/1` `ErrorKind` vocabulary and are candidates for dedicated variants in a future wire version `2`.) |
| E-2 | §8.2, §10.2, §13.2, §14 | Specify the deterministic 2-space block-first YAML 1.2.2 Core emitter (`emit`, `emit_all`, `EmitOptions`, `EmitLimits` with `max_mapping_entries: 100_000` and `max_output_bytes: 8 MiB` so default `emit_all` reloads under default `load_all`), exhaustive implicit/explicit mapping-key rules, root-safe literal block selection, RFC 3986 §3.3 verbatim tag escaping (`[`, `]`, `#`), `CompatQuoting::Yaml11Safe` default quoting (anchored full-string matching, including underscored sexagesimals, comma-grouped integers, and multi-dot numbers), `serde::Serialize` (`to_node`, `to_string`, `to_string_with`, and invertible round-trip scope), CLI `emit`, and four-language byte-for-byte emitter parity; renumber the deferred streaming/migration milestone from `M3` (`ADR-0002`, `spec-v1.0..1.4`) to `M4` as `M3` (`v0.2.0`) delivers emission and I-JSON projection. |
| E-3 | §10.1, §11.6, §13.2 | Specify the six-rule RFC 7493 I-JSON projection (`encode_ijson`, `load_ijson`, `load_ijson_bytes`, `Options::ijson_default()`, CLI `dump --ijson`, and host equivalents) with §7.1 source-validation precedence before Phase 5 document-cardinality checks, exact closed-set `ErrorKind` diagnostics, normative `Type` strings, and RFC 8785 number/string token formatting. |
| E-4 | §1, §2, §11.2, §11.5 | Align multi-language architecture sections with `ADR-0004` (`Profile B`: pure-Go twin, zero-import `wasm32` TypeScript with explicit `64 MiB` WASM wire/owned bridge domain, and `#![forbid(unsafe_code)]` `PyO3` `abi3-py310` Python). |

Wire grammar version 1 (`yamlprims-value/1`) and the §6 subtree-weight formula
are unchanged.

### Revision 1.4 — tagged draft specification

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
