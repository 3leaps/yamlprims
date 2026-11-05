// Package engine implements the internal YAML 1.2.2 lexical scanner, event
// state-machine parser, bounded source-validation pass, and original-byte
// source coordinate translator for github.com/3leaps/yamlprims/yaml.
//
// Attribution and license provenance:
// The YAML 1.2.2 lexical scanner and event state-machine parser in this
// package are ported from granit-parser 2.0.0, which is derived from
// saphyr-parser and yaml-rust:
//   - granit-parser, Copyright (c) 2026 Bourumir Wyngs (MIT OR Apache-2.0)
//   - saphyr-parser, Copyright (c) 2023 Ethiraric (MIT OR Apache-2.0)
//   - yaml-rust, Copyright (c) 2015 Chen Yuheng (MIT OR Apache-2.0)
//
// Licensed under either of Apache License, Version 2.0 (LICENSE-APACHE) or
// MIT License (LICENSE-MIT) at your option.
package engine

import (
	"bytes"
	"fmt"
	"strings"
	"unicode/utf8"
)

// Location is a source coordinate in original input bytes (including any
// initial UTF-8 BOM in ByteOffset, 1-based Line, and 1-based Unicode scalar
// Column with any initial BOM on line 1 excluded from the column count).
type Location struct {
	ByteOffset int
	Line       int
	Column     int
}

// Span is a half-open source range [Start, End).
type Span struct {
	Start Location
	End   Location
}

// EventSpan pairs an event's source Span with an optional explicit tag start
// Location and optional block indentation hint.
type EventSpan struct {
	Span      Span
	TagStart  *Location
	Indent    int
	HasIndent bool
}

// Marker is the scanner/parser internal coordinate within the BOM-stripped
// input slice. Line is 1-based; col is 0-based.
type Marker struct {
	byteOffset    int
	hasByteOffset bool
	charIndex     int
	line          int
	col           int
}

func newMarker(byteOffset int, hasByteOffset bool, charIndex, line, col int) Marker {
	return Marker{
		byteOffset:    byteOffset,
		hasByteOffset: hasByteOffset,
		charIndex:     charIndex,
		line:          line,
		col:           col,
	}
}

type rawSpan struct {
	start     Marker
	end       Marker
	tagStart  *Marker
	indent    int
	hasIndent bool
}

func newRawSpan(start, end Marker) rawSpan {
	return rawSpan{start: start, end: end}
}

func emptyRawSpan(mark Marker) rawSpan {
	return rawSpan{start: mark, end: mark}
}

func (s rawSpan) withIndent(indent int, hasIndent bool) rawSpan {
	s.indent = indent
	s.hasIndent = hasIndent
	return s
}

func (s rawSpan) withTagStart(start *Marker) rawSpan {
	if start != nil {
		cp := *start
		s.tagStart = &cp
	} else {
		s.tagStart = nil
	}
	return s
}

// LimitKind identifies which configured resource limit was exceeded.
type LimitKind uint8

const (
	LimitMaxInputBytes LimitKind = iota + 1
	LimitMaxDepth
	LimitMaxNodes
	LimitMaxExpandedNodes
	LimitMaxAliasEvents
	LimitMaxOwnedBytes
	LimitMaxScalarBytes
	LimitMaxKeyBytes
	LimitMaxMappingEntries
	LimitMaxDocuments
)

// FieldName returns the canonical snake_case limit field name.
func (k LimitKind) FieldName() string {
	switch k {
	case LimitMaxInputBytes:
		return "max_input_bytes"
	case LimitMaxDepth:
		return "max_depth"
	case LimitMaxNodes:
		return "max_nodes"
	case LimitMaxExpandedNodes:
		return "max_expanded_nodes"
	case LimitMaxAliasEvents:
		return "max_alias_events"
	case LimitMaxOwnedBytes:
		return "max_owned_bytes"
	case LimitMaxScalarBytes:
		return "max_scalar_bytes"
	case LimitMaxKeyBytes:
		return "max_key_bytes"
	case LimitMaxMappingEntries:
		return "max_mapping_entries"
	case LimitMaxDocuments:
		return "max_documents"
	default:
		return "unknown_limit"
	}
}

