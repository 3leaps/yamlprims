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
temporary directory. `fuzz-smoke` uses `YAMLPRIMS_FUZZ_OUT` (default: a new
temporary directory), which must be outside the working tree. Each run takes
an exclusive `flock` on that directory's lock file. The locked descriptor is
passed to the campaign shell and is inherited by the processes it starts that
keep inherited descriptors open (the libFuzzer targets and cargo). A second run
is refused while any process that still holds the descriptor is alive, even if
the first run's wrapper was killed; the kernel releases the lock when the last
holder exits, and the lock file is never removed. A subprocess that closes
inherited descriptors, such as the differential's drivers (started with Python's
default `close_fds`), does not hold the lock, so if it outlives every holder it
is no longer excluded. The run writes its
logs, failing inputs, differential report and `campaign.json` record into its
own new directory under `runs/`. Only the corpora under `corpus/` are shared
between runs: seeds enter them by exclusive creation (a name already present
is skipped, never opened), and libFuzzer adds each new unit as
`corpus/<target>/<SHA-1 of its contents>` using `fopen(path, "wb")`, which
the harness does not change. It prints target names,
execution counts, coverage and failure kinds, never an input or a dump of one,
and fails if a run leaves files under `yaml/testdata/` or `fuzz/`.

Output roots are checked before anything is created:

- An output or temporary location (including `TMPDIR`) must not be the
  working tree, inside it, or above it. This is decided by filesystem
  identity, not by path spelling.
- A script creates its output root itself. An existing directory is used only
  if it is empty, or if it carries the `.yamlprims-fuzz-out` marker an earlier
  run wrote (so a campaign can reuse its corpora). Anything else is refused,
  and its contents are left alone.
- No symlink, and no regular file with more than one hard link, may appear
  anywhere below an output root. The scripts check before every step that
  creates or writes files there.
- The harness never writes through an existing file. Seeds and reports are
  created exclusively without following links; logs, settings and
  `campaign.json` are written under a fresh name and renamed into place.
  libFuzzer's own corpus and crash-input writes are the exception: they are
  covered only by the checks before and after each target run and by the
  assumption below, not protected against a path being replaced while
  libFuzzer runs.

These checks assume that the output directory's ancestors, and `TMPDIR`, are
trusted and controlled by the user running the scripts. Out of scope under
that assumption:

- someone replacing a path component between a check and the write;
- someone replacing the lock file's directory entry from outside (for
  example renaming another regular file over it), which would let a second
  run lock a different file.

The lock and these checks are supported on local filesystems only. `flock`
behaviour on NFS or other network filesystems is not relied on or verified.

## Driver time limits

The differential sends records to each driver in chunks of 256, and each chunk
has a time limit: `YAMLPRIMS_FUZZ_DRIVER_TIMEOUT` seconds (default 10, range 1
to 3600) plus 0.05 s per record. A chunk that runs out of time is killed and
split until the record that does not finish is found. That record is reported
as `rust-driver-timeout` or `go-driver-timeout` with its input digest only. A
timeout is a finding, and no known-differences entry can suppress it.

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
