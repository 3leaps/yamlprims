# Decision records

Records follow the [Crucible decision taxonomy](https://github.com/3leaps/crucible/blob/main/docs/repository/decision-records.md).
Numbers are independent per type; this directory is the repository's chosen home.
ADR-0001, ADR-0003 and ADR-0004 are **accepted**. The other records remain **proposed**.
Decision acceptance does not establish implementation qualification.

The revision 1.5 data-layer emitter, caller-tree safety and RFC 7493 I-JSON
projection contract is recorded in [ADR-0005](ADR-0005-data-layer-emitter-and-ijson-projection.md)
and the [specification errata](../spec/yamlprims-spec.md#16-errata). It awaits
review and maintainer acceptance.

| ID                                                               | Decision                                                                               | Status   |
| ---------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------- |
| [ADR-0001](ADR-0001-explicit-merge-policy.md)                    | Reject merge directives by default; opt into Apply or AsKey                            | accepted |
| [ADR-0002](ADR-0002-library-first-workspace.md)                  | Own the data contract in a library-first workspace (M2/M3 superseded in part by ADR-0004; proposed partial supersession in ADR-0005) | proposed |
| [ADR-0003](ADR-0003-strict-indentation.md)                       | Align flow indentation with YAML 1.2.2 in the revision 1.1 profile                     | accepted |
| [ADR-0004](ADR-0004-profile-b-go-twin-and-wasm-pyo3-bindings.md) | Adopt Profile B pure-Go twin and isolated WASM/PyO3 bindings                           | accepted |
| [ADR-0005](ADR-0005-data-layer-emitter-and-ijson-projection.md)  | Own a deterministic Core emitter (`Yaml11Safe`), caller-tree bounds, and RFC 7493 `encode_ijson` | proposed |
| [DDR-0001](DDR-0001-values-locations-and-wire.md)                | Preserve rich values and source provenance across a versioned wire boundary            | proposed |
| [DDR-0002](DDR-0002-boxed-error-ownership.md)                    | Box errors and expose explicit owned detail extraction                                 | proposed |
| [SecDR-0001](SecDR-0001-resource-and-native-boundaries.md)       | Enforce logical budgets and isolate native boundaries (superseded in part by ADR-0004) | proposed |
| [PDR-0001](PDR-0001-specification-first-release.md)              | Publish the specification before implementation; qualify before transfer               | proposed |

Each accepted change records its affected specification/profile revision and
detecting evidence. Superseded expectations remain identifiable; they are not
silently reclassified as passes.
