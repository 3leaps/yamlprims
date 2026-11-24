# Migrating from `libyaml`-backed YAML libraries to `yamlprims`

`yamlprims` provides a memory-safe, resource-bounded **YAML 1.2.2 Core Schema**
data-layer reader, emitter, and migration linter across Rust (`yamlprims`),
pure Go (`github.com/3leaps/yamlprims/yaml`), Python (`yamlprims`), TypeScript
(`@3leaps/yamlprims`), and the CLI (`yamlprims`).

It is designed for applications, configuration loaders, and data pipelines
replacing `libyaml`-lineage or YAML 1.1 libraries (`PyYAML`, `serde_yaml` /
`unsafe-libyaml`, `gopkg.in/yaml.v2` & `yaml.v3`, `js-yaml`). It is **not** a
C-ABI drop-in for `libyaml` and does not perform comment- or formatting-preserving
concrete syntax tree (CST) editing (see
[ADR-0005](decisions/ADR-0005-data-layer-emitter-and-ijson-projection.md)). For
measured behavior against other YAML loaders on hostile and YAML 1.1
inputs, see [benchmarks.md](benchmarks.md).

---

## 1. Behavioral differences (`libyaml` / YAML 1.1 vs `yamlprims` YAML 1.2.2 Core)

| Area | `libyaml` / YAML 1.1 libraries | `yamlprims` ([specification](spec/yamlprims-spec.md)) | Migration tool / option |
| --- | --- | --- | --- |
| **Boolean words** | Unquoted `y`, `Y`, `n`, `N`, `yes`, `no`, `on`, `off` resolve as booleans (`true` / `false`). | Only `true`/`True`/`TRUE` and `false`/`False`/`FALSE` resolve as `Bool` (§4.1). `yes`, `no`, `on`, `off`, `y`, `n` resolve as `String`. | `yamlprims check --lint-yaml11` flags unquoted YAML 1.1 booleans (`yaml11_boolean`); `emit` / `safe_dump` quotes them by default (`CompatQuoting::Yaml11Safe`). |
| **Octal & underscored numbers** | Leading-zero `010` / `0755` is octal (`8` / `493`); `0b1010` is binary; `1_000` is integer `1000`. | Only `0o10` / `0o755` is octal (§4.1). Plain `010` / `0755` is decimal `10` / `755`; `0b1010` and `1_000` are `String`. | `yamlprims check --lint-yaml11` flags legacy octals, binary integers, signed hex, and underscored numbers (`yaml11_number`). |
| **Sexagesimal (`base 60`)** | Unquoted `1:30` or `10:20:30` resolves to integer `90` or `37230`; `10:20.5` resolves to float `620.5`. | Resolved as `String` (`"1:30"`, `"10:20:30"`, `"10:20.5"`). | `yamlprims check --lint-yaml11` flags unquoted sexagesimal tokens (`yaml11_sexagesimal`). |
| **Merge keys (`<<`)** | Often applied implicitly or inconsistently across APIs. | Rejected by default (`MergeKeys::Reject`); callers opt in explicitly via `MergeKeys::Apply` or `MergeKeys::AsKey` ([ADR-0001](decisions/ADR-0001-explicit-merge-policy.md)). | Pass `MergeKeys::Apply` / `merge_keys="apply"` / `mergeKeys: "apply"`, or inspect with `--lint-yaml11` (`yaml11_merge_key`). |
| **Duplicate mapping keys** | Commonly last-wins (silently overwriting earlier entries). | Always rejected (`ErrorKind::DuplicateKey`) with both the duplicate site and `RelatedRole::FirstDefinition` span (§5.2). | Fix duplicate keys in source YAML; surfaced immediately by `yamlprims check`. |
| **Quoted scalar coercion** | Some typed unmarshalers coerce `"8080"` → `8080` or `"true"` → `true`. | Quoted scalars are always `String` (§4.1, §8); typed deserialization never coerces strings into numbers or booleans. | Remove quotes around numeric/boolean values or change the target field type to `String`. |
| **Unknown / custom tags** | Frequently stripped or ignored during typed unmarshaling. | Retained on `Node` trees (`UnknownTags::Preserve`) and rejected by typed / plain / I-JSON projections unless handled explicitly (§8, §11.3, §11.6). | Use lossless `load` / `Load` when inspecting custom tags, or strip/migrate tags before plain projection. |
| **Stream & alias budgets** | Readers and alias expansion are typically unbounded by default. | Enforced before materialization (`max_input_bytes: 8 MiB`, `max_depth: 128`, `max_nodes: 250,000`, `max_expanded_nodes: 100,000`, `max_owned_bytes: 64 MiB`, etc.; §6). They are size and memory budgets, not a time limit. | Customize `Options.limits` / `EmitOptions.limits` when legitimate inputs exceed default budgets. For untrusted input, also apply a time limit of your own (for example, load in a worker process with a deadline). |

