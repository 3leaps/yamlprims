//! Consumer driver for structcorpus 0.1 (`yaml-value/1`, typed and node-span expectations). Fails
//! closed: the exit status is 0 only when at least one expectation ran and every requested
//! expectation passed or was explicitly excluded.
//!
//! Usage: `cargo run --release --example structcorpus -- HYDRATED_DIR SOURCE_DIR [--details N] [--case-file F]`
//!
//! `HYDRATED_DIR` is the output of `tools/corpus.py hydrate` (recipe cases), and `SOURCE_DIR`
//! is the structcorpus checkout it came from (hand-written cases in `manifests/cases.jsonl`).
//! Pins, digests and expected counts are checked by `conformance/scripts/structcorpus_gate.py`, which
//! runs this driver.
//!
//! Expectations are dispatched on the exact `(profile, revision)` (see `profiles()`). Each
//! profile file used is read from `SOURCE_DIR/manifests/profiles/` and its `policy` must
//! equal the policy this driver implements for it, so a changed or unknown profile fails
//! rather than running under the wrong options. A profile with explicit `limits`
//! (`yamlprims-core-limits`, `yamlprims-core-stress`, `yamlprims-core-depth128`,
//! `yamlprims-core-entries2`) runs with exactly those twelve §6 limits; the driver requires
//! all twelve names and no others, and the values must pass `Options::validate`. A profile
//! without `limits` runs with the defaults.
//!
//! Row status:
//! - `target`: revision 4 policy profiles, `yamlprims-core-depth128` r1 and
//!   `yamlprims-core-entries2` r1 cite spec 1.4; revision 3 profiles,
//!   `yamlprims-core-limits` r2 and `yamlprims-core-stress` r1 cite spec 1.3; revision 2
//!   profiles and `yamlprims-core-limits` r1 cite spec 1.1. The candidate implements 1.4,
//!   which restates 1.3 and 1.1 apart from the profile-bound records that exercise the newer
//!   revisions.
//! - `historical`: revision 1 profiles cite spec 1.0 (lenient flow indentation). The driver
//!   runs them under the current strict target: they are historical expectations checked
//!   against the candidate, not a qualified 1.0 configuration.
//! - `excluded`: `yaml-core` r1 (a generic profile with unbounded integers) is the only
//!   allowed exclusion; any other unknown name or revision is a failure.
//!
//! Errors are compared by phase (DDR-0002), and by kind, limit or configured value when the
//! expectation names one. Typed expectations load the document, follow `path`, and convert that node with
//! `from_node`; a load error has the phase of its kind, a conversion error is phase `typed`.
//!
//! `node_locations` (profile bindings) are compared for convention `yamlprims-node` r1, bound to a
//! profile citing spec 1.3. The convention file must define exactly the node kinds compared.
//! Each span's path must resolve in the same profile's value expectation and end at a defined
//! node kind; the loaded node at that path must have that kind and exactly that byte span.
//! Format-level events and event-keyed `locations` are bound to no profile and are not
//! compared; their counts are printed so event-only cases are visible.
//!
//! `--case-file F` reads cases only from `F` (one bounded process per case, as run by
//! `scripts/structcorpus_stress.py`). In that mode an event-only case is loaded under
//! `yamlprims-core` r3, the profile its ladder stays under. Its outcome is printed and counted
//! as `load-only`, along with this process's peak resident memory.
//!
//! Output: one row per profile, revision and expectation kind with its status and counts,
//! then the first failures, then `result PASS` or `result FAIL` with the reasons.

use serde_json::{json, Value as J};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use yamlprims::{
    decode_wire, emit_all_with, encode, encode_error, encode_plain, from_node, load_all_bytes,
    load_bytes, load_ijson_bytes, EmitOptions, Error, ErrorKind, LimitKind, Limits, MergeKeys,
    Node, NodeSpan, Options, UnknownTags, Value, WireEnvelope,
};

struct Profile {
    id: &'static str,
    revision: u64,
    /// `target` (its cited profile revision) or `historical` (older expectations under strict parsing).
    status: &'static str,
    merge: MergeKeys,
    single: bool,
    /// The profile file carries explicit `limits`.
    limits: bool,
    /// The profile file's `policy`, exactly.
    policy: &'static str,
}

const BASE: &str =
    r#""duplicate_keys":"reject","integer_range":"i128-u128","unknown_tags":"tagged-str""#;

