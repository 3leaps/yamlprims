# Conformance checks

The core baseline is tested against specification revision 1.3, the included
synthetic truth/wire regressions and the exact structcorpus inventory at
`400ae349bb8eb311656c39dc27ff3f7489c8b389`.

The lock in `pins/pin-400ae34.json` fixes source files, profile revisions,
hydrated inventory hashes, review-status counts and per-profile result rows.
Earlier locks remain historical evidence. Do not regenerate expectations from
the implementation. The gate rejects pin generation; wire golden blessing is
disabled. Expectation changes require independent semantic review.

Use a clean checkout of the pinned public corpus and a fresh hydration directory
outside this workspace:

```sh
python3 -I "$CORPUS/tools/corpus.py" hydrate --tier conformance --output "$HYDRATED"
bash conformance/requalify.sh "$CORPUS" "$HYDRATED" "$EVIDENCE"
python3 -B -m unittest discover -s conformance -p 'test_*.py'
```

`CORPUS`, `HYDRATED` and `EVIDENCE` are absolute paths. The evidence directory must
not already exist. The script runs Rust 1.89.0 debug and 1.88.0 release suites,
builds the driver, verifies the exact inventory, then runs detecting controls.
The controls include three positive and three deliberately corrupted node
selectors for UTF-8 sequence items, alias definition spans and stream documents.
Failed tests/builds stop execution before the driver. Core package tests need
only the included fixtures; full corpus qualification needs the pinned corpus.

The baseline inventory contains 61,348 provisional records and 90,970 compared
expectations, including 21 node spans, with eight explicit exclusions.
Passing comparisons do not promote provisional records. Semantic acceptance,
source acceptance and driver controls are distinct evidence. Historical rows
retain their original profile binding and run under current strict settings;
they do not establish the older lenient configuration. Event-only cases are
load observations, not event-conformance passes.

This is bounded transfer coverage. General event comparison, broader node kinds,
normative depth boundaries, full error-operation envelopes and all M1 contract
items remain open. See [implementation status](../docs/implementation.md).
Hazardous stress requires separate containment and self-tests; this script does
not run the stress tier or establish production resource qualification.

The corpus driver is a core example and uses development dependencies only.
Allocation instrumentation lives in integration tests; it is not part of the
production library. Generators are retained as review tools and are not invoked
by CI or tests to bless expected output.
