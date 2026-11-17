// Package yaml implements the YAML 1.2.2 Core Schema loader, reader/writer
// stream helpers, deterministic Core emitter, strict RFC 7493 I-JSON projection,
// YAML 1.1 migration lint (yamlprims-lint/1), value model, resource budgets,
// wire format (yamlprims-value/1), plain JSON projection, and reflection-based
// Unmarshal/Marshal helpers for the yamlprims specification (spec-v1.6).
//
// All library calls perform no filesystem access, environment interpolation,
// network requests, or process-global configuration, and are safe to call
// concurrently when inputs and options are not mutated concurrently.
package yaml