func (k LimitKind) String() string {
	return k.FieldName()
}

// SyntaxKind is the closed vocabulary of 81 YAML syntax error codes produced
// by the parser (spec §7.1, §11.1, ADR-0004).
type SyntaxKind uint8

const (
	SyntaxUnexpectedEofFlowSequence SyntaxKind = iota + 1
	SyntaxUnexpectedEofFlowMapping
	SyntaxUnexpectedEofImplicitFlowMapping
	SyntaxUnexpectedEofBlockSequence
	SyntaxUnexpectedEofBlockMapping
	SyntaxUnexpectedEof
	SyntaxExpectedStreamStart
	SyntaxDuplicateVersionDirective
	SyntaxDuplicateTagDirective
	SyntaxExpectedDocumentStart
	SyntaxMissingDocumentEndBeforeDirective
	SyntaxAnchorCountOverflow
	SyntaxExpectedNodeContent
	SyntaxExpectedBlockMappingKey
	SyntaxExpectedFlowMappingSeparator
	SyntaxExpectedFlowSequenceSeparator
	SyntaxExpectedBlockSequenceEntry
	SyntaxUndeclaredTagHandle
	SyntaxExpectedTagBang
	SyntaxExpectedTagDirectiveBang
	SyntaxInvalidGlobalTagCharacter
	SyntaxSimpleKeyExpected
	SyntaxInvalidSimpleKey
	SyntaxInvalidDocumentEnd
	SyntaxInvalidIndentation
	SyntaxBomInsideDocument
	SyntaxUnexpectedCharacter
	SyntaxTabNotAllowed
	SyntaxTabInBlockIndentation
	SyntaxCommentInterceptedScalar
	SyntaxExpectedWhitespace
	SyntaxCommentNotSeparated
	SyntaxInvalidDirectiveTerminator
	SyntaxMissingYamlVersionSeparator
	SyntaxMissingDirectiveName
	SyntaxInvalidDirectiveName
	SyntaxDirectiveByteLimitExceeded
	SyntaxTooManyReservedDirectiveParams
	SyntaxYamlVersionTooLong
	SyntaxMissingYamlVersion
	SyntaxInvalidTagDirectiveTerminator
	SyntaxInvalidTagTerminator
	SyntaxMissingTagUri
	SyntaxUnclosedVerbatimTag
	SyntaxInvalidTagEscape
	SyntaxInvalidTagUtf8LeadingByte
	SyntaxInvalidTagUtf8TrailingByte
	SyntaxInvalidTagUtf8
	SyntaxMissingAnchorOrAliasName
	SyntaxMisplacedFlowCollectionEnd
	SyntaxMismatchedFlowCollectionEnd
	SyntaxUnclosedFlowCollection
	SyntaxBlockEntryInFlowCollection
	SyntaxBlockSequenceEntryNotAllowed
	SyntaxInvalidBlockEntryWhitespace
	SyntaxZeroBlockScalarIndent
	SyntaxInvalidBlockScalarHeader
	SyntaxTabAtBlockScalarStart
	SyntaxInvalidBlockScalarIndent
	SyntaxDocumentIndicatorInQuotedScalar
	SyntaxUnclosedQuotedScalar
	SyntaxTabInIndentation
	SyntaxInvalidQuotedScalarIndent
	SyntaxInvalidTrailingSingleQuotedScalar
	SyntaxInvalidTrailingDoubleQuotedScalar
	SyntaxUnknownQuotedScalarEscape
	SyntaxInvalidQuotedScalarHexEscape
	SyntaxInvalidLowSurrogateHexEscape
	SyntaxInvalidLowSurrogate
	SyntaxMissingLowSurrogate
	SyntaxUnpairedLowSurrogate
	SyntaxInvalidUnicodeEscape
	SyntaxInvalidFlowScalarIndent
	SyntaxPlainScalarStartsWithDashFlowIndicator
	SyntaxTabInPlainScalar
	SyntaxUnexpectedEndOfPlainScalar
	SyntaxMappingKeyNotAllowed
	SyntaxFlowMappingValueAdjacentCollection
	SyntaxInvalidMappingValueWhitespace
	SyntaxInvalidColonPlacement
	SyntaxMappingValueNotAllowed
)

