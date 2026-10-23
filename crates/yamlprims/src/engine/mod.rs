//! The only backend boundary. Translate engine events into private owned vocabulary.
//! Scalar text retains its borrowed/owned storage without copying it.
use crate::budget::limit_err;
use crate::error::internal;
use crate::error::{Error, ErrorKind, LimitKind};
use crate::options::Options;
use crate::resolve::TagClass;
use crate::value::{Location, Span, Tag};
use granit_parser::{Event as GEvent, Marker, Parser, ScanError, Tag as GTag};
use std::borrow::Cow;

const YAML_NS: &str = "tag:yaml.org,2002:";

#[derive(Clone, Copy)]
pub(crate) enum ScalarStyle {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
    Folded,
}
#[derive(Clone, Copy)]
pub(crate) enum StructureStyle {
    Block,
    Flow,
}
#[derive(Clone, Copy)]
pub(crate) struct Version {
    pub(crate) major: u32,
    pub(crate) minor: u32,
}
pub(crate) enum Event<'a> {
    StreamStart,
    StreamEnd,
    DocumentStart(bool, Option<Version>),
    DocumentEnd,
    Alias(usize),
    Scalar(Cow<'a, str>, ScalarStyle, usize, Option<TagClass>),
    SequenceStart(StructureStyle, usize, Option<TagClass>),
    SequenceEnd,
    MappingStart(StructureStyle, usize, Option<TagClass>),
    MappingEnd,
}
pub(crate) struct EventSpan {
    pub(crate) span: Span,
    pub(crate) tag_start: Option<Location>,
}

fn classify(tag: Option<&GTag>) -> TagClass {
    let Some(t) = tag else { return TagClass::None };
    if t.handle().is_empty() && t.suffix() == "!" {
        return TagClass::NonSpecific;
    }
    if let Some(core) = t.core_suffix() {
        let c: &'static str = match core {
            "bool" => "bool",
            "float" => "float",
            "int" => "int",
            "map" => "map",
            "null" => "null",
            "seq" => "seq",
            _ => "str",
        };
        return TagClass::Core(c);
    }
    if t.suffix_in_namespace(YAML_NS).as_deref() == Some("merge") {
        return TagClass::Merge;
    }
    TagClass::Other(Tag {
        uri: t.to_string(),
        handle: t.original_handle().to_owned(),
        suffix: t.suffix().to_owned(),
    })
}

/// granit error kind name in snake_case.
fn syntax_name(e: &ScanError) -> String {
    let dbg = format!("{:?}", e.kind());
    let name: String = dbg
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) struct Engine<'a> {
    parser: Parser<'a, granit_parser::StrInput<'a>>,
    input: &'a str,
    bom: usize,
    opts: &'a Options,
}
impl<'a> Engine<'a> {
    pub(crate) fn new(input: &'a str, bom: usize, opts: &'a Options) -> Self {
        Self {
            parser: Self::parser(input, opts),
            input,
            bom,
            opts,
        }
    }
    fn parser(input: &'a str, opts: &Options) -> Parser<'a, granit_parser::StrInput<'a>> {
        let mut g = granit_parser::Options::default();
        g.strict_indentation = opts.strict_indentation();
        // A pending simple key can hold back a start while the scanner reads
        // K characters. One crossing fetch and one cached token need two more
        // levels. Each native growth consumes an indicator; parser-only starts
        // return immediately. The event walk below enforces combined depth D.
        // Validated D <= 255 and unchanged K = 1024 keep both guards <= 1281.
        let native_depth = opts.limits.max_depth + g.simple_key_max_lookahead + 2;
        g.flow_nesting_limit = native_depth;
        g.block_nesting_limit = native_depth;
        #[cfg(test)]
        validation_tests::observe(|s| s.parsers += 1);
        Parser::new_from_str_with_options(input, g)
    }
    fn loc(&self, m: &Marker) -> Result<Location, Error> {
        let b = m
            .byte_offset()
            .ok_or_else(|| internal("marker without byte offset"))?;
        Ok(Location {
            byte_offset: b + self.bom,
            line: m.line(),
            column: m.col() + 1,
        })
    }

