# yamlprims

Rust implementation under development: owned YAML values, exact integers,
arbitrary mapping keys, explicit merge policy, source locations, typed Serde
conversion and bounded wire encoding.

```rust
let document = yamlprims::load("port: 8080\n")?;
let port = document.root().value().as_mapping().unwrap().get("port").unwrap();
assert_eq!(port.value().as_integer().unwrap().to_u64(), Some(8080));
# Ok::<(), yamlprims::Error>(())
```

Parsing uses UTF-8 and strict indentation. Merge directives are rejected by
default. The library performs no I/O or environment expansion. Its engine is
internal and pinned to granit-parser 2.0.0.

The API is not frozen or published. Remaining work includes depth and synthetic
tree guarantees, checked construction, error vocabulary, wire decoding and
production platform/resource qualification. Passing included regressions does
not establish complete conformance to the
[specification](https://github.com/3leaps/yamlprims/blob/spec-v1.3/docs/spec/yamlprims-spec.md).

Run `cargo test --locked -- --test-threads=1`. Synthetic truth and wire fixtures
are included in this package; they need no external checkout. Golden blessing
is disabled. Allocation instrumentation belongs to integration test binaries;
the library forbids unsafe code.

Licensed under MIT OR Apache-2.0. See the included license texts.
