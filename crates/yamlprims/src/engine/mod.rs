//! The only backend boundary. Translate engine events into private owned vocabulary.
//! Scalar text retains its borrowed/owned storage without copying it.
use crate::budget::limit_err;
use crate::error::internal;
use crate::error::{Error, ErrorKind, LimitKind, SyntaxKind};
use crate::options::Options;
use crate::resolve::TagClass;
use crate::value::{Location, Span, Tag};
use granit_parser::{Event as GEvent, Marker, Parser, ScanError, Scanner, Tag as GTag, TokenType};
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

/// Map a backend grammar/scanner error kind to a closed [`SyntaxKind`] member.
/// Returns `None` for non-syntax terminal translations and non-`StrInput` adapter errors.
pub(crate) fn syntax_kind(kind: &granit_parser::ErrorKind) -> Option<SyntaxKind> {
    use granit_parser::ErrorKind as G;
    match kind {
        G::UnexpectedEofFlowSequence => Some(SyntaxKind::UnexpectedEofFlowSequence),
        G::UnexpectedEofFlowMapping => Some(SyntaxKind::UnexpectedEofFlowMapping),
        G::UnexpectedEofImplicitFlowMapping => Some(SyntaxKind::UnexpectedEofImplicitFlowMapping),
        G::UnexpectedEofBlockSequence => Some(SyntaxKind::UnexpectedEofBlockSequence),
        G::UnexpectedEofBlockMapping => Some(SyntaxKind::UnexpectedEofBlockMapping),
        G::UnexpectedEof => Some(SyntaxKind::UnexpectedEof),
        G::ExpectedStreamStart => Some(SyntaxKind::ExpectedStreamStart),
        G::DuplicateVersionDirective => Some(SyntaxKind::DuplicateVersionDirective),
        G::DuplicateTagDirective => Some(SyntaxKind::DuplicateTagDirective),
        G::ExpectedDocumentStart => Some(SyntaxKind::ExpectedDocumentStart),
        G::MissingDocumentEndBeforeDirective => Some(SyntaxKind::MissingDocumentEndBeforeDirective),
        G::AnchorCountOverflow => Some(SyntaxKind::AnchorCountOverflow),
        G::ExpectedNodeContent => Some(SyntaxKind::ExpectedNodeContent),
        G::ExpectedBlockMappingKey => Some(SyntaxKind::ExpectedBlockMappingKey),
        G::ExpectedFlowMappingSeparator => Some(SyntaxKind::ExpectedFlowMappingSeparator),
        G::ExpectedFlowSequenceSeparator => Some(SyntaxKind::ExpectedFlowSequenceSeparator),
        G::ExpectedBlockSequenceEntry => Some(SyntaxKind::ExpectedBlockSequenceEntry),
        G::UndeclaredTagHandle => Some(SyntaxKind::UndeclaredTagHandle),
        G::ExpectedTagBang => Some(SyntaxKind::ExpectedTagBang),
        G::ExpectedTagDirectiveBang => Some(SyntaxKind::ExpectedTagDirectiveBang),
        G::InvalidGlobalTagCharacter => Some(SyntaxKind::InvalidGlobalTagCharacter),
        G::SimpleKeyExpected => Some(SyntaxKind::SimpleKeyExpected),
        G::InvalidSimpleKey => Some(SyntaxKind::InvalidSimpleKey),
        G::InvalidDocumentEnd => Some(SyntaxKind::InvalidDocumentEnd),
        G::InvalidIndentation => Some(SyntaxKind::InvalidIndentation),
        G::BomInsideDocument => Some(SyntaxKind::BomInsideDocument),
        G::UnexpectedCharacter { .. } => Some(SyntaxKind::UnexpectedCharacter),
        G::TabNotAllowed => Some(SyntaxKind::TabNotAllowed),
        G::TabInBlockIndentation => Some(SyntaxKind::TabInBlockIndentation),
        G::CommentInterceptedScalar => Some(SyntaxKind::CommentInterceptedScalar),
        G::ExpectedWhitespace => Some(SyntaxKind::ExpectedWhitespace),
        G::CommentNotSeparated => Some(SyntaxKind::CommentNotSeparated),
        G::InvalidDirectiveTerminator => Some(SyntaxKind::InvalidDirectiveTerminator),
        G::MissingYamlVersionSeparator => Some(SyntaxKind::MissingYamlVersionSeparator),
        G::MissingDirectiveName => Some(SyntaxKind::MissingDirectiveName),
        G::InvalidDirectiveName => Some(SyntaxKind::InvalidDirectiveName),
        G::DirectiveByteLimitExceeded { .. } => Some(SyntaxKind::DirectiveByteLimitExceeded),
        G::TooManyReservedDirectiveParams { .. } => {
            Some(SyntaxKind::TooManyReservedDirectiveParams)
        }
        G::YamlVersionTooLong => Some(SyntaxKind::YamlVersionTooLong),
        G::MissingYamlVersion => Some(SyntaxKind::MissingYamlVersion),
        G::InvalidTagDirectiveTerminator => Some(SyntaxKind::InvalidTagDirectiveTerminator),
        G::InvalidTagTerminator => Some(SyntaxKind::InvalidTagTerminator),
        G::MissingTagUri => Some(SyntaxKind::MissingTagUri),
        G::UnclosedVerbatimTag => Some(SyntaxKind::UnclosedVerbatimTag),
        G::InvalidTagEscape => Some(SyntaxKind::InvalidTagEscape),
        G::InvalidTagUtf8LeadingByte => Some(SyntaxKind::InvalidTagUtf8LeadingByte),
        G::InvalidTagUtf8TrailingByte => Some(SyntaxKind::InvalidTagUtf8TrailingByte),
        G::InvalidTagUtf8 => Some(SyntaxKind::InvalidTagUtf8),
        G::MissingAnchorOrAliasName => Some(SyntaxKind::MissingAnchorOrAliasName),
        G::MisplacedFlowCollectionEnd => Some(SyntaxKind::MisplacedFlowCollectionEnd),
        G::MismatchedFlowCollectionEnd { .. } => Some(SyntaxKind::MismatchedFlowCollectionEnd),
        G::UnclosedFlowCollection { .. } => Some(SyntaxKind::UnclosedFlowCollection),
        G::BlockEntryInFlowCollection => Some(SyntaxKind::BlockEntryInFlowCollection),
        G::BlockSequenceEntryNotAllowed => Some(SyntaxKind::BlockSequenceEntryNotAllowed),
        G::InvalidBlockEntryWhitespace => Some(SyntaxKind::InvalidBlockEntryWhitespace),
        G::ZeroBlockScalarIndent => Some(SyntaxKind::ZeroBlockScalarIndent),
        G::InvalidBlockScalarHeader => Some(SyntaxKind::InvalidBlockScalarHeader),
        G::TabAtBlockScalarStart => Some(SyntaxKind::TabAtBlockScalarStart),
        G::InvalidBlockScalarIndent => Some(SyntaxKind::InvalidBlockScalarIndent),
        G::DocumentIndicatorInQuotedScalar => Some(SyntaxKind::DocumentIndicatorInQuotedScalar),
        G::UnclosedQuotedScalar => Some(SyntaxKind::UnclosedQuotedScalar),
        G::TabInIndentation => Some(SyntaxKind::TabInIndentation),
        G::InvalidQuotedScalarIndent => Some(SyntaxKind::InvalidQuotedScalarIndent),
        G::InvalidTrailingSingleQuotedScalar => Some(SyntaxKind::InvalidTrailingSingleQuotedScalar),
        G::InvalidTrailingDoubleQuotedScalar => Some(SyntaxKind::InvalidTrailingDoubleQuotedScalar),
        G::UnknownQuotedScalarEscape => Some(SyntaxKind::UnknownQuotedScalarEscape),
        G::InvalidQuotedScalarHexEscape => Some(SyntaxKind::InvalidQuotedScalarHexEscape),
        G::InvalidLowSurrogateHexEscape => Some(SyntaxKind::InvalidLowSurrogateHexEscape),
        G::InvalidLowSurrogate => Some(SyntaxKind::InvalidLowSurrogate),
        G::MissingLowSurrogate => Some(SyntaxKind::MissingLowSurrogate),
        G::UnpairedLowSurrogate => Some(SyntaxKind::UnpairedLowSurrogate),
        G::InvalidUnicodeEscape => Some(SyntaxKind::InvalidUnicodeEscape),
        G::InvalidFlowScalarIndent => Some(SyntaxKind::InvalidFlowScalarIndent),
        G::PlainScalarStartsWithDashFlowIndicator => {
            Some(SyntaxKind::PlainScalarStartsWithDashFlowIndicator)
        }
        G::TabInPlainScalar => Some(SyntaxKind::TabInPlainScalar),
        G::UnexpectedEndOfPlainScalar => Some(SyntaxKind::UnexpectedEndOfPlainScalar),
        G::MappingKeyNotAllowed => Some(SyntaxKind::MappingKeyNotAllowed),
        G::FlowMappingValueAdjacentCollection => {
            Some(SyntaxKind::FlowMappingValueAdjacentCollection)
        }
        G::InvalidMappingValueWhitespace => Some(SyntaxKind::InvalidMappingValueWhitespace),
        G::InvalidColonPlacement => Some(SyntaxKind::InvalidColonPlacement),
        G::MappingValueNotAllowed => Some(SyntaxKind::MappingValueNotAllowed),
        G::UnknownAnchor
        | G::UnsupportedYamlMajorVersion
        | G::RecursionLimitExceeded
        | G::InputIo { .. }
        | G::InputDecoding { .. }
        | G::InputByteLimitExceeded { .. }
        | G::MissingIncludeResolver
        | G::Custom(_)
        | G::MultipleDocumentsUnsupported
        | G::InputOffsetsWithoutSlice
        | G::InputSlicingUnavailable => None,
        // granit_parser::ErrorKind is #[non_exhaustive] (and gates TooManyComments
        // behind the disabled `comments` feature). Any bump of the `granit-parser =2.0.0`
        // pin requires re-auditing all variants against `SyntaxKind::ALL`.
        _ => None,
    }
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
        let g = Self::engine_options(opts);
        #[cfg(test)]
        validation_tests::observe(|s| s.parsers += 1);
        Parser::new_from_str_with_options(input, g)
    }
    fn engine_options(opts: &Options) -> granit_parser::Options {
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
        g
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
            G::UnknownAnchor => Error::at(ErrorKind::UnknownAnchor, loc, "unknown anchor"),
            G::UnsupportedYamlMajorVersion => Error::at(
                ErrorKind::UnsupportedYamlVersion,
                loc,
                "unsupported YAML version",
            ),
            G::RecursionLimitExceeded => {
                limit_err(LimitKind::MaxDepth, self.opts.limits.max_depth, loc)
            }
            other => match syntax_kind(other) {
                Some(sk) => Error::at(
                    ErrorKind::Syntax(sk),
                    loc,
                    format!("syntax error: {detail}"),
                ),
                None => return internal("unexpected engine adapter error"),
            },
        };
        err.engine_detail = Some(detail);
        err
    }

    /// §7.1 resolved-tag identity check for a node event's tag. The
    /// non-specific `!` keeps its own resolution path; every other resolved,
    /// percent-decoded identity must be a local tag or start with an RFC 3986
    /// scheme (YAML 1.2.2 §6.9.1).
    fn check_tag_identity(
        &self,
        tag: Option<&GTag>,
        span: &granit_parser::Span,
    ) -> Result<(), Error> {
        let Some(t) = tag else { return Ok(()) };
        if t.handle().is_empty() && t.suffix() == "!" {
            return Ok(());
        }
        if valid_resolved_tag_identity(&t.to_string()) {
            return Ok(());
        }
        let at = span.tag_start().unwrap_or(span.start);
        Err(Error::at(
            ErrorKind::Syntax(SyntaxKind::InvalidGlobalTagCharacter),
            self.loc(&at)?,
            INVALID_TAG_IDENTITY_MESSAGE,
        ))
    }

    /// `%TAG` handle/prefix separation (YAML 1.2.2 production [88], spec §7.1).
    /// The pinned engine accepts a prefix that immediately follows the handle.
    /// Scan tokens, without parsing, up to the first scanner failure: a `%TAG`
    /// token whose prefix follows the handle with no space or tab, or a scanner
    /// failure inside such a directive after its handle, is `ExpectedWhitespace`
    /// at the first prefix character. Any other scanner failure is left to the
    /// event pass. Name, handle and byte-budget failures precede the check.
    fn directive_prepass(&self) -> Result<(), Error> {
        if !self.input.contains("%TAG") {
            return Ok(());
        }
        let src = self.input.as_bytes();
        let scanner = Scanner::with_options(
            granit_parser::StrInput::new(self.input),
            Self::engine_options(self.opts),
        );
        for tok in scanner {
            match tok {
                Ok(t) => {
                    if let TokenType::TagDirective(..) = t.token_type() {
                        let start = t
                            .span()
                            .start
                            .byte_offset()
                            .ok_or_else(|| internal("directive without byte offset"))?;
                        if let Some(at) = tag_directive_gap(src, start) {
                            return Err(self.separator_error(at)?);
                        }
                    }
                }
                Err(e) => {
                    if let Some(b) = e.marker().byte_offset() {
                        let line_start = b == 0 || matches!(src.get(b - 1), Some(b'\n' | b'\r'));
                        if line_start {
                            if let Some(at) = tag_directive_gap(src, b) {
                                return Err(self.separator_error(at)?);
                            }
                        }
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn separator_error(&self, at: usize) -> Result<Error, Error> {
        let before = self
            .input
            .get(..at)
            .ok_or_else(|| internal("directive offset off a character boundary"))?;
        let bytes = before.as_bytes();
        let mut line = 1usize;
        let mut line_start = 0usize;
        let mut k = 0usize;
        while k < bytes.len() {
            match bytes[k] {
                b'\r' if bytes.get(k + 1) == Some(&b'\n') => {
                    line += 1;
                    k += 2;
                    line_start = k;
                }
                b'\r' | b'\n' => {
                    line += 1;
                    k += 1;
                    line_start = k;
                }
                _ => k += 1,
            }
        }
        let column = before[line_start..].chars().count() + 1;
        let mut err = Error::at(
            ErrorKind::Syntax(SyntaxKind::ExpectedWhitespace),
            Location {
                byte_offset: at + self.bom,
                line,
                column,
            },
            "syntax error: expected whitespace",
        );
        err.engine_detail = Some("expected whitespace".to_owned());
        Ok(err)
    }

    pub(crate) fn syntax_pass(&self) -> Result<(), Error> {
        #[cfg(test)]
        validation_tests::observe(|s| s.source_passes += 1);
        self.directive_prepass()?;
        let mut depth = 0usize;
        for ev in Self::parser(self.input, self.opts) {
            #[cfg(test)]
            validation_tests::observe(|s| s.source_results += 1);
            let (event, span) = ev.map_err(|e| self.scan_err(e))?;
            if let GEvent::Scalar(_, _, _, tag) = &event {
                self.check_tag_identity(tag.as_deref(), &span)?;
            }
            match event {
                GEvent::SequenceStart(_, _, ref tag) | GEvent::MappingStart(_, _, ref tag) => {
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
                    self.check_tag_identity(tag.as_deref(), &span)?;
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
/// Diagnostic for a resolved tag identity outside the local/global forms (§4.4, §7.1).
pub(crate) const INVALID_TAG_IDENTITY_MESSAGE: &str =
    "syntax error: resolved tag is neither a local tag nor a global tag URI";

/// A resolved, percent-decoded identity other than the non-specific `!` is a local
/// tag (`!` plus at least one character) or starts with an RFC 3986 §3.1 scheme and
/// `:` (YAML 1.2.2 §6.9.1). Core Schema URIs satisfy the scheme rule.
pub(crate) fn valid_resolved_tag_identity(uri: &str) -> bool {
    if uri.starts_with('!') {
        return uri.len() >= 2;
    }
    let Some(colon) = uri.find(':') else {
        return false;
    };
    let scheme = &uri.as_bytes()[..colon];
    match scheme.split_first() {
        Some((first, rest)) => {
            first.is_ascii_alphabetic()
                && rest
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        }
        None => false,
    }
}

/// For a directive starting at `pct` (`%`), the byte offset of a `%TAG`
/// prefix that immediately follows its handle, mirroring the engine scanner:
/// directive name, blanks and handle are charged to the 1,024-byte directive
/// budget first, and a malformed handle is the engine's own error.
fn tag_directive_gap(src: &[u8], pct: usize) -> Option<usize> {
    const BUDGET: usize = 1024;
    let is_blank = |b: u8| matches!(b, b' ' | b'\t');
    let is_break = |b: u8| matches!(b, b'\n' | b'\r' | b'\0');
    if src.get(pct) != Some(&b'%') {
        return None;
    }
    let mut i = pct + 1;
    let name_start = i;
    while i < src.len() && !is_blank(src[i]) && !is_break(src[i]) {
        i += 1;
    }
    if &src[name_start..i] != b"TAG" {
        return None;
    }
    let mut budget = BUDGET - 3;
    let mut blanks = 0usize;
    while i < src.len() && is_blank(src[i]) {
        i += 1;
        blanks += 1;
    }
    budget = budget.checked_sub(blanks)?;
    if src.get(i) != Some(&b'!') {
        return None;
    }
    budget = budget.checked_sub(1)?;
    i += 1;
    let mut word = false;
    while i < src.len() && (src[i].is_ascii_alphanumeric() || matches!(src[i], b'_' | b'-')) {
        budget = budget.checked_sub(1)?;
        i += 1;
        word = true;
    }
    if src.get(i) == Some(&b'!') {
        budget.checked_sub(1)?;
        i += 1;
    } else if word {
        return None;
    }
    // A blank, a break, NUL (the scanner's end marker) or the end of input
    // keeps the existing prefix diagnostics.
    match src.get(i) {
        None => None,
        Some(&b) if is_blank(b) || is_break(b) => None,
        Some(_) => Some(i),
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

    #[test]
    fn closed_syntax_kind_vocabulary_and_engine_mapping() {
        use granit_parser::{ErrorKind as G, InputIoError};
        use std::collections::BTreeSet;

        assert_eq!(SyntaxKind::ALL.len(), 81);
        let mut seen_codes = BTreeSet::new();
        for &sk in SyntaxKind::ALL {
            let code = sk.as_str();
            assert!(!code.is_empty() && code.len() <= 64, "{code}");
            assert!(
                code.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{code}"
            );
            assert!(seen_codes.insert(code), "duplicate code {code}");
            assert_eq!(SyntaxKind::from_code(code), Some(sk));
            assert_eq!(sk.to_string(), code);
            assert_eq!(String::from(sk), code);
            assert_eq!(sk, code);
            assert_eq!(code, sk);
            let err = Error::at(
                ErrorKind::syntax(sk),
                Location {
                    byte_offset: 0,
                    line: 1,
                    column: 1,
                },
                "syntax error",
            );
            assert_eq!(err.syntax_kind(), Some(sk));
            assert_eq!(err.kind.syntax_kind(), Some(sk));
            let wire = crate::encode_error(&err, &Options::default()).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&wire).unwrap();
            assert_eq!(parsed["error"]["kind"], "syntax");
            assert_eq!(parsed["error"]["detail"]["syntax_kind"], code);
        }
        assert_eq!(SyntaxKind::from_code(""), None);
        assert_eq!(SyntaxKind::from_code("not_a_registered_syntax_kind"), None);

        let syntax_pairs: &[(G, SyntaxKind)] = &[
            (
                G::UnexpectedEofFlowSequence,
                SyntaxKind::UnexpectedEofFlowSequence,
            ),
            (
                G::UnexpectedEofFlowMapping,
                SyntaxKind::UnexpectedEofFlowMapping,
            ),
            (
                G::UnexpectedEofImplicitFlowMapping,
                SyntaxKind::UnexpectedEofImplicitFlowMapping,
            ),
            (
                G::UnexpectedEofBlockSequence,
                SyntaxKind::UnexpectedEofBlockSequence,
            ),
            (
                G::UnexpectedEofBlockMapping,
                SyntaxKind::UnexpectedEofBlockMapping,
            ),
            (G::UnexpectedEof, SyntaxKind::UnexpectedEof),
            (G::ExpectedStreamStart, SyntaxKind::ExpectedStreamStart),
            (
                G::DuplicateVersionDirective,
                SyntaxKind::DuplicateVersionDirective,
            ),
            (G::DuplicateTagDirective, SyntaxKind::DuplicateTagDirective),
            (G::ExpectedDocumentStart, SyntaxKind::ExpectedDocumentStart),
            (
                G::MissingDocumentEndBeforeDirective,
                SyntaxKind::MissingDocumentEndBeforeDirective,
            ),
            (G::AnchorCountOverflow, SyntaxKind::AnchorCountOverflow),
            (G::ExpectedNodeContent, SyntaxKind::ExpectedNodeContent),
            (
                G::ExpectedBlockMappingKey,
                SyntaxKind::ExpectedBlockMappingKey,
            ),
            (
                G::ExpectedFlowMappingSeparator,
                SyntaxKind::ExpectedFlowMappingSeparator,
            ),
            (
                G::ExpectedFlowSequenceSeparator,
                SyntaxKind::ExpectedFlowSequenceSeparator,
            ),
            (
                G::ExpectedBlockSequenceEntry,
                SyntaxKind::ExpectedBlockSequenceEntry,
            ),
            (G::UndeclaredTagHandle, SyntaxKind::UndeclaredTagHandle),
            (G::ExpectedTagBang, SyntaxKind::ExpectedTagBang),
            (
                G::ExpectedTagDirectiveBang,
                SyntaxKind::ExpectedTagDirectiveBang,
            ),
            (
                G::InvalidGlobalTagCharacter,
                SyntaxKind::InvalidGlobalTagCharacter,
            ),
            (G::SimpleKeyExpected, SyntaxKind::SimpleKeyExpected),
            (G::InvalidSimpleKey, SyntaxKind::InvalidSimpleKey),
            (G::InvalidDocumentEnd, SyntaxKind::InvalidDocumentEnd),
            (G::InvalidIndentation, SyntaxKind::InvalidIndentation),
            (G::BomInsideDocument, SyntaxKind::BomInsideDocument),
            (
                G::UnexpectedCharacter { character: '@' },
                SyntaxKind::UnexpectedCharacter,
            ),
            (G::TabNotAllowed, SyntaxKind::TabNotAllowed),
            (G::TabInBlockIndentation, SyntaxKind::TabInBlockIndentation),
            (
                G::CommentInterceptedScalar,
                SyntaxKind::CommentInterceptedScalar,
            ),
            (G::ExpectedWhitespace, SyntaxKind::ExpectedWhitespace),
            (G::CommentNotSeparated, SyntaxKind::CommentNotSeparated),
            (
                G::InvalidDirectiveTerminator,
                SyntaxKind::InvalidDirectiveTerminator,
            ),
            (
                G::MissingYamlVersionSeparator,
                SyntaxKind::MissingYamlVersionSeparator,
            ),
            (G::MissingDirectiveName, SyntaxKind::MissingDirectiveName),
            (G::InvalidDirectiveName, SyntaxKind::InvalidDirectiveName),
            (
                G::DirectiveByteLimitExceeded { limit: 64 },
                SyntaxKind::DirectiveByteLimitExceeded,
            ),
            (
                G::TooManyReservedDirectiveParams { limit: 8 },
                SyntaxKind::TooManyReservedDirectiveParams,
            ),
            (G::YamlVersionTooLong, SyntaxKind::YamlVersionTooLong),
            (G::MissingYamlVersion, SyntaxKind::MissingYamlVersion),
            (
                G::InvalidTagDirectiveTerminator,
                SyntaxKind::InvalidTagDirectiveTerminator,
            ),
            (G::InvalidTagTerminator, SyntaxKind::InvalidTagTerminator),
            (G::MissingTagUri, SyntaxKind::MissingTagUri),
            (G::UnclosedVerbatimTag, SyntaxKind::UnclosedVerbatimTag),
            (G::InvalidTagEscape, SyntaxKind::InvalidTagEscape),
            (
                G::InvalidTagUtf8LeadingByte,
                SyntaxKind::InvalidTagUtf8LeadingByte,
            ),
            (
                G::InvalidTagUtf8TrailingByte,
                SyntaxKind::InvalidTagUtf8TrailingByte,
            ),
            (G::InvalidTagUtf8, SyntaxKind::InvalidTagUtf8),
            (
                G::MissingAnchorOrAliasName,
                SyntaxKind::MissingAnchorOrAliasName,
            ),
            (
                G::MisplacedFlowCollectionEnd,
                SyntaxKind::MisplacedFlowCollectionEnd,
            ),
            (
                G::MismatchedFlowCollectionEnd {
                    open: '[',
                    close: '}',
                },
                SyntaxKind::MismatchedFlowCollectionEnd,
            ),
            (
                G::UnclosedFlowCollection { open: '[' },
                SyntaxKind::UnclosedFlowCollection,
            ),
            (
                G::BlockEntryInFlowCollection,
                SyntaxKind::BlockEntryInFlowCollection,
            ),
            (
                G::BlockSequenceEntryNotAllowed,
                SyntaxKind::BlockSequenceEntryNotAllowed,
            ),
            (
                G::InvalidBlockEntryWhitespace,
                SyntaxKind::InvalidBlockEntryWhitespace,
            ),
            (G::ZeroBlockScalarIndent, SyntaxKind::ZeroBlockScalarIndent),
            (
                G::InvalidBlockScalarHeader,
                SyntaxKind::InvalidBlockScalarHeader,
            ),
            (G::TabAtBlockScalarStart, SyntaxKind::TabAtBlockScalarStart),
            (
                G::InvalidBlockScalarIndent,
                SyntaxKind::InvalidBlockScalarIndent,
            ),
            (
                G::DocumentIndicatorInQuotedScalar,
                SyntaxKind::DocumentIndicatorInQuotedScalar,
            ),
            (G::UnclosedQuotedScalar, SyntaxKind::UnclosedQuotedScalar),
            (G::TabInIndentation, SyntaxKind::TabInIndentation),
            (
                G::InvalidQuotedScalarIndent,
                SyntaxKind::InvalidQuotedScalarIndent,
            ),
            (
                G::InvalidTrailingSingleQuotedScalar,
                SyntaxKind::InvalidTrailingSingleQuotedScalar,
            ),
            (
                G::InvalidTrailingDoubleQuotedScalar,
                SyntaxKind::InvalidTrailingDoubleQuotedScalar,
            ),
            (
                G::UnknownQuotedScalarEscape,
                SyntaxKind::UnknownQuotedScalarEscape,
            ),
            (
                G::InvalidQuotedScalarHexEscape,
                SyntaxKind::InvalidQuotedScalarHexEscape,
            ),
            (
                G::InvalidLowSurrogateHexEscape,
                SyntaxKind::InvalidLowSurrogateHexEscape,
            ),
            (G::InvalidLowSurrogate, SyntaxKind::InvalidLowSurrogate),
            (G::MissingLowSurrogate, SyntaxKind::MissingLowSurrogate),
            (G::UnpairedLowSurrogate, SyntaxKind::UnpairedLowSurrogate),
            (G::InvalidUnicodeEscape, SyntaxKind::InvalidUnicodeEscape),
            (
                G::InvalidFlowScalarIndent,
                SyntaxKind::InvalidFlowScalarIndent,
            ),
            (
                G::PlainScalarStartsWithDashFlowIndicator,
                SyntaxKind::PlainScalarStartsWithDashFlowIndicator,
            ),
            (G::TabInPlainScalar, SyntaxKind::TabInPlainScalar),
            (
                G::UnexpectedEndOfPlainScalar,
                SyntaxKind::UnexpectedEndOfPlainScalar,
            ),
            (G::MappingKeyNotAllowed, SyntaxKind::MappingKeyNotAllowed),
            (
                G::FlowMappingValueAdjacentCollection,
                SyntaxKind::FlowMappingValueAdjacentCollection,
            ),
            (
                G::InvalidMappingValueWhitespace,
                SyntaxKind::InvalidMappingValueWhitespace,
            ),
            (G::InvalidColonPlacement, SyntaxKind::InvalidColonPlacement),
            (
                G::MappingValueNotAllowed,
                SyntaxKind::MappingValueNotAllowed,
            ),
        ];
        assert_eq!(syntax_pairs.len(), SyntaxKind::ALL.len());
        for (idx, (g, expected)) in syntax_pairs.iter().enumerate() {
            assert_eq!(SyntaxKind::ALL[idx], *expected);
            assert_eq!(syntax_kind(g), Some(*expected));
        }

        for non_syntax in [
            G::UnknownAnchor,
            G::UnsupportedYamlMajorVersion,
            G::RecursionLimitExceeded,
            G::InputIo {
                error: InputIoError::from_message("io"),
            },
            G::InputDecoding {
                message: "utf8".into(),
            },
            G::InputByteLimitExceeded { limit: 1 },
            G::MissingIncludeResolver,
            G::Custom("custom".into()),
            G::MultipleDocumentsUnsupported,
            G::InputOffsetsWithoutSlice,
            G::InputSlicingUnavailable,
        ] {
            assert_eq!(syntax_kind(&non_syntax), None);
        }
    }
}