// AllSyntaxKinds is the complete closed table of all 81 parser-emitted syntax
// error kinds in canonical declaration order.
var AllSyntaxKinds = []SyntaxKind{
	SyntaxUnexpectedEofFlowSequence,
	SyntaxUnexpectedEofFlowMapping,
	SyntaxUnexpectedEofImplicitFlowMapping,
	SyntaxUnexpectedEofBlockSequence,
	SyntaxUnexpectedEofBlockMapping,
	SyntaxUnexpectedEof,
	SyntaxExpectedStreamStart,
	SyntaxDuplicateVersionDirective,
	SyntaxDuplicateTagDirective,
	SyntaxExpectedDocumentStart,
	SyntaxMissingDocumentEndBeforeDirective,
	SyntaxAnchorCountOverflow,
	SyntaxExpectedNodeContent,
	SyntaxExpectedBlockMappingKey,
	SyntaxExpectedFlowMappingSeparator,
	SyntaxExpectedFlowSequenceSeparator,
	SyntaxExpectedBlockSequenceEntry,
	SyntaxUndeclaredTagHandle,
	SyntaxExpectedTagBang,
	SyntaxExpectedTagDirectiveBang,
	SyntaxInvalidGlobalTagCharacter,
	SyntaxSimpleKeyExpected,
	SyntaxInvalidSimpleKey,
	SyntaxInvalidDocumentEnd,
	SyntaxInvalidIndentation,
	SyntaxBomInsideDocument,
	SyntaxUnexpectedCharacter,
	SyntaxTabNotAllowed,
	SyntaxTabInBlockIndentation,
	SyntaxCommentInterceptedScalar,
	SyntaxExpectedWhitespace,
	SyntaxCommentNotSeparated,
	SyntaxInvalidDirectiveTerminator,
	SyntaxMissingYamlVersionSeparator,
	SyntaxMissingDirectiveName,
	SyntaxInvalidDirectiveName,
	SyntaxDirectiveByteLimitExceeded,
	SyntaxTooManyReservedDirectiveParams,
	SyntaxYamlVersionTooLong,
	SyntaxMissingYamlVersion,
	SyntaxInvalidTagDirectiveTerminator,
	SyntaxInvalidTagTerminator,
	SyntaxMissingTagUri,
	SyntaxUnclosedVerbatimTag,
	SyntaxInvalidTagEscape,
	SyntaxInvalidTagUtf8LeadingByte,
	SyntaxInvalidTagUtf8TrailingByte,
	SyntaxInvalidTagUtf8,
	SyntaxMissingAnchorOrAliasName,
	SyntaxMisplacedFlowCollectionEnd,
	SyntaxMismatchedFlowCollectionEnd,
	SyntaxUnclosedFlowCollection,
	SyntaxBlockEntryInFlowCollection,
	SyntaxBlockSequenceEntryNotAllowed,
	SyntaxInvalidBlockEntryWhitespace,
	SyntaxZeroBlockScalarIndent,
	SyntaxInvalidBlockScalarHeader,
	SyntaxTabAtBlockScalarStart,
	SyntaxInvalidBlockScalarIndent,
	SyntaxDocumentIndicatorInQuotedScalar,
	SyntaxUnclosedQuotedScalar,
	SyntaxTabInIndentation,
	SyntaxInvalidQuotedScalarIndent,
	SyntaxInvalidTrailingSingleQuotedScalar,
	SyntaxInvalidTrailingDoubleQuotedScalar,
	SyntaxUnknownQuotedScalarEscape,
	SyntaxInvalidQuotedScalarHexEscape,
	SyntaxInvalidLowSurrogateHexEscape,
	SyntaxInvalidLowSurrogate,
	SyntaxMissingLowSurrogate,
	SyntaxUnpairedLowSurrogate,
	SyntaxInvalidUnicodeEscape,
	SyntaxInvalidFlowScalarIndent,
	SyntaxPlainScalarStartsWithDashFlowIndicator,
	SyntaxTabInPlainScalar,
	SyntaxUnexpectedEndOfPlainScalar,
	SyntaxMappingKeyNotAllowed,
	SyntaxFlowMappingValueAdjacentCollection,
	SyntaxInvalidMappingValueWhitespace,
	SyntaxInvalidColonPlacement,
	SyntaxMappingValueNotAllowed,
}