    fn scan_err(&self, e: ScanError) -> Error {
        use granit_parser::ErrorKind as G;
        let loc = match self.loc(e.marker()) {
            Ok(l) => l,
            Err(err) => return err,
        };
        let detail = e.info();
        let mut err = match e.kind() {
            G::UnknownAnchor { .. } => Error::at(ErrorKind::UnknownAnchor, loc, "unknown anchor"),
            G::UnsupportedYamlMajorVersion { .. } => Error::at(
                ErrorKind::UnsupportedYamlVersion,
                loc,
                "unsupported YAML version",
            ),
            G::RecursionLimitExceeded { .. } => {
                limit_err(LimitKind::MaxDepth, self.opts.limits.max_depth, loc)
            }
            _ => Error::at(
                ErrorKind::Syntax(syntax_name(&e)),
                loc,
                format!("syntax error: {detail}"),
            ),
        };
        err.engine_detail = Some(detail);
        err
    }

    pub(crate) fn syntax_pass(&self) -> Result<(), Error> {
        #[cfg(test)]
        validation_tests::observe(|s| s.source_passes += 1);
        let mut depth = 0usize;
        for ev in Self::parser(self.input, self.opts) {
            #[cfg(test)]
            validation_tests::observe(|s| s.source_results += 1);
            let (event, span) = ev.map_err(|e| self.scan_err(e))?;
            match event {
                GEvent::SequenceStart(..) | GEvent::MappingStart(..) => {
                    let next = depth
                        .checked_add(1)
                        .ok_or_else(|| internal("source depth overflow"))?;
                    if next > self.opts.limits.max_depth {
                        return Err(limit_err(
                            LimitKind::MaxDepth,
                            self.opts.limits.max_depth,
                            self.loc(&span.start)?,
                        ));
                    }
                    depth = next;
                    #[cfg(test)]
                    validation_tests::observe(|s| s.highwater = s.highwater.max(depth));
                }
                GEvent::SequenceEnd | GEvent::MappingEnd => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| internal("source depth underflow"))?;
                }
                GEvent::DocumentStart(..) | GEvent::DocumentEnd | GEvent::StreamEnd => {
                    if depth != 0 {
                        return Err(internal("unbalanced source document"));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(crate) fn next(&mut self) -> Result<(Event<'a>, EventSpan), Error> {
        #[cfg(test)]
        validation_tests::observe(|s| s.build_pulls += 1);
        let (event, span) = match self.parser.next() {
            Some(Ok(ev)) => ev,
            Some(Err(e)) => return Err(self.scan_err(e)),
            None => return Err(internal("event stream ended early")),
        };
        let event = match event {
            GEvent::StreamStart => Event::StreamStart,
            GEvent::StreamEnd => Event::StreamEnd,
            GEvent::DocumentStart(explicit, version) => Event::DocumentStart(
                explicit,
                version.map(|v| Version {
                    major: v.major,
                    minor: v.minor,
                }),
            ),
            GEvent::DocumentEnd => Event::DocumentEnd,
            GEvent::Alias(id) => Event::Alias(id),
            GEvent::Scalar(text, style, anchor, tag) => Event::Scalar(
                text,
                scalar_style(style),
                anchor,
                tag.as_deref().map(|t| classify(Some(t))),
            ),
            GEvent::SequenceStart(style, anchor, tag) => Event::SequenceStart(
                structure_style(style),
                anchor,
                tag.as_deref().map(|t| classify(Some(t))),
            ),
            GEvent::SequenceEnd => Event::SequenceEnd,
            GEvent::MappingStart(style, anchor, tag) => Event::MappingStart(
                structure_style(style),
                anchor,
                tag.as_deref().map(|t| classify(Some(t))),
            ),
            GEvent::MappingEnd => Event::MappingEnd,
            _ => return Err(internal("unsupported engine event")),
        };
        Ok((
            event,
            EventSpan {
                span: Span {
                    start: self.loc(&span.start)?,
                    end: self.loc(&span.end)?,
                },
                tag_start: span.tag_start().map(|m| self.loc(&m)).transpose()?,
            },
        ))
    }
}
fn scalar_style(style: granit_parser::ScalarStyle) -> ScalarStyle {
    use granit_parser::ScalarStyle as G;
    match style {
        G::Plain => ScalarStyle::Plain,
        G::SingleQuoted => ScalarStyle::SingleQuoted,
        G::DoubleQuoted => ScalarStyle::DoubleQuoted,
        G::Literal => ScalarStyle::Literal,
        G::Folded => ScalarStyle::Folded,
    }
}
fn structure_style(style: granit_parser::StructureStyle) -> StructureStyle {
    match style {
        granit_parser::StructureStyle::Block => StructureStyle::Block,
        granit_parser::StructureStyle::Flow => StructureStyle::Flow,
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::{load_all_with, load_bytes, load_with, MergeKeys, Site, UnknownTags};
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct Stats {
        pub(super) parsers: usize,
        pub(super) source_passes: usize,
        pub(super) source_results: usize,
        pub(super) highwater: usize,
        pub(super) build_pulls: usize,
    }

    std::thread_local! {
        static STATS: Cell<Option<Stats>> = const { Cell::new(None) };
    }

    pub(super) fn observe(f: impl FnOnce(&mut Stats)) {
        STATS.with(|cell| {
            if let Some(mut stats) = cell.get() {
                f(&mut stats);
                cell.set(Some(stats));
            }
        });
    }

    fn measured<T>(f: impl FnOnce() -> T) -> (T, Stats) {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                STATS.with(|s| s.set(None));
            }
        }
        STATS.with(|s| assert!(s.replace(Some(Stats::default())).is_none()));
        let reset = Reset;
        let result = f();
        let stats = STATS.with(|s| s.get().unwrap());
        drop(reset);
        (result, stats)
    }

    fn options(depth: usize) -> Options {
        let mut options = Options::default();
        options.limits.max_depth = depth;
        options
    }

    fn depth_error(error: &Error, configured: usize) {
        assert_eq!(
            error.kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxDepth,
                configured: configured as u64,
            }
        );
    }

    #[test]
    fn source_depth_stops_before_build_or_later_syntax() {
        for input in ["[[]]", "---\n[[]]\n---\n[\n"] {
            let (result, stats) = measured(|| load_all_with(input, &options(1)));
            let error = result.unwrap_err();
            depth_error(&error, 1);
            assert!(error.engine_detail.is_none());
            assert!(error.path.is_empty());
            assert_eq!(
                stats,
                Stats {
                    parsers: 2,
                    source_passes: 1,
                    source_results: 4, // stream, document, first start, rejected start
                    highwater: 1,
                    build_pulls: 0,
                }
            );
        }
    }

    #[test]
    fn source_depth_precedes_materialization_and_policy() {
        let mut opts = options(1);
        opts.limits.max_nodes = 1;
        opts.unknown_tags = UnknownTags::Reject;
        opts.merge_keys = MergeKeys::Reject;
        for input in ["!t [[]]", "{<<: []}"] {
            let (result, stats) = measured(|| load_with(input, &opts));
            depth_error(&result.unwrap_err(), 1);
            assert_eq!(stats.parsers, 2);
            assert_eq!(stats.source_passes, 1);
            assert_eq!(stats.highwater, 1);
            assert_eq!(stats.build_pulls, 0);
        }
    }

    #[test]
    fn successful_validation_consumes_one_source_and_one_build_pass() {
        let (result, stats) = measured(|| load_with("[[]]", &options(2)));
        result.unwrap();
        assert_eq!(
            stats,
            Stats {
                parsers: 2,
                source_passes: 1,
                source_results: 8,
                highwater: 2,
                build_pulls: 8,
            }
        );
        let (result, stats) = measured(|| load_all_with("---\n[]\n---\n{}\n", &options(1)));
        assert_eq!(result.unwrap().len(), 2);
        assert_eq!(stats.parsers, 2);
        assert_eq!(stats.source_passes, 1);
        assert_eq!(stats.source_results, 10);
        assert_eq!(stats.highwater, 1);
        assert_eq!(stats.build_pulls, 10);
    }

    #[test]
    fn terminal_errors_do_not_restart_or_construct() {
        for count in [1, 2, 16, 1024] {
            let input = format!("[{}]\n", vec!["*missing"; count].join(", "));
            let (result, stats) = measured(|| load_with(&input, &options(128)));
            assert_eq!(result.unwrap_err().kind, ErrorKind::UnknownAnchor);
            assert_eq!(
                stats,
                Stats {
                    parsers: 2,
                    source_passes: 1,
                    source_results: 4,
                    highwater: 1,
                    build_pulls: 0,
                }
            );
        }
        for count in [1, 2, 128] {
            let input = "%YAML 2.0\n---\n0\n...\n".repeat(count);
            let (result, stats) = measured(|| load_all_with(&input, &options(128)));
            assert_eq!(result.unwrap_err().kind, ErrorKind::UnsupportedYamlVersion);
            assert_eq!(
                stats,
                Stats {
                    parsers: 2,
                    source_passes: 1,
                    source_results: 2,
                    highwater: 0,
                    build_pulls: 0,
                }
            );
        }
    }

    #[test]
    fn materialized_alias_depth_is_checked_after_source_validation() {
        let (result, stats) = measured(|| load_with("a: &a []\nb: [[*a]]\n", &options(3)));
        depth_error(&result.unwrap_err(), 3);
        assert_eq!(stats.parsers, 2);
        assert_eq!(stats.source_passes, 1);
        assert_eq!(stats.highwater, 3);
        assert!(stats.build_pulls > 0);
    }

    #[test]
    fn options_and_raw_input_gates_precede_all_parsers() {
        let mut opts = options(0);
        let (result, stats) = measured(|| load_bytes(&[0xff], &opts));
        assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidOptions);
        assert_eq!(stats, Stats::default());
        opts.limits.max_depth = 1;
        opts.limits.max_input_bytes = 1;
        let (result, stats) = measured(|| load_bytes(&[0xff, 0xff], &opts));
        assert!(matches!(
            result.unwrap_err().kind,
            ErrorKind::LimitExceeded {
                limit: LimitKind::MaxInputBytes,
                ..
            }
        ));
        assert_eq!(stats, Stats::default());
        let (result, stats) = measured(|| load_bytes(&[0xff], &opts));
        assert!(matches!(
            result.unwrap_err().kind,
            ErrorKind::InvalidUtf8 { .. }
        ));
        assert_eq!(stats, Stats::default());
    }

