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

/// Closed vocabulary of YAML syntax error codes produced by the parser (spec §7.1, §11.1).
///
/// [`ErrorKind::Syntax`] carries a [`SyntaxKind`] directly, closing both the Rust API and
/// the `yamlprims-value/1` wire `syntax_kind` vocabulary to the 81 codes in [`SyntaxKind::ALL`].
/// `#[non_exhaustive]` reserves future specification revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SyntaxKind {
    UnexpectedEofFlowSequence,
    UnexpectedEofFlowMapping,
    UnexpectedEofImplicitFlowMapping,
    UnexpectedEofBlockSequence,
    UnexpectedEofBlockMapping,
    UnexpectedEof,
    ExpectedStreamStart,
    DuplicateVersionDirective,
    DuplicateTagDirective,
    ExpectedDocumentStart,
    MissingDocumentEndBeforeDirective,
    AnchorCountOverflow,
    ExpectedNodeContent,
    ExpectedBlockMappingKey,
    ExpectedFlowMappingSeparator,
    ExpectedFlowSequenceSeparator,
    ExpectedBlockSequenceEntry,
    UndeclaredTagHandle,
    ExpectedTagBang,
    ExpectedTagDirectiveBang,
    InvalidGlobalTagCharacter,
    SimpleKeyExpected,
    InvalidSimpleKey,
    InvalidDocumentEnd,
    InvalidIndentation,
    BomInsideDocument,
    UnexpectedCharacter,
    TabNotAllowed,
    TabInBlockIndentation,
    CommentInterceptedScalar,
    ExpectedWhitespace,
    CommentNotSeparated,
    InvalidDirectiveTerminator,
    MissingYamlVersionSeparator,
    MissingDirectiveName,
    InvalidDirectiveName,
    DirectiveByteLimitExceeded,
    TooManyReservedDirectiveParams,
    YamlVersionTooLong,
    MissingYamlVersion,
    InvalidTagDirectiveTerminator,
    InvalidTagTerminator,
    MissingTagUri,
    UnclosedVerbatimTag,
    InvalidTagEscape,
    InvalidTagUtf8LeadingByte,
    InvalidTagUtf8TrailingByte,
    InvalidTagUtf8,
    MissingAnchorOrAliasName,
    MisplacedFlowCollectionEnd,
    MismatchedFlowCollectionEnd,
    UnclosedFlowCollection,
    BlockEntryInFlowCollection,
    BlockSequenceEntryNotAllowed,
    InvalidBlockEntryWhitespace,
    ZeroBlockScalarIndent,
    InvalidBlockScalarHeader,
    TabAtBlockScalarStart,
    InvalidBlockScalarIndent,
    DocumentIndicatorInQuotedScalar,
    UnclosedQuotedScalar,
    TabInIndentation,
    InvalidQuotedScalarIndent,
    InvalidTrailingSingleQuotedScalar,
    InvalidTrailingDoubleQuotedScalar,
    UnknownQuotedScalarEscape,
    InvalidQuotedScalarHexEscape,
    InvalidLowSurrogateHexEscape,
    InvalidLowSurrogate,
    MissingLowSurrogate,
    UnpairedLowSurrogate,
    InvalidUnicodeEscape,
    InvalidFlowScalarIndent,
    PlainScalarStartsWithDashFlowIndicator,
    TabInPlainScalar,
    UnexpectedEndOfPlainScalar,
    MappingKeyNotAllowed,
    FlowMappingValueAdjacentCollection,
    InvalidMappingValueWhitespace,
    InvalidColonPlacement,
    MappingValueNotAllowed,
}