// AsStr returns the canonical wire and diagnostic syntax_kind code.
func (k SyntaxKind) AsStr() string {
	switch k {
	case SyntaxUnexpectedEofFlowSequence:
		return "unexpected_eof_flow_sequence"
	case SyntaxUnexpectedEofFlowMapping:
		return "unexpected_eof_flow_mapping"
	case SyntaxUnexpectedEofImplicitFlowMapping:
		return "unexpected_eof_implicit_flow_mapping"
	case SyntaxUnexpectedEofBlockSequence:
		return "unexpected_eof_block_sequence"
	case SyntaxUnexpectedEofBlockMapping:
		return "unexpected_eof_block_mapping"
	case SyntaxUnexpectedEof:
		return "unexpected_eof"
	case SyntaxExpectedStreamStart:
		return "expected_stream_start"
	case SyntaxDuplicateVersionDirective:
		return "duplicate_version_directive"
	case SyntaxDuplicateTagDirective:
		return "duplicate_tag_directive"
	case SyntaxExpectedDocumentStart:
		return "expected_document_start"
	case SyntaxMissingDocumentEndBeforeDirective:
		return "missing_document_end_before_directive"
	case SyntaxAnchorCountOverflow:
		return "anchor_count_overflow"
	case SyntaxExpectedNodeContent:
		return "expected_node_content"
	case SyntaxExpectedBlockMappingKey:
		return "expected_block_mapping_key"
	case SyntaxExpectedFlowMappingSeparator:
		return "expected_flow_mapping_separator"
	case SyntaxExpectedFlowSequenceSeparator:
		return "expected_flow_sequence_separator"
	case SyntaxExpectedBlockSequenceEntry:
		return "expected_block_sequence_entry"
	case SyntaxUndeclaredTagHandle:
		return "undeclared_tag_handle"
	case SyntaxExpectedTagBang:
		return "expected_tag_bang"
	case SyntaxExpectedTagDirectiveBang:
		return "expected_tag_directive_bang"
	case SyntaxInvalidGlobalTagCharacter:
		return "invalid_global_tag_character"
	case SyntaxSimpleKeyExpected:
		return "simple_key_expected"
	case SyntaxInvalidSimpleKey:
		return "invalid_simple_key"
	case SyntaxInvalidDocumentEnd:
		return "invalid_document_end"
	case SyntaxInvalidIndentation:
		return "invalid_indentation"
	case SyntaxBomInsideDocument:
		return "bom_inside_document"
	case SyntaxUnexpectedCharacter:
		return "unexpected_character"
	case SyntaxTabNotAllowed:
		return "tab_not_allowed"
	case SyntaxTabInBlockIndentation:
		return "tab_in_block_indentation"
	case SyntaxCommentInterceptedScalar:
		return "comment_intercepted_scalar"
	case SyntaxExpectedWhitespace:
		return "expected_whitespace"
	case SyntaxCommentNotSeparated:
		return "comment_not_separated"
	case SyntaxInvalidDirectiveTerminator:
		return "invalid_directive_terminator"
	case SyntaxMissingYamlVersionSeparator:
		return "missing_yaml_version_separator"
	case SyntaxMissingDirectiveName:
		return "missing_directive_name"
	case SyntaxInvalidDirectiveName:
		return "invalid_directive_name"
	case SyntaxDirectiveByteLimitExceeded:
		return "directive_byte_limit_exceeded"
	case SyntaxTooManyReservedDirectiveParams:
		return "too_many_reserved_directive_params"
	case SyntaxYamlVersionTooLong:
		return "yaml_version_too_long"
	case SyntaxMissingYamlVersion:
		return "missing_yaml_version"
	case SyntaxInvalidTagDirectiveTerminator:
		return "invalid_tag_directive_terminator"
	case SyntaxInvalidTagTerminator:
		return "invalid_tag_terminator"
	case SyntaxMissingTagUri:
		return "missing_tag_uri"
	case SyntaxUnclosedVerbatimTag:
		return "unclosed_verbatim_tag"
	case SyntaxInvalidTagEscape:
		return "invalid_tag_escape"
	case SyntaxInvalidTagUtf8LeadingByte:
		return "invalid_tag_utf8_leading_byte"
	case SyntaxInvalidTagUtf8TrailingByte:
		return "invalid_tag_utf8_trailing_byte"
	case SyntaxInvalidTagUtf8:
		return "invalid_tag_utf8"
	case SyntaxMissingAnchorOrAliasName:
		return "missing_anchor_or_alias_name"
	case SyntaxMisplacedFlowCollectionEnd:
		return "misplaced_flow_collection_end"
	case SyntaxMismatchedFlowCollectionEnd:
		return "mismatched_flow_collection_end"
	case SyntaxUnclosedFlowCollection:
		return "unclosed_flow_collection"
	case SyntaxBlockEntryInFlowCollection:
		return "block_entry_in_flow_collection"
	case SyntaxBlockSequenceEntryNotAllowed:
		return "block_sequence_entry_not_allowed"
	case SyntaxInvalidBlockEntryWhitespace:
		return "invalid_block_entry_whitespace"
	case SyntaxZeroBlockScalarIndent:
		return "zero_block_scalar_indent"
	case SyntaxInvalidBlockScalarHeader:
		return "invalid_block_scalar_header"
	case SyntaxTabAtBlockScalarStart:
		return "tab_at_block_scalar_start"
	case SyntaxInvalidBlockScalarIndent:
		return "invalid_block_scalar_indent"
	case SyntaxDocumentIndicatorInQuotedScalar:
		return "document_indicator_in_quoted_scalar"
	case SyntaxUnclosedQuotedScalar:
		return "unclosed_quoted_scalar"
	case SyntaxTabInIndentation:
		return "tab_in_indentation"
	case SyntaxInvalidQuotedScalarIndent:
		return "invalid_quoted_scalar_indent"
	case SyntaxInvalidTrailingSingleQuotedScalar:
		return "invalid_trailing_single_quoted_scalar"
	case SyntaxInvalidTrailingDoubleQuotedScalar:
		return "invalid_trailing_double_quoted_scalar"
	case SyntaxUnknownQuotedScalarEscape:
		return "unknown_quoted_scalar_escape"
	case SyntaxInvalidQuotedScalarHexEscape:
		return "invalid_quoted_scalar_hex_escape"
	case SyntaxInvalidLowSurrogateHexEscape:
		return "invalid_low_surrogate_hex_escape"
	case SyntaxInvalidLowSurrogate:
		return "invalid_low_surrogate"
	case SyntaxMissingLowSurrogate:
		return "missing_low_surrogate"
	case SyntaxUnpairedLowSurrogate:
		return "unpaired_low_surrogate"
	case SyntaxInvalidUnicodeEscape:
		return "invalid_unicode_escape"
	case SyntaxInvalidFlowScalarIndent:
		return "invalid_flow_scalar_indent"
	case SyntaxPlainScalarStartsWithDashFlowIndicator:
		return "plain_scalar_starts_with_dash_flow_indicator"
	case SyntaxTabInPlainScalar:
		return "tab_in_plain_scalar"
	case SyntaxUnexpectedEndOfPlainScalar:
		return "unexpected_end_of_plain_scalar"
	case SyntaxMappingKeyNotAllowed:
		return "mapping_key_not_allowed"
	case SyntaxFlowMappingValueAdjacentCollection:
		return "flow_mapping_value_adjacent_collection"
	case SyntaxInvalidMappingValueWhitespace:
		return "invalid_mapping_value_whitespace"
	case SyntaxInvalidColonPlacement:
		return "invalid_colon_placement"
	case SyntaxMappingValueNotAllowed:
		return "mapping_value_not_allowed"
	default:
		return ""
	}
}

