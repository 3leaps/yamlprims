package yaml

import (
	"fmt"
	"unicode/utf8"

	"github.com/3leaps/yamlprims/yaml/internal/engine"
)

const (
	// MaxMessageBytes is the maximum UTF-8 byte length of Error.Message on the wire.
	MaxMessageBytes = 4096
	// MaxFieldBytes is the maximum UTF-8 byte length of ErrorDetail Expected, Found, and Field strings on the wire.
	MaxFieldBytes = 1024
	truncatedMark = "…"
)

func boundedString(text string, limit int) string {
	if len(text) <= limit {
		return text
	}
	cut := limit - len(truncatedMark)
	if cut < 0 {
		return ""
	}
	for cut > 0 && !utf8.RuneStart(text[cut]) {
		cut--
	}
	return text[:cut] + truncatedMark
}

// SyntaxKind is the closed set of 81 machine-readable syntax failure codes (spec-v1.4 §7.1, ADR-0004).
type SyntaxKind = engine.SyntaxKind

// AllSyntaxKinds is the complete closed table of all 81 parser-emitted syntax error kinds.
var AllSyntaxKinds = engine.AllSyntaxKinds

// SyntaxKindFromCode parses a wire syntax_kind code into a SyntaxKind member.
func SyntaxKindFromCode(code string) (SyntaxKind, bool) {
	return engine.SyntaxKindFromCode(code)
}