impl SyntaxKind {
    /// Complete closed table of all parser-emitted syntax error kinds.
    pub const ALL: &'static [SyntaxKind] = &[
        SyntaxKind::UnexpectedEofFlowSequence,
        SyntaxKind::UnexpectedEofFlowMapping,
        SyntaxKind::UnexpectedEofImplicitFlowMapping,
        SyntaxKind::UnexpectedEofBlockSequence,
        SyntaxKind::UnexpectedEofBlockMapping,
        SyntaxKind::UnexpectedEof,
        SyntaxKind::ExpectedStreamStart,
        SyntaxKind::DuplicateVersionDirective,
        SyntaxKind::DuplicateTagDirective,
        SyntaxKind::ExpectedDocumentStart,
        SyntaxKind::MissingDocumentEndBeforeDirective,
        SyntaxKind::AnchorCountOverflow,
        SyntaxKind::ExpectedNodeContent,
        SyntaxKind::ExpectedBlockMappingKey,
        SyntaxKind::ExpectedFlowMappingSeparator,
        SyntaxKind::ExpectedFlowSequenceSeparator,
        SyntaxKind::ExpectedBlockSequenceEntry,
        SyntaxKind::UndeclaredTagHandle,
        SyntaxKind::ExpectedTagBang,
        SyntaxKind::ExpectedTagDirectiveBang,
        SyntaxKind::InvalidGlobalTagCharacter,
        SyntaxKind::SimpleKeyExpected,
        SyntaxKind::InvalidSimpleKey,
        SyntaxKind::InvalidDocumentEnd,
        SyntaxKind::InvalidIndentation,
        SyntaxKind::BomInsideDocument,
        SyntaxKind::UnexpectedCharacter,
        SyntaxKind::TabNotAllowed,
        SyntaxKind::TabInBlockIndentation,
        SyntaxKind::CommentInterceptedScalar,
        SyntaxKind::ExpectedWhitespace,
        SyntaxKind::CommentNotSeparated,
        SyntaxKind::InvalidDirectiveTerminator,
        SyntaxKind::MissingYamlVersionSeparator,
        SyntaxKind::MissingDirectiveName,
        SyntaxKind::InvalidDirectiveName,
        SyntaxKind::DirectiveByteLimitExceeded,
        SyntaxKind::TooManyReservedDirectiveParams,
        SyntaxKind::YamlVersionTooLong,
        SyntaxKind::MissingYamlVersion,
        SyntaxKind::InvalidTagDirectiveTerminator,
        SyntaxKind::InvalidTagTerminator,
        SyntaxKind::MissingTagUri,
        SyntaxKind::UnclosedVerbatimTag,
        SyntaxKind::InvalidTagEscape,
        SyntaxKind::InvalidTagUtf8LeadingByte,
        SyntaxKind::InvalidTagUtf8TrailingByte,
        SyntaxKind::InvalidTagUtf8,
        SyntaxKind::MissingAnchorOrAliasName,
        SyntaxKind::MisplacedFlowCollectionEnd,
        SyntaxKind::MismatchedFlowCollectionEnd,
        SyntaxKind::UnclosedFlowCollection,
        SyntaxKind::BlockEntryInFlowCollection,
        SyntaxKind::BlockSequenceEntryNotAllowed,
        SyntaxKind::InvalidBlockEntryWhitespace,
        SyntaxKind::ZeroBlockScalarIndent,
        SyntaxKind::InvalidBlockScalarHeader,
        SyntaxKind::TabAtBlockScalarStart,
        SyntaxKind::InvalidBlockScalarIndent,
        SyntaxKind::DocumentIndicatorInQuotedScalar,
        SyntaxKind::UnclosedQuotedScalar,
        SyntaxKind::TabInIndentation,
        SyntaxKind::InvalidQuotedScalarIndent,
        SyntaxKind::InvalidTrailingSingleQuotedScalar,
        SyntaxKind::InvalidTrailingDoubleQuotedScalar,
        SyntaxKind::UnknownQuotedScalarEscape,
        SyntaxKind::InvalidQuotedScalarHexEscape,
        SyntaxKind::InvalidLowSurrogateHexEscape,
        SyntaxKind::InvalidLowSurrogate,
        SyntaxKind::MissingLowSurrogate,
        SyntaxKind::UnpairedLowSurrogate,
        SyntaxKind::InvalidUnicodeEscape,
        SyntaxKind::InvalidFlowScalarIndent,
        SyntaxKind::PlainScalarStartsWithDashFlowIndicator,
        SyntaxKind::TabInPlainScalar,
        SyntaxKind::UnexpectedEndOfPlainScalar,
        SyntaxKind::MappingKeyNotAllowed,
        SyntaxKind::FlowMappingValueAdjacentCollection,
        SyntaxKind::InvalidMappingValueWhitespace,
        SyntaxKind::InvalidColonPlacement,
        SyntaxKind::MappingValueNotAllowed,
    ];

    /// Canonical wire and diagnostic `syntax_kind` code (`[a-z0-9_]{1,64}`).
    pub const fn as_str(&self) -> &'static str {
        match self {
            SyntaxKind::UnexpectedEofFlowSequence => "unexpected_eof_flow_sequence",
            SyntaxKind::UnexpectedEofFlowMapping => "unexpected_eof_flow_mapping",
            SyntaxKind::UnexpectedEofImplicitFlowMapping => "unexpected_eof_implicit_flow_mapping",
            SyntaxKind::UnexpectedEofBlockSequence => "unexpected_eof_block_sequence",
            SyntaxKind::UnexpectedEofBlockMapping => "unexpected_eof_block_mapping",
            SyntaxKind::UnexpectedEof => "unexpected_eof",
            SyntaxKind::ExpectedStreamStart => "expected_stream_start",
            SyntaxKind::DuplicateVersionDirective => "duplicate_version_directive",
            SyntaxKind::DuplicateTagDirective => "duplicate_tag_directive",
            SyntaxKind::ExpectedDocumentStart => "expected_document_start",
            SyntaxKind::MissingDocumentEndBeforeDirective => {
                "missing_document_end_before_directive"
            }
            SyntaxKind::AnchorCountOverflow => "anchor_count_overflow",
            SyntaxKind::ExpectedNodeContent => "expected_node_content",
            SyntaxKind::ExpectedBlockMappingKey => "expected_block_mapping_key",
            SyntaxKind::ExpectedFlowMappingSeparator => "expected_flow_mapping_separator",
            SyntaxKind::ExpectedFlowSequenceSeparator => "expected_flow_sequence_separator",
            SyntaxKind::ExpectedBlockSequenceEntry => "expected_block_sequence_entry",
            SyntaxKind::UndeclaredTagHandle => "undeclared_tag_handle",
            SyntaxKind::ExpectedTagBang => "expected_tag_bang",
            SyntaxKind::ExpectedTagDirectiveBang => "expected_tag_directive_bang",
            SyntaxKind::InvalidGlobalTagCharacter => "invalid_global_tag_character",
            SyntaxKind::SimpleKeyExpected => "simple_key_expected",
            SyntaxKind::InvalidSimpleKey => "invalid_simple_key",
            SyntaxKind::InvalidDocumentEnd => "invalid_document_end",
            SyntaxKind::InvalidIndentation => "invalid_indentation",
            SyntaxKind::BomInsideDocument => "bom_inside_document",
            SyntaxKind::UnexpectedCharacter => "unexpected_character",
            SyntaxKind::TabNotAllowed => "tab_not_allowed",
            SyntaxKind::TabInBlockIndentation => "tab_in_block_indentation",
            SyntaxKind::CommentInterceptedScalar => "comment_intercepted_scalar",
            SyntaxKind::ExpectedWhitespace => "expected_whitespace",
            SyntaxKind::CommentNotSeparated => "comment_not_separated",
            SyntaxKind::InvalidDirectiveTerminator => "invalid_directive_terminator",
            SyntaxKind::MissingYamlVersionSeparator => "missing_yaml_version_separator",
            SyntaxKind::MissingDirectiveName => "missing_directive_name",
            SyntaxKind::InvalidDirectiveName => "invalid_directive_name",
            SyntaxKind::DirectiveByteLimitExceeded => "directive_byte_limit_exceeded",
            SyntaxKind::TooManyReservedDirectiveParams => "too_many_reserved_directive_params",
            SyntaxKind::YamlVersionTooLong => "yaml_version_too_long",
            SyntaxKind::MissingYamlVersion => "missing_yaml_version",
            SyntaxKind::InvalidTagDirectiveTerminator => "invalid_tag_directive_terminator",
            SyntaxKind::InvalidTagTerminator => "invalid_tag_terminator",
            SyntaxKind::MissingTagUri => "missing_tag_uri",
            SyntaxKind::UnclosedVerbatimTag => "unclosed_verbatim_tag",
            SyntaxKind::InvalidTagEscape => "invalid_tag_escape",
            SyntaxKind::InvalidTagUtf8LeadingByte => "invalid_tag_utf8_leading_byte",
            SyntaxKind::InvalidTagUtf8TrailingByte => "invalid_tag_utf8_trailing_byte",
            SyntaxKind::InvalidTagUtf8 => "invalid_tag_utf8",
            SyntaxKind::MissingAnchorOrAliasName => "missing_anchor_or_alias_name",
            SyntaxKind::MisplacedFlowCollectionEnd => "misplaced_flow_collection_end",
            SyntaxKind::MismatchedFlowCollectionEnd => "mismatched_flow_collection_end",
            SyntaxKind::UnclosedFlowCollection => "unclosed_flow_collection",
            SyntaxKind::BlockEntryInFlowCollection => "block_entry_in_flow_collection",
            SyntaxKind::BlockSequenceEntryNotAllowed => "block_sequence_entry_not_allowed",
            SyntaxKind::InvalidBlockEntryWhitespace => "invalid_block_entry_whitespace",
            SyntaxKind::ZeroBlockScalarIndent => "zero_block_scalar_indent",
            SyntaxKind::InvalidBlockScalarHeader => "invalid_block_scalar_header",
            SyntaxKind::TabAtBlockScalarStart => "tab_at_block_scalar_start",
            SyntaxKind::InvalidBlockScalarIndent => "invalid_block_scalar_indent",
            SyntaxKind::DocumentIndicatorInQuotedScalar => "document_indicator_in_quoted_scalar",
            SyntaxKind::UnclosedQuotedScalar => "unclosed_quoted_scalar",
            SyntaxKind::TabInIndentation => "tab_in_indentation",
            SyntaxKind::InvalidQuotedScalarIndent => "invalid_quoted_scalar_indent",
            SyntaxKind::InvalidTrailingSingleQuotedScalar => {
                "invalid_trailing_single_quoted_scalar"
            }
            SyntaxKind::InvalidTrailingDoubleQuotedScalar => {
                "invalid_trailing_double_quoted_scalar"
            }
            SyntaxKind::UnknownQuotedScalarEscape => "unknown_quoted_scalar_escape",
            SyntaxKind::InvalidQuotedScalarHexEscape => "invalid_quoted_scalar_hex_escape",
            SyntaxKind::InvalidLowSurrogateHexEscape => "invalid_low_surrogate_hex_escape",
            SyntaxKind::InvalidLowSurrogate => "invalid_low_surrogate",
            SyntaxKind::MissingLowSurrogate => "missing_low_surrogate",
            SyntaxKind::UnpairedLowSurrogate => "unpaired_low_surrogate",
            SyntaxKind::InvalidUnicodeEscape => "invalid_unicode_escape",
            SyntaxKind::InvalidFlowScalarIndent => "invalid_flow_scalar_indent",
            SyntaxKind::PlainScalarStartsWithDashFlowIndicator => {
                "plain_scalar_starts_with_dash_flow_indicator"
            }
            SyntaxKind::TabInPlainScalar => "tab_in_plain_scalar",
            SyntaxKind::UnexpectedEndOfPlainScalar => "unexpected_end_of_plain_scalar",
            SyntaxKind::MappingKeyNotAllowed => "mapping_key_not_allowed",
            SyntaxKind::FlowMappingValueAdjacentCollection => {
                "flow_mapping_value_adjacent_collection"
            }
            SyntaxKind::InvalidMappingValueWhitespace => "invalid_mapping_value_whitespace",
            SyntaxKind::InvalidColonPlacement => "invalid_colon_placement",
            SyntaxKind::MappingValueNotAllowed => "mapping_value_not_allowed",
        }
    }

    /// Parse a wire `syntax_kind` code into a [`SyntaxKind`] member.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "unexpected_eof_flow_sequence" => Some(SyntaxKind::UnexpectedEofFlowSequence),
            "unexpected_eof_flow_mapping" => Some(SyntaxKind::UnexpectedEofFlowMapping),
            "unexpected_eof_implicit_flow_mapping" => {
                Some(SyntaxKind::UnexpectedEofImplicitFlowMapping)
            }
            "unexpected_eof_block_sequence" => Some(SyntaxKind::UnexpectedEofBlockSequence),
            "unexpected_eof_block_mapping" => Some(SyntaxKind::UnexpectedEofBlockMapping),
            "unexpected_eof" => Some(SyntaxKind::UnexpectedEof),
            "expected_stream_start" => Some(SyntaxKind::ExpectedStreamStart),
            "duplicate_version_directive" => Some(SyntaxKind::DuplicateVersionDirective),
            "duplicate_tag_directive" => Some(SyntaxKind::DuplicateTagDirective),
            "expected_document_start" => Some(SyntaxKind::ExpectedDocumentStart),
            "missing_document_end_before_directive" => {
                Some(SyntaxKind::MissingDocumentEndBeforeDirective)
            }
            "anchor_count_overflow" => Some(SyntaxKind::AnchorCountOverflow),
            "expected_node_content" => Some(SyntaxKind::ExpectedNodeContent),
            "expected_block_mapping_key" => Some(SyntaxKind::ExpectedBlockMappingKey),
            "expected_flow_mapping_separator" => Some(SyntaxKind::ExpectedFlowMappingSeparator),
            "expected_flow_sequence_separator" => Some(SyntaxKind::ExpectedFlowSequenceSeparator),
            "expected_block_sequence_entry" => Some(SyntaxKind::ExpectedBlockSequenceEntry),
            "undeclared_tag_handle" => Some(SyntaxKind::UndeclaredTagHandle),
            "expected_tag_bang" => Some(SyntaxKind::ExpectedTagBang),
            "expected_tag_directive_bang" => Some(SyntaxKind::ExpectedTagDirectiveBang),
            "invalid_global_tag_character" => Some(SyntaxKind::InvalidGlobalTagCharacter),
            "simple_key_expected" => Some(SyntaxKind::SimpleKeyExpected),
            "invalid_simple_key" => Some(SyntaxKind::InvalidSimpleKey),
            "invalid_document_end" => Some(SyntaxKind::InvalidDocumentEnd),
            "invalid_indentation" => Some(SyntaxKind::InvalidIndentation),
            "bom_inside_document" => Some(SyntaxKind::BomInsideDocument),
            "unexpected_character" => Some(SyntaxKind::UnexpectedCharacter),
            "tab_not_allowed" => Some(SyntaxKind::TabNotAllowed),
            "tab_in_block_indentation" => Some(SyntaxKind::TabInBlockIndentation),
            "comment_intercepted_scalar" => Some(SyntaxKind::CommentInterceptedScalar),
            "expected_whitespace" => Some(SyntaxKind::ExpectedWhitespace),
            "comment_not_separated" => Some(SyntaxKind::CommentNotSeparated),
            "invalid_directive_terminator" => Some(SyntaxKind::InvalidDirectiveTerminator),
            "missing_yaml_version_separator" => Some(SyntaxKind::MissingYamlVersionSeparator),
            "missing_directive_name" => Some(SyntaxKind::MissingDirectiveName),
            "invalid_directive_name" => Some(SyntaxKind::InvalidDirectiveName),
            "directive_byte_limit_exceeded" => Some(SyntaxKind::DirectiveByteLimitExceeded),
            "too_many_reserved_directive_params" => {
                Some(SyntaxKind::TooManyReservedDirectiveParams)
            }
            "yaml_version_too_long" => Some(SyntaxKind::YamlVersionTooLong),
            "missing_yaml_version" => Some(SyntaxKind::MissingYamlVersion),
            "invalid_tag_directive_terminator" => Some(SyntaxKind::InvalidTagDirectiveTerminator),
            "invalid_tag_terminator" => Some(SyntaxKind::InvalidTagTerminator),
            "missing_tag_uri" => Some(SyntaxKind::MissingTagUri),
            "unclosed_verbatim_tag" => Some(SyntaxKind::UnclosedVerbatimTag),
            "invalid_tag_escape" => Some(SyntaxKind::InvalidTagEscape),
            "invalid_tag_utf8_leading_byte" => Some(SyntaxKind::InvalidTagUtf8LeadingByte),
            "invalid_tag_utf8_trailing_byte" => Some(SyntaxKind::InvalidTagUtf8TrailingByte),
            "invalid_tag_utf8" => Some(SyntaxKind::InvalidTagUtf8),
            "missing_anchor_or_alias_name" => Some(SyntaxKind::MissingAnchorOrAliasName),
            "misplaced_flow_collection_end" => Some(SyntaxKind::MisplacedFlowCollectionEnd),
            "mismatched_flow_collection_end" => Some(SyntaxKind::MismatchedFlowCollectionEnd),
            "unclosed_flow_collection" => Some(SyntaxKind::UnclosedFlowCollection),
            "block_entry_in_flow_collection" => Some(SyntaxKind::BlockEntryInFlowCollection),
            "block_sequence_entry_not_allowed" => Some(SyntaxKind::BlockSequenceEntryNotAllowed),
            "invalid_block_entry_whitespace" => Some(SyntaxKind::InvalidBlockEntryWhitespace),
            "zero_block_scalar_indent" => Some(SyntaxKind::ZeroBlockScalarIndent),
            "invalid_block_scalar_header" => Some(SyntaxKind::InvalidBlockScalarHeader),
            "tab_at_block_scalar_start" => Some(SyntaxKind::TabAtBlockScalarStart),
            "invalid_block_scalar_indent" => Some(SyntaxKind::InvalidBlockScalarIndent),
            "document_indicator_in_quoted_scalar" => {
                Some(SyntaxKind::DocumentIndicatorInQuotedScalar)
            }
            "unclosed_quoted_scalar" => Some(SyntaxKind::UnclosedQuotedScalar),
            "tab_in_indentation" => Some(SyntaxKind::TabInIndentation),
            "invalid_quoted_scalar_indent" => Some(SyntaxKind::InvalidQuotedScalarIndent),
            "invalid_trailing_single_quoted_scalar" => {
                Some(SyntaxKind::InvalidTrailingSingleQuotedScalar)
            }
            "invalid_trailing_double_quoted_scalar" => {
                Some(SyntaxKind::InvalidTrailingDoubleQuotedScalar)
            }
            "unknown_quoted_scalar_escape" => Some(SyntaxKind::UnknownQuotedScalarEscape),
            "invalid_quoted_scalar_hex_escape" => Some(SyntaxKind::InvalidQuotedScalarHexEscape),
            "invalid_low_surrogate_hex_escape" => Some(SyntaxKind::InvalidLowSurrogateHexEscape),
            "invalid_low_surrogate" => Some(SyntaxKind::InvalidLowSurrogate),
            "missing_low_surrogate" => Some(SyntaxKind::MissingLowSurrogate),
            "unpaired_low_surrogate" => Some(SyntaxKind::UnpairedLowSurrogate),
            "invalid_unicode_escape" => Some(SyntaxKind::InvalidUnicodeEscape),
            "invalid_flow_scalar_indent" => Some(SyntaxKind::InvalidFlowScalarIndent),
            "plain_scalar_starts_with_dash_flow_indicator" => {
                Some(SyntaxKind::PlainScalarStartsWithDashFlowIndicator)
            }
            "tab_in_plain_scalar" => Some(SyntaxKind::TabInPlainScalar),
            "unexpected_end_of_plain_scalar" => Some(SyntaxKind::UnexpectedEndOfPlainScalar),
            "mapping_key_not_allowed" => Some(SyntaxKind::MappingKeyNotAllowed),
            "flow_mapping_value_adjacent_collection" => {
                Some(SyntaxKind::FlowMappingValueAdjacentCollection)
            }
            "invalid_mapping_value_whitespace" => Some(SyntaxKind::InvalidMappingValueWhitespace),
            "invalid_colon_placement" => Some(SyntaxKind::InvalidColonPlacement),
            "mapping_value_not_allowed" => Some(SyntaxKind::MappingValueNotAllowed),
            _ => None,
        }
    }
}