fn profiles() -> Vec<Profile> {
    let p = |id, revision, status, merge, merge_name: &str, single: bool| {
        let docs = if single { "single" } else { "stream" };
        let limits = LIMITS_PROFILES.contains(&id);
        let strict = if revision >= 2 || limits {
            r#","flow_indentation":"strict""#
        } else {
            ""
        };
        let policy =
            format!(r#"{{{BASE},"merge_keys":"{merge_name}","documents":"{docs}"{strict}}}"#);
        Profile {
            id,
            revision,
            status,
            merge,
            single,
            limits,
            policy: Box::leak(policy.into_boxed_str()),
        }
    };
    vec![
        p(
            "yamlprims-core-limits",
            1,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core",
            1,
            "historical",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core",
            2,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core-stream",
            2,
            "target",
            MergeKeys::Reject,
            "reject",
            false,
        ),
        p(
            "yamlprims-core-merge-apply",
            1,
            "historical",
            MergeKeys::Apply,
            "apply",
            true,
        ),
        p(
            "yamlprims-core-merge-apply",
            2,
            "target",
            MergeKeys::Apply,
            "apply",
            true,
        ),
        p(
            "yamlprims-core-merge-as-key",
            1,
            "historical",
            MergeKeys::AsKey,
            "plain-key",
            true,
        ),
        p(
            "yamlprims-core-merge-as-key",
            2,
            "target",
            MergeKeys::AsKey,
            "plain-key",
            true,
        ),
        // Spec 1.3 (DDR-0008, DDR-0009).
        p(
            "yamlprims-core",
            3,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core-stream",
            3,
            "target",
            MergeKeys::Reject,
            "reject",
            false,
        ),
        p(
            "yamlprims-core-merge-apply",
            3,
            "target",
            MergeKeys::Apply,
            "apply",
            true,
        ),
        p(
            "yamlprims-core-merge-as-key",
            3,
            "target",
            MergeKeys::AsKey,
            "plain-key",
            true,
        ),
        p(
            "yamlprims-core-limits",
            2,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core-stress",
            1,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        // Spec 1.4 (DDR-0010, DDR-0011, DDR-0012).
        p(
            "yamlprims-core",
            4,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core-stream",
            4,
            "target",
            MergeKeys::Reject,
            "reject",
            false,
        ),
        p(
            "yamlprims-core-merge-apply",
            4,
            "target",
            MergeKeys::Apply,
            "apply",
            true,
        ),
        p(
            "yamlprims-core-merge-as-key",
            4,
            "target",
            MergeKeys::AsKey,
            "plain-key",
            true,
        ),
        p(
            "yamlprims-core-depth128",
            1,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core-depth255",
            1,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
        p(
            "yamlprims-core-entries2",
            1,
            "target",
            MergeKeys::Reject,
            "reject",
            true,
        ),
    ]
}

/// Profiles citing spec 1.3, the only ones node spans may be bound to (profile bindings).
const SPEC_1_3: &[(&str, u64)] = &[
    ("yamlprims-core", 3),
    ("yamlprims-core-stream", 3),
    ("yamlprims-core-merge-apply", 3),
    ("yamlprims-core-merge-as-key", 3),
    ("yamlprims-core-limits", 2),
    ("yamlprims-core-stress", 1),
];

/// Profiles whose file carries exactly the twelve §6 limits.
const LIMITS_PROFILES: &[&str] = &[
    "yamlprims-core-limits",
    "yamlprims-core-stress",
    "yamlprims-core-depth128",
    "yamlprims-core-depth255",
    "yamlprims-core-entries2",
];

/// The only profiles the driver may skip, and why (the reason is for readers of this table).
const EXCLUDED: &[(&str, u64, &str)] = &[(
    "yaml-core",
    1,
    "generic profile: unbounded integers, plain-key merges, stream",
)];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--stdin") {
        return run_stdin(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("--crosscheck-jsonl") {
        return run_crosscheck_jsonl();
    }
    let (hyd, src) = match &args[..] {
        [h, s, ..] => (PathBuf::from(h), PathBuf::from(s)),
        _ => {
            eprintln!("usage: structcorpus HYDRATED_DIR SOURCE_DIR [--details N]");
            return ExitCode::from(2);
        }
    };
    let details: usize = args
        .iter()
        .position(|a| a == "--details")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(20);
    let case_file = args
        .iter()
        .position(|a| a == "--case-file")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let profiles = profiles();
    let mut problems: Vec<String> = Vec::new();
    let mut checked_profiles: BTreeMap<(String, u64), Option<Limits>> = BTreeMap::new();
    let mut rows: BTreeMap<(String, u64, &str), (&str, usize, usize)> = BTreeMap::new(); // status, pass, fail
    let mut failures = Vec::new();
    let mut ids = HashSet::new();
    let mut cases = 0usize;
    let mut excluded: BTreeMap<(String, u64, &str), usize> = BTreeMap::new();
    let mut convention: Option<Result<(), String>> = None;
    let (mut with_events, mut events_only, mut event_locations) = (0usize, 0usize, 0usize);

    // Recipe cases are in the hydrated directory; hand-written cases in the source manifest.
    // `--case-file` reads only that file (scripts/structcorpus_stress.py runs one case per
    // bounded process this way).
    let manifests = match &case_file {
        Some(f) => vec![f.clone()],
        None => vec![hyd.join("cases.jsonl"), src.join("manifests/cases.jsonl")],
    };
    let mut text = String::new();
    for f in manifests {
        match fs::read_to_string(&f) {
            Ok(t) => text += &t,
            Err(e) => problems.push(format!("{}: {e}", f.display())),
        }
    }
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let case: J = match parse_json(line) {
            Ok(c) => c,
            Err(e) => {
                problems.push(format!("case line {}: {e}", n + 1));
                continue;
            }
        };
        let Some(id) = case["id"].as_str() else {
            problems.push(format!("case line {}: no id", n + 1));
            continue;
        };
        cases += 1;
        if !ids.insert(id.to_string()) {
            problems.push(format!("{id}: duplicate case id"));
        }
        let input = match read_input(&case, &hyd, &src) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{id}: {e}"));
                continue;
            }
        };
        for (kind, list) in [
            ("value", &case["expectations"]["value"]),
            ("typed", &case["expectations"]["typed"]),
        ] {
            for exp in list.as_array().into_iter().flatten() {
                let (Some(profile), Some(rev)) =
                    (exp["profile"].as_str(), exp["profile_revision"].as_u64())
                else {
                    problems.push(format!(
                        "{id}: {kind} expectation without profile and revision"
                    ));
                    continue;
                };
                if EXCLUDED.iter().any(|(p, r, _)| *p == profile && *r == rev) {
                    rows.entry((profile.into(), rev, kind))
                        .or_insert(("excluded", 0, 0));
                    *excluded
                        .entry((profile.to_string(), rev, kind))
                        .or_insert(0) += 1;
                    continue;
                }
                let Some(p) = profiles
                    .iter()
                    .find(|p| p.id == profile && p.revision == rev)
                else {
                    problems.push(format!("{id}: unsupported profile {profile} r{rev}"));
                    continue;
                };
                let limits = checked_profiles
                    .entry((profile.into(), rev))
                    .or_insert_with(|| {
                        check_profile_file(&src, p)
                            .map_err(|e| problems.push(e))
                            .ok()
                    });
                let Some(limits) = limits.clone() else {
                    continue;
                };
                let mut opts = Options::default();
                opts.merge_keys = p.merge;
                opts.limits = limits;
                let (got, want) = match (kind, expected(kind, exp)) {
                    (_, Err(e)) => {
                        problems.push(format!("{id}: malformed {kind} expectation: {e}"));
                        continue;
                    }
                    ("value", Ok(w)) => (run_value(&input, &opts, p.single), w),
                    (_, Ok(w)) => match run_typed(&input, &opts, exp) {
                        Ok(g) => (g, w),
                        Err(e) => {
                            problems.push(format!("{id}: malformed typed expectation: {e}"));
                            continue;
                        }
                    },
                };
                let row = rows
                    .entry((profile.into(), rev, kind))
                    .or_insert((p.status, 0, 0));
                if matches(&got, &want) {
                    row.1 += 1;
                } else {
                    row.2 += 1;
                    failures.push(format!(
                        "{id} [{profile} r{rev} {kind}]\n  want {want}\n  got  {got}"
                    ));
                }
            }
        }
        let ex = &case["expectations"];
        if ex.get("events").is_some() {
            with_events += 1;
            events_only += ["value", "typed", "node_locations"]
                .iter()
                .all(|k| ex.get(*k).is_none()) as usize;
        }
        event_locations += ex.get("locations").is_some() as usize;
        // With `--case-file`, an event-only case is loaded under the profile its ladder stays
        // under (`yamlprims-core` r3, DDR-0009): the check is that loading completes, with a
        // value or an error, inside the process's budgets. The outcome is printed.
        if case_file.is_some()
            && ex.get("events").is_some()
            && ["value", "typed", "node_locations"]
                .iter()
                .all(|k| ex.get(*k).is_none())
        {
            let p = profiles
                .iter()
                .find(|p| p.id == "yamlprims-core" && p.revision == 3)
                .expect("listed");
            let limits = checked_profiles.entry((p.id.into(), 3)).or_insert_with(|| {
                check_profile_file(&src, p)
                    .map_err(|e| problems.push(e))
                    .ok()
            });
            let Some(limits) = limits.clone() else {
                continue;
            };
            let mut opts = Options::default();
            opts.limits = limits;
            let loaded = if p.single {
                load_bytes(&input, &opts).map(|_| 1)
            } else {
                load_all_bytes(&input, &opts).map(|d| d.len())
            };
            let outcome = loaded.map_or_else(|e| error(&e, false), |n| json!({"documents": n}));
            println!("load-only\t{id}\t{outcome}");
            rows.entry((p.id.into(), 3, "load-only"))
                .or_insert((p.status, 0, 0))
                .1 += 1;
        }
        if let Some(nl) = ex.get("node_locations") {
            let (profile, rev) = (
                nl["profile"].as_str().unwrap_or_default(),
                nl["profile_revision"].as_u64().unwrap_or_default(),
            );
            let conv = (
                nl["convention"].as_str().unwrap_or_default(),
                nl["convention_revision"].as_u64().unwrap_or_default(),
            );
            let Some(p) = profiles
                .iter()
                .find(|p| p.id == profile && p.revision == rev)
            else {
                problems.push(format!(
                    "{id}: node_locations for unsupported profile {profile} r{rev}"
                ));
                continue;
            };
            if !SPEC_1_3.contains(&(profile, rev)) {
                problems.push(format!(
                    "{id}: node_locations bound to {profile} r{rev}, which does not cite spec 1.3"
                ));
                continue;
            }
            if conv != ("yamlprims-node", 1) {
                problems.push(format!(
                    "{id}: unsupported node convention {} r{}",
                    conv.0, conv.1
                ));
                continue;
            }
            if let Err(e) = convention.get_or_insert_with(|| check_node_convention(&src)) {
                problems.push(format!("{id}: {e}"));
                continue;
            }
            let limits = checked_profiles
                .entry((profile.into(), rev))
                .or_insert_with(|| {
                    check_profile_file(&src, p)
                        .map_err(|e| problems.push(e))
                        .ok()
                });
            let Some(limits) = limits.clone() else {
                continue;
            };
            let mut opts = Options::default();
            opts.merge_keys = p.merge;
            opts.limits = limits;
            let value =
                ex["value"].as_array().into_iter().flatten().find(|e| {
                    e["profile"] == json!(profile) && e["profile_revision"] == json!(rev)
                });
            match node_spans(&input, &opts, p.single, value, nl) {
                Err(e) => problems.push(format!("{id}: malformed node_locations: {e}")),
                Ok(results) => {
                    for (at, got, want) in results {
                        let row = rows
                            .entry((profile.into(), rev, "node_locations"))
                            .or_insert((p.status, 0, 0));
                        if matches(&got, &want) {
                            row.1 += 1;
                        } else {
                            row.2 += 1;
                            failures.push(format!("{id} [{profile} r{rev} node_locations {at}]\n  want {want}\n  got  {got}"));
                        }
                    }
                }
            }
        }
    }

    println!("profile\trevision\tkind\tstatus\tpass\tfail\texcluded");
    for ((p, r, k), (status, pass, fail)) in &rows {
        let x = excluded.get(&(p.clone(), *r, *k)).copied().unwrap_or(0);
        println!("{p}\t{r}\t{k}\t{status}\t{pass}\t{fail}\t{x}");
    }
    // Format-level events (and event-keyed locations) are bound to no profile; this driver
    // does not compare them. Counted so that event-only cases are visible, not silent.
    println!("not-compared\tevents\t{with_events} cases\t{events_only} with no other expectation\t{event_locations} with event locations");
    for f in failures.iter().take(details) {
        println!("{f}");
    }
    if failures.len() > details {
        println!("... {} more failures", failures.len() - details);
    }
    let ran: usize = rows.values().map(|(_, p, f)| p + f).sum();
    if cases == 0 || ran == 0 {
        problems.push(format!(
            "empty inventory: {cases} cases, {ran} expectations run"
        ));
    }
    if !failures.is_empty() {
        problems.push(format!("{} expectations failed", failures.len()));
    }
    for p in problems.iter().take(details) {
        println!("problem\t{p}");
    }
    if case_file.is_some() {
        // Peak resident memory of this process (Linux), measured after exec, so it excludes
        // the parent that started it.
        let hwm = fs::read_to_string("/proc/self/status").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .map(|l| l.split_whitespace().nth(1).unwrap_or("?").to_string())
        });
        println!("peak_rss_kib\t{}", hwm.unwrap_or_else(|| "unknown".into()));
    }
    if problems.is_empty() {
        println!("result\tPASS\t{cases} cases\t{ran} expectations");
        ExitCode::SUCCESS
    } else {
        println!("result\tFAIL\t{} problems", problems.len());
        ExitCode::FAILURE
    }
}

