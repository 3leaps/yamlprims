---
id: ADR-0005
title: Data-layer YAML 1.2.2 Core emitter, caller-tree bounds, and RFC 7493 I-JSON projection
status: proposed
supersedes_in_part: [ADR-0002]
date: 2026-11-09
---

# ADR-0005: Data-layer YAML 1.2.2 Core emitter, caller-tree bounds, and RFC 7493 I-JSON projection

## Decision

Extend `yamlprims` (`Profile B`, `.fulmen/repo.yaml`, `leaf: true`) in specification
revision `1.5` (`spec-v1.5`) and package release `v0.2.0` with two symmetric,
zero-dependency output surfaces across Rust (`crates/yamlprims`, `crates/yamlprims-cli`),
pure Go (`github.com/3leaps/yamlprims/yaml`), TypeScript WebAssembly
(`@3leaps/yamlprims`), and Python `PyO3/maturin` (`yamlprims`):

1. **Deterministic Data-Layer YAML 1.2.2 Core Emitter** (`emit`, `emit_with`,
   `emit_all`, `emit_all_with`, `to_node`, `to_string`, `to_string_with`, and
   `serde::Serialize` in Rust; `Emit`, `EmitWith`, `EmitAll`, `EmitAllWith`,
   `FromPlain`, and `Marshal` in Go; `fromPlain`, `dump`, and `dumpAll` in
   TypeScript and `from_plain`, `dump`, and `dump_all` in Python; `yamlprims emit`
   in the CLI) governed by explicit `EmitLimits` and `EmitOptions`
   (`CompatQuoting::Yaml11Safe` enabled by default).
2. **Strict RFC 7493 I-JSON Projection** (`encode_ijson`, `load_ijson`,
   `load_ijson_with`, and `load_ijson_bytes` in Rust; `EncodeIJSON`,
   `EncodeIJSONWith`, `LoadIJSON`, `LoadIJSONWith`, and `LoadIJSONBytes` in Go;
   `loadIjson` in TypeScript; `load_ijson` in Python; `yamlprims dump --ijson`
   in the CLI) enforcing exact single-document cardinality, `±(2^53 − 1)` safe
   integers, finite IEEE 754 `binary64` floats (`-0.0` formatted as `0`),
   `String`-only mapping keys, mandatory `UnknownTags::Reject`, and rejection of
   all 66 RFC 7493 Unicode noncharacters.
3. **Checked Caller-Tree Construction, Iterative Destruction, and Symmetric Wire
   Decoding** (`Document::new`, `Document::with_metadata`, `Tag::try_new`,
   `Tag::with_spelling`, `Tagged::new`, iterative heap-stack `Drop`, `Clone`,
   `PartialEq`, and depth-bounded `Debug` for `Node`, `Value`, `Mapping`,
   `Tagged`, and `Document`, and `decode_wire` in Rust matching `DecodeWire` in
   Go) closing the deferred caller-constructed-tree safety gate from `spec-v1.4
   §3`, `§8`, and `§14`.

Signed specification tags `spec-v1.0` through `spec-v1.4` remain immutable at
their tagged digests. `yamlprims-value/1` wire grammar (`§11.1`) remains version
`1` without adding wire fields or breaking existing consumers.

**Partial supersession of `ADR-0002` (following `ADR-0004` M2):** `ADR-0002`
deferred YAML text emission and strict JSON-subset output out of initial M1/M2
scope pending concrete downstream demand, and `ADR-0004` delivered M2
`Profile B` bindings (`v0.1.0`) while preserving that M3 deferral.
Downstream configuration-generation and YAML-to-JCS canonicalization pipelines
now provide concrete consumer demand, so this record supersedes `ADR-0002`'s M3
deferral of emission while preserving `ADR-0002`'s library-first workspace,
data-layer ownership, and `leaf: true` zero-sibling-dependency boundaries.

## Context and rationale

### 1. Why own a data-layer emitter instead of wrapping an event/CST emitter

`yamlprims` (`ADR-0002`, `ADR-0004`) owns a data-layer contract (`Document`,
`Node`, `Value`, `Integer`, `Mapping`, `Tagged`), not a concrete syntax tree:
comments and whitespace trivia are discarded during loading, core tags (`!!int`,
`!!bool`, …) direct resolution without wrapper retention, and anchors/aliases
are materialized into owned trees under explicit resource budgets.

Implementing emission directly over `Document` / `Node` / `Value` in safe Rust
and standard-library Go rather than adding an external event emitter crate:

- **Preserves `leaf: true` and minimal supply-chain closure:** Zero new Cargo,
  `go.mod`, npm, or PyPI dependencies.
- **Guarantees byte-for-byte `Profile B` parity:** Both Rust and Go execute the
  exact same deterministic layout and scalar-quoting state machine, which MUST
  be verified byte-for-byte in `make crosscheck` and across WASM/Python over the
  `yamlprims-value/1` wire bridge.
- **Eliminates cross-schema round-trip drift:** Every emitted scalar is checked
  against `yamlprims`'s own Core Schema resolution rules (`§4.1`) and, by
  default, legacy YAML 1.1 scalar hazards (`CompatQuoting::Yaml11Safe`).

### 2. Explicit non-goals

- **No comment, style, or anchor preservation:** `yamlprims` is not a
  source-preserving YAML formatter or CST refactoring engine.
- **No anchor (`&a`), alias (`*a`), or merge-directive (`<<`) emission:** Trees
  are emitted as explicit materialized values.