// Re-exported SyntaxKind constants covering the 81-code vocabulary.
const (
	SyntaxUnexpectedEofFlowSequence              = engine.SyntaxUnexpectedEofFlowSequence
	SyntaxUnexpectedEofFlowMapping               = engine.SyntaxUnexpectedEofFlowMapping
	SyntaxUnexpectedEofImplicitFlowMapping       = engine.SyntaxUnexpectedEofImplicitFlowMapping
	SyntaxUnexpectedEofBlockSequence             = engine.SyntaxUnexpectedEofBlockSequence
	SyntaxUnexpectedEofBlockMapping              = engine.SyntaxUnexpectedEofBlockMapping
	SyntaxUnexpectedEof                          = engine.SyntaxUnexpectedEof
	SyntaxExpectedStreamStart                    = engine.SyntaxExpectedStreamStart
	SyntaxDuplicateVersionDirective              = engine.SyntaxDuplicateVersionDirective
	SyntaxDuplicateTagDirective                  = engine.SyntaxDuplicateTagDirective
	SyntaxExpectedDocumentStart                  = engine.SyntaxExpectedDocumentStart
	SyntaxMissingDocumentEndBeforeDirective      = engine.SyntaxMissingDocumentEndBeforeDirective
	SyntaxAnchorCountOverflow                    = engine.SyntaxAnchorCountOverflow
	SyntaxExpectedNodeContent                    = engine.SyntaxExpectedNodeContent
	SyntaxExpectedBlockMappingKey                = engine.SyntaxExpectedBlockMappingKey
	SyntaxExpectedFlowMappingSeparator           = engine.SyntaxExpectedFlowMappingSeparator
	SyntaxExpectedFlowSequenceSeparator          = engine.SyntaxExpectedFlowSequenceSeparator
	SyntaxExpectedBlockSequenceEntry             = engine.SyntaxExpectedBlockSequenceEntry
	SyntaxUndeclaredTagHandle                    = engine.SyntaxUndeclaredTagHandle
	SyntaxExpectedTagBang                        = engine.SyntaxExpectedTagBang
	SyntaxExpectedTagDirectiveBang               = engine.SyntaxExpectedTagDirectiveBang
	SyntaxInvalidGlobalTagCharacter              = engine.SyntaxInvalidGlobalTagCharacter
	SyntaxSimpleKeyExpected                      = engine.SyntaxSimpleKeyExpected
	SyntaxInvalidSimpleKey                       = engine.SyntaxInvalidSimpleKey
	SyntaxInvalidDocumentEnd                     = engine.SyntaxInvalidDocumentEnd
	SyntaxInvalidIndentation                     = engine.SyntaxInvalidIndentation
	SyntaxBomInsideDocument                      = engine.SyntaxBomInsideDocument
	SyntaxUnexpectedCharacter                    = engine.SyntaxUnexpectedCharacter
	SyntaxTabNotAllowed                          = engine.SyntaxTabNotAllowed
	SyntaxTabInBlockIndentation                  = engine.SyntaxTabInBlockIndentation
	SyntaxCommentInterceptedScalar               = engine.SyntaxCommentInterceptedScalar
	SyntaxExpectedWhitespace                     = engine.SyntaxExpectedWhitespace
	SyntaxCommentNotSeparated                    = engine.SyntaxCommentNotSeparated
	SyntaxInvalidDirectiveTerminator             = engine.SyntaxInvalidDirectiveTerminator
	SyntaxMissingYamlVersionSeparator            = engine.SyntaxMissingYamlVersionSeparator
	SyntaxMissingDirectiveName                   = engine.SyntaxMissingDirectiveName
	SyntaxInvalidDirectiveName                   = engine.SyntaxInvalidDirectiveName
	SyntaxDirectiveByteLimitExceeded             = engine.SyntaxDirectiveByteLimitExceeded
	SyntaxTooManyReservedDirectiveParams         = engine.SyntaxTooManyReservedDirectiveParams
	SyntaxYamlVersionTooLong                     = engine.SyntaxYamlVersionTooLong
	SyntaxMissingYamlVersion                     = engine.SyntaxMissingYamlVersion
	SyntaxInvalidTagDirectiveTerminator          = engine.SyntaxInvalidTagDirectiveTerminator
	SyntaxInvalidTagTerminator                   = engine.SyntaxInvalidTagTerminator
	SyntaxMissingTagUri                          = engine.SyntaxMissingTagUri
	SyntaxUnclosedVerbatimTag                    = engine.SyntaxUnclosedVerbatimTag
	SyntaxInvalidTagEscape                       = engine.SyntaxInvalidTagEscape
	SyntaxInvalidTagUtf8LeadingByte              = engine.SyntaxInvalidTagUtf8LeadingByte
	SyntaxInvalidTagUtf8TrailingByte             = engine.SyntaxInvalidTagUtf8TrailingByte
	SyntaxInvalidTagUtf8                         = engine.SyntaxInvalidTagUtf8
	SyntaxMissingAnchorOrAliasName               = engine.SyntaxMissingAnchorOrAliasName
	SyntaxMisplacedFlowCollectionEnd             = engine.SyntaxMisplacedFlowCollectionEnd
	SyntaxMismatchedFlowCollectionEnd            = engine.SyntaxMismatchedFlowCollectionEnd
	SyntaxUnclosedFlowCollection                 = engine.SyntaxUnclosedFlowCollection
	SyntaxBlockEntryInFlowCollection             = engine.SyntaxBlockEntryInFlowCollection
	SyntaxBlockSequenceEntryNotAllowed           = engine.SyntaxBlockSequenceEntryNotAllowed
	SyntaxInvalidBlockEntryWhitespace            = engine.SyntaxInvalidBlockEntryWhitespace
	SyntaxZeroBlockScalarIndent                  = engine.SyntaxZeroBlockScalarIndent
	SyntaxInvalidBlockScalarHeader               = engine.SyntaxInvalidBlockScalarHeader
	SyntaxTabAtBlockScalarStart                  = engine.SyntaxTabAtBlockScalarStart
	SyntaxInvalidBlockScalarIndent               = engine.SyntaxInvalidBlockScalarIndent
	SyntaxDocumentIndicatorInQuotedScalar        = engine.SyntaxDocumentIndicatorInQuotedScalar
	SyntaxUnclosedQuotedScalar                   = engine.SyntaxUnclosedQuotedScalar
	SyntaxTabInIndentation                       = engine.SyntaxTabInIndentation
	SyntaxInvalidQuotedScalarIndent              = engine.SyntaxInvalidQuotedScalarIndent
	SyntaxInvalidTrailingSingleQuotedScalar      = engine.SyntaxInvalidTrailingSingleQuotedScalar
	SyntaxInvalidTrailingDoubleQuotedScalar      = engine.SyntaxInvalidTrailingDoubleQuotedScalar
	SyntaxUnknownQuotedScalarEscape              = engine.SyntaxUnknownQuotedScalarEscape
	SyntaxInvalidQuotedScalarHexEscape           = engine.SyntaxInvalidQuotedScalarHexEscape
	SyntaxInvalidLowSurrogateHexEscape           = engine.SyntaxInvalidLowSurrogateHexEscape
	SyntaxInvalidLowSurrogate                    = engine.SyntaxInvalidLowSurrogate
	SyntaxMissingLowSurrogate                    = engine.SyntaxMissingLowSurrogate
	SyntaxUnpairedLowSurrogate                   = engine.SyntaxUnpairedLowSurrogate
	SyntaxInvalidUnicodeEscape                   = engine.SyntaxInvalidUnicodeEscape
	SyntaxInvalidFlowScalarIndent                = engine.SyntaxInvalidFlowScalarIndent
	SyntaxPlainScalarStartsWithDashFlowIndicator = engine.SyntaxPlainScalarStartsWithDashFlowIndicator
	SyntaxTabInPlainScalar                       = engine.SyntaxTabInPlainScalar
	SyntaxUnexpectedEndOfPlainScalar             = engine.SyntaxUnexpectedEndOfPlainScalar
	SyntaxMappingKeyNotAllowed                   = engine.SyntaxMappingKeyNotAllowed
	SyntaxFlowMappingValueAdjacentCollection     = engine.SyntaxFlowMappingValueAdjacentCollection
	SyntaxInvalidMappingValueWhitespace          = engine.SyntaxInvalidMappingValueWhitespace
	SyntaxInvalidColonPlacement                  = engine.SyntaxInvalidColonPlacement
	SyntaxMappingValueNotAllowed                 = engine.SyntaxMappingValueNotAllowed
)