/// Parse a JSON record, allowing up to 1024 levels of container nesting so 255-deep
/// `yaml-value/1` expectations in `yamlprims-core-depth255` r1 do not trip `serde_json`'s
/// default 128-level deserializer recursion cap.
fn parse_json(s: &str) -> Result<J, String> {
    match serde_json::from_str(s) {
        Ok(v) => Ok(v),
        Err(e) if e.to_string().contains("recursion limit exceeded") => {
            let mut rest = trim_json(s);
            let v = parse_deep_json(&mut rest, 0)?;
            if !trim_json(rest).is_empty() {
                return Err("trailing characters after JSON value".into());
            }
            Ok(v)
        }
        Err(e) => Err(e.to_string()),
    }
}

fn is_json_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

fn trim_json(s: &str) -> &str {
    s.trim_matches(is_json_ws)
}

fn trim_json_start(s: &str) -> &str {
    s.trim_start_matches(is_json_ws)
}

fn parse_deep_json(s: &mut &str, depth: usize) -> Result<J, String> {
    if depth > 1024 {
        return Err("JSON nesting exceeds 1024".into());
    }
    *s = trim_json_start(s);
    match s.as_bytes().first().copied() {
        None => Err("unexpected EOF".into()),
        Some(b'{') => {
            *s = trim_json_start(&s[1..]);
            let mut map = serde_json::Map::new();
            if let Some(after) = s.strip_prefix('}') {
                *s = after;
                return Ok(J::Object(map));
            }
            loop {
                let k = parse_json_str(s)?;
                *s = trim_json_start(s);
                *s = s.strip_prefix(':').ok_or("expected ':'")?;
                let v = parse_deep_json(s, depth + 1)?;
                map.insert(k, v);
                *s = trim_json_start(s);
                if let Some(after) = s.strip_prefix(',') {
                    *s = trim_json_start(after);
                } else if let Some(after) = s.strip_prefix('}') {
                    *s = after;
                    return Ok(J::Object(map));
                } else {
                    return Err("expected ',' or '}'".into());
                }
            }
        }
        Some(b'[') => {
            *s = trim_json_start(&s[1..]);
            let mut items = Vec::new();
            if let Some(after) = s.strip_prefix(']') {
                *s = after;
                return Ok(J::Array(items));
            }
            loop {
                items.push(parse_deep_json(s, depth + 1)?);
                *s = trim_json_start(s);
                if let Some(after) = s.strip_prefix(',') {
                    *s = trim_json_start(after);
                } else if let Some(after) = s.strip_prefix(']') {
                    *s = after;
                    return Ok(J::Array(items));
                } else {
                    return Err("expected ',' or ']'".into());
                }
            }
        }
        Some(b'"') => parse_json_str(s).map(J::String),
        Some(_) => {
            let end = s
                .find(|c: char| matches!(c, ',' | ']' | '}' | ' ' | '\t' | '\n' | '\r'))
                .unwrap_or(s.len());
            let (tok, rest) = s.split_at(end);
            *s = rest;
            serde_json::from_str(tok).map_err(|e| e.to_string())
        }
    }
}

