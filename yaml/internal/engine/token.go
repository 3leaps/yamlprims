package engine

import (
	"fmt"
	"strings"
)

// ScalarStyle identifies the source notation used for a scalar.
type ScalarStyle uint8

const (
	ScalarStylePlain ScalarStyle = iota
	ScalarStyleSingleQuoted
	ScalarStyleDoubleQuoted
	ScalarStyleLiteral
	ScalarStyleFolded
)

func (s ScalarStyle) IsPlain() bool {
	return s == ScalarStylePlain
}

func (s ScalarStyle) IsLiteral() bool {
	return s == ScalarStyleLiteral
}

func (s ScalarStyle) IsFolded() bool {
	return s == ScalarStyleFolded
}

// StructureStyle identifies the notation style of a sequence or mapping.
type StructureStyle uint8

const (
	StructureStyleBlock StructureStyle = iota
	StructureStyleFlow
)

// Version is the YAML version declared by a %YAML directive.
type Version struct {
	Major uint32
	Minor uint32
}

// Tag represents a YAML node tag after directive resolution.
type Tag struct {
	Handle         string
	Suffix         string
	OriginalHandle string
}

const yamlCoreSchemaPrefix = "tag:yaml.org,2002:"

var yamlCoreSchemaSuffixes = [7]string{
	"bool", "float", "int", "map", "null", "seq", "str",
}

// CoreSuffix returns the resolved YAML 1.2.2 Core Schema suffix ("bool",
// "float", "int", "map", "null", "seq", or "str") if t resolves to a Core
// Schema tag URI, regardless of how the handle and suffix were split.
func (t *Tag) CoreSuffix() (string, bool) {
	if t == nil {
		return "", false
	}
	if strings.HasPrefix(yamlCoreSchemaPrefix, t.Handle) {
		remPrefix := yamlCoreSchemaPrefix[len(t.Handle):]
		if !strings.HasPrefix(t.Suffix, remPrefix) {
			return "", false
		}
		tail := t.Suffix[len(remPrefix):]
		for _, cand := range yamlCoreSchemaSuffixes {
			if cand == tail {
				return cand, true
			}
		}
		return "", false
	}
	if strings.HasPrefix(t.Handle, yamlCoreSchemaPrefix) {
		handleTail := t.Handle[len(yamlCoreSchemaPrefix):]
		for _, cand := range yamlCoreSchemaSuffixes {
			if strings.HasPrefix(cand, handleTail) && cand[len(handleTail):] == t.Suffix {
				return cand, true
			}
		}
	}
	return "", false
}

func (t Tag) String() string {
	if t.Handle == "!" {
		return "!" + t.Suffix
	}
	return t.Handle + t.Suffix
}

// TagClassKind categorizes a node's tag for Core Schema resolution.
type TagClassKind uint8

const (
	TagClassNone TagClassKind = iota
	TagClassNonSpecific
	TagClassCore
	TagClassMerge
	TagClassOther
)

// TagClass holds the classified tag of an event node.
type TagClass struct {
	Kind       TagClassKind
	CoreSuffix string
	Handle     string
	Suffix     string
}

// ClassifyTag classifies an optional Tag into None, NonSpecific ("!"),
// Core (one of the 7 YAML 1.2.2 Core Schema tags), Merge
// ("tag:yaml.org,2002:merge"), or Other(handle, suffix).
func ClassifyTag(t *Tag) TagClass {
	if t == nil {
		return TagClass{Kind: TagClassNone}
	}
	if t.Handle == "" && t.Suffix == "!" {
		return TagClass{Kind: TagClassNonSpecific}
	}
	if c, ok := t.CoreSuffix(); ok {
		return TagClass{Kind: TagClassCore, CoreSuffix: c}
	}
	if t.Handle+t.Suffix == "tag:yaml.org,2002:merge" {
		return TagClass{Kind: TagClassMerge}
	}
	return TagClass{
		Kind:   TagClassOther,
		Handle: t.Handle,
		Suffix: t.Suffix,
	}
}

