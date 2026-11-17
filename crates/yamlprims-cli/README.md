# yamlprims CLI

A command-line consumer of the shared Rust core. The `yamlprims-cli` crate is
not published to crates.io; the `yamlprims` binary ships as signed release
archives for darwin-arm64, linux-amd64, linux-arm64, windows-amd64 and
windows-arm64 on [GitHub Releases](https://github.com/3leaps/yamlprims/releases).
The CLI contract is frozen in `spec-v1.6 §10.4`.
It forbids unsafe code. All YAML policies, values, limits and encoders come from
`yamlprims`; there is no second YAML parser or resolver.

```sh
cargo run --locked -p yamlprims-cli -- check config.yaml
cargo run --locked -p yamlprims-cli -- check --lint-yaml11 config.yaml
cargo run --locked -p yamlprims-cli -- check --lint-yaml11 --format=json config.yaml
cargo run --locked -p yamlprims-cli -- dump config.yaml
cargo run --locked -p yamlprims-cli -- dump --plain config.yaml
cargo run --locked -p yamlprims-cli -- dump --ijson config.yaml
cargo run --locked -p yamlprims-cli -- emit config.yaml
cargo run --locked -p yamlprims-cli -- --merge=apply check config.yaml
```

Command behavior (`spec-v1.6 §10.4`):

- `check [--lint-yaml11] [--format=text|json] [--] FILE...` validates every
  input as a multi-document stream, including empty inputs and multiple
  documents. Without `--lint-yaml11`, success is silent; YAML and I/O errors go
  to `stderr`, processing continues through the remaining files, and any input
  error exits `1`. With `--lint-yaml11` (`--format=text` default), YAML 1.1
  compatibility hazards are reported on `stderr` as
  `<file>:<line>:<col>: warning[<code>]: <message>` (plus a
  `warning[max_warnings]` line when truncated) and exit `1` when any warning or
  error is found; `--lint-yaml11 --format=json FILE` requires a single `FILE`
  (with optional `--max-output-bytes=N`), writes a `yamlprims-lint/1` JSON line
  (`spec-v1.6 §11.7`) to `stdout`, and exits `0` when `warning_count == 0`, `1`
  when `warning_count > 0` or on a YAML/output-limit error, or `2` on an
  unreadable file.
- `dump [--] FILE` writes the core's canonical `yamlprims-value/1` wire envelope
  for every document. Empty streams produce an empty `documents` array. YAML
  errors produce the core's error envelope on `stdout`, a diagnostic on
  `stderr`, and exit `1`.
- `dump --plain [--] FILE` requires **exactly one document** (`docs.len() == 1`
  from `load_all_bytes`; zero-document empty streams and multi-document streams
  fail with `<file>: --plain needs exactly one document` on `stderr`, no
  `stdout`, and exit `1`). It writes plain JSON (`encode_plain`) or fails with a
  diagnostic on `stderr`, no `stdout`, and exit `1`.
- `dump --ijson [--] FILE` requires a single non-empty-stream document and
  projects it to strict RFC 7493 I-JSON UTF-8 bytes (`spec-v1.6 §11.3`,
  `§11.6`), failing with a diagnostic on `stderr`, no `stdout`, and exit `1` on
  empty streams, multi-document streams, integers outside `±(2^53 − 1)`,
  `NaN`/`±Infinity`, non-string mapping keys, tags, or Unicode noncharacters.
- `emit [--from-wire|--from-plain] [--compat-quoting=yaml11-safe|core-only] [--max-output-bytes=N] [--] FILE`
  reads a YAML stream (or a `yamlprims-value/1` wire envelope with `--from-wire`
  or a single JSON value with `--from-plain`) and writes canonical 2-space
  block-first YAML 1.2.2 Core output (`spec-v1.6 §10.2`, `§10.4`) with
  `--compat-quoting=yaml11-safe|core-only` (`yaml11-safe` default; underscore
  spellings `yaml11_safe` and `core_only` are also accepted) and optional
  `--max-output-bytes=N`.
- `-` selects `stdin`, explicitly, at most once per command. No files means a
  usage error. File paths use operating-system strings; `--` admits filenames
  that begin with `-`. The path `-` is always `stdin`.
- `--merge=reject|apply|as_key` selects the core merge policy (`reject`
  default) and may appear before or after the subcommand before `--`. Unknown or
  repeated options, extra `dump`/`emit`/`check --lint-yaml11 --format=json`
  inputs, `--format` without `--lint-yaml11`, and `--plain`/`--ijson` on `check`
  are usage errors. `--help` (or `-h`) alone prints usage to `stdout` and exits
  `0`. `--version` (or `-V`) alone prints `yamlprims <version>` to `stdout` and
  exits `0`; combined with any other argument it is a usage error. Invalid usage
  prints usage to `stderr` and exits `2`.

Input reads stop at the core's default raw-input limit plus one byte, allowing
its size gate to report the error before UTF-8 validation. Serialized output is
completed in memory before writing any `stdout` (followed by one LF on JSON
subcommands outside the core serialization budget). Encoder failures produce no
`stdout` and exit `1`, including failure to encode a load-error envelope. There
is no empty fallback. An unreadable file under `dump`, `emit`, or
`check --lint-yaml11 --format=json` exits `2`. A `stdout` write or flush failure
exits `2`.

The versioned wire grammars (`yamlprims-value/1` and `yamlprims-lint/1`), CLI
contract (`§10.4`), and YAML semantics are defined by the
[specification](https://github.com/3leaps/yamlprims/blob/spec-v1.6/docs/spec/yamlprims-spec.md).

`yamlprims dump --ijson` output can be canonicalized with RFC 8785 JCS by
[canonprims](https://github.com/3leaps/canonprims):

```sh
yamlprims dump --ijson config.yaml | canonprims jcs
```

Run `cargo test --locked -p yamlprims-cli`. Tests cover command parsing, bounded
reads, failed reads/writes and serialization, file/stdin streams, diagnostics,
wire/plain/I-JSON/lint results, merge policy, and strict parsing. Synthetic
inputs are created in temporary directories; no external data checkout is
needed.

Licensed under MIT OR Apache-2.0. See the included license texts.
