# yamlprims

YAML 1.2.2 Core Schema value loader, strict `yamlprims-value/1` wire decoder, and canonical encoder for Python (CPython 3.10+ via `abi3` extension over the Rust `yamlprims` crate).

## Features

- **Lossless YAML 1.2.2 Core Schema `Document` / `Node` / `Mapping` tree** (`load`, `load_all`, `load_bytes`, `load_all_bytes`) with arbitrary-precision Python `int` for 128-bit integers (`i128::MIN..=u128::MAX`), exact IEEE 754 binary64 float `bits` (`-0.0`, `NaN`, `±Infinity`), `Tag` / `Tagged` preservation, and `Span` / `Location` coordinates (`byte_offset`, `line`, `column`).
- **Single-pass native `safe_load` / `safe_load_all` and `safe_dump` / `safe_dump_all` (`v0.3.0+`)** (`spec-v1.6 §11.2`): ergonomic replacements for `yaml.safe_load` and `yaml.safe_dump` over `str`, `bytes`, and file-like `.read()` / `.write()` streams, materializing Python `dict`, `list`, `str`, `int`, `float`, `bool`, and `None` directly in `PyO3` under bounded resource limits.
- **Ordered `Mapping` without Python `dict` key collisions**: YAML `true`, `1`, and `1.0` remain distinct keys via `yamlprims.key_eq` rather than colliding as in Python's built-in `dict`.
- **Explicit `to_plain()` and `encode_plain()` loss matrix** (`spec-v1.6 §11.3`): `to_plain` projects string-keyed mappings to `dict` and rejects non-`string` mapping keys or `Tagged` nodes with `YamlprimsError(kind="type")`.
- **YAML 1.1 → YAML 1.2.2 Core migration lint (`v0.3.0+`)** (`lint_yaml11`, `encode_lint`, `LintResult`, `LintDiagnostic`, `LINT_CODES`) reporting unquoted YAML 1.1 hazards (`yaml11_boolean`, `yaml11_number`, `yaml11_sexagesimal`, `yaml11_merge_key`) with source spans (`spec-v1.6 §10.3`, `§11.7`).
- **Strict `yamlprims-value/1` wire decoder & canonical encoder** (`decode_wire`, `encode`, `encode_error`) with byte-for-byte round-trip parity (`spec-v1.6 §11.4`).
- **Deterministic YAML 1.2.2 Core emitter & strict RFC 7493 I-JSON projection** (`dump`, `dump_all`, `safe_dump`, `safe_dump_all`, `from_plain`, `load_ijson`, `EmitLimits`, `EmitOptions`) with `yaml11_safe` default compatibility quoting (`spec-v1.6 §10.2`, `§11.6`).

## Example

```python
import yamlprims

value = yamlprims.safe_load("name: demo\nenabled: on\nports: [80, 443]\n")  # v0.3.0+
# {'name': 'demo', 'enabled': 'on', 'ports': [80, 443]}
print(yamlprims.safe_dump(value), end="")  # v0.3.0+ yaml11_safe quoting: enabled: 'on'
yamlprims.load_ijson("b: 2\na: [1.0, 1e21]\n")  # '{"b":2,"a":[1,1e+21]}'
```

`load_plain` and `load_ijson` return JSON text. `load_ijson` output is RFC 7493
I-JSON, ready for RFC 8785 JCS canonicalization (for example with
[canonprims](https://github.com/3leaps/canonprims)).

## License

Dual-licensed under `MIT OR Apache-2.0`. See `LICENSE-MIT`, `LICENSE-APACHE`, and `NOTICE`.
