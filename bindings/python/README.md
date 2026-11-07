# yamlprims

YAML 1.2.2 Core Schema value loader, strict `yamlprims-value/1` wire decoder, and canonical encoder for Python (CPython 3.10+ via `abi3` extension over the Rust `yamlprims` crate).

## Features

- **Lossless YAML 1.2.2 Core Schema `Document` / `Node` / `Mapping` tree** (`load`, `load_all`, `load_bytes`, `load_all_bytes`) with arbitrary-precision Python `int` for 128-bit integers (`i128::MIN..=u128::MAX`), exact IEEE 754 binary64 float `bits` (`-0.0`, `NaN`, `±Infinity`), `Tag` / `Tagged` preservation, and `Span` / `Location` coordinates (`byte_offset`, `line`, `column`).
- **Ordered `Mapping` without Python `dict` key collisions**: YAML `true`, `1`, and `1.0` remain distinct keys via `yamlprims.key_eq` rather than colliding as in Python's built-in `dict`.
- **Explicit `to_plain()` and `encode_plain()` loss matrix** (`spec-v1.4 §11.3`): `to_plain` projects string-keyed mappings to `dict` and rejects non-`string` mapping keys or `Tagged` nodes with `YamlprimsError(kind="type")`.
- **Strict `yamlprims-value/1` wire decoder & canonical encoder** (`decode_wire`, `encode`, `encode_error`) with byte-for-byte round-trip parity (`spec-v1.4 §11.4`).

## License

Dual-licensed under `MIT OR Apache-2.0`. See `LICENSE-MIT`, `LICENSE-APACHE`, and `NOTICE`.