// LimitKind identifies the configured budget that was exceeded.
type LimitKind string

const (
	LimitMaxInputBytes     LimitKind = "max_input_bytes"
	LimitMaxDepth          LimitKind = "max_depth"
	LimitMaxNodes          LimitKind = "max_nodes"
	LimitMaxExpandedNodes  LimitKind = "max_expanded_nodes"
	LimitMaxAliasEvents    LimitKind = "max_alias_events"
	LimitMaxOwnedBytes     LimitKind = "max_owned_bytes"
	LimitMaxScalarBytes    LimitKind = "max_scalar_bytes"
	LimitMaxKeyBytes       LimitKind = "max_key_bytes"
	LimitMaxMappingEntries LimitKind = "max_mapping_entries"
	LimitMaxDocuments      LimitKind = "max_documents"
)

// Name returns the snake_case wire name of the limit.
func (l LimitKind) Name() string {
	return string(l)
}

// FieldName returns the snake_case wire name of the limit.
func (l LimitKind) FieldName() string {
	return string(l)
}

// String returns the snake_case wire name of the limit.
func (l LimitKind) String() string {
	return string(l)
}

// IsValid reports whether l is one of the 10 normative LimitKind values.
func (l LimitKind) IsValid() bool {
	switch l {
	case LimitMaxInputBytes,
		LimitMaxDepth,
		LimitMaxNodes,
		LimitMaxExpandedNodes,
		LimitMaxAliasEvents,
		LimitMaxOwnedBytes,
		LimitMaxScalarBytes,
		LimitMaxKeyBytes,
		LimitMaxMappingEntries,
		LimitMaxDocuments:
		return true
	default:
		return false
	}
}

// ErrorKind is the closed machine-readable failure category (spec-v1.4 §7.1).
type ErrorKind string

