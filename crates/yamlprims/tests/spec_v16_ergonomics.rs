use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use yamlprims::{
    emit_all_writer, emit_all_writer_with, emit_writer, emit_writer_with, encode_lint, from_reader,
    from_reader_with, from_slice, from_slice_with, lint_yaml11, lint_yaml11_bytes,
    lint_yaml11_reader, lint_yaml11_reader_with, lint_yaml11_with, load_all_reader,
    load_all_reader_with, load_reader, load_reader_with, to_writer, to_writer_with, Document,
    EmitOptions, ErrorKind, LimitKind, LintCode, LintDiagnostic, LintResult, Location, MergeKeys,
    Node, Options, PathSegment, Site, Span, Value,
};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceConfig {
    host: String,
    port: u16,
    enabled: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Mode {
    Fast,
    Tuned(u32),
}

struct CountingReader<'a> {
    data: &'a [u8],
    pos: usize,
    bytes_read: &'a mut usize,
    fail_after: Option<usize>,
}

impl<'a> Read for CountingReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(limit) = self.fail_after {
            if *self.bytes_read >= limit {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "simulated read error",
                ));
            }
        }
        if self.pos >= self.data.len() {
            return Ok(0);
        }
        let n = buf.len().min(self.data.len() - self.pos).min(7);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        *self.bytes_read += n;
        Ok(n)
    }
}

struct TrackingWriter<'a> {
    buf: &'a mut Vec<u8>,
    writes: &'a mut usize,
    fail: bool,
}

impl<'a> Write for TrackingWriter<'a> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        *self.writes += 1;
        if self.fail {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "simulated write error",
            ));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn load_reader_and_load_all_reader_roundtrip_and_bounds() {
    let raw = b"\xEF\xBB\xBFhost: localhost\nport: 8080\nenabled: true\n---\nhost: db\nport: 5432\nenabled: false\n";
    let docs = load_all_reader(&raw[..]).expect("load_all_reader should succeed");
    assert_eq!(docs.len(), 2);

    let single_raw = b"host: localhost\nport: 8080\nenabled: true\n";
    let doc = load_reader(&single_raw[..]).expect("load_reader should succeed");
    assert!(matches!(doc.root().value(), Value::Mapping(_)));

    // Oversized stream stops at max_input_bytes + 1 without consuming the rest of the reader.
    let mut opts = Options::default();
    opts.limits.max_input_bytes = 16;
    let large = vec![b'a'; 1024];
    let mut bytes_read = 0usize;
    let err = load_reader_with(
        CountingReader {
            data: &large,
            pos: 0,
            bytes_read: &mut bytes_read,
            fail_after: None,
        },
        &opts,
    )
    .expect_err("oversized stream must fail");
    assert_eq!(bytes_read, 17);
    assert_eq!(
        err.kind,
        ErrorKind::LimitExceeded {
            limit: LimitKind::MaxInputBytes,
            configured: 16,
        }
    );
    assert_eq!(
        err.site,
        Site::Source(Location {
            byte_offset: 0,
            line: 1,
            column: 1,
        })
    );

    // Invalid options fail before reading any bytes.
    let mut bad_opts = Options::default();
    bad_opts.limits.max_depth = 0;
    let mut bytes_read_bad = 0usize;
    let err = load_all_reader_with(
        CountingReader {
            data: single_raw,
            pos: 0,
            bytes_read: &mut bytes_read_bad,
            fail_after: None,
        },
        &bad_opts,
    )
    .expect_err("invalid options must fail");
    assert_eq!(bytes_read_bad, 0);
    assert_eq!(err.kind, ErrorKind::InvalidOptions);
    assert_eq!(err.site, Site::NotSource { last_known: None });

    // Underlying io::Error maps to ErrorKind::Internal at Site::NotSource { last_known: None }.
    let mut bytes_read_io = 0usize;
    let err = load_reader_with(
        CountingReader {
            data: single_raw,
            pos: 0,
            bytes_read: &mut bytes_read_io,
            fail_after: Some(0),
        },
        &Options::default(),
    )
    .expect_err("io failure must fail");
    assert_eq!(err.kind, ErrorKind::Internal);
    assert_eq!(err.site, Site::NotSource { last_known: None });
}

