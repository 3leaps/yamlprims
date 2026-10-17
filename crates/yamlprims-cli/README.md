# yamlprims CLI

An initial command-line consumer of the shared Rust core. The package is under
development, is not published, and does not freeze the remaining CLI contract.
It forbids unsafe code. All YAML policies, values, limits and encoders come from
`yamlprims`; there is no second YAML parser or resolver.

```sh
cargo run --locked -p yamlprims-cli -- check config.yaml
cargo run --locked -p yamlprims-cli -- dump config.yaml
cargo run --locked -p yamlprims-cli -- dump --plain config.yaml
cargo run --locked -p yamlprims-cli -- --merge=apply check config.yaml
```

Current command behavior:

- `check FILE...` validates every input as a stream, including empty inputs and
  multiple documents. Success is silent. YAML and I/O errors go to stderr;
  processing continues through the remaining files. Any input error exits 1.
- `dump FILE` writes the core's canonical wire envelope for every document.
  Empty streams produce an empty `documents` array. YAML errors produce the
  core's error envelope on stdout, a diagnostic on stderr, and exit 1.
- `dump --plain FILE` requires exactly one document. It writes plain JSON or
  fails with a diagnostic and no stdout. Zero/multiple documents are currently
  rejected; their final output representation remains a contract-freeze item.
- `-` selects stdin, explicitly, at most once per command. No files means a usage
  error. File paths use operating-system strings; `--` admits filenames that
  begin with `-`. The path `-` is always stdin.
- `--merge=reject|apply|as_key` selects the core merge policy. Reject is the
  default, and both parsing passes remain strict. Unknown or repeated options,
  extra dump inputs, and `--plain` on `check` are usage errors. `--help` prints
  usage successfully; invalid usage prints to stderr and exits 2.

Input reads stop at the core's default raw-input limit plus one byte, allowing
its size gate to report the error before UTF-8 validation. Core defaults bound
loading and serialized output; no custom-limit flags are exposed yet. Serialized
JSON is completed before writing any stdout, followed by one LF outside the
core serialization budget. Encoder failures produce no stdout and exit 1,
including failure to encode a load-error envelope. There is no empty fallback.
A dump I/O failure exits 2. A stdout write or flush failure can leave a prefix
on the destination and exits 2; this is not an operating-system atomic-write
guarantee. Diagnostic text itself does not have a separate CLI byte cap.

These behaviors need independent review before the CLI contract is frozen.
Production platform/resource qualification and package publication remain open.
The versioned wire grammar and YAML semantics are defined by the
[specification](https://github.com/3leaps/yamlprims/blob/spec-v1.3/docs/spec/yamlprims-spec.md).

Run `cargo test --locked -p yamlprims-cli`. Tests cover command parsing, bounded
reads, failed reads/writes and serialization, file/stdin streams, diagnostics,
wire/plain results, merge policy and strict parsing. Synthetic inputs are created
in temporary directories; no external data checkout is needed.

Licensed under MIT OR Apache-2.0. See the included license texts.