type tokenKind uint8

const (
	tokenNoToken tokenKind = iota
	tokenStreamStart
	tokenStreamEnd
	tokenVersionDirective
	tokenTagDirective
	tokenReservedDirective
	tokenDocumentStart
	tokenDocumentEnd
	tokenBlockSequenceStart
	tokenBlockMappingStart
	tokenBlockEnd
	tokenFlowSequenceStart
	tokenFlowSequenceEnd
	tokenFlowMappingStart
	tokenFlowMappingEnd
	tokenBlockEntry
	tokenFlowEntry
	tokenKey
	tokenValue
	tokenAlias
	tokenAnchor
	tokenTag
	tokenScalar
)

type queuedToken struct {
	span   rawSpan
	kind   tokenKind
	major  uint32
	minor  uint32
	text1  string
	text2  string
	params []string
	style  ScalarStyle
}

type simpleKey struct {
	possible    bool
	required    bool
	tokenNumber int
	mark        Marker
}

func newSimpleKey() simpleKey {
	return simpleKey{
		possible:    false,
		required:    false,
		tokenNumber: 0,
		mark:        newMarker(0, false, 0, 0, 0),
	}
}

type indentEntry struct {
	indent        int
	needsBlockEnd bool
}

type implicitMappingState struct {
	inside    bool
	flowLevel int
}

type chomping uint8

const (
	chompingStrip chomping = iota
	chompingClip
	chompingKeep
)

type skipTabs uint8

const (
	skipTabsYes skipTabs = iota
	skipTabsNo
)

type whitespaceResult struct {
	encounteredTab bool
	hasYamlWs      bool
}

func (w whitespaceResult) foundTabs() bool {
	return w.encounteredTab
}

func (w whitespaceResult) hasValidYamlWs() bool {
	return w.hasYamlWs
}

type scanToNextTokenStatus struct {
	sawComment bool
}

type scanErrKind uint8

const (
	scanErrUnexpectedEofFlowSequence scanErrKind = iota + 1
	scanErrUnexpectedEofFlowMapping
	scanErrUnexpectedEofImplicitFlowMapping
	scanErrUnexpectedEofBlockSequence
	scanErrUnexpectedEofBlockMapping
	scanErrUnexpectedEof
	scanErrExpectedStreamStart
	scanErrDuplicateVersionDirective
	scanErrUnsupportedYamlMajorVersion
	scanErrDuplicateTagDirective
	scanErrExpectedDocumentStart
	scanErrMissingDocumentEndBeforeDirective
	scanErrAnchorCountOverflow
	scanErrUnknownAnchor
	scanErrExpectedNodeContent
	scanErrExpectedBlockMappingKey
	scanErrExpectedFlowMappingSeparator
	scanErrExpectedFlowSequenceSeparator
	scanErrExpectedBlockSequenceEntry
	scanErrUndeclaredTagHandle
	scanErrExpectedTagBang
	scanErrExpectedTagDirectiveBang
	scanErrInvalidGlobalTagCharacter
	scanErrSimpleKeyExpected
	scanErrInvalidSimpleKey
	scanErrInvalidDocumentEnd
	scanErrInvalidIndentation
	scanErrBomInsideDocument
	scanErrUnexpectedCharacter
	scanErrTabNotAllowed
	scanErrTabInBlockIndentation
	scanErrCommentInterceptedScalar
	scanErrExpectedWhitespace
	scanErrCommentNotSeparated
	scanErrInvalidDirectiveTerminator
	scanErrMissingYamlVersionSeparator
	scanErrMissingDirectiveName
	scanErrInvalidDirectiveName
	scanErrDirectiveByteLimitExceeded
	scanErrTooManyReservedDirectiveParams
	scanErrYamlVersionTooLong
	scanErrMissingYamlVersion
	scanErrInvalidTagDirectiveTerminator
	scanErrInvalidTagTerminator
	scanErrMissingTagUri
	scanErrUnclosedVerbatimTag
	scanErrInvalidTagEscape
	scanErrInvalidTagUtf8LeadingByte
	scanErrInvalidTagUtf8TrailingByte
	scanErrInvalidTagUtf8
	scanErrMissingAnchorOrAliasName
	scanErrRecursionLimitExceeded
	scanErrMisplacedFlowCollectionEnd
	scanErrMismatchedFlowCollectionEnd
	scanErrUnclosedFlowCollection
	scanErrBlockEntryInFlowCollection
	scanErrBlockSequenceEntryNotAllowed
	scanErrInvalidBlockEntryWhitespace
	scanErrZeroBlockScalarIndent
	scanErrInvalidBlockScalarHeader
	scanErrTabAtBlockScalarStart
	scanErrInvalidBlockScalarIndent
	scanErrDocumentIndicatorInQuotedScalar
	scanErrUnclosedQuotedScalar
	scanErrTabInIndentation
	scanErrInvalidQuotedScalarIndent
	scanErrInvalidTrailingSingleQuotedScalar
	scanErrInvalidTrailingDoubleQuotedScalar
	scanErrUnknownQuotedScalarEscape
	scanErrInvalidQuotedScalarHexEscape
	scanErrInvalidLowSurrogateHexEscape
	scanErrInvalidLowSurrogate
	scanErrMissingLowSurrogate
	scanErrUnpairedLowSurrogate
	scanErrInvalidUnicodeEscape
	scanErrInvalidFlowScalarIndent
	scanErrPlainScalarStartsWithDashFlowIndicator
	scanErrTabInPlainScalar
	scanErrUnexpectedEndOfPlainScalar
	scanErrMappingKeyNotAllowed
	scanErrFlowMappingValueAdjacentCollection
	scanErrInvalidMappingValueWhitespace
	scanErrInvalidColonPlacement
	scanErrMappingValueNotAllowed
	scanErrInputOffsetsWithoutSlice
)