- **No sibling-primitive coupling:** `yamlprims` remains `leaf: true` and does
  not depend on downstream RFC 8785 JCS libraries. `encode_ijson` produces
  strict RFC 7493 I-JSON UTF-8 bytes with RFC 8785 string/number token
  formatting so downstream consumers can compose YAML ingestion with JCS
  canonicalization cleanly.

## Normative design

### 1. Deterministic block-first YAML 1.2.2 Core emission

- **Encoding and line endings:** Valid UTF-8 without BOM; LF (`\n`) line breaks
  only; every non-empty emitted stream ends with `\n`. `emit_all(&[])` returns
  `""` (zero bytes), preserving `load_all("") -> []`.
- **Stream and document boundaries (YAML 1.2.2 §9.2):**
  - Any `doc.yaml_version` with `major != 1` or `minor > 999_999_999` (exceeding
    the §9 nine-digit scanner limit) is rejected as `UnsupportedYamlVersion`
    (`Site::NotSource { last_known: None }`).
  - For document index `i == 0`: if `doc.yaml_version == Some((1, minor))`, emit
    `%YAML 1.<minor>\n---\n`; otherwise if `doc.explicit_start` is `true`, emit
    `---\n`; otherwise emit the root node directly.
  - For document index `i >= 1`: if `doc.yaml_version == Some((1, minor))`, emit
    `...\n%YAML 1.<minor>\n---\n` so the preceding document is explicitly
    terminated before directives per YAML 1.2.2 §9.2; otherwise emit `---\n`
    before the root node.
  - `Value::Null` always emits as `null` (never empty text), so `emit(&node)` on
    a `Null` node produces `"null\n"`, round-tripping unambiguously through both
    `load` and `load_all`.