---

## 2. Side-by-side language migration (`v0.3.0+`)

### Python (`PyYAML` → `yamlprims` `v0.3.0+`)

`yamlprims.safe_load`, `safe_load_all`, `safe_dump`, and `safe_dump_all` (`v0.3.0+`) accept
`str`, `bytes`, `bytearray`, `memoryview`, or any file-like stream with `.read()`
/ `.write()` and materialize native Python `dict`, `list`, `str`, `int` (up to
128-bit), `float`, `bool`, and `None` in a single `PyO3` pass.

```python
# Before (PyYAML)
import yaml

with open("config.yaml", "rb") as f:
    data = yaml.safe_load(f)
text = yaml.safe_dump(data, sort_keys=False)

# After (yamlprims)
import yamlprims

with open("config.yaml", "rb") as f:
    data = yamlprims.safe_load(f)
text = yamlprims.safe_dump(data)
```

- **Multi-document streams:** replace `list(yaml.safe_load_all(stream))` with
  `yamlprims.safe_load_all(stream)` and `yaml.safe_dump_all(docs)` with
  `yamlprims.safe_dump_all(docs)`.
- **Merge keys:** pass `yamlprims.safe_load(stream, merge_keys="apply")` when
  loading files that use `<<` merge directives.
- **Lossless tree & spans:** use `yamlprims.load(stream)` / `load_all(stream)`
  when you need source `Span` coordinates, non-string mapping keys (`Mapping`),
  or preserved custom `Tag` wrappers.

### Rust (`serde_yaml` → `yamlprims`)

`yamlprims` provides `from_str`, `from_slice`, `from_reader`, `to_string`, and
`to_writer` over `serde::de::DeserializeOwned` and `serde::Serialize`, enforcing
`Options::default()` and `EmitOptions::default()` bounds before materializing or
writing.

```rust
// Before (serde_yaml)
let cfg: Config = serde_yaml::from_slice(&bytes)?;
let cfg_from_file: Config = serde_yaml::from_reader(file)?;
let out: String = serde_yaml::to_string(&cfg)?;
serde_yaml::to_writer(&mut writer, &cfg)?;

// After (yamlprims)
let cfg: Config = yamlprims::from_slice(&bytes)?;
let cfg_from_file: Config = yamlprims::from_reader(file)?;
let out: String = yamlprims::to_string(&cfg)?;
yamlprims::to_writer(&mut writer, &cfg)?;
```

- **Custom limits or merge policy:** use `from_str_with`, `from_slice_with`,
  `from_reader_with`, `to_string_with`, and `to_writer_with`.
- **Struct field hygiene:** add `#[serde(deny_unknown_fields)]` to Rust target
  structs where unknown YAML keys should be rejected (`ErrorKind::UnknownField`).

### Go (`gopkg.in/yaml.v3` → `github.com/3leaps/yamlprims/yaml`)

The pure-Go twin (`CGO_ENABLED=0`, standard library only) provides `Unmarshal`,
`UnmarshalReader`, `Marshal`, and `MarshalWriter`. Struct decoding rejects
unknown fields by default (`ErrorKindUnknownField`, matching `yaml.v3`
`KnownFields(true)`).

```go
// Before (gopkg.in/yaml.v3)
var cfg Config
if err := yaml.Unmarshal(rawBytes, &cfg); err != nil {
    return err
}
out, err := yaml.Marshal(&cfg)

// After (github.com/3leaps/yamlprims/yaml)
var cfg Config
if err := yaml.Unmarshal(rawBytes, &cfg); err != nil {
    return err
}
out, err := yaml.Marshal(&cfg)
```

- **`io.Reader` / `io.Writer` streams:** replace `yaml.NewDecoder(r).Decode(&cfg)`
  with `yaml.UnmarshalReader(r, &cfg)` (bounded by `Limits.MaxInputBytes + 1`
  via `io.LimitReader`) and `yaml.NewEncoder(w).Encode(&cfg)` with
  `yaml.MarshalWriter(w, &cfg)` (buffered until `EmitLimits` validation succeeds
  so failed emissions write zero bytes).
- **Struct tags:** `yaml:"name,omitempty"` and `yaml:"-"` are honored on struct
  fields.

### TypeScript (`js-yaml` → `@3leaps/yamlprims`)

`@3leaps/yamlprims` runs in a zero-import WebAssembly sandbox and exposes
`parse`, `parseAll`, `stringify`, and `stringifyAll` alongside the lossless
`Document` / `Node` API (`load`, `loadAll`, `dump`, `dumpAll`).

