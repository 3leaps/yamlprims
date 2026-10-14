# Contributing

This first release is specification-first. Useful contributions include concrete
semantic examples, API questions, independent expected results and improvements
to the build plan.

Start with the [specification](docs/spec/yamlprims-spec.md) and
[open design items](docs/design-review.md). Cite the affected section and explain
the expected behavior. Proposed changes need maintainer review; publishing a
draft does not freeze every interface.

Use public synthetic inputs or redistributable upstream material with provenance.
Do not submit private configuration, secrets or redacted production examples.
Follow the [Sensitive Local Data Policy](https://github.com/3leaps/oss-policies/blob/main/SENSITIVE-LOCAL-DATA.md).

Run `make check` before submitting. This checks documentation integrity only.
Implementation contributions begin after the qualification gate described in
the [build specification](docs/build-spec.md).

Code and general documentation contributions use MIT OR Apache-2.0; normative
specification text under `docs/spec/` uses CC0-1.0. Preserve upstream notices.
AI-assisted work is welcome under human supervision and the attribution rules
in [AGENTS.md](AGENTS.md).