- **Expanded block collection placement (no compact `- - x` or `- k: v`):**
  - Fixed 2-space indentation per collection level (`pad = " ".repeat(indent)`).
  - Empty `Sequence` and `Mapping` always emit inline as `[]` and `{}` (while
    still counting `1` collection level toward `max_depth`, matching §5.4 loader
    height `H`).
  - Non-empty `Sequence` at `indent`:
    - Single-line scalar or empty `[]`/`{}` item: `{pad}- {rendered}\n`.
    - Literal block `String` item: `{pad}- {header}\n` + body lines indented by
      `indent + 2` spaces (empty body lines emit as bare `\n` with zero spaces).
    - Non-empty `Sequence` or `Mapping` item: `{pad}-\n` followed by the child
      collection at `indent + 2`.
    - `Tagged(tag, inner)` item: `{pad}- {tag} {rendered}\n` for single-line
      `String` or empty `[]`/`{}`; `{pad}- {tag} {header}\n` + body lines at
      `indent + 2` for literal block `String`; `{pad}- {tag}\n` + child
      collection at `indent + 2` for non-empty `Sequence` or `Mapping`.
  - Non-empty `Mapping` at `indent` (preserving `Mapping` entry order; every
    `Value` key variant is partitioned exhaustively below):
    - **Implicit key** — untagged `Null` (`null`), `Bool` (`true` / `false`),
      `Int` (canonical decimal), finite or infinite `Float` (`0.0`, `-0.0`,
      shortest decimal, `.inf`, `-.inf`; `NaN` keys cannot exist in `Mapping`
      per §5.1 and reject as `InvalidKey`), or single-line `String` whose
      rendered UTF-8 length is `<= 1,024` bytes (for example `{Int(1): String("x")}`
      emits `1: x\n` and `{Null: Bool(true)}` emits `null: true\n`):
      - Single-line scalar or empty `[]`/`{}` value: `{pad}{key}: {val}\n`.
      - Literal block `String` value: `{pad}{key}: {header}\n` + body lines at
        `indent + 2`.
      - Non-empty `Sequence` or `Mapping` value: `{pad}{key}:\n` + child
        collection at `indent + 2` (block sequences under mapping keys are
        always indented `indent + 2`, never indentless).
      - `Tagged(tag, inner)` value: `{pad}{key}: {tag} {val}\n` (single-line or
        empty `[]`/`{}`), `{pad}{key}: {tag} {header}\n` + body lines at
        `indent + 2` (literal block), or `{pad}{key}: {tag}\n` + child
        collection at `indent + 2` (non-empty collection).
    - **Explicit key** — `Sequence`, `Mapping`, `Tagged`, or `String` keys that
      select multiline style or whose rendered single-line form exceeds `1,024`
      UTF-8 bytes (honoring §9's 1,024-byte simple-key lookahead rule):
      - Key prefix: `{pad}? {rendered}\n` (single-line scalar, empty `[]`/`{}`,
        or `Tagged` wrapping either; mapping keys containing `\n` always select
        Style 1 double-quoted per §2 step 1), `{pad}?\n` + collection at
        `indent + 2` (non-empty collection), or `{pad}? {tag}\n` + collection at
        `indent + 2` (`Tagged` non-empty collection).
      - Value line immediately following at `{pad}:`: `{pad}: {rendered}\n`,
        `{pad}: [tag ]{header}\n` + body lines at `indent + 2`, `{pad}:\n` +
        collection at `indent + 2`, or `{pad}: {tag}\n` + collection at
        `indent + 2`.
  - Root `Tagged(tag, inner)` at `indent = 0`: `{tag} {rendered}\n` (single-line
    `String` or empty `[]`/`{}`), `{tag} {header}\n` + body lines at `2` spaces
    (literal block when no explicit indentation digit is needed), or `{tag}\n` +
    root collection at `indent = 0` (non-empty collection).

### 2. Scalar formatting and `CompatQuoting` (`Yaml11Safe` default vs `CoreOnly`)

| `Value` kind | Emitted representation |
| --- | --- |
| `Null` | `null` |
| `Bool(b)` | `true` or `false` |
| `Int(i)` | Canonical base-10 decimal (`0` or `-?[1-9][0-9]*` across `i128::MIN..=u128::MAX`) |
| `Float(f)` | `.nan` (any NaN), `.inf` (`+∞`), `-.inf` (`-∞`), `-0.0` (`-0.0`), `0.0` (`+0.0`), else shortest round-trip decimal below |
| `String(s)` | Deterministic 4-style selection below |

**Canonical finite non-zero `Float` formatting:** Using the IEEE 754 shortest
round-trip decimal `(neg, digits, k)` with round-to-even tie-breaking (`|f| =
0.<digits> × 10^k`, `n = digits.len()`, `sci_exp = k − 1`):
- When `-6 <= sci_exp < 21`: emit fixed-point decimal (`[-]digits` + `"0"^(k−n)`
  + `".0"` when `k >= n`, so integral floats always carry `.0` and never resolve
  as `Int`; `[-]digits[..k].digits[k..]` when `0 < k < n`; `[-]0.` + `"0"^(-k)`
  + `digits` when `k <= 0`).
- When `sci_exp < -6` or `sci_exp >= 21`: emit lowercase scientific notation
  with an explicit decimal point and exponent sign (`[-]digits[0].0` when
  `n == 1`, `[-]digits[0].digits[1..]` when `n > 1`, followed by `e+`/`e-` +
  decimal `|sci_exp|` without leading zeros, e.g. `1.0e+21`, `1.5e-7`) so every
  finite YAML float contains `.` and resolves as float across both YAML 1.2.2
  Core and legacy YAML 1.1 loaders.

For `Value::String(s)`:

1. **Double-quoted (`"..."`) when escaping is required (YAML 1.2.2 §5.1):** Used
   whenever `s` contains any C0 control (`0x00..=0x1F` other than `\n`,
   explicitly including `\t` `0x09` and `\r` `0x0D` so tabs never appear
   unescaped in plain, single-quoted, or block scalars), `0x7F`, C1 controls
   (`U+0080..=U+009F`, including `U+0085` NEL), `U+2028`, `U+2029`, `U+FEFF`
   (BOM), `U+FFFE`, or `U+FFFF` — or when `s` consists solely of `\n` bytes
   (`"\n"`, `"\n\n"`, …), or when `s` is a mapping key containing `\n`, or when
   `s` is a root-level scalar (`enclosing_collections == 0`) containing `\n`
   whose first byte is `' '` or `'\n'` (avoiding YAML 1.2.2 `n = −1` vs libyaml
   `n = 0` root indentation-indicator ambiguity). Escapes use the seven 2-byte
   short escapes (`\"` for `0x22`, `\\` for `0x5C`, `\b` for `0x08`, `\t` for
   `0x09`, `\n` for `0x0A`, `\f` for `0x0C`, and `\r` for `0x0D`) and 6-byte
   lowercase `\uXXXX` for all other escaped code points (`0x00..=0x07`, `0x0B`,
   `0x0E..=0x1F`, `0x7F..=0x9F`, `\u2028`, `\u2029`, `\ufeff`, `\ufffe`,
   `\uffff`); all other valid UTF-8 scalar values (including `U+FDD0..=U+FDEF`
   and non-BMP scalar values) are written directly as UTF-8.
2. **Literal block (`|`, `|-`, `|+`) for clean multiline values:** Used in
   non-key positions when `s` contains `\n`, has at least one non-`\n` byte,
   and was not selected by step 1:
   - Chomping: `|-` when `s` does not end with `\n`; `|` when `s` ends with
     `\n` and `s` neither equals `"\n"` nor ends with `"\n\n"`; `|+` when `s`
     ends with `"\n\n"`.
   - Indentation indicator: inside enclosing collections (`enclosing_collections >= 1`),
     emit `|2` / `|2-` / `|2+` when the first byte of `s` is `' '` or `'\n'`,
     otherwise `|` / `|-` / `|+` (at root, leading `' '` or `'\n'` already uses
     Style 1 double-quoted, so root literal blocks never emit an explicit digit).
     Non-empty content lines are indented `indent + 2` spaces (at least 2 spaces
     at root, preventing column-0 `---` or `...` document-marker collisions);
     empty lines emit zero indentation spaces (`\n`).
3. **Plain (unquoted) for unambiguous single-line strings:** Used when `s` is
   non-empty, `<= 1,024` bytes, has no leading or trailing whitespace (`' '`,
   `'\t'`, or Unicode `White_Space`), does not start with
   `, [ ] { } # & * ! | > ' " % @ \``, does not start with `-`, `?`, or `:`
   followed by end-of-string or ASCII whitespace (`' '` or `'\t'`), does not
   equal or start with `---` or `...` followed by end-of-string or whitespace,
   does not contain `: `, `:\t`, ` #`, or `\t#`, does not end with `:`, is not
   `<<` or `=`, **does not match any Core Schema non-string pattern**
   (`§4.1` rows 1–8), and — under `CompatQuoting::Yaml11Safe` (the default) —
   **does not match any YAML 1.1 scalar hazard** (each pattern below is anchored
   across the entire string `\A…\z`):
   - Case-insensitive YAML 1.1 booleans/nulls: `y`, `n`, `yes`, `no`, `on`,
     `off`, `true`, `false`, `null`.
   - Underscore- or comma-grouped, multi-dot, or YAML 1.1 radix/float literals:
     `[+-]?0[0-9_]+`, `[+-]?0b[01_]+`, `[+-]?0x[0-9a-fA-F_]+`,
     `[+-]?([0-9_]+\.[0-9_.]*|\.[0-9_][0-9_.]*)([eE][+-]?[0-9_]+)?` (requiring
     at least one ASCII digit, including multi-dot tokens such as `1.2.3`),
     `[+-]?[0-9][0-9_,]*([eE][+-]?[0-9_]+)?` (including comma-grouped integers
     such as `1,000`), and ASCII case-insensitive `[+-]?\.inf` / `\.nan`.
   - Sexagesimal int and float literals (including underscored leading groups):
     `[+-]?[0-9][0-9_]*(:[0-5]?[0-9])+([.][0-9_]*)?`
     (such as `10:20`, `1_0:20`, or `1:05:30.5_0`).
   - YAML 1.1 `!!timestamp` date/time prefixes:
     `[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}([Tt \t].*)?` (such as `2026-11-09`).
4. **Single-quoted (`'...'`) vs double-quoted (`"..."`) fallback:** Single-line
   printable strings rejected from plain style emit as `'...'` when `!s.contains('\'')`,
   and as `"..."` when `s.contains('\'')`.

**Self-round-trip invariant:** Every document slice `docs` accepted by default
`emit_all(docs)` (`EmitOptions::default()`) reloads under default
`load_all(&emitted)` (`Options::default()`) with zero limit overrides (`8 MiB`
`max_output_bytes <= max_input_bytes`, `100,000` `max_mapping_entries`, `64 KiB`
`max_key_bytes` charging `2 * tag.uri.len()`, and `max_owned_bytes <= 65_536 +
128 * 250_000 + 4 * 8_388_608 = 65_619_968 <= 67_108_864 = 64 MiB`). More
generally, for any custom `emit_opts`, loading the emitted UTF-8 text under
`reload_opts` with `UnknownTags::Preserve`, `MergeKeys::Reject`, and
`reload_opts.limits` satisfying `max_input_bytes >= emitted.len()`,
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
`yaml_version` and root `Value` under `§5.1` `key_eq` extended so `NaN` equals
`NaN` and `-0.0` preserves its sign bit (`f64::to_bits` modulo canonical `NaN`).
Document `0` preserves `explicit_start || yaml_version.is_some()`, while
documents at index `i >= 1` load back with `explicit_start == true` and
`yaml_version == doc.yaml_version` (so a document `i >= 1` with
`yaml_version == None` after an earlier `%YAML` document reloads with
`yaml_version == None`). Because `Tagged` scalar payloads are `Value::String`
and source-only scalar style (`inner_plain`) is not part of `Value` equality or
`yamlprims-value/1`, source-dependent `!LocalTag 7` plain-scalar enum coercion
(`§8.1`) is not preserved through data-layer `emit`, whereas Serde typed
round-trip (`from_str_with::<T>(&to_string_with(&v, &emit_opts)?, &reload_opts)`)
holds for all `Serialize + DeserializeOwned` types whose `§8.2` representation
is injective and accepted by `§8.1` (excluding `Option<()>` / nested
`Option<Option<T>>` where `Some(None)` and `None` both serialize to `Null`,
`TaggedValue<T>` wrapping non-`String` scalars or nested `TaggedValue`,
`Spanned<T>` source spans, and non-injective caller Serde attributes such as
ambiguous `#[serde(untagged)]`, colliding `#[serde(flatten)]`, or asymmetric
`skip_serializing_if`).

### 3. Caller-tree safety, `EmitLimits`, tag grammar, and `decode_wire`

- **Iterative `Drop`, `Clone`, `PartialEq`, and non-recursive crate traversals:**
  `Drop`, `Clone`, and `PartialEq` for `Node`, `Value`, `Mapping`, `Tagged`, and
  `Document` in Rust traverse nested `Sequence`, `Mapping`, and `Tagged`
  children iteratively using heap work stacks (`Node::into_value` moves `Value`
  out cleanly without E0509 destructuring restrictions), and `Debug` for `Node`,
  `Value`, `Mapping`, `Tagged`, and `Document` bounds formatting nesting at
  depth `255` (rendering deeper collection children as `"..."`), preventing
  call-stack overflow on arbitrarily deep caller-constructed trees even when no
  `Options` are present. Crate-owned and binding traversals (`key_eq`,
  `contains_nan`, `hash_into`, `budget::metrics`, `from_node` preflight,
  `to_node`, wire encoders, `decode_wire`, `encode_ijson`, `emit`, Go
  `Emit*`/`FromPlain`/`Marshal`, TypeScript `fromPlain`/`dump*`, and Python
  `from_plain`/`dump*`) use iterative heap stacks or check `max_depth` (`<= 255`)
  before descending into child collections so over-deep or cyclic host values
  fail with `LimitExceeded(MaxDepth)` rather than exhausting the call stack.
- **`EmitLimits` and emitter diagnostic precedence (every limit `>= 1`, `max_depth <= 255`, `max_scalar_bytes <= max_output_bytes`, `max_key_bytes <= max_output_bytes`):**
  - `max_depth: usize` (default `128`, `<= 255`; counts collection levels
    identically to §5.4 loader height `H`, empty collections included and
    `Tagged` transparent)
  - `max_nodes: usize` (default `250_000`; counts every visited `Node` across
    the call, including `Tagged` wrappers)
  - `max_scalar_bytes: usize` (default `1 MiB`; UTF-8 byte length of any
    `Value::String`)
  - `max_key_bytes: usize` (default `64 KiB`; logical subtree weight of any
    mapping key, charging `NODE_OVERHEAD + 2 * tag.uri.len()` per `Tagged`
    wrapper so reloaded keys with `handle = ""` and `suffix == uri` fit within
    the same `Limits::max_key_bytes` budget)
  - `max_mapping_entries: usize` (default `100_000`; entry count `entries.len()`
    of any single `Value::Mapping`, matching `Limits::max_mapping_entries`)
  - `max_tag_bytes: usize` (default `1_024`; UTF-8 byte length of `Tag::uri`,
    reported as `InvalidOptions` because `yamlprims-value/1` `LimitKind` is
    frozen at wire version `1`)
  - `max_documents: usize` (default `1_024`)
  - `max_output_bytes: usize` (default `8 MiB` `8_388_608`, matching
    `Limits::default().max_input_bytes`; checked incrementally while writing
    after structural preflight succeeds, failing with `OutputLimitExceeded` and
    returning no partial output)
  - **Deterministic error precedence:** `EmitOptions::validate()` runs first
    (`InvalidOptions`), then `docs.len() > max_documents` (`LimitExceeded(MaxDocuments)`),
    then a complete iterative pre-order structural and `EmitLimits` preflight
    runs over `0..docs.len()` (`UnsupportedYamlVersion` on `yaml_version`; at
    each visited `Node` in pre-order DFS: `LimitExceeded(MaxNodes)`, mapping-key
    `LimitExceeded(MaxKeyBytes)`, `LimitExceeded(MaxScalarBytes)` on `String`,
    `InvalidKey` on `Float(NaN)` key and `DuplicateKey` on duplicate keys,
    `InvalidOptions` on invalid/oversized `Tag` then `TagKindMismatch` on
    `Tagged`, `LimitExceeded(MaxDepth)` when entering `Sequence`/`Mapping`, and
    `LimitExceeded(MaxMappingEntries)` on `Mapping`) **before** emitting UTF-8
    YAML bytes with incremental `max_output_bytes` checks
    (`OutputLimitExceeded`), all with `Site::NotSource { last_known: None }`.
- **Checked constructors (`Tag`, `Tagged`, `Document`) and YAML 1.2.2 §5.6 / §6.9.1 tag rules:**
  - `Tag::uri` stores the resolved decoded tag identity. Existing infallible
    `Tag::new(uri)` is retained for source compatibility, while `Tag::try_new(uri)`
    and `Tag::with_spelling(uri, handle, suffix)` validate syntax and structural
    consistency (independent of `EmitLimits::max_tag_bytes`, so any valid tag
    produced by `load*` or custom `EmitLimits` can be constructed and decoded on
    `yamlprims-value/1`): `uri` MUST be either a **local tag** starting with
    `'!'` (`uri.len() >= 2`, rejecting `"!"`) or a **global tag URI** starting
    with an RFC 3986 scheme `[A-Za-z][A-Za-z0-9+.-]*:` (permitting an empty
    scheme-specific part such as `"x:"` per RFC 3986 §3 `path-empty` while
    rejecting scheme-less verbatim strings such as `"$:?"` per YAML 1.2.2 §6.9.1
    Example 6.25), and MUST NOT be one of the seven Core Schema tag URIs
    (`tag:yaml.org,2002:{null,bool,int,float,str,seq,map}`), returning
    `InvalidOptions` (`Site::NotSource { last_known: None }`) on failure.
    `Tag::with_spelling` additionally requires `handle` to be `""`
    (`uri == suffix`), `"!"`, `"!!"`, or a named handle `!<ns-word-char>+!`
    (`[0-9A-Za-z-]+`), where any non-empty `handle` requires a non-empty
    `suffix` and a non-empty prefix
    (`uri.strip_suffix(&suffix).is_some_and(|prefix| !prefix.is_empty())`,
    supporting both default `!`/`!!` handles and `%TAG`-redefined `!`, `!!`, and
    named handles whose expanded `uri` is a valid local tag or global URI).
  - `Tagged::new(tag, value)` validates `tag` via the syntax rules above
    (`InvalidOptions`) and enforces §4.4 inner-kind compatibility at
    construction: `value.value()` MUST be `Value::String`, `Value::Sequence`, or
    `Value::Mapping` (and MUST be `Value::String` when
    `tag.uri() == "tag:yaml.org,2002:merge"`), returning `TagKindMismatch`
    (`Site::NotSource { last_known: None }`) on `Null`, `Bool`, `Int`, `Float`,
    nested `Tagged`, or `!!merge` collections.
  - During emission (`emit` / `emit_all`), every `Value::Tagged` is revalidated
    against those `Tag` syntax rules and `EmitLimits::max_tag_bytes`
    (`InvalidOptions`), rejects `tag:yaml.org,2002:merge` in mapping-key
    position or on a collection with `TagKindMismatch`, and rejects inner
    `Null`, `Bool`, `Int`, `Float`, or nested `Tagged` with `TagKindMismatch`.
  - Canonical tag rendering percent-encodes UTF-8 bytes outside YAML 1.2.2 §5.6
    `ns-tag-char \ {'%'}` (`[0-9A-Za-z#;/?:@&=+$_.~*'()-]`) for shorthand tags or
    outside `ns-uri-char \ {'%', '[', ']', '#'}`
    (`[0-9A-Za-z;/?:@&=+$_.~*'()-,!]`, matching RFC 3986 §3.3 `pchar / "/" / "?"`
    so URI path/query characters never contain unescaped `[`, `]`, or `#`) for
    verbatim global URIs as uppercase `%XX`:
    1. Local tag (`uri` starts with `'!'`, `uri.len() >= 2`): `'!'` +
       `percent_encode(&uri[1..], ns-tag-char \ {'%'})`.
    2. Secondary `tag:yaml.org,2002:` non-core tag (`&uri[18..]` non-empty):
       `"!!"` + `percent_encode(&uri[18..], ns-tag-char \ {'%'})`.
    3. Verbatim global URI (all other valid global `uri` values, preserving
       valid commas such as `!<tag:example.com,2026:v>`, permitting empty
       scheme-specific `!<x:>`, and encoding `Tag::try_new("x:[")` as
       `!<x:%5B>`): `"!<"` +
       `percent_encode(&uri, ns-uri-char \ {'%', '[', ']', '#'})` + `">"`.
  - `Document::new(root)` (`yaml_version: None`, `explicit_start: false`) and
    `Document::with_metadata(root, yaml_version, explicit_start)` validate that
    any `Some((major, minor))` satisfies `major == 1 && minor <= 999_999_999`
    (`UnsupportedYamlVersion` otherwise).
  - Host string validation across Go (`FromPlain`, `Marshal`, `Emit*`,
    `EncodeIJSON*`), TypeScript (`fromPlain`, `dump`, `dumpAll`, `loadIjson`),
    and Python (`from_plain`, `dump`, `dump_all`, `load_ijson`) rejects
    ill-formed UTF-8 bytes (`!utf8.ValidString` in Go) and unpaired UTF-16
    surrogates (`U+D800..=U+DFFF` in TypeScript and Python) with `InvalidUtf8`
    before `TextEncoder` replacement, `UnicodeEncodeError`, `max_scalar_bytes`,
    or `noncharacter` checks. Go `FromPlain`/`Marshal` sorts `map[K]V` keys
    deterministically by `bytes.Compare` of each key's `CompatQuoting::CoreOnly`
    emitted text.
  - Wire decoding (`decode_wire` / `DecodeWire` / `decodeWire`) enforces the
    `yamlprims-value/1` transport grammar and §3's structural `Tagged` and `Tag`
    invariants (`[u32, u32]` `yaml_version`, non-empty non-`"!"` non-Core
    `tag.uri` with consistent `handle`/`suffix` and `tag_bytes` accounted
    against `Limits::max_owned_bytes` and `Limits::max_key_bytes` without any
    fixed 4,096-byte cap, `tagged` inner kind in `{string, sequence, mapping}`
    with no `!!merge` collection, no `NaN`/duplicate mapping keys; loader height
    **H** on `documents[*].root` and error-key nesting metric **P** on
    `error.path[*].key`), while `emit*` revalidates `yaml_version`, `Tag`,
    `max_tag_bytes`, and `Tagged` invariants and `encode_ijson` rejects any
    `Value::Tagged` node per §4 Rule 5.
- **Symmetric `decode_wire` contract and host-emission bridge budget:**
  - Rust exposes `decode_wire(wire_json: &str, opts: &Options) -> Result<WireEnvelope, Error>`
    (`WireEnvelope::{Documents(Vec<Document>), Error(Error)}`) matching Go's
    `DecodeWire`, TypeScript's `decodeWire`, and Python's `decode_wire`.
  - Across all four decoders: `opts.validate()` runs first (`InvalidOptions`);
    exceeding `Limits` (`max_input_bytes` on `wire_json` UTF-8 byte length,
    `max_documents`, `max_depth` using loader height **H** on document roots and
    error-key metric **P** on `error.path[*].key` trees, `max_nodes`,
    `max_scalar_bytes`, `max_key_bytes`, `max_mapping_entries`, `max_owned_bytes`)
    returns `LimitExceeded` (`Site::NotSource { last_known: None }`); all
    malformed JSON, duplicate object keys, unknown/missing members, unsupported
    `format` or `version != 1`, non-canonical integer or metadata tokens,
    non-lowercase-hex16 float `bits`, `tagged` inner kinds outside
    `{string, sequence, mapping}` or `!!merge` collections, empty/non-specific/Core
    `tag.uri` or inconsistent `tag.handle`/`tag.suffix`, `NaN` or duplicate
    mapping keys, unknown `ErrorKind`/`SyntaxKind`/`LimitKind`/`RelatedRole`, or
    non-null locations on error `PathSegment::Key` nodes fail with `Internal`
    (`Site::NotSource { last_known: None }`). Standard RFC 8259 whitespace and
    valid string escapes are accepted on input; any `NaN` bit pattern normalizes
    to `0x7ff8000000000000`. Document- and error-envelope round-trip is
    byte-identical whenever `dec_opts.limits` accommodate the wire JSON length
    and the decoded document or `error.path` node/byte/entry budgets.
  - When TypeScript (`dump` / `dumpAll`) and Python (`dump` / `dump_all`) bridge
    host trees through `yamlprims-value/1` into native `decode_wire` +
    `emit_all_with` (Rust and Go emit directly from in-memory `Document`/`Node`
    trees without wire serialization), host serialization runs the pre-order
    structural/`EmitLimits` walk first, strips source-only metadata on the
    emission wire envelope (`span: null`, `via: null`, `tag_start: null`,
    `tag: { uri, handle: "", suffix: uri }`), verifies
    `sum(string_utf8_bytes) + sum(rendered_tag_min_bytes(tag.uri)) <= max_output_bytes`
    (`OutputLimitExceeded`, where `rendered_tag_min_bytes(uri)` is
    `2 + (uri.len() - 18)` for non-empty `tag:yaml.org,2002:*` URIs and
    `uri.len()` otherwise — a strict lower bound on YAML output bytes that never
    rejects valid secondary-tag inputs such as `!!foo ''\n`), and derives the
    internal bridge `decode_wire` `Limits` from `Limits::default()` with the
    following fields overridden from `emit_opts.limits` (while `max_tag_bytes`
    is enforced by `emit_all_with` after wire decoding):
    `max_depth = emit_limits.max_depth`,
    `max_nodes = emit_limits.max_nodes`, `max_scalar_bytes = emit_limits.max_scalar_bytes`,
    `max_key_bytes = emit_limits.max_key_bytes`,
    `max_mapping_entries = emit_limits.max_mapping_entries`,
    `max_documents = emit_limits.max_documents`,
    `wire_cap = saturating_add(65_536, saturating_add(saturating_mul(max_nodes, 512), saturating_mul(max_output_bytes, 18)))`,
    `owned_cap = saturating_add(65_536, saturating_add(saturating_mul(max_nodes, 128), saturating_mul(max_output_bytes, 4)))`.
    On 64-bit `PyO3` (`Python`), `max_input_bytes = wire_cap` and
    `max_owned_bytes = owned_cap` so wire framing never narrows `EmitLimits`. On
    32-bit `wasm32` (`TypeScript`), linear memory is bounded, so the supported
    WASM emission domain is `EmitLimits` intersected with a `64 MiB`
    (`67_108_864`-byte) canonical wire-envelope and `64 MiB` logical-owned-weight
    ceiling (`min(67_108_864, wire_cap)` and `min(67_108_864, owned_cap)`),
    mirroring `Limits::wasm_safe()` on load: inputs whose wire JSON and logical
    owned weight lie within that `64 MiB` WASM ceiling have byte-identical YAML
    output and diagnostic parity across all four bindings, while inputs
    exceeding the `64 MiB` WASM wire or owned ceiling fail deterministically
    with `LimitExceeded(MaxInputBytes, configured: 67_108_864)` or
    `LimitExceeded(MaxOwnedBytes, configured: 67_108_864)`
    (`Site::NotSource { last_known: None }`).

### 4. Strict RFC 7493 `encode_ijson` / `EncodeIJSON` contract

`encode_ijson` / `load_ijson*` (Rust), `EncodeIJSON*` / `LoadIJSON*` (Go),
`loadIjson` (TypeScript), `load_ijson` (Python), and `yamlprims dump --ijson`
(CLI) emit compact UTF-8 JSON (no insignificant whitespace; RFC 8785 §3.2.2.2
string escaping; object members in `Mapping` insertion order, leaving UTF-16 key
sorting to downstream JCS canonicalizers; `Node`-level `encode_ijson` /
`EncodeIJSON*` is exposed in Rust and Go, while stream-level `load_ijson*` /
`LoadIJSON*` / `loadIjson` is exposed across all four bindings so TypeScript and
Python avoid an extra `yamlprims-value/1` wire round-trip). Because JSON has a single `number`
type and no distinct `-0` token in RFC 8785 JCS, this is an intentional
**many-to-one projection from the YAML 1.2 data model onto the RFC 7493 / RFC
8785 JSON value domain** (`Value::Int(1)` and `Value::Float(1.0)` both project
to `"1"`; `Value::Float(-0.0)` and `Value::Float(0.0)` both project to `"0"`),
while enforcing six fail-closed normative rules so out-of-range `Value::Int`
values, non-finite floats, non-`String` mapping keys, and `Value::Tagged`
wrappers never silently round, stringify, or strip tags:

| # | Rule | Normative requirement | `ErrorKind` classification |
| --- | --- | --- | --- |
| 1 | **Single non-empty-stream document** | `load_ijson*` preserves §7.1 error precedence: Phases 1–4 (`InvalidOptions`, `max_input_bytes`, `InvalidUtf8`, and §7.1 bounded source syntax and `max_depth` validation across the whole input buffer) run before Phase 5 document-cardinality checks (`MultipleDocuments` at the start of any second document `Site::Source`, independent of `max_documents`, and empty-stream refusal at EOF `Site::Source` on zero-document streams such as `""`, comments/whitespace only, or bare `...` instead of synthesizing `null`). Thus `null\n---\n[\n` fails with `Syntax(unclosed_flow_collection)` and `null\n---\n[[[]]]\n` (`max_depth=2`) fails with `LimitExceeded(max_depth)`, while valid-syntax `null\n---\n1\n` fails with `MultipleDocuments`. Explicit `---\n` or `null`/`~` emits `"null"`; bare single documents (`a: 1`) are accepted. | `Type { expected: "single YAML document", found: "empty_stream" }` (`0` docs) / `MultipleDocuments` (`> 1` docs) |
| 2 | **Safe integers (`±(2^53 − 1)`)** | `Value::Int` MUST lie in `-9_007_199_254_740_991..=9_007_199_254_740_991`. `Value::Int` values outside `±(2^53 − 1)` (including `±2^53`, `i64::MIN`, `u64::MAX`, `2^64`) fail closed so distinct YAML integer literals never round-collide in `binary64`. | `IntegerOutOfRange` |
| 3 | **Finite `binary64` floats** | `Value::Float` MUST be finite. `.nan`, `.inf`, and `-.inf` fail closed. Finite `Value::Float` values (which are already IEEE 754 `binary64` floats in the YAML data model) format per RFC 8785 §3.2.2.3 (ECMAScript `Number::toString`, with `-0.0` formatted as `"0"`, `1.0` -> `"1"`, `1e20` -> `"100000000000000000000"`, `1e21` -> `"1e+21"`). | `FloatOutOfRange` |
| 4 | **String-only mapping keys** | Every mapping key MUST be `Value::String`. `Bool`, `Int`, `Float`, `Null`, `Sequence`, and `Mapping` keys fail closed without stringification (`Tagged` keys fail under Rule 5 `UnknownTag`, which takes precedence). | `Type { expected: "I-JSON string key", found: "non_string_key" }` |
| 5 | **Tags and merge keys** | `load_ijson*` enforces `UnknownTags::Reject` unconditionally (overriding `Options::default()`'s `UnknownTags::Preserve` so callers can pass modified `Options` structs without manually resetting `unknown_tags`), and `encode_ijson` rejects any `Value::Tagged` node (keys and values alike) as `UnknownTag` before key-kind checks. `MergeKeys::Reject` is the default; `MergeKeys::Apply` is opt-in via explicit `Options`; `MergeKeys::AsKey` (never a default) is rejected by `load_ijson*` with `InvalidOptions` so unquoted `<<` is never silently treated as a literal key across canonicalization boundaries. | `UnknownTag` / `MergeRejected` / `InvalidOptions` |
| 6 | **RFC 7493 Unicode noncharacters & output cap** | Validate well-formed UTF-8 first (`InvalidUtf8` on Go ill-formed UTF-8 strings or TypeScript/Python unpaired UTF-16 surrogates `U+D800..=U+DFFF` before `TextEncoder` replacement or `UnicodeEncodeError`), then reject all 66 Unicode noncharacters (`U+FDD0..=U+FDEF`, and `(cp & 0xFFFE) == 0xFFFE` across planes `0..=16`) in mapping keys and string values. `Options::ijson_default()` (Rust) and `DefaultIJSONOptions()` (Go) return `Options::default()` with `unknown_tags = UnknownTags::Reject` and `limits.max_output_bytes = 16 MiB` (`16_777_216`). Parameterless entrypoints (`load_ijson(input)` in Rust, `EncodeIJSON(node)` / `LoadIJSON(input)` in Go) and partial-option entrypoints when `max_output_bytes` is omitted (`loadIjson(input, opts?)` in TypeScript, `load_ijson(input, **opts)` in Python, and CLI `dump --ijson` without `--max-output-bytes`) use `Options::ijson_default()` (`16 MiB` default output cap), whereas explicit `&Options` / `*Options` structs passed to `encode_ijson` / `load_ijson_with` / `load_ijson_bytes` (Rust) or `EncodeIJSONWith` / `LoadIJSONWith` / `LoadIJSONBytes` (Go) use `opts.limits.max_output_bytes` directly as supplied (constructing modified `Options` from `Options::ijson_default()` / `DefaultIJSONOptions()` preserves the `16 MiB` cap unless overridden). | `Type { expected: "RFC 7493 I-JSON string", found: "noncharacter" }` / `InvalidUtf8` / `OutputLimitExceeded` / `LimitExceeded` |

**Traversal, normative `Type` strings, and `Site` parity:** The `Type.expected`
and `Type.found` strings in Rules 1, 4, and 6 above are normative across all
four bindings. Projection walks the tree in iterative pre-order DFS (`Sequence`
items `0..len`; `Mapping` entries `0..len`, inspecting each entry's `key` before
its `value`) using a fresh per-call node counter (`1..=opts.limits.max_nodes`).
Each visited `Node` charges `max_nodes` first, then checks `Value::Tagged`
(`UnknownTag`), then (for keys) non-`String` kind (`non_string_key`), then
scalar/collection limits (`max_scalar_bytes`, `max_key_bytes`, `max_depth`,
`max_mapping_entries`) and value validity (`noncharacter`, `IntegerOutOfRange`,
`FloatOutOfRange`), using `Site::Source(span.start)` when the offending `Node`
carries a source span (with `position_span()` for alias-expanded nodes) and
`Site::NotSource { last_known: None }` on synthetic nodes, and checking
`max_output_bytes` incrementally as tokens are written (`OutputLimitExceeded`
with `Site::NotSource { last_known: None }`).

## Consequences

- **Positive:** Completes symmetric read/write YAML 1.2.2 Core support and
  fail-closed RFC 7493 I-JSON projection across all four `Profile B` bindings
  with zero new runtime dependencies, closing caller-tree stack-safety and
  YAML 1.1 consumer-quoting footguns by default.
- **Trade-offs:** Callers requiring CST comment retention or custom anchor/alias
  graph emission must use a syntax-tree library rather than `yamlprims`. Callers
  migrating from `encode_plain` who want `i128` integers or `-0.0` preserved as
  `-0.0` continue using `encode_plain` (unchanged), while `encode_ijson`
  enforces strict RFC 7493 / RFC 8785 interoperability.
