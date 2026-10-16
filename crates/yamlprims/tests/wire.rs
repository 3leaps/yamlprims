//! Immutable wire regression fixtures (spec §11). Expectation changes require review.

use serde::Deserialize;
use yamlprims::{MergeKeys, Node, Options, Value};

fn dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wire")
}

fn input(name: &str) -> Vec<u8> {
    std::fs::read(dir().join(name)).unwrap()
}

fn out(r: Result<String, yamlprims::Error>) -> String {
    match r {
        Ok(s) => s,
        Err(e) => panic!("encoder failed: {e:?}"),
    }
}

fn load_or_error(bytes: &[u8], o: &Options) -> String {
    match yamlprims::load_all_bytes(bytes, o) {
        Ok(d) => out(yamlprims::encode(&d, o)),
        Err(e) => out(yamlprims::encode_error(&e, o)),
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Svc {
    name: String,
    port: u16,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Port {
    port: u16,
}

fn cases() -> Vec<(&'static str, String)> {
    let d = Options::default();
    let mut apply = Options::default();
    apply.merge_keys = MergeKeys::Apply;
    let mut tiny = Options::default();
    tiny.limits.max_output_bytes = 64;

    let typed = match yamlprims::from_str_with::<Svc>(
        std::str::from_utf8(&input("typed-port.yaml")).unwrap(),
        &d,
    ) {
        Ok(_) => panic!("typed-port must fail"),
        Err(e) => out(yamlprims::encode_error(&e, &d)),
    };
    let mut m = yamlprims::Mapping::new();
    m.insert_new(
        Node::synthetic(Value::String("port".into())),
        Node::synthetic(Value::String("x".into())),
    )
    .unwrap();
    let synthetic = match yamlprims::from_node::<Port>(&Node::synthetic(Value::Mapping(m)), &d) {
        Ok(_) => panic!("synthetic must fail"),
        Err(e) => out(yamlprims::encode_error(&e, &d)),
    };
    let docs = yamlprims::load_all_bytes(&input("success.yaml"), &d).unwrap();
    let limit = match yamlprims::encode(&docs, &tiny) {
        Ok(_) => panic!("output limit must trip"),
        Err(e) => out(yamlprims::encode_error(&e, &d)),
    };
    let plain_ok = out(yamlprims::encode_plain(
        yamlprims::load_bytes(&input("plain-ok.yaml"), &d)
            .unwrap()
            .root(),
        &d,
    ));
    let plain_tagged = match yamlprims::encode_plain(docs[0].root(), &d) {
        Ok(_) => panic!("tagged value must be refused"),
        Err(e) => out(yamlprims::encode_error(&e, &d)),
    };
    vec![
        ("success", load_or_error(&input("success.yaml"), &d)),
        (
            "merge-apply",
            load_or_error(&input("merge-apply.yaml"), &apply),
        ),
        (
            "merge-reject",
            load_or_error(&input("merge-apply.yaml"), &d),
        ),
        ("duplicate", load_or_error(&input("duplicate.yaml"), &d)),
        (
            "invalid-utf8",
            load_or_error(&input("invalid-utf8.yaml"), &d),
        ),
        ("typed-port", typed),
        ("synthetic", synthetic),
        ("output-limit", limit),
        ("plain-ok", plain_ok),
        ("plain-tagged", plain_tagged),
    ]
}

#[test]
fn wire_goldens() {
    assert!(
        std::env::var_os("YAMLPRIMS_BLESS").is_none(),
        "golden blessing is disabled"
    );
    let mut bad = Vec::new();
    for (name, got) in cases() {
        // Every output is one JSON value.
        serde_json::from_str::<serde_json::Value>(&got)
            .unwrap_or_else(|e| panic!("{name}: not JSON: {e}"));
        let path = dir().join(format!("{name}.json"));
        let want = std::fs::read_to_string(&path).expect("missing wire fixture");
        if want.trim_end_matches('\n') != got {
            bad.push(format!("{name}\n  want {}\n  got  {got}", want.trim_end()));
        }
    }
    assert!(
        bad.is_empty(),
        "wire golden mismatches:\n{}",
        bad.join("\n")
    );
}
