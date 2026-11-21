# Fuzzing

Fuzz targets for both cores, plus a Rust/Go differential. Nothing in this
directory is published. It is not a member of the Cargo workspace, so the
published crates never depend on `libfuzzer-sys`.

## Targets

| Rust (`fuzz/fuzz_targets/`) | Go (`yaml/fuzz_test.go`) | Specification properties |
| --- | --- | --- |
| `load` | `FuzzLoad` | Error kinds are in the closed vocabulary (§7.1); single- and multi-document loading agree (§5.5). |
| `decode_wire` | `FuzzDecodeWire` | An accepted envelope re-encodes, and that encoder output round-trips byte-identically (§11.1). |
| `emit_round_trip` | `FuzzEmitRoundTrip` | Documents accepted by the default emitter reload to equal values and `%YAML` versions (§10.2). |
| `lint_yaml11` | `FuzzLintYAML11` | Spans are 1-based and in bounds; `encode_lint` accepts the result (§7.2, §11.7). |
| `encode` | `FuzzEncode` | Wire and plain output is JSON; wire output round-trips (§11.1). |

Separately from those properties, an implementation-quality check reports an
`Internal` error from an in-memory load, emit, lint or plain-encoding call as
failure kind `quality`. The specification allows `Internal` (for example
when the engine cannot supply a byte offset, §7.2), so this is a prompt for
review, not a conformance failure.

The `load`, `emit_round_trip` and `encode` targets use the first input byte to
pick the options: default, I-JSON default, `MergeKeys::AsKey` or
`MergeKeys::Apply`. Seeds come from `conformance/vectors/cases.jsonl` and the
wire fixtures in `crates/yamlprims/tests/fixtures/wire/`.

## Running

```sh
make fuzz-replay         # every target over the committed seeds, no mutation, stable toolchains
make fuzz-differential   # Rust/Go differential over the committed seeds
make fuzz-smoke          # mutating run of the Rust targets, then the differential
```

`fuzz-replay` is the only fuzzing step meant for public CI. In Go it runs the
`Fuzz*` seed corpora as ordinary tests (`make test-go` skips them so that
their failure text stays out of its output). In Rust it runs
`fuzz/tests/replay.rs` on stable, which calls the same `targets::fuzz_*`
functions as the libFuzzer binaries. Files under `fuzz/regressions/<target>/`
are replayed too; an input is added there only after the fix it covers is
released.

`fuzz-smoke` mutates, so it can find new inputs. Run it locally or on a private
host only, never in public CI. It covers the Rust targets: libFuzzer enforces
`-timeout` (10 s per input), `-rss_limit_mb` and `-malloc_limit_mb` (2048) and
`-max_len` (65536). The targets need the nightly toolchain pinned in
`toolchain.txt`; the wrapper builds them with the `libfuzzer` feature and the
same instrumentation flags as `cargo fuzz`. Set `YAMLPRIMS_FUZZ_TOOLCHAIN` to
use a locally installed toolchain with the pinned commit hash. See
`scripts/fuzz-smoke.sh` for the other settings.

Go mutation (`go test -fuzz`) is not run locally until an enforced memory
ceiling is available. The Go `Fuzz*` targets run their seed corpora under
`fuzz-replay`, with per-input time and heap assertions.

## Output

`fuzz-replay` prints the target and the failure kind only; full logs stay in a
temporary directory. `fuzz-smoke` writes logs, corpora, failing inputs and a
`campaign.json` record to `YAMLPRIMS_FUZZ_OUT` (default: a new temporary
directory), which must be outside the working tree. It prints target names,
execution counts, coverage and failure kinds, never an input or a dump of one,
and fails if a run leaves files under `yaml/testdata/` or `fuzz/`.

## Known differences

`known-differences.jsonl` lists Rust/Go differences that are under review.
The differential fails on any difference not listed. Each line is one JSON
object; every field except `note` is required. This example passes the
validator (`make test-fuzz-tooling` checks it):

```json
{"id": "KD-001", "input_sha256": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08", "operation": "wire", "options": "default", "revisions": ["v0.3.0"], "owner": "yamlprims maintainers", "status": "open"}
```

- `options` is one of `default`, `ijson-default`, `merge-as-key` or
  `merge-apply`.
- `operation` is one of `wire`, `plain`, `emit`, `ijson`, `lint` (the twins'
  outputs differ), `rust-roundtrip` or `go-roundtrip` (a twin's wire round
  trip differs from its own wire), `rust-driver-error` or `go-driver-error`.
- `revisions` is a non-empty list of commit ids or `vX.Y.Z` tags the
  difference was observed on, and `owner` is a non-empty string.
- `status` is `open`, `accepted` or `closed`. Only `open` and `accepted`
  entries suppress a difference; `closed` entries are history.

An entry matches one exact input, operation and option set, never a category.
Twin agreement is not a correctness oracle on its own: the targets' property
checks apply whether or not the twins agree. A corpus file that cannot be
decoded is reported as `corpus-decode` with the file's digest.
`scripts/fuzz-differential.test.sh` (`make test-fuzz-tooling`) holds the
controls for these rules.

## Reporting a crash

A panic, hang or memory-limit breach in a released parser is a security issue.
Report it as described in [`SECURITY.md`](../SECURITY.md). Do not put the
input, or a description that reproduces it, in a public issue, pull request,
commit or CI log before a fixed release is published.
