//! Owned errors (spec §7).

use crate::value::{Location, Span, Value};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Site {
    /// Input-attributable error on parsed input or a parsed node.
    Source(Location),
    /// Synthetic node or non-input failure. `last_known` is context only.
    NotSource { last_known: Option<Location> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelatedRole {
    FirstDefinition,
    AliasUse,
    MergeDirective,
    AnchorDefinition,
}

impl RelatedRole {
    pub fn name(&self) -> &'static str {
        match self {
            RelatedRole::FirstDefinition => "first_definition",
            RelatedRole::AliasUse => "alias_use",
            RelatedRole::MergeDirective => "merge_directive",
            RelatedRole::AnchorDefinition => "anchor_definition",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Related {
    pub role: RelatedRole,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum PathSegment {
    Key(Box<Value>),
    Index(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    MaxInputBytes,
    MaxDepth,
    MaxNodes,
    MaxExpandedNodes,
    MaxAliasEvents,
    MaxOwnedBytes,
    MaxScalarBytes,
    MaxKeyBytes,
    MaxMappingEntries,
    MaxDocuments,
}

impl LimitKind {
    pub fn field_name(&self) -> &'static str {
        match self {
            LimitKind::MaxInputBytes => "max_input_bytes",
            LimitKind::MaxDepth => "max_depth",
            LimitKind::MaxNodes => "max_nodes",
            LimitKind::MaxExpandedNodes => "max_expanded_nodes",
            LimitKind::MaxAliasEvents => "max_alias_events",
            LimitKind::MaxOwnedBytes => "max_owned_bytes",
            LimitKind::MaxScalarBytes => "max_scalar_bytes",
            LimitKind::MaxKeyBytes => "max_key_bytes",
            LimitKind::MaxMappingEntries => "max_mapping_entries",
            LimitKind::MaxDocuments => "max_documents",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    InvalidUtf8 { invalid_len: Option<usize> },
    Syntax(String),
    UnsupportedYamlVersion,
    LimitExceeded { limit: LimitKind, configured: u64 },
    InvalidOptions,
    DuplicateKey,
    InvalidKey,
    UnknownAnchor,
    RecursiveAlias,
    MergeRejected,
    MergeSource,
    UnknownTag,
    TagKindMismatch,
    TagValueMismatch,
    IntegerOutOfRange,
    FloatOutOfRange,
    MultipleDocuments,
    Type { expected: String, found: String },
    MissingField(String),
    UnknownField(String),
    Custom,
    OutputLimitExceeded { configured: u64 },
    Internal,
}

impl ErrorKind {
    pub fn name(&self) -> &'static str {
        match self {
            ErrorKind::InvalidUtf8 { .. } => "invalid_utf8",
            ErrorKind::Syntax(_) => "syntax",
            ErrorKind::UnsupportedYamlVersion => "unsupported_yaml_version",
            ErrorKind::LimitExceeded { .. } => "limit_exceeded",
            ErrorKind::InvalidOptions => "invalid_options",
            ErrorKind::DuplicateKey => "duplicate_key",
            ErrorKind::InvalidKey => "invalid_key",
            ErrorKind::UnknownAnchor => "unknown_anchor",
            ErrorKind::RecursiveAlias => "recursive_alias",
            ErrorKind::MergeRejected => "merge_rejected",
            ErrorKind::MergeSource => "merge_source",
            ErrorKind::UnknownTag => "unknown_tag",
            ErrorKind::TagKindMismatch => "tag_kind_mismatch",
            ErrorKind::TagValueMismatch => "tag_value_mismatch",
            ErrorKind::IntegerOutOfRange => "integer_out_of_range",
            ErrorKind::FloatOutOfRange => "float_out_of_range",
            ErrorKind::MultipleDocuments => "multiple_documents",
            ErrorKind::Type { .. } => "type",
            ErrorKind::MissingField(_) => "missing_field",
            ErrorKind::UnknownField(_) => "unknown_field",
            ErrorKind::Custom => "custom",
            ErrorKind::OutputLimitExceeded { .. } => "output_limit_exceeded",
            ErrorKind::Internal => "internal",
        }
    }
}

pub(crate) const MAX_MESSAGE: usize = 4096;
/// Bound on text copied into an `ErrorKind` payload (a type description or field name).
pub(crate) const MAX_FIELD: usize = 1024;

/// An error. The details are boxed so `Error` itself is one pointer wide and does not
/// inflate `Result<T, Error>`. Read fields through `Deref` (`e.kind`, `&e.related`,
/// `match &e.kind`); to move a non-`Copy` field out, use `into_detail()`.
///
/// Compile probes (spec §13.2). Borrowing, mutating through `DerefMut` and moving out of
/// `into_detail()` compile:
///
/// ```
/// let mut e = yamlprims::load("[").unwrap_err();
/// let kind: &yamlprims::ErrorKind = &e.kind;
/// let _ = kind;
/// e.message.push('!');
/// let _ = yamlprims::encode_error(&e, &yamlprims::Options::default());
/// let message: String = e.into_detail().message;
/// assert!(message.ends_with('!'));
/// ```
///
/// Revision 1.0 struct patterns do not:
///
/// ```compile_fail,E0026
/// let e = yamlprims::load("[").unwrap_err();
/// let yamlprims::Error { kind,.. } = e;
/// ```
///
/// Nor do moves of non-`Copy` fields through `Deref`:
///
/// ```compile_fail,E0507
/// let e = yamlprims::load("[").unwrap_err();
/// let message: String = e.message;
/// ```
///
/// ```compile_fail,E0507
/// let e = yamlprims::load("[").unwrap_err();
/// let kind: yamlprims::ErrorKind = e.kind;
/// ```
#[derive(Clone)]
pub struct Error(Box<ErrorDetail>);

/// The fields of an [`Error`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ErrorDetail {
    pub kind: ErrorKind,
    pub site: Site,
    pub related: Vec<Related>,
    pub path: Vec<PathSegment>,
    pub message: String,
    pub engine_detail: Option<String>,
    /// Set for errors raised through serde's trait constructors before the
    /// deserializer attaches the node's site.
    pub(crate) unsited: bool,
}

impl Error {
    pub(crate) fn at(kind: ErrorKind, loc: Location, message: impl fmt::Display) -> Error {
        Error::new(kind, Site::Source(loc), message)
    }
    pub(crate) fn not_source(
        kind: ErrorKind,
        last_known: Option<Location>,
        message: impl fmt::Display,
    ) -> Error {
        Error::new(kind, Site::NotSource { last_known }, message)
    }
    /// An error with empty `related` and `path` and no `engine_detail`. The message is
    /// formatted into a buffer of at most 4096 bytes, truncated with `…` on a character
    /// boundary; longer output is never built first.
    pub fn new(kind: ErrorKind, site: Site, message: impl fmt::Display) -> Error {
        Error(Box::new(ErrorDetail {
            kind,
            site,
            related: Vec::new(),
            path: Vec::new(),
            message: bounded(message, MAX_MESSAGE),
            engine_detail: None,
            unsited: false,
        }))
    }
    /// The details, by value.
    pub fn into_detail(self) -> ErrorDetail {
        *self.0
    }
    pub(crate) fn with_related(mut self, role: RelatedRole, span: Span) -> Error {
        self.related.push(Related { role, span });
        self
    }
    pub fn location(&self) -> Option<Location> {
        match self.site {
            Site::Source(l) => Some(l),
            Site::NotSource { .. } => None,
        }
    }
}

impl std::ops::Deref for Error {
    type Target = ErrorDetail;
    fn deref(&self) -> &ErrorDetail {
        &self.0
    }
}

impl std::ops::DerefMut for Error {
    fn deref_mut(&mut self) -> &mut ErrorDetail {
        &mut self.0
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Format `msg` into at most `max` bytes, truncating on a character boundary with `…`.
/// Formatting stops once the buffer is full, so longer text is never built.
pub(crate) fn bounded(msg: impl fmt::Display, max: usize) -> String {
    struct Buf {
        s: String,
        max: usize,
        over: bool,
    }
    impl fmt::Write for Buf {
        fn write_str(&mut self, t: &str) -> fmt::Result {
            let room = self.max - self.s.len();
            if t.len() <= room {
                self.s.push_str(t);
                return Ok(());
            }
            let mut i = room;
            while !t.is_char_boundary(i) {
                i -= 1;
            }
            self.s.push_str(&t[..i]);
            self.over = true;
            Err(fmt::Error)
        }
    }
    let mut b = Buf {
        s: String::new(),
        max,
        over: false,
    };
    let _ = fmt::write(&mut b, format_args!("{msg}"));
    if b.over {
        let mut i = max.saturating_sub('…'.len_utf8());
        while !b.s.is_char_boundary(i) {
            i -= 1;
        }
        b.s.truncate(i);
        b.s.push('…');
    }
    b.s
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.site {
            Site::Source(l) => write!(
                f,
                "{}:{}: {}: {}",
                l.line,
                l.column,
                self.kind.name(),
                self.message
            ),
            Site::NotSource { .. } => write!(f, "{}: {}", self.kind.name(), self.message),
        }
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        let mut e = Error::not_source(ErrorKind::Custom, None, msg);
        e.unsited = true;
        e
    }
    fn invalid_type(unexp: serde::de::Unexpected, exp: &dyn serde::de::Expected) -> Self {
        let mut e = Error::not_source(
            ErrorKind::Type {
                expected: bounded(exp, MAX_FIELD),
                found: bounded(unexp, MAX_FIELD),
            },
            None,
            format_args!("expected {exp}, found {unexp}"),
        );
        e.unsited = true;
        e
    }
    fn missing_field(field: &'static str) -> Self {
        let mut e = Error::not_source(
            ErrorKind::MissingField(field.into()),
            None,
            format_args!("missing field `{field}`"),
        );
        e.unsited = true;
        e
    }
    fn unknown_field(field: &str, expected: &'static [&'static str]) -> Self {
        let mut e = Error::not_source(
            ErrorKind::UnknownField(bounded(field, MAX_FIELD)),
            None,
            format_args!(
                "unknown field `{}`, expected one of {:?}",
                bounded(field, 256),
                expected
            ),
        );
        e.unsited = true;
        e
    }
}

pub(crate) fn internal(what: &str) -> Error {
    Error::not_source(ErrorKind::Internal, None, format!("internal error: {what}"))
}
