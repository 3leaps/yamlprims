# Containment, Migration Divergence, and Workload Evidence (`docpresser`)

This page records out-of-process evidence for `yamlprims` `v0.3.0` produced with
[`3leaps/docpresser`](https://github.com/3leaps/docpresser) `v0.2.2`. It covers
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
| `docpresser` | `v0.2.2` (commit `067692a`), built from the signed tag with Go `1.26.10`; every run manifest records `docpresser_version` `0.2.2` |
| `yamlprims-rust` driver | `yamlprims` `=0.3.0` from crates.io, built with `--locked` from the committed `drivers/yamlprims-rust/Cargo.lock` |
| `yamlprims-go` driver | `github.com/3leaps/yamlprims` `v0.3.0`, `canonprims` `v0.1.5` |
| `go-yamlv3` baseline | `gopkg.in/yaml.v3` `v3.0.1` |
| PyYAML baselines | PyYAML `6.0.3` on Python `3.14.6`: `SafeLoader` (`python-pyyaml-safe`) and `CSafeLoader` with libyaml `0.2.5` (`python-pyyaml-csafe`) |
| `node-yaml` baseline | `js-yaml` `4.1.0` on Node.js `v22.23.4` |
| Host | Apple M2 Max, macOS (`darwin/arm64`), 64 GiB; drivers built with Rust `1.89.0` and Go `1.27.4` |
| Packs | generated with `--seed 424242 --scale large` |

Subject versions are the ones each driver reported in its handshake and are
recorded in each run's `run-manifest.json`.

## 1. Adversarial Containment

Pack `diabolical-alias-pyramid` (SHA-256 `a302670a66879c279d772b2d06a893fef67d5504dea701cb586d4412d1ff0745`), three cases, each with a 2,000 ms wall-clock timeout and a declared 64 MiB RSS budget:

- `alias-pyramid` (734 bytes): one scalar anchor followed by 22 anchored sequences, each holding four aliases to the previous anchor, so the last value alone expands to 4^22 scalars.
- `anchor-churn` (286,000 bytes): 2,600 definitions of the anchor `&shared` and 10,400 `*shared` aliases.
- `deep-nesting` (322 bytes): 160 nested flow sequences.

**Target run `v030-diabolical-containment`**: verdict `pass`. Both `yamlprims` drivers refused all three cases with `limit_exceeded` (6 of 6 `refused_bounded`). Observations SHA-256 `e826c045811f294d0a9eb05f79712e9d3cb9e48aef15688e0f53ecb9952176b9`.

**Comparative run `v030-diabolical-comparative`**: verdict `fail`, because baselines accepted or timed out on hostile input. Observations SHA-256 `0d1c7cee84b2a22a9a9746d41c76f9c85cc50f0e04974a30414d49bfc4b59f66`.

| Subject | `alias-pyramid` | `anchor-churn` | `deep-nesting` | Peak RSS |
| --- | --- | --- | --- | ---: |
| `yamlprims-rust` | refused, `limit_exceeded` (14 ms) | refused, `limit_exceeded` (29 ms) | refused, `limit_exceeded` (3 ms) | 27.1 MiB |
| `yamlprims-go` | refused, `limit_exceeded` (18 ms) | refused, `limit_exceeded` (71 ms) | refused, `limit_exceeded` (5 ms) | 37.7 MiB |
| `go-yamlv3` | refused (`yaml_v3_error`, phase `limit`, 5 ms) | **accepted** (102 ms) | **accepted** (5 ms) | 58.8 MiB |
| `python-pyyaml-safe` | **timeout** (killed at 2,013 ms) | refused (`ComposerError`, 51 ms) | **accepted** (49 ms) | **375.0 MiB** |
| `python-pyyaml-csafe` | **timeout** (killed at 2,008 ms) | refused (`ComposerError`, 46 ms) | **accepted** (44 ms) | **383.9 MiB** |

Both PyYAML drivers were still running on the alias pyramid when the timeout
killed them, by then well past the declared 64 MiB budget. The evidence does not
record whether the time went to loading or to the driver's conversion of the
loaded value. `docpresser` records the overshoot; it does not prevent it.

## 2. YAML 1.1 → YAML 1.2.2 Core Migration Divergence

Pack `yaml11-migration` (SHA-256 `a906f559367318eeb3a1a4a90676337ec13915d46b785fadae4e4f6abe41116a`), five cases. Run `v030-yaml11-migration`: verdict `pass` (30 of 30 expectations), 11 baseline divergences recorded. Observations SHA-256 `109b7a4a17961d9a5d98e85883b56fce6ccf5acc35de74d2bb4996892e0d131c`.

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

Pack `k8s-bundle` (SHA-256 `e378c765723b6f88a144a0de2c150b67fcf393a83940a519defd1a239bc2f1eb`): six `ConfigMap` manifests and one malformed control that must be rejected. Verdict `pass` (42 of 42). Observations SHA-256 `0faa5e6b959270cb3c5e9c7452454143a5a0bd1d1b3e545e505bf0042bb4545b`.

| Subject | Cases | p50 per case | p95 per case | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| `yamlprims-rust` | 7 | 3.5 ms | 4.6 ms | 2.3 MiB |
| `yamlprims-go` | 7 | 3.8 ms | 4.3 ms | 6.0 MiB |
| `go-yamlv3` | 7 | 3.9 ms | 4.1 ms | 5.9 MiB |
| `python-pyyaml-safe` | 7 | 43.2 ms | 48.3 ms | 20.5 MiB |
| `python-pyyaml-csafe` | 7 | 42.7 ms | 45.4 ms | 20.7 MiB |
| `node-yaml` | 7 | 42.2 ms | 43.6 ms | 44.8 MiB |

These documents are small, so the per-case times are mostly process start-up:
compare the native drivers with each other and the interpreted drivers with each
other, not across the two groups.

### 3.2 Combined synthetic workload (`v030-synthetic-workload`)

Pack `synthetic-workload` (SHA-256 `5f814e16b618bff284773a400dc3f0c27307f74dc676c0fae417909aa50c9b6b`): 22 cases drawn from the Kubernetes, OpenAPI, RFC 8785 JCS projection, adversarial and migration scenarios. Run against both `yamlprims` drivers. Verdict `pass`: 43 of 44 expectations passed and none failed. The three adversarial cases were `refused_bounded` on both drivers. The remaining case, `synth.openapi.jcs-canonical`, is `unsupported` on `yamlprims-rust` because that driver has no JCS projection (the Go driver projects JCS with `canonprims`). Observations SHA-256 `5aa33807d77c3d37e28ea11986af53992c6e86ade6f9e7664a5f160eff9168f7`.

| Subject | Cases | p50 per case | p95 per case | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| `yamlprims-rust` | 22 | 3.0 ms | 13.5 ms | 27.0 MiB |
| `yamlprims-go` | 22 | 4.2 ms | 16.4 ms | 37.7 MiB |

Peak RSS is the highest of any case, which here is an adversarial input that each driver refused.

## 4. Reproducing

From a checkout of the signed `docpresser` `v0.2.2` tag, outside the
`yamlprims` working tree:

```sh
git tag -v v0.2.2
git checkout --detach v0.2.2
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
  --run-id v030-synthetic-workload --subject "$RUST" --subject "$GO" --out /tmp/dcp/runs/synthetic-workload

# For each run (pack = the run's pack directory):
bin/docpresser verify --run /tmp/dcp/runs/<run> --pack /tmp/dcp/packs/<pack>
bin/docpresser summarize --run /tmp/dcp/runs/<run> --pack /tmp/dcp/packs/<pack> --format markdown
```

Packs are deterministic, so the pack digests above should match. Observation
digests and timings depend on the host and will differ. `verify` exits non-zero
for the comparative run, whose expected verdict is `fail`.

The peak RSS figures above were measured on macOS. On Linux, `docpresser`
reports a driver's peak as never lower than the memory `docpresser` itself held
when it started the driver, so small drivers are over-reported there (see its
`SECURITY.md`).