const (
	ErrorKindInvalidOptions         ErrorKind = "invalid_options"
	ErrorKindInvalidUtf8            ErrorKind = "invalid_utf8"
	ErrorKindSyntax                 ErrorKind = "syntax"
	ErrorKindUnsupportedYamlVersion ErrorKind = "unsupported_yaml_version"
	ErrorKindLimitExceeded          ErrorKind = "limit_exceeded"
	ErrorKindDuplicateKey           ErrorKind = "duplicate_key"
	ErrorKindInvalidKey             ErrorKind = "invalid_key"
	ErrorKindUnknownAnchor          ErrorKind = "unknown_anchor"
	ErrorKindRecursiveAlias         ErrorKind = "recursive_alias"
	ErrorKindMergeRejected          ErrorKind = "merge_rejected"
	ErrorKindMergeSource            ErrorKind = "merge_source"
	ErrorKindUnknownTag             ErrorKind = "unknown_tag"
	ErrorKindTagKindMismatch        ErrorKind = "tag_kind_mismatch"
	ErrorKindTagValueMismatch       ErrorKind = "tag_value_mismatch"
	ErrorKindIntegerOutOfRange      ErrorKind = "integer_out_of_range"
	ErrorKindFloatOutOfRange        ErrorKind = "float_out_of_range"
	ErrorKindMultipleDocuments      ErrorKind = "multiple_documents"
	ErrorKindType                   ErrorKind = "type"
	ErrorKindMissingField           ErrorKind = "missing_field"
	ErrorKindUnknownField           ErrorKind = "unknown_field"
	ErrorKindCustom                 ErrorKind = "custom"
	ErrorKindOutputLimitExceeded    ErrorKind = "output_limit_exceeded"
	ErrorKindInternal               ErrorKind = "internal"

	// Aliases for descriptive spellings used in task briefs.
	ErrorKindMergeKeyRejected  ErrorKind = ErrorKindMergeRejected
	ErrorKindInvalidMergeValue ErrorKind = ErrorKindMergeSource
)

// Name returns the snake_case wire name of the error kind.
func (k ErrorKind) Name() string {
	return string(k)
}

// String returns the snake_case wire name of the error kind.
func (k ErrorKind) String() string {
	return string(k)
}

// IsValid reports whether k is one of the 23 normative ErrorKind values.
func (k ErrorKind) IsValid() bool {
	switch k {
	case ErrorKindInvalidOptions,
		ErrorKindInvalidUtf8,
		ErrorKindSyntax,
		ErrorKindUnsupportedYamlVersion,
		ErrorKindLimitExceeded,
		ErrorKindDuplicateKey,
		ErrorKindInvalidKey,
		ErrorKindUnknownAnchor,
		ErrorKindRecursiveAlias,
		ErrorKindMergeRejected,
		ErrorKindMergeSource,
		ErrorKindUnknownTag,
		ErrorKindTagKindMismatch,
		ErrorKindTagValueMismatch,
		ErrorKindIntegerOutOfRange,
		ErrorKindFloatOutOfRange,
		ErrorKindMultipleDocuments,
		ErrorKindType,
		ErrorKindMissingField,
		ErrorKindUnknownField,
		ErrorKindCustom,
		ErrorKindOutputLimitExceeded,
		ErrorKindInternal:
		return true
	default:
		return false
	}
}

// ErrorDetail carries the structured detail payload associated with Error.Kind.
type ErrorDetail struct {
	// InvalidUtf8Len is the invalid byte sequence length for ErrorKindInvalidUtf8 (nil if unknown).
	InvalidUtf8Len *int
	// HasInvalidUtf8 marks that Detail was initialized for ErrorKindInvalidUtf8.
	HasInvalidUtf8 bool
	// SyntaxKind is the machine-readable code for ErrorKindSyntax.
	SyntaxKind SyntaxKind
	// Limit is the exceeded budget for ErrorKindLimitExceeded.
	Limit LimitKind
	// Configured is the configured budget value for ErrorKindLimitExceeded or ErrorKindOutputLimitExceeded.
	Configured uint64
	// HasConfigured marks that Configured was set for ErrorKindLimitExceeded or ErrorKindOutputLimitExceeded.
	HasConfigured bool
	// Expected is the expected type description for ErrorKindType.
	Expected string
	// Found is the found type description for ErrorKindType.
	Found string
	// HasType marks that Expected/Found were set for ErrorKindType.
	HasType bool
	// Field is the field name for ErrorKindMissingField or ErrorKindUnknownField.
	Field string
	// HasField marks that Field was set for ErrorKindMissingField or ErrorKindUnknownField.
	HasField bool
}

// SiteKind distinguishes source-located errors from non-source errors.
type SiteKind uint8

const (
	SiteKindSource SiteKind = iota
	SiteKindNotSource
)

// Site records whether an error occurred at a specific input Location or outside input source.
type Site struct {
	Kind      SiteKind
	Source    Location
	LastKnown *Location
}

// SiteSource constructs a source-located Site.
func SiteSource(loc Location) Site {
	return Site{Kind: SiteKindSource, Source: loc}
}

