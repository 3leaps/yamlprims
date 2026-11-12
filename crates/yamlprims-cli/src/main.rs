//! File/stdin handling and presentation over the shared YAML core.
#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use yamlprims::{
    CompatQuoting, Document, EmitOptions, Error, ErrorKind, Integer, LimitKind, Mapping, MergeKeys,
    Node, Options, Site, Value, WireEnvelope,
};

const USAGE: &str = "usage: yamlprims [--merge=reject|apply|as_key] check FILE...\n       yamlprims [--merge=reject|apply|as_key] dump [--plain|--ijson] [--max-output-bytes=N] FILE\n       yamlprims [--merge=reject|apply|as_key] emit [--from-wire|--from-plain] [--compat-quoting=yaml11-safe|core-only] [--max-output-bytes=N] FILE\n       yamlprims --help|-h\n       yamlprims --version|-V\nUse - for stdin, and -- before filenames beginning with -.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Dump,
    Emit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DumpFormat {
    Wire,
    Plain,
    Ijson,
}

struct Command {
    mode: Mode,
    dump_format: DumpFormat,
    from_wire: bool,
    from_plain: bool,
    files: Vec<PathBuf>,
    opts: Options,
    emit_opts: EmitOptions,
}

fn parse_merge(val: &str) -> Option<MergeKeys> {
    match val {
        "reject" => Some(MergeKeys::Reject),
        "apply" => Some(MergeKeys::Apply),
        "as_key" => Some(MergeKeys::AsKey),
        _ => None,
    }
}

fn parse_quoting(val: &str) -> Option<CompatQuoting> {
    match val {
        "yaml11-safe" | "yaml11_safe" => Some(CompatQuoting::Yaml11Safe),
        "core-only" | "core_only" => Some(CompatQuoting::CoreOnly),
        _ => None,
    }
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Option<Command> {
    let mut mode = None;
    let mut plain = false;
    let mut ijson = false;
    let mut from_wire = false;
    let mut from_plain = false;
    let mut quoting = None;
    let mut max_output_bytes: Option<usize> = None;
    let mut merge = None;
    let mut files = Vec::new();
    let mut literal = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if !literal && arg == "--" {
            literal = true;
            continue;
        }
        if !literal && arg == "--plain" {
            if plain || ijson {
                return None;
            }
            plain = true;
        } else if !literal && arg == "--ijson" {
            if plain || ijson {
                return None;
            }
            ijson = true;
        } else if !literal && arg == "--from-wire" {
            if from_wire || from_plain {
                return None;
            }
            from_wire = true;
        } else if !literal && arg == "--from-plain" {
            if from_wire || from_plain {
                return None;
            }
            from_plain = true;
        } else if !literal && arg == "--compat-quoting" {
            if quoting.is_some() {
                return None;
            }
            let next = iter.next()?;
            quoting = Some(parse_quoting(next.to_str()?)?);
        } else if !literal
            && arg
                .to_str()
                .is_some_and(|a| a.starts_with("--compat-quoting="))
        {
            if quoting.is_some() {
                return None;
            }
            let val = arg.to_str()?.strip_prefix("--compat-quoting=")?;
            quoting = Some(parse_quoting(val)?);
        } else if !literal && arg == "--max-output-bytes" {
            if max_output_bytes.is_some() {
                return None;
            }
            let next = iter.next()?;
            let n: usize = next.to_str()?.parse().ok()?;
            if n == 0 {
                return None;
            }
            max_output_bytes = Some(n);
        } else if !literal
            && arg
                .to_str()
                .is_some_and(|a| a.starts_with("--max-output-bytes="))
        {
            if max_output_bytes.is_some() {
                return None;
            }
            let val = arg.to_str()?.strip_prefix("--max-output-bytes=")?;
            let n: usize = val.parse().ok()?;
            if n == 0 {
                return None;
            }
            max_output_bytes = Some(n);
        } else if !literal && arg.to_str().is_some_and(|a| a.starts_with("--merge=")) {
            if merge.is_some() {
                return None;
            }
            merge = Some(parse_merge(arg.to_str()?.strip_prefix("--merge=")?)?);
        } else if mode.is_none() {
            mode = Some(match arg.to_str()? {
                "check" => Mode::Check,
                "dump" => Mode::Dump,
                "emit" => Mode::Emit,
                _ => return None,
            });
        } else {
            if !literal && arg != "-" && arg.as_encoded_bytes().starts_with(b"-") {
                return None;
            }
            files.push(PathBuf::from(arg));
        }
    }
    let mode = mode?;
    if files.is_empty()
        || (mode != Mode::Check && files.len() != 1)
        || (mode != Mode::Dump && (plain || ijson))
        || (mode != Mode::Emit && (from_wire || from_plain || quoting.is_some()))
        || (mode == Mode::Check && max_output_bytes.is_some())
        || files.iter().filter(|p| p.as_os_str() == "-").count() > 1
    {
        return None;
    }
    let dump_format = if plain {
        DumpFormat::Plain
    } else if ijson {
        DumpFormat::Ijson
    } else {
        DumpFormat::Wire
    };
    let mut opts = if ijson {
        Options::ijson_default()
    } else {
        Options::default()
    };
    opts.merge_keys = merge.unwrap_or(MergeKeys::Reject);
    if let Some(n) = max_output_bytes {
        opts.limits.max_output_bytes = n;
    }
    let mut emit_opts = EmitOptions::default();
    if let Some(q) = quoting {
        emit_opts.compat_quoting = q;
    }
    if let Some(n) = max_output_bytes {
        emit_opts.limits.max_output_bytes = n;
        emit_opts.limits.max_scalar_bytes = emit_opts.limits.max_scalar_bytes.min(n);
        emit_opts.limits.max_key_bytes = emit_opts.limits.max_key_bytes.min(n);
    }
    Some(Command {
        mode,
        dump_format,
        from_wire,
        from_plain,
        files,
        opts,
        emit_opts,
    })
}