func (k SyntaxKind) String() string {
	return k.AsStr()
}

// SyntaxKindFromCode parses a wire syntax_kind code into a SyntaxKind member.
func SyntaxKindFromCode(code string) (SyntaxKind, bool) {
	for _, k := range AllSyntaxKinds {
		if k.AsStr() == code {
			return k, true
		}
	}
	return 0, false
}

// ErrorKind identifies the category of an engine-level error.
type ErrorKind uint8

const (
	ErrorKindInvalidUtf8 ErrorKind = iota + 1
	ErrorKindSyntax
	ErrorKindUnsupportedYamlVersion
	ErrorKindLimitExceeded
	ErrorKindInvalidOptions
	ErrorKindUnknownAnchor
	ErrorKindInternal
)

// Name returns the canonical wire name for the error kind.
func (k ErrorKind) Name() string {
	switch k {
	case ErrorKindInvalidUtf8:
		return "invalid_utf8"
	case ErrorKindSyntax:
		return "syntax"
	case ErrorKindUnsupportedYamlVersion:
		return "unsupported_yaml_version"
	case ErrorKindLimitExceeded:
		return "limit_exceeded"
	case ErrorKindInvalidOptions:
		return "invalid_options"
	case ErrorKindUnknownAnchor:
		return "unknown_anchor"
	case ErrorKindInternal:
		return "internal"
	default:
		return "internal"
	}
}

