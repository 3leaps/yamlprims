# Decision records

Records follow the [Crucible decision taxonomy](https://github.com/3leaps/crucible/blob/main/docs/repository/decision-records.md).
Numbers are independent per type; this directory is the repository's chosen home.
ADR-0003 is **accepted** for revision 1.1. The other records remain **proposed**.
Decision acceptance does not establish implementation qualification.

| ID                                                         | Decision                                                                    | Status   |
| ---------------------------------------------------------- | --------------------------------------------------------------------------- | -------- |
| [ADR-0001](ADR-0001-explicit-merge-policy.md)              | Reject merge directives by default; opt into Apply or AsKey                 | proposed |
| [ADR-0002](ADR-0002-library-first-workspace.md)            | Own the data contract in a library-first workspace                          | proposed |
| [ADR-0003](ADR-0003-strict-indentation.md)                 | Align flow indentation with YAML 1.2.2 in the revision 1.1 profile          | accepted |
| [DDR-0001](DDR-0001-values-locations-and-wire.md)          | Preserve rich values and source provenance across a versioned wire boundary | proposed |
| [DDR-0002](DDR-0002-boxed-error-ownership.md)              | Box errors and expose explicit owned detail extraction                      | proposed |
| [SecDR-0001](SecDR-0001-resource-and-native-boundaries.md) | Enforce logical budgets and isolate native boundaries                       | proposed |
| [PDR-0001](PDR-0001-specification-first-release.md)        | Publish the specification before implementation; qualify before transfer    | proposed |

Each accepted change records its affected specification/profile revision and
detecting evidence. Superseded expectations remain identifiable; they are not
silently reclassified as passes.