#[test]
fn from_slice_and_from_reader_preserve_resolver_and_enum_semantics() {
    let raw = b"\xEF\xBB\xBFhost: api.internal\nport: 9090\nenabled: true\n";
    let cfg: ServiceConfig = from_slice(raw).expect("from_slice should succeed");
    assert_eq!(
        cfg,
        ServiceConfig {
            host: "api.internal".to_string(),
            port: 9090,
            enabled: true,
        }
    );

    let cfg_reader: ServiceConfig = from_reader(&raw[..]).expect("from_reader should succeed");
    assert_eq!(cfg_reader, cfg);

    // Local-tag enum payload re-resolution (`!Tuned 7` vs `!Tuned "7"`) works identically
    // through from_slice_with and from_reader_with.
    let m1: Mode = from_slice_with(b"!Tuned 7\n", &Options::default()).unwrap();
    assert_eq!(m1, Mode::Tuned(7));
    let m2: Mode = from_reader_with(&b"!Tuned 7\n"[..], &Options::default()).unwrap();
    assert_eq!(m2, Mode::Tuned(7));

    let err_quoted = from_slice::<Mode>(b"!Tuned \"7\"\n").expect_err("quoted payload must fail");
    assert!(matches!(err_quoted.kind, ErrorKind::Type { .. }));
}

#[test]
fn writer_entrypoints_buffer_and_validate_before_touching_writer() {
    let cfg = ServiceConfig {
        host: "localhost".to_string(),
        port: 8080,
        enabled: true,
    };
    let mut out = Vec::new();
    to_writer(&mut out, &cfg).expect("to_writer should succeed");
    let decoded: ServiceConfig = from_slice(&out).expect("round-trip from_slice should succeed");
    assert_eq!(decoded, cfg);

    let doc = load_reader(&out[..]).unwrap();
    let mut node_out = Vec::new();
    emit_writer(&mut node_out, doc.root()).expect("emit_writer should succeed");
    assert_eq!(node_out, out);

    let mut all_out = Vec::new();
    emit_all_writer(&mut all_out, std::slice::from_ref(&doc))
        .expect("emit_all_writer should succeed");
    assert_eq!(all_out, out);

    // OutputLimitExceeded must write 0 bytes to writer.
    let mut tight_opts = EmitOptions::default();
    tight_opts.limits.max_output_bytes = 128;
    tight_opts.limits.max_scalar_bytes = 128;
    tight_opts.limits.max_key_bytes = 128;
    let mut buf = Vec::new();
    let mut writes = 0usize;
    let many_items: Vec<u32> = (0..100).collect();
    let err = to_writer_with(
        TrackingWriter {
            buf: &mut buf,
            writes: &mut writes,
            fail: false,
        },
        &many_items,
        &tight_opts,
    )
    .expect_err("tight output budget must fail");
    assert_eq!(err.kind, ErrorKind::OutputLimitExceeded { configured: 128 });
    assert_eq!(err.site, Site::NotSource { last_known: None });
    assert_eq!(writes, 0);
    assert!(buf.is_empty());

    // Emit writer I/O error maps to Internal with Site::NotSource { last_known: None }.
    let mut writes_io = 0usize;
    let err_io = emit_writer_with(
        TrackingWriter {
            buf: &mut buf,
            writes: &mut writes_io,
            fail: true,
        },
        &Node::synthetic(Value::String("hello".to_string())),
        &EmitOptions::default(),
    )
    .expect_err("writer failure must fail");
    assert_eq!(err_io.kind, ErrorKind::Internal);
    assert_eq!(err_io.site, Site::NotSource { last_known: None });
    assert_eq!(writes_io, 1);

    let mut writes_all_io = 0usize;
    let err_all_io = emit_all_writer_with(
        TrackingWriter {
            buf: &mut buf,
            writes: &mut writes_all_io,
            fail: true,
        },
        &[Document::new(Node::synthetic(Value::Null))],
        &EmitOptions::default(),
    )
    .expect_err("emit_all_writer failure must fail");
    assert_eq!(err_all_io.kind, ErrorKind::Internal);
    assert_eq!(err_all_io.site, Site::NotSource { last_known: None });
    assert_eq!(writes_all_io, 1);
}