// SiteKind distinguishes source-attributable errors from non-source errors.
type SiteKind uint8

const (
	SiteSource SiteKind = iota + 1
	SiteNotSource
)

// Site holds either a source Location or an optional last-known Location.
type Site struct {
	Kind      SiteKind
	Location  Location
	LastKnown *Location
}

// Error represents a terminal engine or input-gate error.
type Error struct {
	Kind           ErrorKind
	Syntax         SyntaxKind
	Limit          LimitKind
	Configured     uint64
	InvalidUtf8Len *int
	Site           Site
	Message        string
	EngineDetail   string
}

func (e *Error) Error() string {
	if e.Site.Kind == SiteSource {
		return fmt.Sprintf("%d:%d: %s: %s", e.Site.Location.Line, e.Site.Location.Column, e.Kind.Name(), e.Message)
	}
	return fmt.Sprintf("%s: %s", e.Kind.Name(), e.Message)
}

// Location returns the source Location when Site.Kind == SiteSource.
func (e *Error) Location() (Location, bool) {
	if e.Site.Kind == SiteSource {
		return e.Site.Location, true
	}
	return Location{}, false
}

func newSourceError(kind ErrorKind, loc Location, msg string) *Error {
	return &Error{
		Kind:    kind,
		Site:    Site{Kind: SiteSource, Location: loc},
		Message: BoundedString(msg, 4096),
	}
}

func newNotSourceError(kind ErrorKind, lastKnown *Location, msg string) *Error {
	var lk *Location
	if lastKnown != nil {
		cp := *lastKnown
		lk = &cp
	}
	return &Error{
		Kind:    kind,
		Site:    Site{Kind: SiteNotSource, LastKnown: lk},
		Message: BoundedString(msg, 4096),
	}
}

func newLimitError(limit LimitKind, configured uint64, loc Location) *Error {
	return &Error{
		Kind:       ErrorKindLimitExceeded,
		Limit:      limit,
		Configured: configured,
		Site:       Site{Kind: SiteSource, Location: loc},
		Message:    BoundedString(fmt.Sprintf("%s limit (%d) exceeded", limit.FieldName(), configured), 4096),
	}
}

func newInternalError(what string) *Error {
	return newNotSourceError(ErrorKindInternal, nil, "internal error: "+what)
}

// BoundedString truncates s to at most max UTF-8 bytes on a rune boundary,
// appending '…' (3 UTF-8 bytes) if truncated.
func BoundedString(s string, max int) string {
	if len(s) <= max {
		return s
	}
	const ellipsis = "…"
	if max < len(ellipsis) {
		return ""
	}
	i := max - len(ellipsis)
	for i > 0 && !utf8.RuneStart(s[i]) {
		i--
	}
	return s[:i] + ellipsis
}

// MergeKeys configures YAML 1.1 `<<` merge key handling.
type MergeKeys uint8

const (
	MergeKeysReject MergeKeys = iota
	MergeKeysApply
	MergeKeysAsKey
)

// UnknownTags configures handling of tags outside the YAML 1.2 Core Schema.
type UnknownTags uint8

const (
	UnknownTagsPreserve UnknownTags = iota
	UnknownTagsReject
)