fn parse_json_str(s: &mut &str) -> Result<String, String> {
    *s = trim_json_start(s);
    if !s.starts_with('"') {
        return Err("expected string".into());
    }
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => {
                let (tok, rest) = s.split_at(i + 1);
                *s = rest;
                return serde_json::from_str(tok).map_err(|e| e.to_string());
            }
            _ => i += 1,
        }
    }
    Err("unterminated string".into())
}

/// The profile file must exist and its `policy` must be exactly what the driver implements.
/// Returns the limits to run with: exactly the profile's twelve, or the defaults.
fn check_profile_file(src: &Path, p: &Profile) -> Result<Limits, String> {
    let f = src.join(format!("manifests/profiles/{}.r{}.json", p.id, p.revision));
    let text = fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    let doc: J = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", f.display()))?;
    let want: J = serde_json::from_str(p.policy).expect("driver policy is JSON");
    if doc["id"] != json!(p.id) || doc["revision"] != json!(p.revision) || doc["policy"] != want {
        return Err(format!(
            "{}: policy {} differs from the driver's {}",
            f.display(),
            doc["policy"],
            want
        ));
    }
    match (doc.get("limits"), p.limits) {
        (None, false) => Ok(Limits::default()),
        (Some(l), true) => limits(l).map_err(|e| format!("{}: limits: {e}", f.display())),
        (l, _) => Err(format!(
            "{}: limits {} where the driver expects {}",
            f.display(),
            l.is_some(),
            p.limits
        )),
    }
}

/// The node convention file must define exactly what the driver compares.
fn check_node_convention(src: &Path) -> Result<(), String> {
    let f = src.join("manifests/span-conventions/yamlprims-node.r1.json");
    let doc: J = fs::read_to_string(&f)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .map_err(|e| format!("{}: {e}", f.display()))?;
    let want = (
        json!("yamlprims-node"),
        json!(1),
        json!("utf8-byte"),
        json!("half-open"),
        json!("value-node"),
        json!(NODE_KINDS),
    );
    let got = (
        doc["id"].clone(),
        doc["revision"].clone(),
        doc["offset_unit"].clone(),
        doc["interval"].clone(),
        doc["selects"].clone(),
        doc["defined_nodes"].clone(),
    );
    if got != want || doc.get("defined_events").is_some() {
        return Err(format!("{}: convention differs from the driver's (yamlprims-node r1, utf8-byte, half-open, value-node, {NODE_KINDS:?})", f.display()));
    }
    Ok(())
}

/// Node kinds `yamlprims-node` r1 defines spans for.
const NODE_KINDS: [&str; 4] = ["null", "str", "map", "tagged"];

/// For each span: where it is, the observed node kind and span, and the expected ones. The
/// path must resolve in the same profile's expected value and end at a defined node kind, so
/// a malformed record fails as malformed rather than as a span mismatch.
fn node_spans(
    input: &[u8],
    opts: &Options,
    single: bool,
    value: Option<&J>,
    nl: &J,
) -> Result<Vec<(String, J, J)>, String> {
    let want_docs = value
        .and_then(|v| v["documents"].as_array())
        .ok_or("no value expectation with documents for this profile")?;
    let spans = nl["spans"]
        .as_array()
        .filter(|s| !s.is_empty())
        .ok_or("no spans")?;
    let docs = if single {
        load_bytes(input, opts).map(|d| vec![d])
    } else {
        load_all_bytes(input, opts)
    };
    let mut out = Vec::new();
    for s in spans {
        let d = s["document"]
            .as_u64()
            .ok_or("span without a document index")? as usize;
        let steps = s["path"].as_array().ok_or("span without a path")?;
        let (Some(start), Some(end)) = (s["start"].as_u64(), s["end"].as_u64()) else {
            return Err("span without start and end".into());
        };
        if start > end || end > input.len() as u64 {
            return Err(format!("span [{start}, {end}) outside the input"));
        }
        let mut w = want_docs
            .get(d)
            .ok_or("document index outside the expected value")?;
        for st in steps {
            w = expected_step(w, st)?;
        }
        let kind = w["t"].as_str().ok_or("expected node without a kind")?;
        if !NODE_KINDS.contains(&kind) {
            return Err(format!("node kind {kind} has no span in yamlprims-node r1"));
        }
        let at = format!("document {d} path {}", J::Array(steps.clone()));
        let got = match &docs {
            Err(e) => error(e, false),
            Ok(docs) => match docs
                .get(d)
                .and_then(|doc| steps.iter().try_fold(doc.root(), loaded_step))
            {
                None => json!({"missing": at}),
                Some(n) => match n.span() {
                    NodeSpan::Source(sp) => {
                        json!({"t": notation(n)["t"], "start": sp.start.byte_offset, "end": sp.end.byte_offset})
                    }
                    NodeSpan::Synthetic => json!({"t": notation(n)["t"], "span": "synthetic"}),
                },
            },
        };
        out.push((at, got, json!({"t": kind, "start": start, "end": end})));
    }
    Ok(out)
}

