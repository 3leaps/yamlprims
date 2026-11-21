//! The only backend boundary. Translate engine events into private owned vocabulary.
//! Scalar text retains its borrowed/owned storage without copying it.
use crate::budget::limit_err;
use crate::error::internal;
use crate::error::{Error, ErrorKind, LimitKind, SyntaxKind};
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
        let at = span.tag_start().unwrap_or(span.start);
        if t.handle().is_empty() && t.suffix() == "!" {
            // Only the bare `!` token is non-specific; a verbatim `!<!>` is a
            // specific tag whose identity is `!` alone (YAML 1.2.2 Example 6.25).
            let verbatim = at
                .byte_offset()
                .and_then(|b| self.input.get(b..))
                .is_some_and(|rest| rest.starts_with("!<"));
            if !verbatim {
                return Ok(());
            }
        } else if valid_resolved_tag_identity(&t.to_string()) {
            return Ok(());
        }
        Err(Error::at(
            ErrorKind::Syntax(SyntaxKind::InvalidGlobalTagCharacter),
            self.loc(&at)?,
            INVALID_TAG_IDENTITY_MESSAGE,
        ))
    }

    /// Revision 1.7 `%TAG` checks (spec §7.1), made where the parser processes
    /// a document prologue. The pinned engine accepts a named handle containing
    /// `_` and a prefix that immediately follows the handle. `from` is the end
    /// of the last yielded event (stream start or document end), and `until`
    /// the position of the next event or engine error. A directive whose line
    /// (up to its break or NUL, trailing comment included) ends at or before it
    /// was tokenized and is checked in source order, as the parser processes
    /// it; a directive the error falls inside, or starts at, is the one the
    /// engine failed on while scanning or processing it (including a duplicate
    /// handle or a malformed trailing comment), and keeps that failure. Before it the engine has accepted only directive
    /// tokens, comments, blank lines and document end markers; a content line
    /// ends the prologue.
    fn prologue_directives(&self, from: usize, until: usize) -> Result<Option<Error>, Error> {
        let src = self.input.as_bytes();
        let until = until.min(src.len());
        let mut i = from.min(until);
        if i > 0 && !matches!(src[i - 1], b'\n' | b'\r') {
            i = next_line(src, i);
        }
        while i < until {
            // A document prologue may start with byte order marks (YAML 1.2.2
            // l-document-prefix); the engine skips them and does not count them
            // as columns.
            let mut c = i;
            while src[c..].starts_with(BOM) {
                c += BOM.len();
            }
            match src.get(c).copied().unwrap_or(b'\n') {
                b'%' => {
                    // The engine tokenized this directive only if it reached the
                    // directive's end (its trailing comment included). A failure
                    // inside the line is the engine's own scanner diagnostic.
                    let mut end = c;
                    while end < src.len() && !matches!(src[end], b'\n' | b'\r' | b'\0') {
                        end += 1;
                    }
                    if end > until || !directive_completes(src, c, end) {
                        return Ok(None);
                    }
                    if let Some((kind, at)) = tag_directive_fault(src, c) {
                        return Ok(Some(self.directive_error(kind, at)?));
                    }
                }
                b'#' | b'\n' | b'\r' | b' ' | b'\t' => {
                    let mut j = c;
                    while j < src.len() && matches!(src[j], b' ' | b'\t') {
                        j += 1;
                    }
                    if j < src.len() && !matches!(src[j], b'#' | b'\n' | b'\r') {
                        return Ok(None);
                    }
                }
                b'.' if src[c..].starts_with(b"...") => {}
                _ => return Ok(None),
            }
            i = next_line(src, i);
        }
        Ok(None)
    }

    /// A tag using a named handle outside `[0-9A-Za-z-]+` reaches the engine
    /// as an undeclared handle (its directive is refused first); report it as
    /// `InvalidTagTerminator` at the tag (spec §4.4, §7.1).
    fn malformed_handle_use(&self, e: &ScanError) -> Option<Error> {
        if !matches!(e.kind(), granit_parser::ErrorKind::UndeclaredTagHandle) {
            return None;
        }
        let b = e.marker().byte_offset()?;
        let src = self.input.as_bytes();
        let (_, malformed) = scan_handle(src, b)?;
        if !malformed {
            return None;
        }
        let loc = self.loc(e.marker()).ok()?;
        let mut err = Error::at(
            ErrorKind::Syntax(SyntaxKind::InvalidTagTerminator),
            loc,
            format!("syntax error: {INVALID_TAG_TERMINATOR_DETAIL}"),
        );
        err.engine_detail = Some(INVALID_TAG_TERMINATOR_DETAIL.to_owned());
        Some(err)
    }

    fn directive_error(&self, kind: SyntaxKind, at: usize) -> Result<Error, Error> {
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
        let line_text = &before[line_start..];
        let column = line_text.trim_start_matches('\u{feff}').chars().count() + 1;
        let detail = match kind {
            SyntaxKind::ExpectedWhitespace => "expected whitespace",
            _ => EXPECTED_TAG_DIRECTIVE_BANG_DETAIL,
        };
        let mut err = Error::at(
            ErrorKind::Syntax(kind),
            Location {
                byte_offset: at + self.bom,
                line,
                column,
            },
            format!("syntax error: {detail}"),
        );
        err.engine_detail = Some(detail.to_owned());
        Ok(err)
    }

    pub(crate) fn syntax_pass(&self) -> Result<(), Error> {
        #[cfg(test)]
        validation_tests::observe(|s| s.source_passes += 1);
        let mut depth = 0usize;
        // End of the last stream-start or document-end event while the parser
        // is in a document prologue; `None` inside a document.
        let mut prologue: Option<usize> = None;
        for ev in Self::parser(self.input, self.opts) {
            #[cfg(test)]
            validation_tests::observe(|s| s.source_results += 1);
            let (event, span) = match ev {
                Ok(ok) => ok,
                Err(e) => {
                    if let (Some(from), Some(b)) = (prologue, e.marker().byte_offset()) {
                        if let Some(err) = self.prologue_directives(from, b)? {
                            return Err(err);
                        }
                    }
                    if let Some(err) = self.malformed_handle_use(&e) {
                        return Err(err);
                    }
                    return Err(self.scan_err(e));
                }
            };
            if let (GEvent::DocumentStart(true, _), Some(from)) = (&event, prologue) {
                let at = span
                    .start
                    .byte_offset()
                    .ok_or_else(|| internal("document start without byte offset"))?;
                if let Some(err) = self.prologue_directives(from, at)? {
                    return Err(err);
                }
            }
            prologue = match &event {
                GEvent::StreamStart | GEvent::DocumentEnd => span.end.byte_offset(),
                _ => None,
            };
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

const BOM: &[u8] = "\u{feff}".as_bytes();

const EXPECTED_TAG_DIRECTIVE_BANG_DETAIL: &str =
    "while parsing a tag directive, did not find expected '!'";
const INVALID_TAG_TERMINATOR_DETAIL: &str =
    "while scanning a tag, did not find expected whitespace or line break";

/// Index just past the line break ending the line that holds `i` (or the end).
fn next_line(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && !matches!(src[i], b'\n' | b'\r') {
        i += 1;
    }
    if i < src.len() {
        i += if src[i] == b'\r' && src.get(i + 1) == Some(&b'\n') {
            2
        } else {
            1
        };
    }
    i
}

/// A tag handle starting at `b`, scanned as the engine does: `!`, word
/// characters (ASCII alphanumerics, `_`, `-`), then an optional `!`. Returns
/// the end of the handle and whether it is a named handle `!word!` whose word
/// uses a character outside `[0-9A-Za-z-]` (YAML 1.2.2 productions [38], [92]).
/// `None` unless `b` holds `!`.
fn scan_handle(src: &[u8], b: usize) -> Option<(usize, bool)> {
    if src.get(b) != Some(&b'!') {
        return None;
    }
    let mut i = b + 1;
    let mut underscore = false;
    while i < src.len() && (src[i].is_ascii_alphanumeric() || matches!(src[i], b'_' | b'-')) {
        underscore |= src[i] == b'_';
        i += 1;
    }
    let named = i > b + 1 && src.get(i) == Some(&b'!');
    if src.get(i) == Some(&b'!') {
        i += 1;
    }
    Some((i, named && underscore))
}

/// Whether the engine finishes tokenizing the `%TAG` directive at `pct` whose
/// line ends at `end` (a break, NUL or the end of input). A trailing comment
/// runs to the next break and must then be followed by a printable character,
/// so a comment ended by a NUL fails inside the directive's own scan; a NUL
/// directly after the prefix (or its trailing blanks) ends the directive.
fn directive_completes(src: &[u8], pct: usize, end: usize) -> bool {
    if src.get(end) != Some(&b'\0') {
        return true;
    }
    let is_blank = |b: u8| matches!(b, b' ' | b'\t');
    let line = &src[pct..end];
    if !line.starts_with(b"%TAG") {
        return true;
    }
    let mut i = 4;
    while i < line.len() && is_blank(line[i]) {
        i += 1;
    }
    let Some((h, _)) = scan_handle(line, i) else {
        return true;
    };
    i = h;
    while i < line.len() && is_blank(line[i]) {
        i += 1;
    }
    while i < line.len() && !is_blank(line[i]) {
        i += 1;
    }
    while i < line.len() && is_blank(line[i]) {
        i += 1;
    }
    line.get(i) != Some(&b'#')
}

/// For a directive at `pct` that the engine has tokenized, the revision 1.7
/// `%TAG` fault in processing order: a malformed named handle
/// (`ExpectedTagDirectiveBang` at the directive start), then a prefix that
/// immediately follows the handle (`ExpectedWhitespace` at that character).
fn tag_directive_fault(src: &[u8], pct: usize) -> Option<(SyntaxKind, usize)> {
    let is_blank = |b: u8| matches!(b, b' ' | b'\t');
    let is_breakz = |b: u8| matches!(b, b'\n' | b'\r' | b'\0');
    if src.get(pct) != Some(&b'%') {
        return None;
    }
    let mut i = pct + 1;
    let name_start = i;
    while i < src.len() && !is_blank(src[i]) && !is_breakz(src[i]) {
        i += 1;
    }
    if &src[name_start..i] != b"TAG" {
        return None;
    }
    while i < src.len() && is_blank(src[i]) {
        i += 1;
    }
    let (end, malformed) = scan_handle(src, i)?;
    if malformed {
        return Some((SyntaxKind::ExpectedTagDirectiveBang, pct));
    }
    match src.get(end) {
        Some(&b) if !is_blank(b) && !is_breakz(b) => Some((SyntaxKind::ExpectedWhitespace, end)),
        _ => None,
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
