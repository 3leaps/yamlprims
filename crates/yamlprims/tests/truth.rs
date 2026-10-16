//! Runs the provisional truth oracle (fixtures/truth) against the implementation.
//! Mismatches are written to $YAMLPRIMS_TRUTH_REPORT (default target/truth-report.tsv).

use serde::Deserialize;
use std::fmt::Write as _;
use yamlprims::{Error, MergeKeys, Node, Options, Value};

fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/truth")
}

fn js(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn unjs(s: &str) -> String {
    serde_json::from_str(s).unwrap()
}

fn render(n: &Node) -> String {
    match n.value() {
        Value::Null => "null".into(),
        Value::Bool(b) => format!("bool:{b}"),
        Value::Int(i) => format!("int:{i}"),
        Value::Float(f) => format!(
            "float:{:016x}",
            if f.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                f.to_bits()
            }
        ),
        Value::String(s) => format!("str:{}", js(s)),
        Value::Tagged(t) => format!("tagged:{}:{}", js(t.tag().uri()), render(t.value())),
        Value::Sequence(s) => format!("seq:{}", s.len()),
        Value::Mapping(m) => format!("map:{}", m.len()),
        _ => "?".into(),
    }
}

fn kind(e: &Error) -> String {
    if matches!(e.kind, yamlprims::ErrorKind::Syntax(_)) {
        "syntax".into()
    } else {
        format!("err:{}", e.kind.name())
    }
}

fn first_key(n: &Node) -> Option<&Node> {
    n.value().as_mapping()?.iter().next().map(|(k, _)| k)
}

fn probe(root: &Node, position: &str) -> Option<String> {
    let n = match position {
        "block_value" | "flow_value" => root.value().as_mapping()?.get("k")?,
        "block_implicit_key" | "block_explicit_key" | "flow_key" => first_key(root)?,
        "block_seq" | "flow_seq" => root.value().as_sequence()?.first()?,
        "top" => root,
        _ => return None,
    };
    Some(render(n))
}

#[derive(Deserialize)]
struct KBool {
    #[allow(dead_code)]
    k: bool,
}
#[derive(Deserialize)]
struct KString {
    #[allow(dead_code)]
    k: String,
}
#[derive(Deserialize)]
struct KOptBool {
    #[allow(dead_code)]
    k: Option<bool>,
}

fn typed<T, F: Fn(T) -> String>(yaml: &str, opts: &Options, f: F) -> String
where
    T: serde::de::DeserializeOwned,
{
    match yamlprims::from_str_with::<T>(yaml, opts) {
        Ok(v) => f(v),
        Err(e) => {
            // A resolution error during load reports its own kind; typed mismatches are "type".
            kind(&e)
        }
    }
}

struct Report {
    rows: usize,
    bad: Vec<String>,
}

impl Report {
    fn check(&mut self, id: &str, what: &str, expected: &str, got: &str, yaml: &str) {
        self.rows += 1;
        if expected != got {
            self.bad
                .push(format!("{id}\t{what}\t{expected}\t{got}\t{}", js(yaml)));
        }
    }
}

fn tsv(name: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(fixtures().join(name)).unwrap();
    text.lines()
        .skip(1)
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect()
}

fn run_booleans(r: &mut Report) {
    let opts = Options::default();
    for row in tsv("booleans.tsv") {
        let (id, pos, yaml, expected) = (&row[0], &row[2], unjs(&row[6]), &row[7]);
        let got = match yamlprims::load_with(&yaml, &opts) {
            Ok(d) => probe(d.root(), pos).unwrap_or_else(|| "probe-failed".into()),
            Err(e) => kind(&e),
        };
        r.check(id, "value", expected, &got, &yaml);
        if row[8] != "-" {
            let b = typed::<KBool, _>(&yaml, &opts, |v| format!("bool:{}", v.k));
            r.check(id, "typed_bool", &row[8], &b, &yaml);
            let s = typed::<KString, _>(&yaml, &opts, |v| format!("str:{}", js(&v.k)));
            r.check(id, "typed_string", &row[9], &s, &yaml);
            let o = typed::<KOptBool, _>(&yaml, &opts, |v| {
                v.k.map_or("none".into(), |b| format!("bool:{b}"))
            });
            r.check(id, "typed_option_bool", &row[10], &o, &yaml);
        }
    }
}

fn run_collisions(r: &mut Report) {
    let opts = Options::default();
    for row in tsv("collisions.tsv") {
        let yaml = unjs(&row[5]);
        let got = match yamlprims::load_with(&yaml, &opts) {
            Ok(_) => "ok".into(),
            Err(e) => kind(&e),
        };
        r.check(&row[0], "collision", &row[6], &got, &yaml);
    }
}

macro_rules! num_target {
    ($yaml:expr, $opts:expr, $t:ty) => {{
        #[derive(Deserialize)]
        struct K {
            k: $t,
        }
        typed::<K, _>($yaml, $opts, |v| format!("int:{}", v.k))
    }};
}