/// One path step: `{"key": i}`, `{"value": i}`, `{"item": i}` or `{"inner": true}`.
fn step(st: &J) -> Result<(&str, usize), String> {
    let o = st
        .as_object()
        .filter(|o| o.len() == 1)
        .ok_or_else(|| format!("path step {st} is not a single step"))?;
    let (k, v) = o.iter().next().unwrap();
    match (k.as_str(), v) {
        ("key" | "value" | "item", J::Number(n)) => Ok((
            k,
            n.as_u64()
                .ok_or_else(|| format!("path step {st}: bad index"))? as usize,
        )),
        ("inner", J::Bool(true)) => Ok(("inner", 0)),
        _ => Err(format!("unknown path step {st}")),
    }
}

fn expected_step<'a>(w: &'a J, st: &J) -> Result<&'a J, String> {
    let (k, i) = step(st)?;
    let r = match (k, w["t"].as_str()) {
        ("key", Some("map")) => w["entries"].get(i).map(|e| &e[0]),
        ("value", Some("map")) => w["entries"].get(i).map(|e| &e[1]),
        ("item", Some("seq")) => w["items"].get(i),
        ("inner", Some("tagged")) => w.get("inner"),
        _ => None,
    };
    r.ok_or_else(|| format!("path step {st} does not resolve in the expected value"))
}

fn loaded_step<'a>(n: &'a Node, st: &J) -> Option<&'a Node> {
    let (k, i) = step(st).ok()?;
    match (k, n.value()) {
        ("key", Value::Mapping(m)) => m.iter().nth(i).map(|(k, _)| k),
        ("value", Value::Mapping(m)) => m.iter().nth(i).map(|(_, v)| v),
        ("item", Value::Sequence(s)) => s.get(i),
        ("inner", Value::Tagged(t)) => Some(t.value()),
        _ => None,
    }
}

/// Exactly the twelve §6 limit names, each a positive integer, valid together.
fn limits(l: &J) -> Result<Limits, String> {
    let obj = l.as_object().ok_or("not an object")?;
    let mut out = Limits::default();
    let fields: [(&str, &mut usize); 12] = [
        ("max_input_bytes", &mut out.max_input_bytes),
        ("max_depth", &mut out.max_depth),
        ("max_nodes", &mut out.max_nodes),
        ("max_expanded_nodes", &mut out.max_expanded_nodes),
        ("max_alias_events", &mut out.max_alias_events),
        ("max_owned_bytes", &mut out.max_owned_bytes),
        ("max_scalar_bytes", &mut out.max_scalar_bytes),
        ("max_key_bytes", &mut out.max_key_bytes),
        ("max_mapping_entries", &mut out.max_mapping_entries),
        ("max_documents", &mut out.max_documents),
        ("max_output_bytes", &mut out.max_output_bytes),
        ("max_warnings", &mut out.max_warnings),
    ];
    let names: HashSet<&str> = fields.iter().map(|(n, _)| *n).collect();
    if let Some(k) = obj.keys().find(|k| !names.contains(k.as_str())) {
        return Err(format!("unknown limit {k}"));
    }
    for (name, slot) in fields {
        let v = obj.get(name).ok_or_else(|| format!("missing {name}"))?;
        *slot = v
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| format!("{name} is not a non-negative integer"))?;
    }
    let mut o = Options::default();
    o.limits = out.clone();
    o.validate().map_err(|e| e.message.clone())?;
    Ok(out)
}

/// The expected result, or why the expectation is malformed.
fn expected(kind: &str, exp: &J) -> Result<J, String> {
    if let Some(e) = exp.get("error") {
        let phase = e["phase"].as_str().ok_or("error without phase")?;
        if ![
            "input",
            "syntax",
            "resolution",
            "structure",
            "limit",
            "typed",
        ]
        .contains(&phase)
        {
            return Err(format!("unknown phase {phase}"));
        }
        if phase == "limit" && e.get("limit").is_none() {
            return Err("limit error without a limit name".into());
        }
        return Ok(json!({"error": e}));
    }
    match kind {
        "value" => exp
            .get("documents")
            .filter(|d| d.is_array())
            .map(|d| json!({"documents": d}))
            .ok_or_else(|| "no documents or error".into()),
        _ => exp
            .get("result")
            .filter(|r| r.is_object())
            .cloned()
            .ok_or_else(|| "no result or error".into()),
    }
}

fn read_input(case: &J, hyd: &Path, src: &Path) -> Result<Vec<u8>, String> {
    let input = &case["input"];
    let path = match input["path"].as_str() {
        Some(p) if hyd.join(p).exists() => hyd.join(p),
        Some(p) => src.join(p),
        None => {
            // Recipe cases: inputs/yaml/<recipe id>/<case suffix>.yaml
            let id = case["id"].as_str().unwrap_or_default();
            let (recipe, n) = id
                .rsplit_once('.')
                .ok_or("recipe case id without a suffix")?;
            hyd.join("inputs/yaml")
                .join(recipe)
                .join(format!("{n}.yaml"))
        }
    };
    let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if Some(bytes.len() as u64) != input["bytes"].as_u64() {
        return Err(format!(
            "{}: {} bytes, record says {}",
            path.display(),
            bytes.len(),
            input["bytes"]
        ));
    }
    Ok(bytes)
}

