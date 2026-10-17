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
        g.flow_nesting_limit = opts.limits.max_depth;
        g.block_nesting_limit = opts.limits.max_depth;
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
        for ev in Self::parser(self.input, self.opts) {
            if let Err(e) = ev {
                return Err(self.scan_err(e));
            }
        }
        Ok(())
    }
    pub(crate) fn next(&mut self) -> Result<(Event<'a>, EventSpan), Error> {
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