type scanError struct {
	mark        Marker
	kind        scanErrKind
	ch          rune
	closing     rune
	expected    rune
	openingLine int
	openingCol  int
}

func newScanError(mark Marker, kind scanErrKind) *scanError {
	return &scanError{mark: mark, kind: kind}
}

func escapeDefault(c rune) string {
	switch c {
	case '\t':
		return `\t`
	case '\r':
		return `\r`
	case '\n':
		return `\n`
	case '\\':
		return `\\`
	case '\'':
		return `\'`
	case '"':
		return `\"`
	default:
		if c >= 0x20 && c <= 0x7E {
			return string(c)
		}
		return fmt.Sprintf(`\u{%x}`, c)
	}
}

func (e *scanError) info() string {
	switch e.kind {
	case scanErrUnexpectedEofFlowSequence:
		return "unexpected EOF while parsing a flow sequence"
	case scanErrUnexpectedEofFlowMapping:
		return "unexpected EOF while parsing a flow mapping"
	case scanErrUnexpectedEofImplicitFlowMapping:
		return "unexpected EOF while parsing an implicit flow mapping"
	case scanErrUnexpectedEofBlockSequence:
		return "unexpected EOF while parsing a block sequence"
	case scanErrUnexpectedEofBlockMapping:
		return "unexpected EOF while parsing a block mapping"
	case scanErrUnexpectedEof:
		return "unexpected eof"
	case scanErrExpectedStreamStart:
		return "did not find expected <stream-start>"
	case scanErrDuplicateVersionDirective:
		return "duplicate version directive"
	case scanErrUnsupportedYamlMajorVersion:
		return "unsupported YAML major version"
	case scanErrDuplicateTagDirective:
		return "the TAG directive must only be given at most once per handle in the same document"
	case scanErrExpectedDocumentStart:
		return "did not find expected <document start>"
	case scanErrMissingDocumentEndBeforeDirective:
		return "missing explicit document end marker before directive"
	case scanErrAnchorCountOverflow:
		return "while parsing anchor, anchor count exceeded supported limit"
	case scanErrUnknownAnchor:
		return "while parsing node, found unknown anchor"
	case scanErrExpectedNodeContent:
		return "while parsing a node, did not find expected node content"
	case scanErrExpectedBlockMappingKey:
		return "while parsing a block mapping, did not find expected key"
	case scanErrExpectedFlowMappingSeparator:
		return "while parsing a flow mapping, did not find expected ',' or '}'"
	case scanErrExpectedFlowSequenceSeparator:
		return "while parsing a flow sequence, expected ',' or ']'"
	case scanErrExpectedBlockSequenceEntry:
		return "while parsing a block collection, did not find expected '-' indicator"
	case scanErrUndeclaredTagHandle:
		return "the handle wasn't declared"
	case scanErrExpectedTagBang:
		return "while scanning a tag, did not find expected '!'"
	case scanErrExpectedTagDirectiveBang:
		return "while parsing a tag directive, did not find expected '!'"
	case scanErrInvalidGlobalTagCharacter:
		return "invalid global tag character"
	case scanErrSimpleKeyExpected:
		return "simple key expected ':'"
	case scanErrInvalidSimpleKey:
		return "simple key is no longer valid"
	case scanErrInvalidDocumentEnd:
		return "invalid content after document end marker"
	case scanErrInvalidIndentation:
		return "invalid indentation"
	case scanErrBomInsideDocument:
		return "a BOM must not appear inside a document"
	case scanErrUnexpectedCharacter:
		return fmt.Sprintf("unexpected character: `%s'", escapeDefault(e.ch))
	case scanErrTabNotAllowed:
		return "tabs disallowed in this context"
	case scanErrTabInBlockIndentation:
		return "tabs disallowed within this context (block indentation)"
	case scanErrCommentInterceptedScalar:
		return "comment intercepting the multiline text"
	case scanErrExpectedWhitespace:
		return "expected whitespace"
	case scanErrCommentNotSeparated:
		return "comments must be separated from other tokens by whitespace"
	case scanErrInvalidDirectiveTerminator:
		return "while scanning a directive, did not find expected comment or line break"
	case scanErrMissingYamlVersionSeparator:
		return "while scanning a YAML directive, did not find expected digit or '.' character"
	case scanErrMissingDirectiveName:
		return "while scanning a directive, could not find expected directive name"
	case scanErrInvalidDirectiveName:
		return "while scanning a directive, found unexpected non-alphabetical character"
	case scanErrDirectiveByteLimitExceeded:
		return fmt.Sprintf("directive exceeds the configured limit of %d bytes", maxDirectiveBytes)
	case scanErrTooManyReservedDirectiveParams:
		return fmt.Sprintf("reserved directive exceeds the configured limit of %d parameters", maxReservedDirectiveParams)
	case scanErrYamlVersionTooLong:
		return "while scanning a YAML directive, found extremely long version number"
	case scanErrMissingYamlVersion:
		return "while scanning a YAML directive, did not find expected version number"
	case scanErrInvalidTagDirectiveTerminator:
		return "while scanning TAG, did not find expected whitespace or line break"
	case scanErrInvalidTagTerminator:
		return "while scanning a tag, did not find expected whitespace or line break"
	case scanErrMissingTagUri:
		return "while parsing a tag, did not find expected tag URI"
	case scanErrUnclosedVerbatimTag:
		return "while scanning a verbatim tag, did not find the expected '>'"
	case scanErrInvalidTagEscape:
		return "while parsing a tag, found an invalid escape sequence"
	case scanErrInvalidTagUtf8LeadingByte:
		return "while parsing a tag, found an incorrect leading UTF-8 byte"
	case scanErrInvalidTagUtf8TrailingByte:
		return "while parsing a tag, found an incorrect trailing UTF-8 byte"
	case scanErrInvalidTagUtf8:
		return "while parsing a tag, found an invalid UTF-8 codepoint"
	case scanErrMissingAnchorOrAliasName:
		return "while scanning an anchor or alias, did not find expected alphabetic or numeric character"
	case scanErrRecursionLimitExceeded:
		return "recursion limit exceeded"
	case scanErrMisplacedFlowCollectionEnd:
		return "misplaced bracket"
	case scanErrMismatchedFlowCollectionEnd:
		return fmt.Sprintf("mismatched bracket '%c' closed by '%c'", e.expected, e.closing)
	case scanErrUnclosedFlowCollection:
		return fmt.Sprintf("unclosed bracket '%c'", e.closing)
	case scanErrBlockEntryInFlowCollection:
		return `"-" is only valid inside a block`
	case scanErrBlockSequenceEntryNotAllowed:
		return "block sequence entries are not allowed in this context"
	case scanErrInvalidBlockEntryWhitespace:
		return "'-' must be followed by a valid YAML whitespace"
	case scanErrZeroBlockScalarIndent:
		return "while scanning a block scalar, found an indentation indicator equal to 0"
	case scanErrInvalidBlockScalarHeader:
		return "while scanning a block scalar, did not find expected comment or line break"
	case scanErrTabAtBlockScalarStart:
		return "a block scalar content cannot start with a tab"
	case scanErrInvalidBlockScalarIndent:
		return "wrongly indented line in block scalar"
	case scanErrDocumentIndicatorInQuotedScalar:
		return "while scanning a quoted scalar, found unexpected document indicator"
	case scanErrUnclosedQuotedScalar:
		return "unclosed quote"
	case scanErrTabInIndentation:
		return "tab cannot be used as indentation"
	case scanErrInvalidQuotedScalarIndent:
		return "invalid indentation in multiline quoted scalar"
	case scanErrInvalidTrailingSingleQuotedScalar:
		return "invalid trailing content after single-quoted scalar"
	case scanErrInvalidTrailingDoubleQuotedScalar:
		return "invalid trailing content after double-quoted scalar"
	case scanErrUnknownQuotedScalarEscape:
		return "while parsing a quoted scalar, found unknown escape character"
	case scanErrInvalidQuotedScalarHexEscape:
		return "while parsing a quoted scalar, did not find expected hexadecimal number"
	case scanErrInvalidLowSurrogateHexEscape:
		return "while parsing a quoted scalar, did not find expected hexadecimal number for low surrogate"
	case scanErrInvalidLowSurrogate:
		return "while parsing a quoted scalar, found invalid low surrogate"
	case scanErrMissingLowSurrogate:
		return "while parsing a quoted scalar, found high surrogate without following low surrogate"
	case scanErrUnpairedLowSurrogate:
		return "while parsing a quoted scalar, found unpaired low surrogate"
	case scanErrInvalidUnicodeEscape:
		return "while parsing a quoted scalar, found invalid Unicode character escape code"
	case scanErrInvalidFlowScalarIndent:
		return "invalid indentation in flow construct"
	case scanErrPlainScalarStartsWithDashFlowIndicator:
		return "plain scalar cannot start with '-' followed by ,[]{}"
	case scanErrTabInPlainScalar:
		return "while scanning a plain scalar, found a tab"
	case scanErrUnexpectedEndOfPlainScalar:
		return "unexpected end of plain scalar"
	case scanErrMappingKeyNotAllowed:
		return "mapping keys are not allowed in this context"
	case scanErrFlowMappingValueAdjacentCollection:
		return "':' may not precede any of `[{` in flow mapping"
	case scanErrInvalidMappingValueWhitespace:
		return "':' must be followed by a valid YAML whitespace"
	case scanErrInvalidColonPlacement:
		return "illegal placement of ':' indicator"
	case scanErrMappingValueNotAllowed:
		return "mapping values are not allowed in this context"
	case scanErrInputOffsetsWithoutSlice:
		return "internal error: input advertised offsets but did not provide a slice"
	default:
		return ""
	}
}