fn run_numbers(r: &mut Report) {
    let opts = Options::default();
    let targets = [
        "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "f32", "f64",
    ];
    for row in tsv("numbers.tsv") {
        let yaml = unjs(&row[4]);
        let got = match yamlprims::load_with(&yaml, &opts) {
            Ok(d) => probe(d.root(), "block_value").unwrap_or_default(),
            Err(e) => kind(&e),
        };
        r.check(&row[0], "value", &row[5], &got, &yaml);
        for (i, t) in targets.iter().enumerate() {
            let y = yaml.as_str();
            let got = match *t {
                "u8" => num_target!(y, &opts, u8),
                "u16" => num_target!(y, &opts, u16),
                "u32" => num_target!(y, &opts, u32),
                "u64" => num_target!(y, &opts, u64),
                "u128" => num_target!(y, &opts, u128),
                "i8" => num_target!(y, &opts, i8),
                "i16" => num_target!(y, &opts, i16),
                "i32" => num_target!(y, &opts, i32),
                "i64" => num_target!(y, &opts, i64),
                "i128" => num_target!(y, &opts, i128),
                "f32" => {
                    #[derive(Deserialize)]
                    struct K {
                        k: f32,
                    }
                    typed::<K, _>(y, &opts, |v| {
                        format!(
                            "f32:{:08x}",
                            if v.k.is_nan() {
                                0x7fc0_0000
                            } else {
                                v.k.to_bits()
                            }
                        )
                    })
                }
                _ => {
                    #[derive(Deserialize)]
                    struct K {
                        k: f64,
                    }
                    typed::<K, _>(y, &opts, |v| {
                        format!(
                            "float:{:016x}",
                            if v.k.is_nan() {
                                0x7ff8_0000_0000_0000
                            } else {
                                v.k.to_bits()
                            }
                        )
                    })
                }
            };
            r.check(&row[0], &format!("typed_{t}"), &row[6 + i], &got, &yaml);
        }
    }
}

fn golden_probe(root: &Node, probe: &str) -> Option<String> {
    if probe == "root" {
        return Some(render(root));
    }
    if probe == "key0" {
        return first_key(root).map(render);
    }
    if let Some(i) = probe.strip_prefix('[').and_then(|p| p.strip_suffix(']')) {
        return root
            .value()
            .as_sequence()?
            .get(i.parse::<usize>().ok()?)
            .map(render);
    }
    let mut n = root;
    for part in probe.split('/') {
        let key = part.strip_prefix("k=")?;
        n = n.value().as_mapping()?.get(key)?;
    }
    Some(render(n))
}

fn keys_of(n: &Node) -> String {
    let m = n.value().as_mapping().unwrap();
    let ks: Vec<String> = m
        .iter()
        .map(|(k, _)| match k.value() {
            Value::String(s) if s == "<<" => "\"<<\"".to_owned(),
            Value::String(s) => s.clone(),
            _ => render(k),
        })
        .collect();
    format!("keys:{}", ks.join(","))
}

fn run_golden(r: &mut Report) {
    for row in tsv("golden.tsv") {
        let (id, yaml, probe, o, expected) = (&row[0], unjs(&row[1]), &row[2], &row[3], &row[4]);
        let mut opts = Options::default();
        opts.merge_keys = match o.as_str() {
            "apply" => MergeKeys::Apply,
            "as_key" => MergeKeys::AsKey,
            _ => MergeKeys::Reject,
        };
        let got = match probe.as_str() {
            "typed_u16" => {
                #[derive(Deserialize)]
                struct K {
                    k: u16,
                }
                typed::<K, _>(&yaml, &opts, |v| format!("int:{}", v.k))
            }
            "typed_bool" => typed::<KBool, _>(&yaml, &opts, |v| format!("bool:{}", v.k)),
            "typed_string" => typed::<KString, _>(&yaml, &opts, |v| format!("str:{}", js(&v.k))),
            "typed_option_bool" => typed::<KOptBool, _>(&yaml, &opts, |v| {
                v.k.map_or("none".into(), |b| format!("bool:{b}"))
            }),
            _ => match yamlprims::load_with(&yaml, &opts) {
                Err(e) => kind(&e),
                Ok(d) if expected == "ok" && probe == "root" => {
                    let _ = d;
                    "ok".into()
                }
                Ok(d) if expected.starts_with("keys:") => {
                    let mut n = d.root();
                    for part in probe.split('/') {
                        n = n
                            .value()
                            .as_mapping()
                            .and_then(|m| m.get(part.strip_prefix("k=").unwrap_or(part)))
                            .unwrap_or(n);
                    }
                    keys_of(n)
                }
                Ok(d) => golden_probe(d.root(), probe).unwrap_or_else(|| "probe-failed".into()),
            },
        };
        r.check(id, probe, expected, &got, &yaml);
    }
}

#[test]
fn truth_tables() {
    let mut out = String::new();
    let mut total_bad = 0;
    let mut summary = String::new();
    for (name, f) in [
        ("golden", run_golden as fn(&mut Report)),
        ("booleans", run_booleans),
        ("collisions", run_collisions),
        ("numbers", run_numbers),
    ] {
        let mut r = Report {
            rows: 0,
            bad: Vec::new(),
        };
        f(&mut r);
        let _ = writeln!(
            summary,
            "{name}: {} checks, {} mismatches",
            r.rows,
            r.bad.len()
        );
        total_bad += r.bad.len();
        for b in r.bad {
            let _ = writeln!(out, "{name}\t{b}");
        }
    }
    let path = std::env::var("YAMLPRIMS_TRUTH_REPORT")
        .unwrap_or_else(|_| format!("{}/truth-report.tsv", env!("CARGO_TARGET_TMPDIR")));
    std::fs::write(
        &path,
        format!("table\tid\tcheck\texpected\tgot\tyaml\n{out}"),
    )
    .unwrap();
    std::fs::write(format!("{path}.summary"), &summary).unwrap();
    eprintln!("{summary}");
    assert_eq!(total_bad, 0, "truth-table mismatches; see {path}");
}
