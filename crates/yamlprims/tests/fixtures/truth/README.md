# Scalar and key expectations

These synthetic inputs exercise scalar resolution, typed conversion, key
collisions and explicit structure policies. `booleans.tsv`, `collisions.tsv`
and `numbers.tsv` are generated from specification rules by an independent
Python oracle. `golden.tsv` contains hand-written expectations. The Rust tests
consume the checked-in bytes; they do not generate expected results.

The notation uses `null`, `bool:`, `int:` decimal values, `float:` binary64 hex
bits, `f32:` binary32 hex bits, `str:` JSON strings, `tagged:`, `seq:`, `keys:`,
`ok`, `none`, `err:` and `syntax`. YAML and string fields are JSON-escaped within
tab-separated rows. Historical rows remain unchanged; the Rust revision tests
exercise subsequent contract additions separately.

The fixtures and generator are original synthetic project material under
MIT OR Apache-2.0. They contain no user configurations or external dataset copy.
Changes to expectations require independent semantic review. See the workspace's
conformance documentation for the separately pinned corpus inventory.
