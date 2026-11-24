# Security

## Supported versions

`yamlprims` is pre-1.0. Security fixes ship as a patch release of the latest
minor release line only; earlier minor lines are not patched. Upgrade to the
latest release to receive fixes.

This policy covers every package built from this repository:

- the Rust crate `yamlprims` (crates.io);
- the `yamlprims` CLI (release archives, Homebrew and Scoop);
- the pure-Go module `github.com/3leaps/yamlprims`;
- the Python package `yamlprims` (PyPI);
- the TypeScript package `@3leaps/yamlprims` (npm).

## Reporting a vulnerability

Report suspected vulnerabilities privately to
[security@3leaps.net](mailto:security@3leaps.net), not in a public issue. Include
the affected package and version, the entrypoint and options used, and a minimal
synthetic reproduction when you have one. Do not send credentials or private
configuration.

We aim to acknowledge a report within five business days and to agree on a fix
and disclosure timeline with you. Please keep the details private until a fixed
release is published.

Examples of what to report privately:

- a crash, panic or abort on any input;
- a load, lint, emit or encode call that does not finish in time proportional to
  its input, or that uses memory beyond its configured limits;
- input that the Rust core and the Go module accept differently in a way that
  matters for security;
- a way around release signature or checksum verification.

General questions about the specification can be discussed in public issues using
synthetic examples.

## Advisories

When a fix is released, we publish a GitHub security advisory for each affected
package ecosystem. Go advisories reach the Go vulnerability database and
`govulncheck`. The release notes describe the fix without reproduction inputs.

## Resource limits

Every load, lint, emit and encode entrypoint enforces finite limits on input
size, nesting depth, node counts, alias expansion and output size
([specification §6](docs/spec/yamlprims-spec.md)). These are size and memory
budgets, not a time limit. When you parse untrusted input, also apply a deadline
of your own, for example by parsing in a worker process.

See also [the resource and native-boundary decision](docs/decisions/SecDR-0001-resource-and-native-boundaries.md)
and [release signing and verification](docs/security/README.md).