impl fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<SyntaxKind> for &'static str {
    fn from(kind: SyntaxKind) -> Self {
        kind.as_str()
    }
}

impl From<SyntaxKind> for String {
    fn from(kind: SyntaxKind) -> Self {
        kind.as_str().to_owned()
    }
}

impl PartialEq<str> for SyntaxKind {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SyntaxKind {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<SyntaxKind> for str {
    fn eq(&self, other: &SyntaxKind) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<SyntaxKind> for &str {
    fn eq(&self, other: &SyntaxKind) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<String> for SyntaxKind {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<SyntaxKind> for String {
    fn eq(&self, other: &SyntaxKind) -> bool {
        self.as_str() == other.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    InvalidUtf8 { invalid_len: Option<usize> },
    Syntax(SyntaxKind),
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
    /// Construct a [`ErrorKind::Syntax`] from a closed [`SyntaxKind`] member.
    pub const fn syntax(kind: SyntaxKind) -> Self {
        ErrorKind::Syntax(kind)
    }

    /// Return the closed [`SyntaxKind`] when this is a syntax error.
    pub const fn syntax_kind(&self) -> Option<SyntaxKind> {
        match self {
            ErrorKind::Syntax(kind) => Some(*kind),
            _ => None,
        }
    }

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
    /// Return the closed [`SyntaxKind`] when this is a parser-vocabulary syntax error.
    pub fn syntax_kind(&self) -> Option<SyntaxKind> {
        self.kind.syntax_kind()
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

impl serde::ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::not_source(ErrorKind::Custom, None, msg)
    }
}

pub(crate) fn internal(what: &str) -> Error {
    Error::not_source(ErrorKind::Internal, None, format!("internal error: {what}"))
}