```ts
// Before (js-yaml)
import yaml from "js-yaml";

const data = yaml.load(sourceText);
const out = yaml.dump(data);

// After (@3leaps/yamlprims)
import { parse, stringify } from "@3leaps/yamlprims";

const data = parse(sourceText);
const out = stringify(data);
```

- **64-bit / 128-bit integers:** `parse` returns JavaScript `number` for integers
  within `Number.MIN_SAFE_INTEGER..=Number.MAX_SAFE_INTEGER` (`±(2^53 - 1)`) and
  `bigint` for integers outside the safe IEEE 754 range (`i128::MIN..=u128::MAX`).

---

## 3. Auditing and normalizing existing YAML files (`--lint-yaml11` and `emit`, `v0.3.0+`)

Before switching a repository or configuration fleet from a YAML 1.1 parser to
`yamlprims`, run `yamlprims check --lint-yaml11` (`v0.3.0+`) to inspect files
against the four closed `LintCode` hazard categories (`yaml11_boolean`,
`yaml11_number`, `yaml11_sexagesimal`, `yaml11_merge_key`):

- **What `--lint-yaml11` covers:** unquoted YAML 1.1 boolean words (`y`, `Y`, `n`,
  `N`, `yes`, `no`, `on`, `off`), legacy leading-zero octals (`010`, `0755`),
  `0b` binary integers, signed `±0x` hex integers, underscore-grouped numbers
  (`1_000`, `1_000.5`), sexagesimal numbers (`1:30`, `10:20:30`, `10:20.5`), and
  unquoted `<<` merge keys.
- **What `--lint-yaml11` does not cover:** YAML 1.1 timestamps such as
  `2026-11-09` or ISO-8601 date-times (which `PyYAML` resolves to
  `datetime.date` / `datetime.datetime` and YAML 1.2 Core resolves to `String`;
  note that `yamlprims emit` / `safe_dump` quotes `YYYY-MM-DD` strings under
  `CompatQuoting::Yaml11Safe`), `~` / `null` spellings (which resolve to `Null`
  in both), and non-YAML-1.1 comma-grouped or multi-dot tokens (`1,000`,
  `1.2.3`).
- **Warning retention bound (`max_warnings`):** `lint_yaml11*` retains up to
  `Limits.max_warnings` (`1,000` by default) diagnostics per call; when a stream
  exceeds `max_warnings`, `truncated` is set to `true` while `warning_count`
  continues counting all matched hazards.

```sh
$ cat legacy.yaml
enabled: yes
mode: 0755
timeout: 1:30
retries: 1_000

$ yamlprims check --lint-yaml11 legacy.yaml
legacy.yaml:1:10: warning[yaml11_boolean]: YAML 1.1 compatibility hazard (yaml11_boolean): unquoted "yes" resolves as string ("yes") in YAML 1.2 Core (YAML 1.1 boolean literal (true))
legacy.yaml:2:7: warning[yaml11_number]: YAML 1.1 compatibility hazard (yaml11_number): unquoted "0755" resolves as int (755) in YAML 1.2 Core (YAML 1.1 base-8 (octal) integer literal; use 0o prefix for octal in YAML 1.2 or quote as string)
legacy.yaml:3:10: warning[yaml11_sexagesimal]: YAML 1.1 compatibility hazard (yaml11_sexagesimal): unquoted "1:30" resolves as string ("1:30") in YAML 1.2 Core (YAML 1.1 sexagesimal integer literal)
legacy.yaml:4:10: warning[yaml11_number]: YAML 1.1 compatibility hazard (yaml11_number): unquoted "1_000" resolves as string ("1_000") in YAML 1.2 Core (YAML 1.1 underscore-grouped numeric literal)
# exits 1 when any warning is present
```

For machine-readable CI pipelines, pass `--format=json` on a single input file
to emit a canonical [`yamlprims-lint/1`](spec/yamlprims-spec.md#117-yaml-11-migration-lint-wire-format-yamlprims-lint1)
JSON line on `stdout`:

```sh
$ yamlprims check --lint-yaml11 --format=json legacy.yaml
```

Once values are updated to explicit YAML 1.2 Core tokens (`true`, `0o755`, `90`,
`1000`, or quoted strings where strings are intended), `yamlprims emit` formats
documents deterministically with `CompatQuoting::Yaml11Safe` by default so any
remaining string values such as `'yes'`, `'0755'`, or `'1:30'` stay single-quoted
and unambiguous across both YAML 1.1 and YAML 1.2 tools:

```sh
$ yamlprims emit legacy.yaml
enabled: 'yes'
mode: 755
timeout: '1:30'
retries: '1_000'
```
