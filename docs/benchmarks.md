# Adversarial Containment, Migration Divergence, and Workload Evidence (`docpresser` Dossier)

This document summarizes reproducible, out-of-process qualification and comparative evidence for `yamlprims` (`v0.3.0`, implementing [`spec-v1.6`](spec/yamlprims-spec.md)) produced with [`3leaps/docpresser`](https://github.com/3leaps/docpresser) (`v0.1.0`) and [`3leaps/structcorpus`](https://github.com/3leaps/structcorpus) (`b52fcd090d6879a0bd58ec5a5496c6a4de90504e`).

`yamlprims` keeps its repository dependency graph leaf-clean (`leaf: true`): competitor and YAML 1.1 baseline parsers (`PyYAML`, `gopkg.in/yaml.v3`, `js-yaml`) are never linked into `yamlprims`. Instead, `docpresser` drives each engine as an isolated out-of-process subprocess under OS wall-clock and memory containment and verifies retained NDJSON observations against content-hashed input packs.

---

## 1. Four-Axis Evaluation Model

Following the `docpresser` verification contract, results are separated into four orthogonal axes and are never collapsed into a composite score:

1. **Correctness (`correctness`):** Agreement with the pack's normative YAML 1.2.2 Core Schema, RFC 7493 I-JSON, or RFC 8785 JCS expectations (`pass`, `fail`, `unsupported`, `inconclusive`).
2. **Adversarial containment (`containment`):** Structured, resource-bounded refusal (`refused_bounded`) before materializing hostile amplification inputs versus unbounded expansion (`unbounded_accept`), wall-clock timeout (`timeout_exceeded`), process crash (`crash_signal`), or out-of-memory termination (`oom_killed`).
3. **Migration divergence (`migration_divergence`):** Structured semantic differences between YAML 1.2.2 Core Schema (`yamlprims`) and YAML 1.1 / `libyaml`-lineage baselines (`PyYAML`, `gopkg.in/yaml.v3`), plus verification that `CompatQuoting::Yaml11Safe` emitted output round-trips identically across both dialects.
4. **Telemetry (`telemetry`):** Throughput, latency percentiles (`p50`, `p95`, `p99`), and peak physical RSS measured per subprocess invocation and tied to recorded host and toolchain metadata.

---

## 2. Adversarial Containment (`diabolical-containment`, Seed `424242`, Scale `large`)

- **Pack:** `diabolical-alias-pyramid` (`3` adversarial cases, pack SHA-256 `740ccfba0e96cc6143af261d2015859899aa7012da05fd935807793dba0fc699`, per-case containment budget `max_wall_ms = 2,000`, `max_rss_bytes = 64 MiB`):
  1. `synth.diabolical.alias-pyramid` (`734` input bytes): 22-level exponential alias pyramid (`a0` → `a22`, $4^{22} \approx 1.76 \times 10^{13}$ logical nodes).
  2. `synth.diabolical.anchor-churn` (`286,000` input bytes): `2,600` repeated `&shared` anchor redefinitions and `*shared` 4-way alias fan-outs (`10,400` alias events).
  3. `synth.diabolical.deep-nesting` (`322` input bytes): `160`-level nested flow sequence (`[[[...]]]`, exceeding `Limits::max_depth = 128`).
- **Target qualification run (`v030-diabolical-containment`):** Observations SHA-256 `8292d03e9e5f33f0ca7e5293d84ff17ee3339111ea9c737d52e32e623b80fd69` — **Verdict: `pass` (`refused_bounded`: `6/6`)**.
- **Comparative run (`v030-diabolical-comparative`):** Observations SHA-256 `f3feb540e55e1a8081700a68844550c133a7d165418de8d0e25cfae9a8be2b07`.

| Subject | Engine / Language | `alias-pyramid` (`734 B`) | `anchor-churn` (`286 KiB`) | `deep-nesting` (`322 B`) | Peak RSS Across Pack |
| --- | --- | --- | --- | --- | ---: |
| `yamlprims-rust` (`v0.3.0`) | Safe Rust (`granit-parser` + `yamlprims`) | `refused_bounded` (`limit_exceeded`, `14.4 ms`) | `refused_bounded` (`limit_exceeded`, `26.8 ms`) | `refused_bounded` (`limit_exceeded`, `3.5 ms`) | `28.3 MiB` |
| `yamlprims-go` (`v0.3.0`) | Pure Go (`CGO_ENABLED=0`) | `refused_bounded` (`limit_exceeded`, `16.6 ms`) | `refused_bounded` (`limit_exceeded`, `68.6 ms`) | `refused_bounded` (`limit_exceeded`, `5.9 ms`) | `39.7 MiB` |
| `go-yamlv3` (`v3.0.1`) | Go `gopkg.in/yaml.v3` | Heuristic alias-ratio rejection (`3.8 ms`) | **`unbounded_accept`** (materializes full expansion, `100.7 ms`, `61.9 MiB` RSS) | **`unbounded_accept`** (materializes `160` levels; default ceiling is `10,000`) | `61.9 MiB` |
| `python-pyyaml-safe` | Python 3.15 YAML 1.1 baseline | **`timeout_exceeded`** (killed at `2,007 ms` wall-clock cap, **`453.5 MiB` RSS**) | **`unbounded_accept`** (materializes full expansion, `124.7 ms`, `43.5 MiB` RSS) | `refused_bounded` (`recursion_error`, `39.7 ms`) | **`453.5 MiB`** |

Key takeaways:
- Both `yamlprims-rust` and `yamlprims-go` enforce `max_expanded_nodes`, `max_alias_events`, `max_owned_bytes`, and `max_depth` **before** materializing alias copies or deep containers, refusing all three adversarial inputs with structured `LimitExceeded` errors in `3.5–68.6 ms`.
- Unbounded YAML 1.1 / `yaml.v3` loaders either expand repeated anchor-churn streams in full or exhaust wall-clock and memory budgets on exponential alias pyramids (`453.5 MiB` RSS before the 2-second supervisor kill).

---

## 3. YAML 1.1 → YAML 1.2.2 Core Migration Divergence (`yaml11-migration`, Seed `424242`)

- **Pack:** `yaml11-migration` (`5` cases, pack SHA-256 `9267516d55d66818db31cd7ec50f7f9446fe36ba74af3a2d0aa11742d348ef65`).
- **Run (`v030-yaml11-migration`):** Observations SHA-256 `f87ea3b6c3f975bd5279713b1cefa0404daca7420f6831584e8493700d3655f5` — **Verdict: `pass` (`20/20` expectations satisfied, `6` structured baseline divergences recorded, `0` divergences on `CompatQuoting::Yaml11Safe` quoted output)**.

| Case ID | Construct | `yamlprims-rust` & `yamlprims-go` (YAML 1.2 Core) | `go-yamlv3` Baseline | `python-pyyaml-safe` (YAML 1.1) Baseline | `yamlprims check --lint-yaml11` Code |
| --- | --- | --- | --- | --- | --- |
| `synth.migration.norway-booleans` | `country: NO`, `switch: off`, `enabled: yes` | Strings (`"NO"`, `"off"`, `"yes"`) | Strings (`"NO"`, `"off"`, `"yes"`) | **Diverges:** Booleans (`false`, `false`, `true`) | `yaml11_boolean` |
| `synth.migration.legacy-octal` | `mode: 0755`, `count: 010` | Decimal ints (`755`, `10`) | **Diverges:** Octal ints (`493`, `8`) | **Diverges:** Octal ints (`493`, `8`) | `yaml11_number` |
| `synth.migration.sexagesimal` | `window: 10:20` | String (`"10:20"`) | String (`"10:20"`) | **Diverges:** Sexagesimal int (`620`) | `yaml11_sexagesimal` |
| `synth.migration.merge-key` | `<<: *b` (under `MergeKeys::AsKey`) | Literal `"<<"` key (`MergeKeys::Reject` rejects by default; `MergeKeys::Apply` splices explicitly) | **Diverges:** Implicitly splices `x: 1` into mapping | **Diverges:** Implicitly splices `x: 1` into mapping | `yaml11_merge_key` |
| `synth.migration.yaml11safe-quoted` | `country: "NO"`, `switch: "off"`, `window: "10:20"` | Strings (`"NO"`, `"off"`, `"10:20"`) | Identical strings (`0` divergence) | Identical strings (`0` divergence) | Clean (`0` warnings) |

---

## 4. Synthetic Workload & Conformance Summary (`k8s-bundle` & `synthetic-workload`, Seed `424242`, Scale `large`)

- **Host environment:** Apple Silicon (`darwin/arm64`), Rust `1.89.0`, Go `1.27.4`, CPython `3.15`.
- **Note on subprocess harness overhead:** `docpresser run` executes each case in a fresh, isolated OS child process (`fork`/`exec` + JSON base64 request/response framing) to guarantee crash and RSS isolation. Per-case latencies below therefore include full OS process spawn and teardown overhead (`~2.5–3.5 ms` for native Rust/Go binaries, `~35 ms` for Python interpreter startup).

### 4.1 Multi-Subject Kubernetes Manifest Bundle (`v030-k8s-bundle`)

- **Pack SHA-256:** `d48f357b2a86b6bffb0577702319c4c563dd19ea11c3c6029bddac5ebb1d9560` (`6` positive synthetic `ConfigMap` manifests + `1` negative unterminated flow-sequence control).
- **Observations SHA-256:** `15f38ed1fb737df564ce25b993a2d41fc371e69faa0d7be388a13c7ad5239814` (`28/28` passed).

| Subject | Role | Cases | Subprocess Docs/s | Subprocess `p50` | Subprocess `p95` | Peak Child RSS |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `yamlprims-rust` (`v0.3.0`) | `target` | 7 | `312.4 docs/s` | `3.13 ms` | `3.63 ms` | `2.49 MiB` |
| `yamlprims-go` (`v0.3.0`) | `target` | 7 | `238.6 docs/s` | `4.20 ms` | `4.41 ms` | `6.19 MiB` |
| `go-yamlv3` (`v3.0.1`) | `baseline` | 7 | `239.7 docs/s` | `4.18 ms` | `4.36 ms` | `6.00 MiB` |
| `python-pyyaml-safe` | `baseline` | 7 | `27.4 docs/s` | `36.51 ms` | `37.98 ms` | `20.28 MiB` |

### 4.2 Combined Multi-Format Synthetic Workload (`v030-synthetic-workload`)

- **Pack SHA-256:** `81341e23b1197be08b151f15b028d36e595efaa22abb014397224f57b808d294` (`22` cases combining `k8s-bundle`, `openapi-graph`, `project_jcs` RFC 8785 JCS canonicalization, `diabolical-alias-pyramid`, and `yaml11-migration`).
- **Observations SHA-256:** `f4f88f205780c53f2eaf11f94d4eb24a4b92d406369d4dfec90e3f62a4437a94` — **Verdict: `pass` (`22/22` passed, `3/3` adversarial cases `refused_bounded`)**.

### 4.3 Pinned `structcorpus` Conformance Qualification

In-process conformance qualification (`conformance/requalify.sh` and `make crosscheck` / `make ts-check` / `make python-check`) verifies:
- **`structcorpus` `b52fcd090d6879a0bd58ec5a5496c6a4de90504e` (`conformance/pins/pin-b52fcd0.json`):** `61,366` corpus cases / `90,988` expectations across `20` profile rows (`spec-v1.0` through `spec-v1.5`), including `load -> emit_all_with -> load` semantic round-trip across every positive value expectation in both Rust and pure Go.
- **In-repo 4-binding differential vectors (`conformance/vectors/cases.jsonl`):** `188` cases byte-identical across Rust, pure Go, TypeScript WASM, and Python `PyO3` for `yamlprims-value/1`, `decode_wire`, plain JSON, `emit_all`, `encode_ijson`, and `yamlprims-lint/1` `encode_lint`.

---

## 5. Reproducing the `docpresser` Evidence Pack

With [`docpresser`](https://github.com/3leaps/docpresser) (`v0.1.0+`) and the standalone subject drivers built outside the `yamlprims` working tree:

```sh
# 1. Generate and validate deterministic packs (seed 424242, scale large)
docpresser generate --seed 424242 --scenario diabolical-alias-pyramid --scale large --out /tmp/dcp-packs/diabolical
docpresser generate --seed 424242 --scenario yaml11-migration --scale large --out /tmp/dcp-packs/migration
docpresser generate --seed 424242 --scenario k8s-bundle --scale large --out /tmp/dcp-packs/k8s
docpresser validate --pack /tmp/dcp-packs/diabolical
docpresser validate --pack /tmp/dcp-packs/migration
docpresser validate --pack /tmp/dcp-packs/k8s

# 2. Run and verify adversarial containment
docpresser run \
  --pack /tmp/dcp-packs/diabolical \
  --profile diabolical-containment \
  --run-id v030-diabolical-containment \
  --subject "yamlprims-rust=/path/to/driver-yamlprims-rust" \
  --subject "yamlprims-go=/path/to/driver-yamlprims-go" \
  --out /tmp/dcp-runs/diabolical
docpresser verify --run /tmp/dcp-runs/diabolical --pack /tmp/dcp-packs/diabolical
docpresser summarize --run /tmp/dcp-runs/diabolical --pack /tmp/dcp-packs/diabolical --format markdown

# 3. Run and verify YAML 1.1 -> YAML 1.2.2 Core migration divergence
docpresser run \
  --pack /tmp/dcp-packs/migration \
  --profile yaml11-migration \
  --run-id v030-yaml11-migration \
  --subject "yamlprims-rust=/path/to/driver-yamlprims-rust" \
  --subject "yamlprims-go=/path/to/driver-yamlprims-go" \
  --baseline "go-yamlv3=/path/to/driver-go-yamlv3" \
  --baseline "python-pyyaml-safe=python3 /path/to/driver.py --engine pyyaml-safe" \
  --out /tmp/dcp-runs/migration
docpresser verify --run /tmp/dcp-runs/migration --pack /tmp/dcp-packs/migration
docpresser summarize --run /tmp/dcp-runs/migration --pack /tmp/dcp-packs/migration --format markdown
```