    #[test]
    fn finite_native_guards_remain_resource_errors_at_native_sites() {
        assert_eq!(
            granit_parser::Options::default().simple_key_max_lookahead,
            1024
        );
        for depth in [1, 255] {
            let opts = options(depth);
            let ceiling = depth + 1024 + 2;
            for (input, byte) in [
                (
                    format!("{}0{}", "[".repeat(2048), "]".repeat(2048)),
                    ceiling,
                ),
                (format!("{}0", "- ".repeat(2048)), 2 * ceiling),
            ] {
                // Private iterator control: deliberately omit the adapter walk.
                // The parser factory is the one used by both production passes.
                let engine = Engine::new(&input, 3, &opts);
                let native = Engine::parser(&input, &opts).find_map(Result::err).unwrap();
                assert!(matches!(
                    native.kind(),
                    granit_parser::ErrorKind::RecursionLimitExceeded { .. }
                ));
                assert_eq!(native.marker().byte_offset(), Some(byte));
                let error = engine.scan_err(native);
                depth_error(&error, depth);
                assert_eq!(
                    error.site,
                    Site::Source(Location {
                        byte_offset: byte + 3,
                        line: 1,
                        column: byte + 1,
                    })
                );
                assert!(error.engine_detail.is_some());
            }
        }
    }
}