// Limits holds the 12 configurable resource limits (spec §6).
type Limits struct {
	MaxInputBytes     int
	MaxDepth          int
	MaxNodes          int
	MaxExpandedNodes  int
	MaxAliasEvents    int
	MaxOwnedBytes     int
	MaxScalarBytes    int
	MaxKeyBytes       int
	MaxMappingEntries int
	MaxDocuments      int
	MaxOutputBytes    int
	MaxWarnings       int
}

// DefaultLimits returns the spec §6 default resource limits.
func DefaultLimits() Limits {
	return Limits{
		MaxInputBytes:     8 << 20,
		MaxDepth:          128,
		MaxNodes:          250_000,
		MaxExpandedNodes:  100_000,
		MaxAliasEvents:    10_000,
		MaxOwnedBytes:     64 << 20,
		MaxScalarBytes:    1 << 20,
		MaxKeyBytes:       64 << 10,
		MaxMappingEntries: 100_000,
		MaxDocuments:      1_024,
		MaxOutputBytes:    64 << 20,
		MaxWarnings:       1_000,
	}
}

// Options configures loader behavior and resource limits.
type Options struct {
	MergeKeys   MergeKeys
	UnknownTags UnknownTags
	Limits      Limits
}

// DefaultOptions returns Options initialized with DefaultLimits,
// MergeKeysReject, and UnknownTagsPreserve.
func DefaultOptions() Options {
	return Options{
		MergeKeys:   MergeKeysReject,
		UnknownTags: UnknownTagsPreserve,
		Limits:      DefaultLimits(),
	}
}

// Validate checks that all 12 limits are >= 1, MaxDepth <= 255, and
// MaxScalarBytes and MaxKeyBytes do not exceed MaxOwnedBytes.
func (o Options) Validate() *Error {
	l := o.Limits
	all := [12]int{
		l.MaxInputBytes,
		l.MaxDepth,
		l.MaxNodes,
		l.MaxExpandedNodes,
		l.MaxAliasEvents,
		l.MaxOwnedBytes,
		l.MaxScalarBytes,
		l.MaxKeyBytes,
		l.MaxMappingEntries,
		l.MaxDocuments,
		l.MaxOutputBytes,
		l.MaxWarnings,
	}
	for _, v := range all {
		if v <= 0 {
			return newNotSourceError(ErrorKindInvalidOptions, nil, "every limit must be at least 1")
		}
	}
	if l.MaxDepth > 255 {
		return newNotSourceError(ErrorKindInvalidOptions, nil, "max_depth must be at most 255")
	}
	if l.MaxScalarBytes > l.MaxOwnedBytes || l.MaxKeyBytes > l.MaxOwnedBytes {
		return newNotSourceError(ErrorKindInvalidOptions, nil, "max_scalar_bytes and max_key_bytes must not exceed max_owned_bytes")
	}
	return nil
}

var utf8BOMBytes = []byte{0xEF, 0xBB, 0xBF}

const utf8BOMStr = "\xEF\xBB\xBF"

// GateBytes validates raw byte length against MaxInputBytes before stripping
// an optional initial UTF-8 BOM and validating UTF-8 (spec §6, §7.1, §7.2).
func GateBytes(input []byte, opts Options) (string, int, *Error) {
	if len(input) > opts.Limits.MaxInputBytes {
		return "", 0, newLimitError(
			LimitMaxInputBytes,
			uint64(opts.Limits.MaxInputBytes),
			Location{ByteOffset: 0, Line: 1, Column: 1},
		)
	}
	body := input
	bom := 0
	if bytes.HasPrefix(input, utf8BOMBytes) {
		body = input[3:]
		bom = 3
	}
	validUpTo, errLen, ok := validateUTF8(body)
	if !ok {
		loc := ScanLocation(body[:validUpTo], bom)
		err := newSourceError(ErrorKindInvalidUtf8, loc, "input is not valid UTF-8")
		err.InvalidUtf8Len = errLen
		return "", 0, err
	}
	return string(body), bom, nil
}