#[test]
fn lint_yaml11_and_encode_lint_contract() {
    let src = "\
flag: yes
ratio: 19:59:59
octal: 0755
quoted: 'yes'
tagged: !custom yes
";
    let res = lint_yaml11(src).expect("lint_yaml11 should succeed");
    assert_eq!(res.warning_count, 3);
    assert!(!res.truncated);
    let codes: Vec<LintCode> = res.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![
            LintCode::Yaml11Boolean,
            LintCode::Yaml11Sexagesimal,
            LintCode::Yaml11Number,
        ]
    );

    let res_bytes = lint_yaml11_bytes(src.as_bytes(), &Options::default()).unwrap();
    assert_eq!(res_bytes, res);
    let res_reader = lint_yaml11_reader(src.as_bytes()).unwrap();
    assert_eq!(res_reader, res);

    // Alias and merge non-amplification + merge_keys modes
    let merge_src = "\
base: &b
  a: yes
alias_copy: *b
merged:
  <<: *b
";
    let mut apply_opts = Options::default();
    apply_opts.merge_keys = MergeKeys::Apply;
    let res_apply = lint_yaml11_with(merge_src, &apply_opts).unwrap();
    let apply_codes: Vec<LintCode> = res_apply.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        apply_codes,
        vec![LintCode::Yaml11Boolean, LintCode::Yaml11MergeKey]
    );

    let mut as_key_opts = Options::default();
    as_key_opts.merge_keys = MergeKeys::AsKey;
    let res_as_key =
        lint_yaml11_reader_with(&b"plain:\n  <<: 1\nquoted:\n  '<<': 2\n"[..], &as_key_opts)
            .unwrap();
    assert_eq!(res_as_key.diagnostics.len(), 1);
    assert_eq!(res_as_key.diagnostics[0].code, LintCode::Yaml11MergeKey);

    // max_warnings truncation
    let mut trunc_opts = Options::default();
    trunc_opts.limits.max_warnings = 2;
    let res_trunc = lint_yaml11_with(src, &trunc_opts).unwrap();
    assert_eq!(res_trunc.diagnostics.len(), 2);
    assert_eq!(res_trunc.warning_count, 3);
    assert!(res_trunc.truncated);

    // encode_lint wire format and invariant checks
    let wire = encode_lint(&res_apply, &apply_opts).expect("encode_lint should succeed");
    assert!(wire.starts_with("{\"format\":\"yamlprims-lint\",\"version\":1,"));

    // Invalid warning_count -> InvalidOptions error
    let mut bad_res: LintResult = res.clone();
    bad_res.warning_count = 99;
    let err = encode_lint(&bad_res, &Options::default()).expect_err("mismatched warning_count");
    assert_eq!(err.kind, ErrorKind::InvalidOptions);

    // Span with start > end -> InvalidOptions error
    let mut bad_span_res: LintResult = res.clone();
    bad_span_res.diagnostics[0] = LintDiagnostic::new(
        LintCode::Yaml11Boolean,
        Span {
            start: Location {
                byte_offset: 10,
                line: 2,
                column: 1,
            },
            end: Location {
                byte_offset: 5,
                line: 1,
                column: 6,
            },
        },
        0,
        vec![PathSegment::Key(Box::new(Value::String(
            "flag".to_string(),
        )))],
        "yes",
        "str (\"yes\")",
        "bool (true) in YAML 1.1",
        "bad span",
    );
    let err_span = encode_lint(&bad_span_res, &Options::default()).expect_err("bad span");
    assert_eq!(err_span.kind, ErrorKind::InvalidOptions);
}