fn run_value(input: &[u8], opts: &Options, single: bool) -> J {
    let docs = if single {
        load_bytes(input, opts).map(|d| vec![d])
    } else {
        load_all_bytes(input, opts)
    };
    match docs {
        Ok(docs) => {
            let want_docs = docs.iter().map(|d| notation(d.root())).collect::<Vec<_>>();
            let mut emit_opts = EmitOptions::default();
            emit_opts.limits.max_depth = opts.limits.max_depth;
            emit_opts.limits.max_nodes = opts.limits.max_nodes.max(opts.limits.max_expanded_nodes);
            emit_opts.limits.max_scalar_bytes = opts.limits.max_scalar_bytes;
            emit_opts.limits.max_key_bytes = opts.limits.max_key_bytes;
            emit_opts.limits.max_mapping_entries = opts.limits.max_mapping_entries;
            emit_opts.limits.max_tag_bytes = opts.limits.max_scalar_bytes.max(1024);
            emit_opts.limits.max_documents = opts.limits.max_documents;
            emit_opts.limits.max_output_bytes = opts
                .limits
                .max_input_bytes
                .saturating_mul(8)
                .max(opts.limits.max_scalar_bytes.saturating_mul(4))
                .max(opts.limits.max_key_bytes.saturating_mul(4))
                .max(16 * 1024 * 1024);
            let emitted = match emit_all_with(&docs, &emit_opts) {
                Ok(s) => s,
                Err(e) => return json!({"emit_error": e.to_string()}),
            };
            let mut reparse_opts = opts.clone();
            reparse_opts.merge_keys = MergeKeys::AsKey;
            reparse_opts.limits.max_input_bytes = emitted
                .len()
                .max(opts.limits.max_input_bytes)
                .max(16 * 1024 * 1024);
            reparse_opts.limits.max_owned_bytes = reparse_opts
                .limits
                .max_owned_bytes
                .max(reparse_opts.limits.max_input_bytes);
            reparse_opts.limits.max_nodes = reparse_opts
                .limits
                .max_nodes
                .max(reparse_opts.limits.max_expanded_nodes);
            let reparsed = match load_all_bytes(emitted.as_bytes(), &reparse_opts) {
                Ok(d) => d,
                Err(e) => return json!({"reparse_error": e.to_string(), "emitted": emitted}),
            };
            let reparsed_docs = reparsed
                .iter()
                .map(|d| notation(d.root()))
                .collect::<Vec<_>>();
            if reparsed_docs != want_docs {
                return json!({
                    "roundtrip_mismatch": {
                        "emitted": emitted,
                        "reparsed": reparsed_docs,
                        "original": want_docs,
                    }
                });
            }
            json!({"documents": want_docs})
        }
        Err(e) => error(&e, false),
    }
}

/// The typed result, or why the expectation cannot be run (a malformed path or target).
fn run_typed(input: &[u8], opts: &Options, exp: &J) -> Result<J, String> {
    let doc = match load_bytes(input, opts) {
        Ok(d) => d,
        Err(e) => return Ok(error(&e, false)),
    };
    let mut n: &Node = doc.root();
    for seg in exp["path"].as_array().ok_or("no path")? {
        n = match (seg, n.value()) {
            (J::String(k), Value::Mapping(m)) => {
                m.get(k).ok_or_else(|| format!("path key {seg} missing"))?
            }
            (J::Number(i), Value::Sequence(s)) => i
                .as_u64()
                .and_then(|i| s.get(i as usize))
                .ok_or_else(|| format!("path index {seg} missing"))?,
            _ => return Err(format!("path {seg} does not apply")),
        };
    }
    let r: Result<J, Error> = match exp["target"].as_str().ok_or("no target")? {
        "bool" => from_node::<bool>(n, opts).map(|v| json!({"t": "bool", "v": v})),
        "option-bool" => from_node::<Option<bool>>(n, opts)
            .map(|v| v.map_or(json!({"t": "none"}), |v| json!({"t": "bool", "v": v}))),
        "string" => from_node::<String>(n, opts).map(|v| json!({"t": "str", "v": v})),
        "u8" => from_node::<u8>(n, opts).map(int),
        "u16" => from_node::<u16>(n, opts).map(int),
        "u32" => from_node::<u32>(n, opts).map(int),
        "u64" => from_node::<u64>(n, opts).map(int),
        "u128" => from_node::<u128>(n, opts).map(int),
        "i8" => from_node::<i8>(n, opts).map(int),
        "i16" => from_node::<i16>(n, opts).map(int),
        "i32" => from_node::<i32>(n, opts).map(int),
        "i64" => from_node::<i64>(n, opts).map(int),
        "i128" => from_node::<i128>(n, opts).map(int),
        "f32" => from_node::<f32>(n, opts)
            .map(|v| json!({"t": "float32", "bits": format!("{:08x}", v.to_bits())})),
        "f64" => from_node::<f64>(n, opts)
            .map(|v| json!({"t": "float", "bits": format!("{:016x}", v.to_bits())})),
        t => return Err(format!("unknown target {t}")),
    };
    Ok(r.unwrap_or_else(|e| error(&e, true)))
}

fn int(v: impl std::fmt::Display) -> J {
    json!({"t": "int", "v": v.to_string()})
}

fn notation(n: &Node) -> J {
    match n.value() {
        Value::Null => json!({"t": "null"}),
        Value::Bool(b) => json!({"t": "bool", "v": b}),
        Value::Int(i) => json!({"t": "int", "v": i.to_string()}),
        Value::Float(f) => {
            let bits = if f.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                f.to_bits()
            };
            json!({"t": "float", "bits": format!("{bits:016x}")})
        }
        Value::String(s) => json!({"t": "str", "v": s}),
        Value::Sequence(items) => {
            json!({"t": "seq", "items": items.iter().map(notation).collect::<Vec<_>>()})
        }
        Value::Mapping(m) => {
            json!({"t": "map", "entries": m.iter().map(|(k, v)| json!([notation(k), notation(v)])).collect::<Vec<_>>()})
        }
        Value::Tagged(t) => {
            json!({"t": "tagged", "tag": t.tag().uri(), "inner": notation(t.value())})
        }
        v => panic!("no yaml-value/1 notation for {}", v.kind_name()),
    }
}