// SiteNotSource constructs a non-source Site with an optional last-known Location.
func SiteNotSource(lastKnown *Location) Site {
	var copyLoc *Location
	if lastKnown != nil {
		l := *lastKnown
		copyLoc = &l
	}
	return Site{Kind: SiteKindNotSource, LastKnown: copyLoc}
}

// RelatedRole identifies the role of a secondary Span attached to an Error.
type RelatedRole string

const (
	RelatedRoleFirstDefinition  RelatedRole = "first_definition"
	RelatedRoleAliasUse         RelatedRole = "alias_use"
	RelatedRoleMergeDirective   RelatedRole = "merge_directive"
	RelatedRoleAnchorDefinition RelatedRole = "anchor_definition"
)

// Name returns the snake_case wire name of the related role.
func (r RelatedRole) Name() string {
	return string(r)
}

// IsValid reports whether r is one of the 4 normative RelatedRole values.
func (r RelatedRole) IsValid() bool {
	switch r {
	case RelatedRoleFirstDefinition,
		RelatedRoleAliasUse,
		RelatedRoleMergeDirective,
		RelatedRoleAnchorDefinition:
		return true
	default:
		return false
	}
}

// Related attaches a secondary source Span with a machine-readable role.
type Related struct {
	Role RelatedRole
	Span Span
}

// PathSegmentKind distinguishes mapping-key path segments from sequence-index path segments.
type PathSegmentKind uint8

const (
	PathSegmentKey PathSegmentKind = iota
	PathSegmentIndex
)

// PathSegment is one step in an Error's logical path from the document root.
type PathSegment struct {
	Kind  PathSegmentKind
	Key   Value
	Index int
}

// PathKey constructs a mapping-key PathSegment from a deep defensive copy of key.
func PathKey(key Value) PathSegment {
	return PathSegment{Kind: PathSegmentKey, Key: key.Clone()}
}

// PathIndex constructs a sequence-index PathSegment.
func PathIndex(index int) PathSegment {
	return PathSegment{Kind: PathSegmentIndex, Index: index}
}

// Error is a structured failure returned by loading, typed conversion, or wire/plain encoding.
type Error struct {
	Kind         ErrorKind
	Detail       ErrorDetail
	Site         Site
	Related      []Related
	Path         []PathSegment
	Message      string
	engineDetail string
}

// Error implements the standard Go error interface (matching Rust Display for Error).
func (e *Error) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Site.Kind == SiteKindSource {
		loc := e.Site.Source
		return fmt.Sprintf("%d:%d: %s: %s", loc.Line, loc.Column, e.Kind.Name(), e.Message)
	}
	return fmt.Sprintf("%s: %s", e.Kind.Name(), e.Message)
}

// Location returns the source Location when Site.Kind == SiteKindSource.
func (e *Error) Location() (Location, bool) {
	if e != nil && e.Site.Kind == SiteKindSource {
		return e.Site.Source, true
	}
	return Location{}, false
}

// SyntaxKind returns the SyntaxKind when Kind == ErrorKindSyntax.
func (e *Error) SyntaxKind() (SyntaxKind, bool) {
	if e != nil && e.Kind == ErrorKindSyntax && e.Detail.SyntaxKind.AsStr() != "" {
		return e.Detail.SyntaxKind, true
	}
	return 0, false
}

// EngineDetail returns the optional raw parser diagnostic message for syntax errors.
func (e *Error) EngineDetail() string {
	if e == nil {
		return ""
	}
	return e.engineDetail
}

// WithRelated appends a Related span and returns e.
func (e *Error) WithRelated(role RelatedRole, span Span) *Error {
	e.Related = append(e.Related, Related{Role: role, Span: span})
	return e
}

func newSourceError(kind ErrorKind, detail ErrorDetail, loc Location, message string) *Error {
	return &Error{
		Kind:    kind,
		Detail:  detail,
		Site:    SiteSource(loc),
		Message: boundedString(message, MaxMessageBytes),
	}
}

func newNotSourceError(kind ErrorKind, detail ErrorDetail, lastKnown *Location, message string) *Error {
	return &Error{
		Kind:    kind,
		Detail:  detail,
		Site:    SiteNotSource(lastKnown),
		Message: boundedString(message, MaxMessageBytes),
	}
}

