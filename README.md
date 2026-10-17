# yamlprims

**A specified YAML data layer for Rust and language bindings. Built in public,
starting with the contract.**

The repository contains the [specification](docs/spec/yamlprims-spec.md),
[build specification](docs/build-spec.md), [decision records](docs/decisions/README.md)
and a working Rust core with an initial [CLI](crates/yamlprims-cli/README.md). No library package, CLI or binding has been
released. See [implementation status](docs/implementation.md) for current coverage
and remaining work.

## What we are building

- Owned values with exact integers, arbitrary mapping keys and retained non-core tags.
- YAML 1.2 Core resolution, duplicate-key errors and explicit merge policy.
- Source locations and structured errors shared by generic and typed Rust APIs.
- Finite resource budgets, including alias and merge expansion accounting.
- One Rust implementation, a thin CLI, then Go and TypeScript bindings.

The initial design uses `granit-parser` as its internal event source. yamlprims
owns the value, policy and error contract; it does not expose backend types or
promise compatibility with every existing YAML library. It performs no includes,
environment interpolation or network access.

## Status and participation

Current tagged specification: [**spec-v1.3**](https://github.com/3leaps/yamlprims/tree/spec-v1.3),
a draft contract covering nested
empty-scalar anchors, zero-width collection anchors, UTF-8 input scope,
binary64-to-f32 conversion and exact keep-chomping EOF examples.
Earlier specification tags remain immutable. The normative file retains its
tagged bytes, including its historical draft-status wording. These tags do not
imply a crate release or API freeze.
Existing corpus profiles remain bound to their cited specification revision.
GitHub release publication is separate from specification tagging.
See [the roadmap and acceptance gates](docs/build-spec.md#delivery-gates) and
[open design items](docs/design-review.md).
The [release procedure](docs/releases.md) covers signed specification tags and
package-publication timing.

Feedback on examples, semantic boundaries and acceptance criteria is welcome.
Please state the specification section and expected behavior when proposing a
change. We will qualify the implementation against pinned, independently reviewed
test material, including [structcorpus](https://github.com/3leaps/structcorpus).

```sh
make check    # checks the specification release; no Rust toolchain required
make test-rust  # runs the included core, truth, wire and stack regressions
```

## License

Code and general documentation: [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE). The normative specification under `docs/spec/` is
[CC0-1.0](docs/spec/LICENSE). Upstream material retains its own notices.