/// DDR-0002 phase of an error. Conversion errors from `from_node` are `typed`. The raw input
/// size is checked at the input gate, before UTF-8 validation, so `max_input_bytes` is phase
/// `input` like invalid UTF-8, not `limit`.
fn error(e: &Error, typed: bool) -> J {
    let kind = e.kind.name();
    let phase = match &e.kind {
        _ if typed => "typed",
        ErrorKind::InvalidUtf8 { .. }
        | ErrorKind::LimitExceeded {
            limit: LimitKind::MaxInputBytes,
            ..
        } => "input",
        ErrorKind::Syntax(_) => "syntax",
        ErrorKind::LimitExceeded { .. } => "limit",
        ErrorKind::DuplicateKey
        | ErrorKind::InvalidKey
        | ErrorKind::UnknownAnchor
        | ErrorKind::RecursiveAlias
        | ErrorKind::MergeRejected
        | ErrorKind::MergeSource
        | ErrorKind::MultipleDocuments => "structure",
        _ => "resolution",
    };
    let mut out = json!({"phase": phase, "kind": kind});
    if let ErrorKind::LimitExceeded { limit, configured } = &e.kind {
        out["limit"] = json!(limit.field_name());
        out["configured"] = json!(configured);
    }
    json!({"error": out})
}

/// Exact match, except that an expected error is compared only on the members it names.
fn matches(got: &J, want: &J) -> bool {
    match (got.get("error"), want.get("error")) {
        (Some(g), Some(w)) => w
            .as_object()
            .unwrap()
            .iter()
            .all(|(k, v)| g.get(k) == Some(v)),
        (None, None) => got == want,
        _ => false,
    }
}

fn run_stdin(args: &[String]) -> ExitCode {
    let mut format = "wire";
    let mut single = false;
    let mut opts = Options::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--format" if i + 1 < args.len() => {
                format = args[i + 1].as_str();
                i += 1;
            }
            "--single" => {
                single = true;
            }
            "--merge" if i + 1 < args.len() => {
                opts.merge_keys = match args[i + 1].as_str() {
                    "apply" => MergeKeys::Apply,
                    "as-key" | "plain-key" => MergeKeys::AsKey,
                    _ => MergeKeys::Reject,
                };
                i += 1;
            }
            "--unknown-tags" if i + 1 < args.len() => {
                opts.unknown_tags = match args[i + 1].as_str() {
                    "reject" => UnknownTags::Reject,
                    _ => UnknownTags::Preserve,
                };
                i += 1;
            }
            "--limits" if i + 1 < args.len() => {
                for kv in args[i + 1].split(',') {
                    let Some((k, v)) = kv.split_once('=') else {
                        continue;
                    };
                    let Ok(n) = v.parse::<usize>() else {
                        return ExitCode::from(2);
                    };
                    match k {
                        "max_input_bytes" => opts.limits.max_input_bytes = n,
                        "max_depth" => opts.limits.max_depth = n,
                        "max_nodes" => opts.limits.max_nodes = n,
                        "max_expanded_nodes" => opts.limits.max_expanded_nodes = n,
                        "max_alias_events" => opts.limits.max_alias_events = n,
                        "max_owned_bytes" => opts.limits.max_owned_bytes = n,
                        "max_scalar_bytes" => opts.limits.max_scalar_bytes = n,
                        "max_key_bytes" => opts.limits.max_key_bytes = n,
                        "max_mapping_entries" => opts.limits.max_mapping_entries = n,
                        "max_documents" => opts.limits.max_documents = n,
                        "max_output_bytes" => opts.limits.max_output_bytes = n,
                        "max_warnings" => opts.limits.max_warnings = n,
                        _ => {}
                    }
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let mut input = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut input) {
        eprintln!("{e}");
        return ExitCode::from(2);
    }

    let def_opts = Options::default();
    match format {
        "wire" => {
            let loaded = if single {
                load_bytes(&input, &opts).map(|d| vec![d])
            } else {
                load_all_bytes(&input, &opts)
            };
            match loaded {
                Err(e) => match encode_error(&e, &def_opts) {
                    Ok(out) => {
                        println!("{out}");
                        ExitCode::SUCCESS
                    }
                    Err(e2) => {
                        eprintln!("{e2}");
                        ExitCode::from(2)
                    }
                },
                Ok(docs) => match encode(&docs, &opts) {
                    Ok(out) => {
                        println!("{out}");
                        ExitCode::SUCCESS
                    }
                    Err(enc_err) => match encode_error(&enc_err, &def_opts) {
                        Ok(out) => {
                            println!("{out}");
                            ExitCode::SUCCESS
                        }
                        Err(e2) => {
                            eprintln!("{e2}");
                            ExitCode::from(2)
                        }
                    },
                },
            }
        }
        "plain" => match load_bytes(&input, &opts) {
            Err(e) => match encode_error(&e, &def_opts) {
                Ok(out) => {
                    println!("{out}");
                    ExitCode::SUCCESS
                }
                Err(e2) => {
                    eprintln!("{e2}");
                    ExitCode::from(2)
                }
            },
            Ok(doc) => match encode_plain(doc.root(), &opts) {
                Ok(out) => {
                    println!("{out}");
                    ExitCode::SUCCESS
                }
                Err(perr) => match encode_error(&perr, &def_opts) {
                    Ok(out) => {
                        println!("{out}");
                        ExitCode::SUCCESS
                    }
                    Err(e2) => {
                        eprintln!("{e2}");
                        ExitCode::from(2)
                    }
                },
            },
        },
        _ => {
            eprintln!("unknown format {format}");
            ExitCode::from(2)
        }
    }
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("odd hex length".into());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let b = u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string())?;
        out.push(b);
    }
    Ok(out)
}

