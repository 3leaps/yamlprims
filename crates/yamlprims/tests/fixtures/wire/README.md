# Wire regression fixtures

Each JSON file fixes one output exercised by `tests/wire.rs`: values, merge
policy, duplicate keys, invalid UTF-8, typed errors, synthetic sites, output
limits or plain projection. The YAML inputs and expected bytes are original
synthetic project material under MIT OR Apache-2.0.

These snapshots were initially produced by an implementation and reviewed
against the wire contract. They are regression evidence, not an independently
derived oracle for every error message. Ordinary loader message prose is not
a normative wire guarantee. Fixed-message operation tests cover encoder rules.

Golden blessing is disabled. Expectation changes require independent review.
The invalid-UTF-8 input must retain its exact binary bytes across checkouts.
