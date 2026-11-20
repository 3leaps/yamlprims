# Containment, Migration Divergence, and Workload Evidence (`docpresser`)

This page records out-of-process evidence for `yamlprims` `v0.3.0` produced with
[`3leaps/docpresser`](https://github.com/3leaps/docpresser) `v0.2.1`. It covers
three things: how the loaders behave on hostile alias and nesting inputs, where
YAML 1.1 loaders read the same document differently from YAML 1.2.2 Core, and
per-case timing on small synthetic workloads.

Baseline parsers (`gopkg.in/yaml.v3`, PyYAML, `js-yaml`) are never linked into
`yamlprims`. `docpresser` runs every engine as a separate driver process, one
process per case, with a wall-clock timeout, and records the outcome, duration
and peak RSS of each case. On Unix the driver's process group is
terminated on timeout and peak RSS is measured after it exits. Memory is
measured, not enforced: the packs declare a per-case RSS budget, but nothing
stops a driver from exceeding it (see the comparative run below). This is
supervision, not a sandbox.

Every run below is a single run on one host. The timings include process start
and teardown and are not throughput benchmarks; use them to compare outcomes and
orders of magnitude, not small differences.

## Run Environment

| Item | Value |
| --- | --- |
| `docpresser` | `v0.2.1` (commit `a8dd276`), built from the signed tag; every run manifest records `docpresser_version` `0.2.1` |
| `yamlprims-rust` driver | `yamlprims` `=0.3.0` from crates.io, built with `--locked` from the committed `drivers/yamlprims-rust/Cargo.lock` |
| `yamlprims-go` driver | `github.com/3leaps/yamlprims` `v0.3.0`, `canonprims` `v0.1.5` |
| `go-yamlv3` baseline | `gopkg.in/yaml.v3` `v3.0.1` |
| PyYAML baselines | PyYAML `6.0.3` on Python `3.14.6`: `SafeLoader` (`python-pyyaml-safe`) and `CSafeLoader` with libyaml `0.2.5` (`python-pyyaml-csafe`) |
| `node-yaml` baseline | `js-yaml` `4.1.0` on Node.js `v22.23.4` |
| Host | Apple M2 Max, macOS (`darwin/arm64`), 64 GiB; Rust `1.89.0`, Go `1.27.4` |
| Packs | generated with `--seed 424242 --scale large` |

Subject versions are the ones each driver reported in its handshake and are
recorded in each run's `run-manifest.json`.

## 1. Adversarial Containment

Pack `diabolical-alias-pyramid` (SHA-256 `a302670a66879c279d772b2d06a893fef67d5504dea701cb586d4412d1ff0745`), three cases, each with a 2,000 ms wall-clock timeout and a declared 64 MiB RSS budget:

- `alias-pyramid` (734 bytes): 23 anchors, each a sequence of four aliases to the previous one, so full expansion is 4^22 leaves.
- `anchor-churn` (286,000 bytes): 2,600 redefinitions of the anchor `&shared` and 10,400 `*shared` aliases.
- `deep-nesting` (322 bytes): 160 nested flow sequences.

**Target run `v030-diabolical-containment`**: verdict `pass`. Both `yamlprims` drivers refused all three cases with `limit_exceeded` (6 of 6 `refused_bounded`). Observations SHA-256 `6e4fd894f49e1dbcb9f788f4e61b7fa6880e6ba41fe80e4fe36e37c06fc0390e`.

**Comparative run `v030-diabolical-comparative`**: verdict `fail`, because baselines accepted or timed out on hostile input. Observations SHA-256 `bb0d91c97db22ec9f8be1c01c7f4dbb42cb1bc17f546b5c3139692134862b2c7`.

| Subject | `alias-pyramid` | `anchor-churn` | `deep-nesting` | Peak RSS |
| --- | --- | --- | --- | ---: |
| `yamlprims-rust` | refused, `limit_exceeded` (31 ms) | refused, `limit_exceeded` (63 ms) | refused, `limit_exceeded` (13 ms) | 27.0 MiB |
| `yamlprims-go` | refused, `limit_exceeded` (79 ms) | refused, `limit_exceeded` (306 ms) | refused, `limit_exceeded` (19 ms) | 38.5 MiB |
| `go-yamlv3` | refused (alias-ratio error, 27 ms) | **accepted** (392 ms) | **accepted** (21 ms) | 54.8 MiB |
| `python-pyyaml-safe` | **timeout** (killed at 2,007 ms) | refused (`ComposerError`, 125 ms) | **accepted** (122 ms) | **303.1 MiB** |
| `python-pyyaml-csafe` | **timeout** (killed at 2,012 ms) | refused (`ComposerError`, 160 ms) | **accepted** (82 ms) | **243.2 MiB** |

Both PyYAML loaders were still expanding the alias pyramid when the timeout
killed them, by then well past the declared 64 MiB budget. `docpresser` records
the overshoot; it does not prevent it.

## 2. YAML 1.1 → YAML 1.2.2 Core Migration Divergence

Pack `yaml11-migration` (SHA-256 `a906f559367318eeb3a1a4a90676337ec13915d46b785fadae4e4f6abe41116a`), five cases. Run `v030-yaml11-migration`: verdict `pass` (30 of 30 expectations), 11 baseline divergences recorded. Observations SHA-256 `52eb230c8d3bee1882044ae3c3f7a77a1b3f6b8cc86edb7182ba94f2f97d383b`.

Both `yamlprims` drivers produced the YAML 1.2.2 Core value for every case. A
baseline is listed as diverging when its value differs from that.

| Case | Input | YAML 1.2.2 Core value | Diverging baselines | `yamlprims check --lint-yaml11` |
| --- | --- | --- | --- | --- |
| `norway-booleans` | `country: NO`, `switch: off`, `enabled: yes` | strings | PyYAML (both): booleans | `yaml11_boolean` |
| `legacy-octal` | `mode: 0755`, `count: 010` | decimal `755`, `10` | `go-yamlv3`, PyYAML (both): octal `493`, `8` | `yaml11_number` |
| `sexagesimal` | `window: 10:20` | string `"10:20"` | PyYAML (both): integer `620` | `yaml11_sexagesimal` |
| `merge-key` | `<<: *b` | key `"<<"` kept as a literal key (this profile loads with `MergeKeys::AsKey`; the default `MergeKeys::Reject` refuses the document) | `go-yamlv3`, PyYAML (both), `js-yaml`: merged into the mapping | `yaml11_merge_key` |
| `yaml11safe-quoted` | `country: "NO"`, `switch: "off"`, `window: "10:20"` | strings | none | no warnings |

The last case is the form `yamlprims` emits with `CompatQuoting::Yaml11Safe`
(the default): every engine reads it the same way. See the
[libyaml migration guide](migrating-from-libyaml.md) for the lint and the
`MergeKeys` options.

## 3. Synthetic Workloads

### 3.1 Kubernetes manifest bundle (`v030-k8s-bundle`)

Pack `k8s-bundle` (SHA-256 `e378c765723b6f88a144a0de2c150b67fcf393a83940a519defd1a239bc2f1eb`): six `ConfigMap` manifests and one malformed control that must be rejected. Verdict `pass` (42 of 42). Observations SHA-256 `5ba336edd5ebb95c1191ca9ffb6a663a9182613c902f0cba859eeecf812d9957`.

| Subject | Cases | p50 per case | p95 per case | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| `yamlprims-rust` | 7 | 13.5 ms | 17.4 ms | 2.4 MiB |
| `yamlprims-go` | 7 | 19.2 ms | 31.8 ms | 5.9 MiB |
| `go-yamlv3` | 7 | 20.5 ms | 25.8 ms | 5.7 MiB |
| `python-pyyaml-safe` | 7 | 109.7 ms | 128.4 ms | 20.8 MiB |
| `python-pyyaml-csafe` | 7 | 143.7 ms | 156.2 ms | 20.8 MiB |
| `node-yaml` | 7 | 101.8 ms | 120.7 ms | 45.2 MiB |

These documents are small, so the per-case times are mostly process start-up:
compare the native drivers with each other and the interpreted drivers with each
other, not across the two groups.

### 3.2 Combined synthetic workload (`v030-synthetic-workload`)

Pack `synthetic-workload` (SHA-256 `5f814e16b618bff284773a400dc3f0c27307f74dc676c0fae417909aa50c9b6b`): 22 cases drawn from the Kubernetes, OpenAPI, RFC 8785 JCS projection, adversarial and migration scenarios. Run against `yamlprims-go`, the driver that implements the JCS and I-JSON projections. Verdict `pass` (22 of 22; the three adversarial cases `refused_bounded`). Observations SHA-256 `d46caccff4674be58dc919fc406e2d31887509baf5477b2923d08b2b8b05a299`.

## 4. Reproducing

From a checkout of the signed `docpresser` `v0.2.1` tag, outside the
`yamlprims` working tree:

```sh
git tag -v v0.2.1
make build
make driver-yamlprims-rust
(cd drivers/yamlprims-go && go build -o ../../bin/driver-yamlprims-go .)
(cd drivers/go-yamlv3 && go build -o ../../bin/driver-go-yamlv3 .)
python3 -m venv /tmp/dcp-venv && /tmp/dcp-venv/bin/pip install 'PyYAML==6.0.3'
(cd drivers/node-yaml && npm install --no-save --ignore-scripts js-yaml@4.1.0)

RUST="yamlprims-rust=$PWD/drivers/yamlprims-rust/target/release/yamlprims-rust"
GO="yamlprims-go=$PWD/bin/driver-yamlprims-go"
YV3="go-yamlv3=$PWD/bin/driver-go-yamlv3"
PYS="python-pyyaml-safe=/tmp/dcp-venv/bin/python $PWD/drivers/python-yaml/driver.py --engine pyyaml-safe"
PYC="python-pyyaml-csafe=/tmp/dcp-venv/bin/python $PWD/drivers/python-yaml/driver.py --engine pyyaml-csafe"
NODE="node-yaml=node $PWD/drivers/node-yaml/driver.mjs"

for sc in diabolical-alias-pyramid yaml11-migration k8s-bundle synthetic-workload; do
  bin/docpresser generate --seed 424242 --scenario "$sc" --scale large --out "/tmp/dcp/packs/$sc"
  bin/docpresser validate --pack "/tmp/dcp/packs/$sc"
done

bin/docpresser run --pack /tmp/dcp/packs/diabolical-alias-pyramid --profile diabolical-containment \
  --run-id v030-diabolical-containment --subject "$RUST" --subject "$GO" --out /tmp/dcp/runs/diabolical-containment
bin/docpresser run --pack /tmp/dcp/packs/diabolical-alias-pyramid --profile diabolical-containment \
  --run-id v030-diabolical-comparative --subject "$RUST" --subject "$GO" \
  --baseline "$YV3" --baseline "$PYS" --baseline "$PYC" --out /tmp/dcp/runs/diabolical-comparative
bin/docpresser run --pack /tmp/dcp/packs/yaml11-migration --profile yaml11-migration \
  --run-id v030-yaml11-migration --subject "$RUST" --subject "$GO" \
  --baseline "$YV3" --baseline "$PYS" --baseline "$PYC" --baseline "$NODE" --out /tmp/dcp/runs/yaml11-migration
bin/docpresser run --pack /tmp/dcp/packs/k8s-bundle --profile synthetic-workload \
  --run-id v030-k8s-bundle --subject "$RUST" --subject "$GO" \
  --baseline "$YV3" --baseline "$PYS" --baseline "$PYC" --baseline "$NODE" --out /tmp/dcp/runs/k8s-bundle
bin/docpresser run --pack /tmp/dcp/packs/synthetic-workload --profile synthetic-workload \
  --run-id v030-synthetic-workload --subject "$GO" --out /tmp/dcp/runs/synthetic-workload

# For each run (pack = the run's pack directory):
bin/docpresser verify --run /tmp/dcp/runs/<run> --pack /tmp/dcp/packs/<pack>
bin/docpresser summarize --run /tmp/dcp/runs/<run> --pack /tmp/dcp/packs/<pack> --format markdown
```

Packs are deterministic, so the pack digests above should match. Observation
digests and timings depend on the host and will differ. `verify` exits non-zero
for the comparative run, whose expected verdict is `fail`.