fn run_crosscheck_jsonl() -> ExitCode {
    let mut text = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut text) {
        eprintln!("{e}");
        return ExitCode::from(2);
    }
    let def_opts = Options::default();
    for (n, line) in text.split('\n').enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let rec: J = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("line {}: {e}", n + 1);
                return ExitCode::from(2);
            }
        };
        let id = rec["id"].as_str().unwrap_or("");
        if let Some(code) = rec["syntax_code"].as_str() {
            let Some(sk) = yamlprims::SyntaxKind::from_code(code) else {
                eprintln!("{id}: unknown syntax_code {code}");
                return ExitCode::from(2);
            };
            let err = Error::new(
                ErrorKind::syntax(sk),
                yamlprims::Site::Source(yamlprims::Location {
                    byte_offset: 0,
                    line: 1,
                    column: 1,
                }),
                format!("syntax error: {code}"),
            );
            let wire = encode_error(&err, &def_opts).expect("encode syntax error");
            println!(
                "{}",
                json!({
                    "id": id,
                    "wire": wire,
                    "roundtrip": wire,
                    "plain": "",
                    "emit": "",
                    "ijson": "",
                })
            );
            continue;
        }
        let input = if let Some(hex) = rec["input_hex"].as_str() {
            match decode_hex(hex) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{id}: {e}");
                    return ExitCode::from(2);
                }
            }
        } else if let Some(s) = rec["input"].as_str() {
            s.as_bytes().to_vec()
        } else {
            eprintln!("{id}: missing input or input_hex");
            return ExitCode::from(2);
        };
        let single = rec["single"].as_bool().unwrap_or(false);
        let check_plain = rec["check_plain"].as_bool().unwrap_or(false);
        let mut opts = Options::default();
        if let Some(m) = rec["merge"].as_str() {
            opts.merge_keys = match m {
                "apply" => MergeKeys::Apply,
                "as-key" | "plain-key" => MergeKeys::AsKey,
                _ => MergeKeys::Reject,
            };
        }
        if let Some(u) = rec["unknown_tags"].as_str() {
            opts.unknown_tags = match u {
                "reject" => UnknownTags::Reject,
                _ => UnknownTags::Preserve,
            };
        }
        if let Some(lim_obj) = rec.get("limits").and_then(|l| l.as_object()) {
            for (k, v) in lim_obj {
                let Some(n) = v.as_u64().map(|x| x as usize) else {
                    continue;
                };
                match k.as_str() {
                    "max_input_bytes" => opts.limits.max_input_bytes = n,
                    "max_depth" => opts.limits.max_depth = n,
                    "max_nodes" => opts.limits.max_nodes = n,
                    "max_expanded_nodes" => opts.limits.max_expanded_nodes = n,
                    "max_alias_events" => opts.limits.max_alias_events = n,
                    "max_owned_bytes" => opts.limits.max_owned_bytes = n,
                    "max_scalar_bytes" => opts.limits.max_scalar_bytes = n,
                    "max_key_bytes" => opts.limits.max_key_bytes = n,
                    "max_mapping_entries" => opts.limits.max_mapping_entries = n,
                    "max_documents" => opts.limits.max_documents = n,
                    "max_output_bytes" => opts.limits.max_output_bytes = n,
                    "max_warnings" => opts.limits.max_warnings = n,
                    _ => {}
                }
            }
        }
        let mut rt_opts = def_opts.clone();
        if opts.validate().is_ok() {
            rt_opts.limits.max_input_bytes = rt_opts
                .limits
                .max_input_bytes
                .max(opts.limits.max_input_bytes);
            rt_opts.limits.max_depth = rt_opts.limits.max_depth.max(opts.limits.max_depth);
            rt_opts.limits.max_nodes = rt_opts.limits.max_nodes.max(opts.limits.max_nodes);
            rt_opts.limits.max_expanded_nodes = rt_opts
                .limits
                .max_expanded_nodes
                .max(opts.limits.max_expanded_nodes);
            rt_opts.limits.max_alias_events = rt_opts
                .limits
                .max_alias_events
                .max(opts.limits.max_alias_events);
            rt_opts.limits.max_owned_bytes = rt_opts
                .limits
                .max_owned_bytes
                .max(opts.limits.max_owned_bytes);
            rt_opts.limits.max_scalar_bytes = rt_opts
                .limits
                .max_scalar_bytes
                .max(opts.limits.max_scalar_bytes);
            rt_opts.limits.max_key_bytes =
                rt_opts.limits.max_key_bytes.max(opts.limits.max_key_bytes);
            rt_opts.limits.max_mapping_entries = rt_opts
                .limits
                .max_mapping_entries
                .max(opts.limits.max_mapping_entries);
            rt_opts.limits.max_documents =
                rt_opts.limits.max_documents.max(opts.limits.max_documents);
            rt_opts.limits.max_output_bytes = rt_opts
                .limits
                .max_output_bytes
                .max(opts.limits.max_output_bytes);
            rt_opts.limits.max_warnings = rt_opts.limits.max_warnings.max(opts.limits.max_warnings);
        }
        let loaded = if single {
            load_bytes(&input, &opts).map(|d| vec![d])
        } else {
            load_all_bytes(&input, &opts)
        };
        let wire = match &loaded {
            Err(e) => encode_error(e, &rt_opts).expect("encode_error"),
            Ok(docs) => match encode(docs, &opts) {
                Ok(out) => out,
                Err(enc_err) => encode_error(&enc_err, &rt_opts).expect("encode_error"),
            },
        };
        let roundtrip = match decode_wire(&wire, &rt_opts) {
            Ok(WireEnvelope::Documents(docs)) => encode(&docs, &rt_opts).expect("rt encode"),
            Ok(WireEnvelope::Error(e)) => encode_error(&e, &rt_opts).expect("rt encode_error"),
            Err(e) => {
                eprintln!("{id}: decode_wire failed: {e}");
                return ExitCode::from(2);
            }
        };
        let plain = if check_plain {
            match load_bytes(&input, &opts) {
                Err(e) => encode_error(&e, &rt_opts).expect("encode_error"),
                Ok(doc) => match encode_plain(doc.root(), &opts) {
                    Ok(out) => out,
                    Err(perr) => encode_error(&perr, &rt_opts).expect("encode_error"),
                },
            }
        } else {
            String::new()
        };
        let emit_out = match &loaded {
            Err(_) => String::new(),
            Ok(docs) => match emit_all_with(docs, &EmitOptions::default()) {
                Ok(out) => out,
                Err(e) => encode_error(&e, &rt_opts).expect("encode_error"),
            },
        };
        let mut ijson_opts = opts.clone();
        ijson_opts.unknown_tags = UnknownTags::Reject;
        let ijson_out = match load_ijson_bytes(&input, &ijson_opts) {
            Ok(out) => out,
            Err(e) => encode_error(&e, &rt_opts).expect("encode_error"),
        };
        println!(
            "{}",
            json!({
                "id": id,
                "wire": wire,
                "roundtrip": roundtrip,
                "plain": plain,
                "emit": emit_out,
                "ijson": ijson_out,
            })
        );
    }
    ExitCode::SUCCESS
}
