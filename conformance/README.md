# Conformance checks

The core baseline is tested against specification revision 1.5, the included
synthetic truth/wire regressions and the exact structcorpus inventory at
`b52fcd090d6879a0bd58ec5a5496c6a4de90504e`.

The lock in `pins/pin-b52fcd0.json` fixes source files, profile revisions,
hydrated inventory hashes, review-status counts and per-profile result rows.
Earlier locks (`pins/pin-6b1266e.json`, `pins/pin-c8b7421.json`, `pins/pin-400ae34.json`,
`pins/pin-d37ce93.json`, `pins/pin-444bc12.json`, `pins/pin-03ba2de.json`) remain
historical evidence.
Do not regenerate expectations from the implementation. The gate rejects pin
generation; wire golden blessing is disabled. Expectation changes require
independent semantic review.

Use a clean checkout of the pinned public corpus and a fresh hydration directory
outside this workspace:

```sh
python3 -I "$CORPUS/tools/corpus.py" hydrate --tier conformance --output "$HYDRATED"
bash conformance/requalify.sh "$CORPUS" "$HYDRATED" "$EVIDENCE"
bash conformance/requalify.sh --pin conformance/pins/pin-400ae34.json "$CORPUS_400AE34" "$HYDRATED_400AE34" "$EVIDENCE_400AE34"
python3 -B -m unittest discover -s conformance -p 'test_*.py'
```

`CORPUS`, `HYDRATED` and `EVIDENCE` are absolute paths. The evidence directory must
not already exist. The script runs Rust 1.89.0 debug and 1.88.0 release suites,
builds the driver, verifies the exact inventory, then runs detecting controls
(`structcorpus_probes_v13.py`, `structcorpus_probes_v14.py` when the revision 1.4
profiles are present, `structcorpus_gate_probes.py` and `node_selectors.py`).
The controls include three positive and three deliberately corrupted node
selectors for UTF-8 sequence items, alias definition spans and stream documents,
plus synthetic positive and negative controls for `yamlprims-core-depth128` r1,
`yamlprims-core-depth255` r1 (when present),
`yamlprims-core-entries2` r1 and the four revision 4 and revision 5 policy
profiles (revision 5 when present).
Failed tests/builds stop execution before the driver. Core package tests need
only the included fixtures; full corpus qualification needs the pinned corpus.

The baseline inventory at `b52fcd0` contains 61,366 provisional records and
90,988 compared expectations across 20 result rows, including 21 node spans,
6 expectations under `yamlprims-core-depth128` r1, 6 expectations under
`yamlprims-core-depth255` r1 and 6 expectations under
`yamlprims-core-entries2` r1, with eight explicit exclusions (`yaml-core` r1).
The four revision 4 and four revision 5 policy profiles (`yamlprims-core`,
`yamlprims-core-stream`, `yamlprims-core-merge-apply`, `yamlprims-core-merge-as-key`
r4 and r5) carry zero shipped corpus records in `b52fcd0`; revision 5 cites
specification 1.5 and keeps the revision 4 source-loader policy fields. The
driver validates their policy bindings
and `structcorpus_probes_v14.py` exercises them synthetically without adding
unbacked rows to the frozen corpus pin.
Passing comparisons do not promote provisional records. Semantic acceptance,
source acceptance and driver controls are distinct evidence. Historical rows
retain their original profile binding and run under current strict settings;
they do not establish the older lenient configuration. Event-only cases are
load observations, not event-conformance passes.

This is bounded transfer coverage. General event comparison, broader node kinds,
full error-operation envelopes and remaining M1 contract items remain open. See
[implementation status](../docs/implementation.md).
Hazardous stress requires separate containment and self-tests; this script does
not run the stress tier or establish production resource qualification.

The corpus driver is a core example and uses development dependencies only.
Allocation instrumentation lives in integration tests; it is not part of the
production library. Generators are retained as review tools and are not invoked
by CI or tests to bless expected output.
