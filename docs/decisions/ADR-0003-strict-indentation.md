---
id: ADR-0003
title: Strict indentation for YAML 1.2.2 conformance
status: accepted
date: 2026-10-14
---

# ADR-0003: Strict indentation for YAML 1.2.2 conformance

Accepted for specification revision 1.1 on 2026-10-14. Implementation
qualification remains outstanding.

## Decision

Revision 1.1 requires `strict_indentation = true` for the pinned event engine in
every parsing pass, including any syntax-only pre-pass. This is a fixed profile
setting, not a new public Options toggle. The owned resolver, builder and crate
boundaries remain unchanged.

The initial public API is intentionally **strict-only**, not merely strict by
default. A candidate's `Compatible` (lenient) mode is outside this public
contract. Acceptance of that mode for migration testing does not require its
exposure in production. Keep lenient operation in qualification and migration
tooling; a public compatibility option requires an established consumer need
and a separate contract decision.

This corrects flow-indentation conformance with
[YAML 1.2.2 §7.4](https://yaml.org/spec/1.2.2/#74-flow-collection-styles).
The detecting case is yaml-test-suite
[9C9N, “Wrong indented flow sequence”](https://github.com/yaml/yaml-test-suite/blob/v2022-01-17/src/9C9N.yaml),
marked `fail: true` upstream. The engine setting implements the format rule;
invalid indentation is not merely a yamlprims-specific policy rejection.

## Contract and compatibility

Revision 1.0 specified `false`. Under-indented flow entries such as
`---\nflow: [a,\nb,\nc]\n` must be syntax errors in revision 1.1. Both settings remain
qualification configurations, but only the strict setting defines the new target
profile. A lenient implementation cannot claim conformance by reclassifying a
malformed case as an expected success.

The new corpus profile MUST explicitly pin strict indentation in every parsing
pass. Historical lenient results remain separately identified and cannot satisfy
the strict profile's qualification gate.

Corpus expectations retain this as a format-level syntax failure. Record
revision 1.0's acceptance as a leniency difference against that expectation;
preserving a historical profile does not change YAML validity.

The engine already exposes this setting; no adapter lexer is required. Consumers
whose inputs rely on the earlier leniency need explicit migration results before
the new profile is frozen. Keep the published revision 1.0 tag and expectations.

## Qualification requirements

Run both configurations against the same pinned yaml-test-suite inventory and
migration fixtures. Verify every ID exactly once, compare error categories and
values independently, and retain duplicate-key policy differences separately.
Report every suite ID whose verdict changes between the two settings, alongside
consumer migration results.
Include a control that substitutes `InvalidOptions` for the expected syntax
error and require the qualification driver to fail it. Run the scalar truth,
typed, wire and source-location suites against the selected strict configuration.
Acceptance of the strict-only API decision does not establish candidate
conformance with the additional revision 1.1 requirements; that needs the pinned
qualification evidence.
