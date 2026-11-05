// Package yaml implements the YAML 1.2.2 Core Schema loader, value model,
// resource budgets, wire format (yamlprims-value/1), plain JSON projection,
// and typed conversion helpers for the yamlprims specification (spec-v1.4).
//
// All library calls perform no filesystem access, environment interpolation,
// network requests, or process-global configuration, and are safe to call
// concurrently when inputs and options are not mutated concurrently.
package yaml