func errInvalidOptions(message string) *Error {
	return newNotSourceError(ErrorKindInvalidOptions, ErrorDetail{}, nil, message)
}

func errInternal(what string) *Error {
	return newNotSourceError(ErrorKindInternal, ErrorDetail{}, nil, "internal error: "+what)
}

func errOutputLimit(configured int) *Error {
	return newNotSourceError(
		ErrorKindOutputLimitExceeded,
		ErrorDetail{Configured: uint64(configured), HasConfigured: true},
		nil,
		fmt.Sprintf("encoded output exceeds max_output_bytes (%d)", configured),
	)
}

func errLimit(limit LimitKind, configured int, loc Location) *Error {
	return newSourceError(
		ErrorKindLimitExceeded,
		ErrorDetail{Limit: limit, Configured: uint64(configured), HasConfigured: true},
		loc,
		fmt.Sprintf("%s (%d) exceeded", limit.Name(), configured),
	)
}

func errSyntax(loc Location, syntaxKind SyntaxKind, message string, engineDetail string) *Error {
	err := newSourceError(
		ErrorKindSyntax,
		ErrorDetail{SyntaxKind: syntaxKind},
		loc,
		message,
	)
	if engineDetail != "" {
		err.engineDetail = boundedString(engineDetail, MaxMessageBytes)
	}
	return err
}

func errInvalidUtf8(loc Location, invalidLen *int) *Error {
	var copyLen *int
	if invalidLen != nil {
		v := *invalidLen
		copyLen = &v
	}
	return newSourceError(
		ErrorKindInvalidUtf8,
		ErrorDetail{InvalidUtf8Len: copyLen, HasInvalidUtf8: true},
		loc,
		"invalid UTF-8",
	)
}

func errSourceSimple(kind ErrorKind, loc Location, message string) *Error {
	return newSourceError(kind, ErrorDetail{}, loc, message)
}

func errTypeAtSite(site Site, expected, found, message string) *Error {
	return &Error{
		Kind: ErrorKindType,
		Detail: ErrorDetail{
			Expected: boundedString(expected, MaxFieldBytes),
			Found:    boundedString(found, MaxFieldBytes),
			HasType:  true,
		},
		Site:    site,
		Message: boundedString(message, MaxMessageBytes),
	}
}

func fromEngineLoc(l engine.Location) Location {
	return Location{
		ByteOffset: l.ByteOffset,
		Line:       l.Line,
		Column:     l.Column,
	}
}

func fromEngineSpan(s engine.Span) Span {
	return Span{
		Start: fromEngineLoc(s.Start),
		End:   fromEngineLoc(s.End),
	}
}

func fromEngineLimit(k engine.LimitKind) LimitKind {
	return LimitKind(k.FieldName())
}

func fromEngineError(ee *engine.Error) *Error {
	if ee == nil {
		return nil
	}
	loc := fromEngineLoc(ee.Site.Location)
	switch ee.Kind {
	case engine.ErrorKindInvalidUtf8:
		return errInvalidUtf8(loc, ee.InvalidUtf8Len)
	case engine.ErrorKindSyntax:
		return errSyntax(loc, ee.Syntax, ee.Message, ee.EngineDetail)
	case engine.ErrorKindUnsupportedYamlVersion:
		err := errSourceSimple(ErrorKindUnsupportedYamlVersion, loc, ee.Message)
		err.engineDetail = ee.EngineDetail
		return err
	case engine.ErrorKindLimitExceeded:
		err := errLimit(fromEngineLimit(ee.Limit), int(ee.Configured), loc)
		err.engineDetail = ee.EngineDetail
		return err
	case engine.ErrorKindInvalidOptions:
		return errInvalidOptions(ee.Message)
	case engine.ErrorKindUnknownAnchor:
		err := errSourceSimple(ErrorKindUnknownAnchor, loc, ee.Message)
		err.engineDetail = ee.EngineDetail
		return err
	default:
		var lastKnown *Location
		if ee.Site.LastKnown != nil {
			l := fromEngineLoc(*ee.Site.LastKnown)
			lastKnown = &l
		}
		return newNotSourceError(ErrorKindInternal, ErrorDetail{}, lastKnown, ee.Message)
	}
}