/// Read one byte beyond the raw-input budget so the core reports MaxInputBytes.
fn read_limited(reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read(path: &Path, opts: &Options) -> io::Result<Vec<u8>> {
    if path.as_os_str() == "-" {
        read_limited(io::stdin().lock(), opts.limits.max_input_bytes)
    } else {
        read_limited(std::fs::File::open(path)?, opts.limits.max_input_bytes)
    }
}

fn report(path: &Path, e: &Error) -> String {
    let path = path.display();
    match e.site {
        Site::Source(l) => format!(
            "{path}:{}:{}: {}: {}",
            l.line,
            l.column,
            e.kind.name(),
            e.message
        ),
        Site::NotSource { .. } => format!("{path}: {}: {}", e.kind.name(), e.message),
    }
}

fn diagnostic(message: impl std::fmt::Display) {
    let _ = writeln!(io::stderr().lock(), "{message}");
}

/// Serialization has completed before this function is called. An I/O error can
/// still leave a prefix on the destination; it is reported as a failed command.
fn write_line(mut writer: impl Write, text: &str) -> io::Result<()> {
    writer.write_all(text.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn write_raw(mut writer: impl Write, text: &str) -> io::Result<()> {
    writer.write_all(text.as_bytes())?;
    writer.flush()
}

fn dump(path: &Path, bytes: &[u8], command: &Command, writer: impl Write) -> u8 {
    let opts = &command.opts;
    if command.dump_format == DumpFormat::Ijson {
        return match yamlprims::load_ijson_bytes(bytes, opts) {
            Ok(text) => {
                if let Err(e) = write_line(writer, &text) {
                    diagnostic(format!("cannot write stdout: {e}"));
                    return 2;
                }
                0
            }
            Err(e) => {
                diagnostic(report(path, &e));
                1
            }
        };
    }

    let (text, status) = match yamlprims::load_all_bytes(bytes, opts) {
        Ok(docs) => {
            let result = if command.dump_format == DumpFormat::Plain {
                match docs.as_slice() {
                    [doc] => yamlprims::encode_plain(doc.root(), opts),
                    _ => {
                        diagnostic(format!(
                            "{}: --plain needs exactly one document",
                            path.display()
                        ));
                        return 1;
                    }
                }
            } else {
                yamlprims::encode(&docs, opts)
            };
            match result {
                Ok(text) => (text, 0),
                Err(e) => {
                    diagnostic(report(path, &e));
                    return 1;
                }
            }
        }
        Err(e) => {
            diagnostic(report(path, &e));
            if command.dump_format == DumpFormat::Plain {
                return 1;
            }
            match yamlprims::encode_error(&e, opts) {
                Ok(text) => (text, 1),
                Err(encoding) => {
                    diagnostic(report(path, &encoding));
                    return 1;
                }
            }
        }
    };
    if let Err(e) = write_line(writer, &text) {
        diagnostic(format!("cannot write stdout: {e}"));
        return 2;
    }
    status
}

enum PlainFrame {
    Value {
        depth: usize,
    },
    FinishArray {
        items: Vec<Node>,
        depth: usize,
    },
    FinishObject {
        map: Mapping,
        pending_key: Option<String>,
        depth: usize,
    },
}

struct PlainJsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    opts: &'a Options,
    nodes_used: usize,
}

impl<'a> PlainJsonParser<'a> {
    fn err(kind: ErrorKind, msg: impl std::fmt::Display) -> Error {
        Error::new(kind, Site::NotSource { last_known: None }, msg)
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n')
        {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn consume_literal(&mut self, lit: &str) -> bool {
        self.skip_ws();
        if self.bytes[self.pos..].starts_with(lit.as_bytes()) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn charge_node(&mut self) -> Result<(), Error> {
        self.nodes_used = self.nodes_used.saturating_add(1);
        if self.nodes_used > self.opts.limits.max_nodes {
            return Err(Self::err(
                ErrorKind::LimitExceeded {
                    limit: LimitKind::MaxNodes,
                    configured: self.opts.limits.max_nodes as u64,
                },
                "plain JSON node count exceeds max_nodes",
            ));
        }
        Ok(())
    }

    fn parse_hex4(&mut self) -> Result<u16, Error> {
        if self.pos + 4 > self.bytes.len() {
            return Err(Self::err(
                ErrorKind::Internal,
                "plain JSON: truncated \\uXXXX escape",
            ));
        }
        let s = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
            .map_err(|_| Self::err(ErrorKind::Internal, "plain JSON: invalid \\uXXXX escape"))?;
        let v = u16::from_str_radix(s, 16)
            .map_err(|_| Self::err(ErrorKind::Internal, "plain JSON: invalid \\uXXXX escape"))?;
        self.pos += 4;
        Ok(v)
    }

    fn parse_string(&mut self, is_key: bool) -> Result<String, Error> {
        self.skip_ws();
        if self.bytes.get(self.pos) != Some(&b'"') {
            return Err(Self::err(
                ErrorKind::Internal,
                "plain JSON: expected string",
            ));
        }
        self.pos += 1;
        let mut out = String::new();
        loop {
            if self.pos >= self.bytes.len() {
                return Err(Self::err(
                    ErrorKind::Internal,
                    "plain JSON: unterminated string",
                ));
            }
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                break;
            }
            if b < 0x20 {
                return Err(Self::err(
                    ErrorKind::Internal,
                    "plain JSON: unescaped control character in string",
                ));
            }
            if b == b'\\' {
                self.pos += 1;
                let Some(&esc) = self.bytes.get(self.pos) else {
                    return Err(Self::err(
                        ErrorKind::Internal,
                        "plain JSON: unterminated escape",
                    ));
                };
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\x08'),
                    b'f' => out.push('\x0c'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let u = self.parse_hex4()?;
                        if (0xD800..=0xDBFF).contains(&u) {
                            if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                                return Err(Self::err(
                                    ErrorKind::Internal,
                                    "plain JSON: unpaired UTF-16 high surrogate",
                                ));
                            }
                            self.pos += 2;
                            let low = self.parse_hex4()?;
                            if !(0xDC00..=0xDFFF).contains(&low) {
                                return Err(Self::err(
                                    ErrorKind::Internal,
                                    "plain JSON: invalid UTF-16 low surrogate",
                                ));
                            }
                            let cp =
                                0x10000 + (((u as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                            let Some(ch) = char::from_u32(cp) else {
                                return Err(Self::err(
                                    ErrorKind::Internal,
                                    "plain JSON: invalid surrogate scalar value",
                                ));
                            };
                            out.push(ch);
                        } else if (0xDC00..=0xDFFF).contains(&u) {
                            return Err(Self::err(
                                ErrorKind::Internal,
                                "plain JSON: unpaired UTF-16 low surrogate",
                            ));
                        } else if let Some(ch) = char::from_u32(u as u32) {
                            out.push(ch);
                        }
                    }
                    _ => {
                        return Err(Self::err(
                            ErrorKind::Internal,
                            "plain JSON: invalid string escape",
                        ))
                    }
                }
            } else {
                let s = std::str::from_utf8(&self.bytes[self.pos..]).map_err(|_| {
                    Self::err(
                        ErrorKind::InvalidUtf8 { invalid_len: None },
                        "invalid UTF-8",
                    )
                })?;
                let ch = s.chars().next().unwrap();
                out.push(ch);
                self.pos += ch.len_utf8();
            }
            if out.len() > self.opts.limits.max_scalar_bytes {
                return Err(Self::err(
                    ErrorKind::LimitExceeded {
                        limit: LimitKind::MaxScalarBytes,
                        configured: self.opts.limits.max_scalar_bytes as u64,
                    },
                    "plain JSON string exceeds max_scalar_bytes",
                ));
            }
        }
        if is_key && 64usize.saturating_add(out.len()) > self.opts.limits.max_key_bytes {
            return Err(Self::err(
                ErrorKind::LimitExceeded {
                    limit: LimitKind::MaxKeyBytes,
                    configured: self.opts.limits.max_key_bytes as u64,
                },
                "plain JSON key exceeds max_key_bytes",
            ));
        }
        Ok(out)
    }

    fn parse_number(&mut self) -> Result<Node, Error> {
        self.skip_ws();
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        let int_start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if b.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == int_start || (self.pos - int_start > 1 && self.bytes[int_start] == b'0') {
            return Err(Self::err(
                ErrorKind::Internal,
                "plain JSON: invalid number token",
            ));
        }
        let mut is_float = false;
        if self.bytes.get(self.pos) == Some(&b'.') {
            is_float = true;
            self.pos += 1;
            let frac_start = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == frac_start {
                return Err(Self::err(
                    ErrorKind::Internal,
                    "plain JSON: invalid number fraction",
                ));
            }
        }
        if matches!(self.bytes.get(self.pos), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == exp_start {
                return Err(Self::err(
                    ErrorKind::Internal,
                    "plain JSON: invalid number exponent",
                ));
            }
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
        if !is_float {
            if raw == "-0" {
                return Ok(Node::synthetic(Value::Int(Integer::from_i128(0))));
            }
            if let Ok(i) = raw.parse::<i128>() {
                return Ok(Node::synthetic(Value::Int(Integer::from_i128(i))));
            }
            if let Ok(u) = raw.parse::<u128>() {
                return Ok(Node::synthetic(Value::Int(Integer::from_u128(u))));
            }
            return Err(Self::err(
                ErrorKind::IntegerOutOfRange,
                "plain JSON integer exceeds 128-bit range",
            ));
        }
        let f: f64 = raw
            .parse()
            .map_err(|_| Self::err(ErrorKind::FloatOutOfRange, "plain JSON float out of range"))?;
        if !f.is_finite() {
            return Err(Self::err(
                ErrorKind::FloatOutOfRange,
                "plain JSON float overflows IEEE 754 binary64",
            ));
        }
        Ok(Node::synthetic(Value::Float(f)))
    }

    fn parse_root(&mut self) -> Result<Document, Error> {
        self.opts.validate()?;
        if self.bytes.len() > self.opts.limits.max_input_bytes {
            return Err(Self::err(
                ErrorKind::LimitExceeded {
                    limit: LimitKind::MaxInputBytes,
                    configured: self.opts.limits.max_input_bytes as u64,
                },
                "plain JSON input exceeds max_input_bytes",
            ));
        }
        if self.bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.pos = 3;
        }
        let mut stack = vec![PlainFrame::Value { depth: 0 }];
        let mut results: Vec<Node> = Vec::new();

        while let Some(frame) = stack.pop() {
            match frame {
                PlainFrame::Value { depth } => {
                    self.charge_node()?;
                    let Some(b) = self.peek() else {
                        return Err(Self::err(ErrorKind::Internal, "plain JSON: unexpected EOF"));
                    };
                    match b {
                        b'n' if self.consume_literal("null") => {
                            results.push(Node::synthetic(Value::Null))
                        }
                        b't' if self.consume_literal("true") => {
                            results.push(Node::synthetic(Value::Bool(true)))
                        }
                        b'f' if self.consume_literal("false") => {
                            results.push(Node::synthetic(Value::Bool(false)))
                        }
                        b'"' => {
                            let s = self.parse_string(false)?;
                            results.push(Node::synthetic(Value::String(s)));
                        }
                        b'-' | b'0'..=b'9' => {
                            let n = self.parse_number()?;
                            results.push(n);
                        }
                        b'[' => {
                            if depth + 1 > self.opts.limits.max_depth {
                                return Err(Self::err(
                                    ErrorKind::LimitExceeded {
                                        limit: LimitKind::MaxDepth,
                                        configured: self.opts.limits.max_depth as u64,
                                    },
                                    "plain JSON array nesting exceeds max_depth",
                                ));
                            }
                            self.pos += 1;
                            if self.peek() == Some(b']') {
                                self.pos += 1;
                                results.push(Node::synthetic(Value::Sequence(Vec::new())));
                            } else {
                                stack.push(PlainFrame::FinishArray {
                                    items: Vec::new(),
                                    depth: depth + 1,
                                });
                                stack.push(PlainFrame::Value { depth: depth + 1 });
                            }
                        }
                        b'{' => {
                            if depth + 1 > self.opts.limits.max_depth {
                                return Err(Self::err(
                                    ErrorKind::LimitExceeded {
                                        limit: LimitKind::MaxDepth,
                                        configured: self.opts.limits.max_depth as u64,
                                    },
                                    "plain JSON object nesting exceeds max_depth",
                                ));
                            }
                            self.pos += 1;
                            if self.peek() == Some(b'}') {
                                self.pos += 1;
                                results.push(Node::synthetic(Value::Mapping(Mapping::new())));
                            } else {
                                self.charge_node()?;
                                let k = self.parse_string(true)?;
                                if self.peek() != Some(b':') {
                                    return Err(Self::err(
                                        ErrorKind::Internal,
                                        "plain JSON: expected ':' after object key",
                                    ));
                                }
                                self.pos += 1;
                                stack.push(PlainFrame::FinishObject {
                                    map: Mapping::new(),
                                    pending_key: Some(k),
                                    depth: depth + 1,
                                });
                                stack.push(PlainFrame::Value { depth: depth + 1 });
                            }
                        }
                        _ => {
                            return Err(Self::err(
                                ErrorKind::Internal,
                                "plain JSON: unexpected token",
                            ))
                        }
                    }
                }
                PlainFrame::FinishArray { mut items, depth } => {
                    let item = results.pop().expect("array item");
                    items.push(item);
                    match self.peek() {
                        Some(b',') => {
                            self.pos += 1;
                            stack.push(PlainFrame::FinishArray { items, depth });
                            stack.push(PlainFrame::Value { depth });
                        }
                        Some(b']') => {
                            self.pos += 1;
                            results.push(Node::synthetic(Value::Sequence(items)));
                        }
                        _ => {
                            return Err(Self::err(
                                ErrorKind::Internal,
                                "plain JSON: expected ',' or ']' in array",
                            ))
                        }
                    }
                }
                PlainFrame::FinishObject {
                    mut map,
                    pending_key,
                    depth,
                } => {
                    let key_str = pending_key.expect("pending object key");
                    let val = results.pop().expect("object value");
                    if map.len() + 1 > self.opts.limits.max_mapping_entries {
                        return Err(Self::err(
                            ErrorKind::LimitExceeded {
                                limit: LimitKind::MaxMappingEntries,
                                configured: self.opts.limits.max_mapping_entries as u64,
                            },
                            "plain JSON object exceeds max_mapping_entries",
                        ));
                    }
                    if map
                        .insert_new(Node::synthetic(Value::String(key_str)), val)
                        .is_err()
                    {
                        return Err(Self::err(
                            ErrorKind::DuplicateKey,
                            "plain JSON object has duplicate key",
                        ));
                    }
                    match self.peek() {
                        Some(b',') => {
                            self.pos += 1;
                            self.charge_node()?;
                            let next_k = self.parse_string(true)?;
                            if self.peek() != Some(b':') {
                                return Err(Self::err(
                                    ErrorKind::Internal,
                                    "plain JSON: expected ':' after object key",
                                ));
                            }
                            self.pos += 1;
                            stack.push(PlainFrame::FinishObject {
                                map,
                                pending_key: Some(next_k),
                                depth,
                            });
                            stack.push(PlainFrame::Value { depth });
                        }
                        Some(b'}') => {
                            self.pos += 1;
                            results.push(Node::synthetic(Value::Mapping(map)));
                        }
                        _ => {
                            return Err(Self::err(
                                ErrorKind::Internal,
                                "plain JSON: expected ',' or '}' in object",
                            ))
                        }
                    }
                }
            }
        }
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err(Self::err(
                ErrorKind::Internal,
                "plain JSON: trailing content after root value",
            ));
        }
        let root = results.pop().expect("root node");
        Ok(Document::new(root))
    }
}

fn emit_cmd(path: &Path, bytes: &[u8], command: &Command, writer: impl Write) -> u8 {
    let docs = if command.from_wire {
        let Ok(wire_str) = std::str::from_utf8(bytes) else {
            diagnostic(format!(
                "{}: invalid_utf8: wire input is not valid UTF-8",
                path.display()
            ));
            return 1;
        };
        match yamlprims::decode_wire(wire_str, &command.opts) {
            Ok(WireEnvelope::Documents(docs)) => docs,
            Ok(WireEnvelope::Error(e)) | Err(e) => {
                diagnostic(report(path, &e));
                return 1;
            }
        }
    } else if command.from_plain {
        let Ok(plain_str) = std::str::from_utf8(bytes) else {
            diagnostic(format!(
                "{}: invalid_utf8: plain JSON input is not valid UTF-8",
                path.display()
            ));
            return 1;
        };
        let mut parser = PlainJsonParser {
            bytes: plain_str.as_bytes(),
            pos: 0,
            opts: &command.opts,
            nodes_used: 0,
        };
        match parser.parse_root() {
            Ok(doc) => vec![doc],
            Err(e) => {
                diagnostic(report(path, &e));
                return 1;
            }
        }
    } else {
        match yamlprims::load_all_bytes(bytes, &command.opts) {
            Ok(docs) => docs,
            Err(e) => {
                diagnostic(report(path, &e));
                return 1;
            }
        }
    };

    let text = match yamlprims::emit_all_with(&docs, &command.emit_opts) {
        Ok(text) => text,
        Err(e) => {
            diagnostic(report(path, &e));
            return 1;
        }
    };
    if let Err(e) = write_raw(writer, &text) {
        diagnostic(format!("cannot write stdout: {e}"));
        return 2;
    }
    0
}

fn run(command: Command) -> u8 {
    let mut status = 0;
    for path in &command.files {
        let bytes = match read(path, &command.opts) {
            Ok(bytes) => bytes,
            Err(e) => {
                diagnostic(format!("{}: cannot read: {e}", path.display()));
                status = if command.mode == Mode::Check { 1 } else { 2 };
                continue;
            }
        };
        match command.mode {
            Mode::Check => {
                if let Err(e) = yamlprims::load_all_bytes(&bytes, &command.opts) {
                    diagnostic(report(path, &e));
                    status = status.max(1);
                }
            }
            Mode::Dump => return dump(path, &bytes, &command, io::stdout().lock()),
            Mode::Emit => return emit_cmd(path, &bytes, &command, io::stdout().lock()),
        }
    }
    status
}

fn version_line() -> String {
    format!("yamlprims {}", env!("CARGO_PKG_VERSION"))
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        return ExitCode::from(match write_line(io::stdout().lock(), USAGE) {
            Ok(()) => 0,
            Err(e) => {
                diagnostic(format!("cannot write stdout: {e}"));
                2
            }
        });
    }
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        return ExitCode::from(match write_line(io::stdout().lock(), &version_line()) {
            Ok(()) => 0,
            Err(e) => {
                diagnostic(format!("cannot write stdout: {e}"));
                2
            }
        });
    }
    match parse(args) {
        Some(command) => ExitCode::from(run(command)),
        None => {
            diagnostic(USAGE);
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_stops_one_byte_past_limit() {
        let mut reader = &b"0123456789"[..];
        assert_eq!(read_limited(&mut reader, 3).unwrap(), b"0123");
        assert_eq!(reader, b"456789");
        assert!(read_limited(&b""[..], 3).unwrap().is_empty());
    }

    #[test]
    fn read_failure_is_not_a_partial_input() {
        struct FailingReader(bool);
        impl Read for FailingReader {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    bytes[0] = b'a';
                    Ok(1)
                } else {
                    Err(io::Error::other("read failed"))
                }
            }
        }
        assert!(read_limited(FailingReader(false), 3).is_err());
    }

    #[test]
    fn failed_encoding_emits_no_prefix_or_empty_fallback() {
        let mut command = parse([OsString::from("dump"), OsString::from("-")]).unwrap();
        command.opts.limits.max_output_bytes = 1;
        for input in [b"a: 1".as_slice(), b"[".as_slice()] {
            let mut output = Vec::new();
            assert_eq!(dump(Path::new("-"), input, &command, &mut output), 1);
            assert!(output.is_empty());
        }
    }

    #[test]
    fn output_failure_is_propagated() {
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(write_line(FailingWriter, "{}").is_err());
        let command = parse([OsString::from("dump"), OsString::from("-")]).unwrap();
        assert_eq!(dump(Path::new("-"), b"true", &command, FailingWriter), 2);
    }
}