// GateStr validates string byte length against MaxInputBytes before stripping
// an optional initial UTF-8 BOM and validating UTF-8 (spec §6, §7.1, §7.2).
// A Go string can hold arbitrary bytes, so GateStr enforces the same UTF-8
// validity, InvalidUtf8Len, and original-byte Location rules as GateBytes.
func GateStr(input string, opts Options) (string, int, *Error) {
	if len(input) > opts.Limits.MaxInputBytes {
		return "", 0, newLimitError(
			LimitMaxInputBytes,
			uint64(opts.Limits.MaxInputBytes),
			Location{ByteOffset: 0, Line: 1, Column: 1},
		)
	}
	body := input
	bom := 0
	if strings.HasPrefix(input, utf8BOMStr) {
		body = input[3:]
		bom = 3
	}
	if !utf8.ValidString(body) {
		raw := []byte(body)
		validUpTo, errLen, _ := validateUTF8(raw)
		loc := ScanLocation(raw[:validUpTo], bom)
		err := newSourceError(ErrorKindInvalidUtf8, loc, "input is not valid UTF-8")
		err.InvalidUtf8Len = errLen
		return "", 0, err
	}
	return body, bom, nil
}

// ScanLocation computes the 1-based Line, 1-based Unicode scalar Column, and
// original ByteOffset at the end of a valid UTF-8 prefix following an optional
// stripped BOM (bom is 0 or 3). Recognizes LF, CRLF, and lone CR as line breaks.
func ScanLocation(prefix []byte, bom int) Location {
	line := 1
	col := 1
	for i := 0; i < len(prefix); {
		b := prefix[i]
		if b == '\r' {
			if i+1 < len(prefix) && prefix[i+1] == '\n' {
				i += 2
			} else {
				i++
			}
			line++
			col = 1
			continue
		}
		if b == '\n' {
			i++
			line++
			col = 1
			continue
		}
		if b < 0x80 {
			i++
			col++
			continue
		}
		_, size := utf8.DecodeRune(prefix[i:])
		i += size
		col++
	}
	return Location{
		ByteOffset: len(prefix) + bom,
		Line:       line,
		Column:     col,
	}
}

// validateUTF8 matches Rust std::str::from_utf8's (valid_up_to, error_len)
// semantics so InvalidUtf8 wire details match byte-for-byte.
func validateUTF8(b []byte) (validUpTo int, errLen *int, ok bool) {
	n := len(b)
	i := 0
	intPtr := func(v int) *int { return &v }
	for i < n {
		first := b[i]
		if first < 0x80 {
			i++
			continue
		}
		switch {
		case first >= 0xC2 && first <= 0xDF:
			if i+1 >= n {
				return i, nil, false
			}
			if b[i+1] < 0x80 || b[i+1] > 0xBF {
				return i, intPtr(1), false
			}
			i += 2
		case first >= 0xE0 && first <= 0xEF:
			if i+1 >= n {
				return i, nil, false
			}
			b1 := b[i+1]
			validSecond := b1 >= 0x80 && b1 <= 0xBF
			if first == 0xE0 {
				validSecond = b1 >= 0xA0 && b1 <= 0xBF
			} else if first == 0xED {
				validSecond = b1 >= 0x80 && b1 <= 0x9F
			}
			if !validSecond {
				return i, intPtr(1), false
			}
			if i+2 >= n {
				return i, nil, false
			}
			if b[i+2] < 0x80 || b[i+2] > 0xBF {
				return i, intPtr(2), false
			}
			i += 3
		case first >= 0xF0 && first <= 0xF4:
			if i+1 >= n {
				return i, nil, false
			}
			b1 := b[i+1]
			validSecond := b1 >= 0x80 && b1 <= 0xBF
			if first == 0xF0 {
				validSecond = b1 >= 0x90 && b1 <= 0xBF
			} else if first == 0xF4 {
				validSecond = b1 >= 0x80 && b1 <= 0x8F
			}
			if !validSecond {
				return i, intPtr(1), false
			}
			if i+2 >= n {
				return i, nil, false
			}
			if b[i+2] < 0x80 || b[i+2] > 0xBF {
				return i, intPtr(2), false
			}
			if i+3 >= n {
				return i, nil, false
			}
			if b[i+3] < 0x80 || b[i+3] > 0xBF {
				return i, intPtr(3), false
			}
			i += 4
		default:
			return i, intPtr(1), false
		}
	}
	return n, nil, true
}
