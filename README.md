# yamlprims

**A specified YAML data layer for Rust and language bindings. Built in public,
starting with the contract.**

The first release contains the [draft specification](docs/spec/yamlprims-spec.md),
[build specification](docs/build-spec.md) and [decision records](docs/decisions/README.md).
There is no installable parser library, CLI or binding in this release.

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

Current tagged specification: [**spec-v1.1**](https://github.com/3leaps/yamlprims/tree/spec-v1.1),
a draft contract covering span, empty-stream, error-ownership and strict-indentation
changes. The earlier [spec-v1.0](https://github.com/3leaps/yamlprims/tree/spec-v1.0)
baseline remains available. These tags do not imply a crate release or API freeze.
Existing corpus profiles remain bound to their cited specification revision.
GitHub release publication is separate from specification tagging; no GitHub
release has been published as of October 14, 2026.
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
```

## License

Code and general documentation: [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE). The normative specification under `docs/spec/` is
[CC0-1.0](docs/spec/LICENSE). Upstream material retains its own notices.
