package engine

import (
	"strings"
	"testing"
)

func runGateAndSyntaxPassBytes(input []byte, opts Options) *Error {
	if err := opts.Validate(); err != nil {
		return err
	}
	text, bom, err := GateBytes(input, opts)
	if err != nil {
		return err
	}
	return New(text, bom, opts).SyntaxPass()
}

func runGateAndSyntaxPassStr(input string, opts Options) *Error {
	if err := opts.Validate(); err != nil {
		return err
	}
	text, bom, err := GateStr(input, opts)
	if err != nil {
		return err
	}
	return New(text, bom, opts).SyntaxPass()
}

func assertLimitDepth(t *testing.T, err *Error, wantConfigured uint64, wantLoc Location) {
	t.Helper()
	if err == nil {
		t.Fatalf("expected LimitMaxDepth error, got nil")
	}
	if err.Kind != ErrorKindLimitExceeded || err.Limit != LimitMaxDepth || err.Configured != wantConfigured {
		t.Fatalf("unexpected error kind/limit: %+v", err)
	}
	loc, ok := err.Location()
	if !ok || loc != wantLoc {
		t.Fatalf("location = (%+v, %v), want (%+v, true)", loc, ok, wantLoc)
	}
}

func assertSyntaxError(t *testing.T, err *Error, wantKind SyntaxKind, wantLoc Location) {
	t.Helper()
	if err == nil {
		t.Fatalf("expected syntax error %s, got nil", wantKind)
	}
	if err.Kind != ErrorKindSyntax || err.Syntax != wantKind {
		t.Fatalf("got kind=%v syntax=%v, want syntax=%v (%+v)", err.Kind, err.Syntax, wantKind, err)
	}
	loc, ok := err.Location()
	if !ok || loc != wantLoc {
		t.Fatalf("location = (%+v, %v), want (%+v, true)", loc, ok, wantLoc)
	}
}

func TestSourceDepthStopsBeforeBuildOrLaterSyntax(t *testing.T) {
	opts := DefaultOptions()
	opts.Limits.MaxDepth = 1

	err := runGateAndSyntaxPassStr("[[]]", opts)
	assertLimitDepth(t, err, 1, Location{ByteOffset: 1, Line: 1, Column: 2})

	err = runGateAndSyntaxPassStr("---\n[[]]\n---\n[\n", opts)
	assertLimitDepth(t, err, 1, Location{ByteOffset: 5, Line: 2, Column: 2})

	err = runGateAndSyntaxPassStr("a:\n  - 1\n", opts)
	assertLimitDepth(t, err, 1, Location{ByteOffset: 5, Line: 2, Column: 3})

	err = runGateAndSyntaxPassStr("[[1]]\n*missing\n", opts)
	assertLimitDepth(t, err, 1, Location{ByteOffset: 1, Line: 1, Column: 2})

	opts.Limits.MaxDepth = 2
	err = runGateAndSyntaxPassStr("[[", opts)
	assertSyntaxError(t, err, SyntaxUnclosedFlowCollection, Location{ByteOffset: 1, Line: 1, Column: 2})
}

func TestSourceDepthPrecedesMaterializationAndPolicy(t *testing.T) {
	opts := DefaultOptions()
	opts.Limits.MaxDepth = 1
	opts.UnknownTags = UnknownTagsReject
	opts.MergeKeys = MergeKeysReject
	opts.Limits.MaxNodes = 1
	opts.Limits.MaxExpandedNodes = 1
	opts.Limits.MaxAliasEvents = 1
	opts.Limits.MaxOwnedBytes = 64
	opts.Limits.MaxScalarBytes = 1
	opts.Limits.MaxKeyBytes = 1
	opts.Limits.MaxMappingEntries = 1
	opts.Limits.MaxDocuments = 1

	cases := []string{
		"!t [[]]",
		"{<<: []}",
		"&a [[1]]\n*a\n",
		"[[!custom x]]\n",
		"a: 1\na: [[2]]\n",
		"[[9999999999999999999999999999999999999999]]\n",
	}
	for _, src := range cases {
		err := runGateAndSyntaxPassStr(src, opts)
		if err == nil || err.Kind != ErrorKindLimitExceeded || err.Limit != LimitMaxDepth || err.Configured != 1 {
			t.Fatalf("input %q: got %+v, want LimitMaxDepth(1)", src, err)
		}
	}
}

func TestTerminalErrorsDoNotRestartOrConstruct(t *testing.T) {
	opts := DefaultOptions()

	eng := New("*missing\na: [1, 2\n", 0, opts)
	var firstErr *Error
	for {
		_, _, ok, err := eng.Next()
		if err != nil {
			firstErr = err
			break
		}
		if !ok {
			t.Fatal("expected unknown_anchor error, stream ended cleanly")
		}
	}
	if firstErr.Kind != ErrorKindUnknownAnchor {
		t.Fatalf("got %v, want ErrorKindUnknownAnchor", firstErr.Kind)
	}
	if loc, ok := firstErr.Location(); !ok || loc != (Location{ByteOffset: 0, Line: 1, Column: 1}) {
		t.Fatalf("got location (%+v, %v), want {0, 1, 1}", loc, ok)
	}
	// Stream is fused after a terminal error.
	if _, _, ok, err := eng.Next(); ok || err != nil {
		t.Fatalf("expected fused iterator after terminal error, got ok=%v err=%v", ok, err)
	}

	err := runGateAndSyntaxPassStr("%YAML 2.0\n---\n[unclosed\n", opts)
	if err == nil || err.Kind != ErrorKindUnsupportedYamlVersion {
		t.Fatalf("got %+v, want ErrorKindUnsupportedYamlVersion", err)
	}
	if loc, ok := err.Location(); !ok || loc != (Location{ByteOffset: 0, Line: 1, Column: 1}) {
		t.Fatalf("got location (%+v, %v), want {0, 1, 1}", loc, ok)
	}

	err = runGateAndSyntaxPassStr("a: 1\n\tb: 2\n", opts)
	assertSyntaxError(t, err, SyntaxTabInPlainScalar, Location{ByteOffset: 3, Line: 1, Column: 4})

	err = runGateAndSyntaxPassStr("a:\n  b: \"1\"\n\tc: 2\n", opts)
	assertSyntaxError(t, err, SyntaxTabInBlockIndentation, Location{ByteOffset: 13, Line: 3, Column: 2})

	err = runGateAndSyntaxPassStr("a: &dup 1\nb: *missing\n---\n[unclosed\n", opts)
	if err == nil || err.Kind != ErrorKindUnknownAnchor {
		t.Fatalf("got %+v, want ErrorKindUnknownAnchor", err)
	}
	if loc, ok := err.Location(); !ok || loc != (Location{ByteOffset: 13, Line: 2, Column: 4}) {
		t.Fatalf("got location (%+v, %v), want {13, 2, 4}", loc, ok)
	}
}

func TestOptionsAndRawInputGatesPrecedeAllParsers(t *testing.T) {
	// Zero-value Options{} fails Validate() with InvalidOptions.
	if err := (Options{}).Validate(); err == nil || err.Kind != ErrorKindInvalidOptions || err.Site.Kind != SiteNotSource {
		t.Fatalf("zero Options.Validate() = %+v, want InvalidOptions", err)
	}

	opts := DefaultOptions()
	opts.Limits.MaxDepth = 0
	err := runGateAndSyntaxPassBytes([]byte{0xff}, opts)
	if err == nil || err.Kind != ErrorKindInvalidOptions || err.Site.Kind != SiteNotSource || err.Site.LastKnown != nil {
		t.Fatalf("got %+v, want InvalidOptions with nil LastKnown", err)
	}

	opts = DefaultOptions()
	opts.Limits.MaxDepth = 256
	if err := runGateAndSyntaxPassStr("a: 1\n", opts); err == nil || err.Kind != ErrorKindInvalidOptions {
		t.Fatalf("got %+v, want InvalidOptions for MaxDepth=256", err)
	}

	opts = DefaultOptions()
	opts.Limits.MaxOwnedBytes = 128
	opts.Limits.MaxScalarBytes = 256
	opts.Limits.MaxKeyBytes = 64
	if err := runGateAndSyntaxPassStr("a: 1\n", opts); err == nil || err.Kind != ErrorKindInvalidOptions {
		t.Fatalf("got %+v, want InvalidOptions for MaxScalarBytes > MaxOwnedBytes", err)
	}

	opts = DefaultOptions()
	opts.Limits.MaxOwnedBytes = 128
	opts.Limits.MaxScalarBytes = 64
	opts.Limits.MaxKeyBytes = 256
	if err := runGateAndSyntaxPassStr("a: 1\n", opts); err == nil || err.Kind != ErrorKindInvalidOptions {
		t.Fatalf("got %+v, want InvalidOptions for MaxKeyBytes > MaxOwnedBytes", err)
	}

	// MaxInputBytes precedes UTF-8 validation.
	opts = DefaultOptions()
	opts.Limits.MaxInputBytes = 1
	err = runGateAndSyntaxPassBytes([]byte{0xff, 0xff}, opts)
	if err == nil || err.Kind != ErrorKindLimitExceeded || err.Limit != LimitMaxInputBytes || err.Configured != 1 {
		t.Fatalf("got %+v, want LimitMaxInputBytes(1)", err)
	}
	if loc, ok := err.Location(); !ok || loc != (Location{ByteOffset: 0, Line: 1, Column: 1}) {
		t.Fatalf("got location (%+v, %v), want {0, 1, 1}", loc, ok)
	}

	// BOM included in ByteOffset, excluded from line-1 Column.
	err = runGateAndSyntaxPassBytes([]byte("\xEF\xBB\xBF\xff"), DefaultOptions())
	if err == nil || err.Kind != ErrorKindInvalidUtf8 || err.InvalidUtf8Len == nil || *err.InvalidUtf8Len != 1 {
		t.Fatalf("got %+v, want InvalidUtf8(len=1)", err)
	}
	if loc, ok := err.Location(); !ok || loc != (Location{ByteOffset: 3, Line: 1, Column: 1}) {
		t.Fatalf("got location (%+v, %v), want {3, 1, 1}", loc, ok)
	}

	// CRLF, lone CR, and LF line breaks + truncated multi-byte UTF-8 at EOF (InvalidUtf8Len == nil).
	err = runGateAndSyntaxPassBytes([]byte("a\r\nb\rc\n\x80"), DefaultOptions())
	if err == nil || err.Kind != ErrorKindInvalidUtf8 || err.InvalidUtf8Len == nil || *err.InvalidUtf8Len != 1 {
		t.Fatalf("got %+v, want InvalidUtf8(len=1)", err)
	}
	if loc, ok := err.Location(); !ok || loc != (Location{ByteOffset: 7, Line: 4, Column: 1}) {
		t.Fatalf("got location (%+v, %v), want {7, 4, 1}", loc, ok)
	}

	err = runGateAndSyntaxPassBytes([]byte("é\xE2\x82"), DefaultOptions())
	if err == nil || err.Kind != ErrorKindInvalidUtf8 || err.InvalidUtf8Len != nil {
		t.Fatalf("got %+v, want InvalidUtf8(len=nil) for truncated 3-byte sequence at EOF", err)
	}
	if loc, ok := err.Location(); !ok || loc != (Location{ByteOffset: 2, Line: 1, Column: 2}) {
		t.Fatalf("got location (%+v, %v), want {2, 1, 2}", loc, ok)
	}
}

func TestFiniteNativeGuardsRemainResourceErrorsAtNativeSites(t *testing.T) {
	for _, depth := range []int{1, 255} {
		opts := DefaultOptions()
		opts.Limits.MaxDepth = depth
		ceiling := depth + 1024 + 2
		cases := []struct {
			src      string
			wantByte int
		}{
			{strings.Repeat("[", 2048) + "0" + strings.Repeat("]", 2048), ceiling},
			{strings.Repeat("- ", 2048) + "0", 2 * ceiling},
		}
		for _, tc := range cases {
			eng := New(tc.src, 3, opts)
			var nativeErr *Error
			for {
				_, _, ok, err := eng.Next()
				if err != nil {
					nativeErr = err
					break
				}
				if !ok {
					t.Fatalf("depth=%d: expected native recursion limit error, stream ended cleanly", depth)
				}
			}
			assertLimitDepth(t, nativeErr, uint64(depth), Location{
				ByteOffset: tc.wantByte + 3,
				Line:       1,
				Column:     tc.wantByte + 1,
			})
			if nativeErr.EngineDetail == "" {
				t.Fatalf("depth=%d: expected non-empty EngineDetail", depth)
			}
		}
	}
}

func TestSyntaxKindClosedVocabularyAndExhaustiveMapping(t *testing.T) {
	if len(AllSyntaxKinds) != 81 {
		t.Fatalf("len(AllSyntaxKinds) = %d, want 81", len(AllSyntaxKinds))
	}
	seen := make(map[string]SyntaxKind, len(AllSyntaxKinds))
	for _, sk := range AllSyntaxKinds {
		code := sk.AsStr()
		if code == "" {
			t.Fatalf("SyntaxKind(%d) returned empty code", sk)
		}
		if prev, dup := seen[code]; dup {
			t.Fatalf("duplicate SyntaxKind code %q for %d and %d", code, prev, sk)
		}
		seen[code] = sk
		parsed, ok := SyntaxKindFromCode(code)
		if !ok || parsed != sk {
			t.Fatalf("SyntaxKindFromCode(%q) = (%v, %v), want (%v, true)", code, parsed, ok, sk)
		}
	}
	if _, ok := SyntaxKindFromCode("not_a_syntax_kind"); ok {
		t.Fatal("SyntaxKindFromCode accepted unknown code")
	}

	// Verify Engine.scanErr maps every scanErrKind in the syntax vocabulary to its exact SyntaxKind.
	eng := New("", 0, DefaultOptions())
	locMark := newMarker(4, true, 4, 2, 1)
	table := []struct {
		raw  scanErrKind
		want SyntaxKind
	}{
		{scanErrUnexpectedEofFlowSequence, SyntaxUnexpectedEofFlowSequence},
		{scanErrUnexpectedEofFlowMapping, SyntaxUnexpectedEofFlowMapping},
		{scanErrUnexpectedEofImplicitFlowMapping, SyntaxUnexpectedEofImplicitFlowMapping},
		{scanErrUnexpectedEofBlockSequence, SyntaxUnexpectedEofBlockSequence},
		{scanErrUnexpectedEofBlockMapping, SyntaxUnexpectedEofBlockMapping},
		{scanErrUnexpectedEof, SyntaxUnexpectedEof},
		{scanErrExpectedStreamStart, SyntaxExpectedStreamStart},
		{scanErrDuplicateVersionDirective, SyntaxDuplicateVersionDirective},
		{scanErrDuplicateTagDirective, SyntaxDuplicateTagDirective},
		{scanErrExpectedDocumentStart, SyntaxExpectedDocumentStart},
		{scanErrMissingDocumentEndBeforeDirective, SyntaxMissingDocumentEndBeforeDirective},
		{scanErrAnchorCountOverflow, SyntaxAnchorCountOverflow},
		{scanErrExpectedNodeContent, SyntaxExpectedNodeContent},
		{scanErrExpectedBlockMappingKey, SyntaxExpectedBlockMappingKey},
		{scanErrExpectedFlowMappingSeparator, SyntaxExpectedFlowMappingSeparator},
		{scanErrExpectedFlowSequenceSeparator, SyntaxExpectedFlowSequenceSeparator},
		{scanErrExpectedBlockSequenceEntry, SyntaxExpectedBlockSequenceEntry},
		{scanErrUndeclaredTagHandle, SyntaxUndeclaredTagHandle},
		{scanErrExpectedTagBang, SyntaxExpectedTagBang},
		{scanErrExpectedTagDirectiveBang, SyntaxExpectedTagDirectiveBang},
		{scanErrInvalidGlobalTagCharacter, SyntaxInvalidGlobalTagCharacter},
		{scanErrSimpleKeyExpected, SyntaxSimpleKeyExpected},
		{scanErrInvalidSimpleKey, SyntaxInvalidSimpleKey},
		{scanErrInvalidDocumentEnd, SyntaxInvalidDocumentEnd},
		{scanErrInvalidIndentation, SyntaxInvalidIndentation},
		{scanErrBomInsideDocument, SyntaxBomInsideDocument},
		{scanErrUnexpectedCharacter, SyntaxUnexpectedCharacter},
		{scanErrTabNotAllowed, SyntaxTabNotAllowed},
		{scanErrTabInBlockIndentation, SyntaxTabInBlockIndentation},
		{scanErrCommentInterceptedScalar, SyntaxCommentInterceptedScalar},
		{scanErrExpectedWhitespace, SyntaxExpectedWhitespace},
		{scanErrCommentNotSeparated, SyntaxCommentNotSeparated},
		{scanErrInvalidDirectiveTerminator, SyntaxInvalidDirectiveTerminator},
		{scanErrMissingYamlVersionSeparator, SyntaxMissingYamlVersionSeparator},
		{scanErrMissingDirectiveName, SyntaxMissingDirectiveName},
		{scanErrInvalidDirectiveName, SyntaxInvalidDirectiveName},
		{scanErrDirectiveByteLimitExceeded, SyntaxDirectiveByteLimitExceeded},
		{scanErrTooManyReservedDirectiveParams, SyntaxTooManyReservedDirectiveParams},
		{scanErrYamlVersionTooLong, SyntaxYamlVersionTooLong},
		{scanErrMissingYamlVersion, SyntaxMissingYamlVersion},
		{scanErrInvalidTagDirectiveTerminator, SyntaxInvalidTagDirectiveTerminator},
		{scanErrInvalidTagTerminator, SyntaxInvalidTagTerminator},
		{scanErrMissingTagUri, SyntaxMissingTagUri},
		{scanErrUnclosedVerbatimTag, SyntaxUnclosedVerbatimTag},
		{scanErrInvalidTagEscape, SyntaxInvalidTagEscape},
		{scanErrInvalidTagUtf8LeadingByte, SyntaxInvalidTagUtf8LeadingByte},
		{scanErrInvalidTagUtf8TrailingByte, SyntaxInvalidTagUtf8TrailingByte},
		{scanErrInvalidTagUtf8, SyntaxInvalidTagUtf8},
		{scanErrMissingAnchorOrAliasName, SyntaxMissingAnchorOrAliasName},
		{scanErrMisplacedFlowCollectionEnd, SyntaxMisplacedFlowCollectionEnd},
		{scanErrMismatchedFlowCollectionEnd, SyntaxMismatchedFlowCollectionEnd},
		{scanErrUnclosedFlowCollection, SyntaxUnclosedFlowCollection},
		{scanErrBlockEntryInFlowCollection, SyntaxBlockEntryInFlowCollection},
		{scanErrBlockSequenceEntryNotAllowed, SyntaxBlockSequenceEntryNotAllowed},
		{scanErrInvalidBlockEntryWhitespace, SyntaxInvalidBlockEntryWhitespace},
		{scanErrZeroBlockScalarIndent, SyntaxZeroBlockScalarIndent},
		{scanErrInvalidBlockScalarHeader, SyntaxInvalidBlockScalarHeader},
		{scanErrTabAtBlockScalarStart, SyntaxTabAtBlockScalarStart},
		{scanErrInvalidBlockScalarIndent, SyntaxInvalidBlockScalarIndent},
		{scanErrDocumentIndicatorInQuotedScalar, SyntaxDocumentIndicatorInQuotedScalar},
		{scanErrUnclosedQuotedScalar, SyntaxUnclosedQuotedScalar},
		{scanErrTabInIndentation, SyntaxTabInIndentation},
		{scanErrInvalidQuotedScalarIndent, SyntaxInvalidQuotedScalarIndent},
		{scanErrInvalidTrailingSingleQuotedScalar, SyntaxInvalidTrailingSingleQuotedScalar},
		{scanErrInvalidTrailingDoubleQuotedScalar, SyntaxInvalidTrailingDoubleQuotedScalar},
		{scanErrUnknownQuotedScalarEscape, SyntaxUnknownQuotedScalarEscape},
		{scanErrInvalidQuotedScalarHexEscape, SyntaxInvalidQuotedScalarHexEscape},
		{scanErrInvalidLowSurrogateHexEscape, SyntaxInvalidLowSurrogateHexEscape},
		{scanErrInvalidLowSurrogate, SyntaxInvalidLowSurrogate},
		{scanErrMissingLowSurrogate, SyntaxMissingLowSurrogate},
		{scanErrUnpairedLowSurrogate, SyntaxUnpairedLowSurrogate},
		{scanErrInvalidUnicodeEscape, SyntaxInvalidUnicodeEscape},
		{scanErrInvalidFlowScalarIndent, SyntaxInvalidFlowScalarIndent},
		{scanErrPlainScalarStartsWithDashFlowIndicator, SyntaxPlainScalarStartsWithDashFlowIndicator},
		{scanErrTabInPlainScalar, SyntaxTabInPlainScalar},
		{scanErrUnexpectedEndOfPlainScalar, SyntaxUnexpectedEndOfPlainScalar},
		{scanErrMappingKeyNotAllowed, SyntaxMappingKeyNotAllowed},
		{scanErrFlowMappingValueAdjacentCollection, SyntaxFlowMappingValueAdjacentCollection},
		{scanErrInvalidMappingValueWhitespace, SyntaxInvalidMappingValueWhitespace},
		{scanErrInvalidColonPlacement, SyntaxInvalidColonPlacement},
		{scanErrMappingValueNotAllowed, SyntaxMappingValueNotAllowed},
	}
	if len(table) != 81 {
		t.Fatalf("len(table) = %d, want 81", len(table))
	}
	for _, tc := range table {
		err := eng.scanErr(newScanError(locMark, tc.raw))
		assertSyntaxError(t, err, tc.want, Location{ByteOffset: 4, Line: 2, Column: 2})
	}
}

func TestOrdinaryVsIndentlessSequenceSpansAndBOMCoordinates(t *testing.T) {
	opts := DefaultOptions()
	// BOM (3 bytes) + "a:\n- !t x\nb:\n  - y\n"
	raw := "\xEF\xBB\xBFa:\n- !t x\nb:\n  - y\n"
	text, bom, err := GateStr(raw, opts)
	if err != nil {
		t.Fatalf("GateStr failed: %v", err)
	}
	if bom != 3 {
		t.Fatalf("bom = %d, want 3", bom)
	}
	eng := New(text, bom, opts)
	var seqStarts []EventSpan
	var taggedScalar EventSpan
	for {
		ev, sp, ok, err := eng.Next()
		if err != nil {
			t.Fatalf("Next failed: %v", err)
		}
		if !ok {
			break
		}
		if ev.Kind == EventSequenceStart {
			seqStarts = append(seqStarts, sp)
		}
		if ev.Kind == EventScalar && ev.Value == "x" {
			taggedScalar = sp
		}
	}
	if len(seqStarts) != 2 {
		t.Fatalf("got %d sequence starts, want 2", len(seqStarts))
	}
	// First sequence is indentless under `a:`: empty span at the first token after `-` (`!t` at byte 8, line 2, col 3).
	if seqStarts[0].Span != (Span{
		Start: Location{ByteOffset: 8, Line: 2, Column: 3},
		End:   Location{ByteOffset: 8, Line: 2, Column: 3},
	}) {
		t.Fatalf("indentless seq start span = %+v", seqStarts[0].Span)
	}
	// Second sequence is an ordinary indented block sequence under `b:`: empty span at `-` [18, 18) on line 4 col 3.
	if seqStarts[1].Span != (Span{
		Start: Location{ByteOffset: 18, Line: 4, Column: 3},
		End:   Location{ByteOffset: 18, Line: 4, Column: 3},
	}) {
		t.Fatalf("ordinary seq start span = %+v", seqStarts[1].Span)
	}
	// Tagged scalar `!t x` has TagStart at `!t` (byte 8, line 2, col 3) and Span at `x` [11, 12).
	if taggedScalar.TagStart == nil || *taggedScalar.TagStart != (Location{ByteOffset: 8, Line: 2, Column: 3}) {
		t.Fatalf("tagged scalar TagStart = %+v, want {8, 2, 3}", taggedScalar.TagStart)
	}
	if taggedScalar.Span != (Span{
		Start: Location{ByteOffset: 11, Line: 2, Column: 6},
		End:   Location{ByteOffset: 12, Line: 2, Column: 7},
	}) {
		t.Fatalf("tagged scalar Span = %+v", taggedScalar.Span)
	}
}

func TestBlockScalarChompingAndTagClassification(t *testing.T) {
	opts := DefaultOptions()
	// EOF keep-chomping with indented whitespace-only line vs header-only.
	cases := []struct {
		src  string
		want string
	}{
		{"- |+\n  ", "\n"},
		{"- |+\n", ""},
		{"- |-\n  hello\n\n", "hello"},
		{"- |\n  hello\n\n", "hello\n"},
		{"- |+\n  hello\n\n", "hello\n\n"},
	}
	for _, tc := range cases {
		eng := New(tc.src, 0, opts)
		var scalars []string
		for {
			ev, _, ok, err := eng.Next()
			if err != nil {
				t.Fatalf("input %q failed: %v", tc.src, err)
			}
			if !ok {
				break
			}
			if ev.Kind == EventScalar {
				scalars = append(scalars, ev.Value)
			}
		}
		if len(scalars) != 1 || scalars[0] != tc.want {
			t.Fatalf("input %q: got scalars %q, want [%q]", tc.src, scalars, tc.want)
		}
	}

	// Verify %TAG split handle resolution for Core Schema and Merge tags.
	tagSrc := "%TAG !m! tag:yaml.org,2002:\n---\n!m!merge <<\n"
	eng := New(tagSrc, 0, opts)
	var foundMerge bool
	for {
		ev, _, ok, err := eng.Next()
		if err != nil {
			t.Fatalf("tagSrc failed: %v", err)
		}
		if !ok {
			break
		}
		if ev.Kind == EventScalar {
			if ClassifyTag(ev.Tag).Kind == TagClassMerge {
				foundMerge = true
			}
		}
	}
	if !foundMerge {
		t.Fatal("expected ClassifyTag to recognize split TAG directive merge tag")
	}
}

func TestGateStrAndGateBytesUTF8Parity(t *testing.T) {
	opts := DefaultOptions()

	intPtr := func(v int) *int { return &v }
	equalIntPtr := func(a, b *int) bool {
		if a == nil && b == nil {
			return true
		}
		if a == nil || b == nil {
			return false
		}
		return *a == *b
	}

	invalidCases := []struct {
		name    string
		raw     string
		wantLoc Location
		wantLen *int
	}{
		{
			name:    "single_0xff",
			raw:     "\xff",
			wantLoc: Location{ByteOffset: 0, Line: 1, Column: 1},
			wantLen: intPtr(1),
		},
		{
			name:    "bom_then_0xff",
			raw:     "\xef\xbb\xbf\xff",
			wantLoc: Location{ByteOffset: 3, Line: 1, Column: 1},
			wantLen: intPtr(1),
		},
		{
			name:    "mapping_value_0xff",
			raw:     "a: \xff\n",
			wantLoc: Location{ByteOffset: 3, Line: 1, Column: 4},
			wantLen: intPtr(1),
		},
		{
			name:    "comment_0xff",
			raw:     "#\xff",
			wantLoc: Location{ByteOffset: 1, Line: 1, Column: 2},
			wantLen: intPtr(1),
		},
		{
			name:    "truncated_3byte_after_multibyte",
			raw:     "é\xe2\x82",
			wantLoc: Location{ByteOffset: 2, Line: 1, Column: 2},
			wantLen: nil,
		},
		{
			name:    "truncated_2byte",
			raw:     "\xc2",
			wantLoc: Location{ByteOffset: 0, Line: 1, Column: 1},
			wantLen: nil,
		},
		{
			name:    "truncated_4byte",
			raw:     "\xf0\x90\x80",
			wantLoc: Location{ByteOffset: 0, Line: 1, Column: 1},
			wantLen: nil,
		},
		{
			name:    "bom_crlf_cr_before_bad_byte",
			raw:     "\xef\xbb\xbfé\r\nx\r\xff",
			wantLoc: Location{ByteOffset: 9, Line: 3, Column: 1},
			wantLen: intPtr(1),
		},
	}

	for _, tc := range invalidCases {
		_, _, errB := GateBytes([]byte(tc.raw), opts)
		_, _, errS := GateStr(tc.raw, opts)
		if errB == nil || errS == nil {
			t.Fatalf("%s: expected InvalidUtf8 on both paths, got GateBytes=%v GateStr=%v", tc.name, errB, errS)
		}
		if errB.Kind != ErrorKindInvalidUtf8 || errS.Kind != ErrorKindInvalidUtf8 {
			t.Fatalf("%s: kind mismatch GateBytes=%v GateStr=%v", tc.name, errB.Kind, errS.Kind)
		}
		locB, okB := errB.Location()
		locS, okS := errS.Location()
		if !okB || !okS || locB != tc.wantLoc || locS != tc.wantLoc {
			t.Fatalf("%s: location mismatch GateBytes=(%+v,%v) GateStr=(%+v,%v) want=%+v", tc.name, locB, okB, locS, okS, tc.wantLoc)
		}
		if !equalIntPtr(errB.InvalidUtf8Len, tc.wantLen) || !equalIntPtr(errS.InvalidUtf8Len, tc.wantLen) {
			t.Fatalf("%s: InvalidUtf8Len mismatch GateBytes=%v GateStr=%v want=%v", tc.name, errB.InvalidUtf8Len, errS.InvalidUtf8Len, tc.wantLen)
		}
	}

	// Raw byte limit precedence over InvalidUtf8 on both GateStr and GateBytes.
	smallOpts := DefaultOptions()
	smallOpts.Limits.MaxInputBytes = 3
	overRaw := "\xef\xbb\xbf\xff"
	_, _, errB := GateBytes([]byte(overRaw), smallOpts)
	_, _, errS := GateStr(overRaw, smallOpts)
	for label, err := range map[string]*Error{"GateBytes": errB, "GateStr": errS} {
		if err == nil || err.Kind != ErrorKindLimitExceeded || err.Limit != LimitMaxInputBytes {
			t.Fatalf("%s raw-limit precedence failed: %+v", label, err)
		}
		loc, ok := err.Location()
		if !ok || loc != (Location{ByteOffset: 0, Line: 1, Column: 1}) {
			t.Fatalf("%s raw-limit location = %+v (%v)", label, loc, ok)
		}
	}

	// Valid U+FFFD replacement character must be preserved as valid input on both paths.
	validFFFD := "\xef\xbb\xbfa: \xef\xbf\xbd\n"
	sB, bomB, errB := GateBytes([]byte(validFFFD), opts)
	sS, bomS, errS := GateStr(validFFFD, opts)
	if errB != nil || errS != nil {
		t.Fatalf("valid U+FFFD rejected: GateBytes=%v GateStr=%v", errB, errS)
	}
	if sB != "a: \uFFFD\n" || sS != "a: \uFFFD\n" || bomB != 3 || bomS != 3 {
		t.Fatalf("valid U+FFFD mismatch: (%q,%d) vs (%q,%d)", sB, bomB, sS, bomS)
	}
	eng := New(sS, bomS, opts)
	if err := eng.SyntaxPass(); err != nil {
		t.Fatalf("SyntaxPass on U+FFFD failed: %v", err)
	}
}

func TestScannerParityEmbeddedNulTagsQuotedFoldAndMidDocBOM(t *testing.T) {
	opts := DefaultOptions()

	// F1: Embedded NUL is not treated as EOF.
	nulCases := []struct {
		src      string
		wantLoc  Location
		wantCode SyntaxKind
	}{
		{"a: 1\x00\nb: 2\n", Location{ByteOffset: 4, Line: 1, Column: 5}, SyntaxUnexpectedCharacter},
		{"\"a\x00b\"", Location{ByteOffset: 2, Line: 1, Column: 3}, SyntaxUnexpectedCharacter},
		{"[a\x00]", Location{ByteOffset: 2, Line: 1, Column: 3}, SyntaxUnexpectedCharacter},
		{"|\n  a\x00\n", Location{ByteOffset: 5, Line: 2, Column: 4}, SyntaxUnexpectedCharacter},
	}
	for _, tc := range nulCases {
		eng := New(tc.src, 0, opts)
		err := eng.SyntaxPass()
		if err == nil || err.Kind != ErrorKindSyntax || err.Syntax != tc.wantCode {
			t.Fatalf("F1 input %q: got err=%+v, want Syntax(%s)", tc.src, err, tc.wantCode.AsStr())
		}
		loc, ok := err.Location()
		if !ok || loc != tc.wantLoc {
			t.Fatalf("F1 input %q: location = %+v (%v), want %+v", tc.src, loc, ok, tc.wantLoc)
		}
	}

	// F2: '%' in URI character set for tag shorthand and verbatim tag escapes.
	tagShorthand := "%TAG !e! tag:x,1:\n---\n- !e!tag%21 x\n"
	eng := New(tagShorthand, 0, opts)
	if err := eng.SyntaxPass(); err != nil {
		t.Fatalf("F2 shorthand SyntaxPass failed: %v", err)
	}
	var gotTag *Tag
	for {
		ev, _, ok, err := eng.Next()
		if err != nil {
			t.Fatalf("F2 shorthand Next failed: %v", err)
		}
		if !ok {
			break
		}
		if ev.Kind == EventScalar {
			gotTag = ev.Tag
		}
	}
	if gotTag == nil || gotTag.Handle != "tag:x,1:" || gotTag.Suffix != "tag!" || gotTag.OriginalHandle != "!e!" {
		t.Fatalf("F2 shorthand tag = %+v, want handle=tag:x,1: suffix=tag! orig=!e!", gotTag)
	}

	verbatimTag := "--- !<tag:x%2Fy> v\n"
	eng = New(verbatimTag, 0, opts)
	gotTag = nil
	for {
		ev, _, ok, err := eng.Next()
		if err != nil {
			t.Fatalf("F2 verbatim Next failed: %v", err)
		}
		if !ok {
			break
		}
		if ev.Kind == EventScalar {
			gotTag = ev.Tag
		}
	}
	if gotTag == nil || gotTag.Handle != "" || gotTag.Suffix != "tag:x/y" {
		t.Fatalf("F2 verbatim tag = %+v, want suffix=tag:x/y", gotTag)
	}

	badTagCases := []struct {
		src      string
		wantCode SyntaxKind
	}{
		{"--- !e%GG x\n", SyntaxInvalidTagEscape},
		{"--- !e%80 x\n", SyntaxInvalidTagUtf8LeadingByte},
		{"--- !e%C2%20 x\n", SyntaxInvalidTagUtf8TrailingByte},
		{"--- !e%C0%80 x\n", SyntaxInvalidTagUtf8},
	}
	for _, tc := range badTagCases {
		eng := New(tc.src, 0, opts)
		err := eng.SyntaxPass()
		if err == nil || err.Kind != ErrorKindSyntax || err.Syntax != tc.wantCode {
			t.Fatalf("F2 bad tag %q: got %+v, want %s", tc.src, err, tc.wantCode.AsStr())
		}
	}

	// F3: Trailing blanks before line fold in quoted scalars are discarded.
	foldCases := []struct {
		src  string
		want string
	}{
		{"k: \"folded \n  to\"\n", "folded to"},
		{"k: '  \n  '\n", " "},
		{"k: \"folded \n  \\tto\"\n", "folded \tto"},
	}
	for _, tc := range foldCases {
		eng := New(tc.src, 0, opts)
		if err := eng.SyntaxPass(); err != nil {
			t.Fatalf("F3 SyntaxPass %q failed: %v", tc.src, err)
		}
		var scalars []string
		for {
			ev, _, ok, err := eng.Next()
			if err != nil {
				t.Fatalf("F3 Next %q failed: %v", tc.src, err)
			}
			if !ok {
				break
			}
			if ev.Kind == EventScalar {
				scalars = append(scalars, ev.Value)
			}
		}
		if len(scalars) != 2 || scalars[1] != tc.want {
			t.Fatalf("F3 %q: got scalars %q, want second scalar %q", tc.src, scalars, tc.want)
		}
	}

	// F4: U+FEFF inside plain scalars / flow collections vs BomInsideDocument.
	for _, validBOMInDoc := range []string{"[a, b\uFEFF]\n", "a: b\uFEFF:d\n"} {
		eng := New(validBOMInDoc, 0, opts)
		if err := eng.SyntaxPass(); err != nil {
			t.Fatalf("F4 %q unexpected error: %v", validBOMInDoc, err)
		}
	}
	bomInside := "---\na: 1\n\uFEFFb: 2\n"
	eng = New(bomInside, 0, opts)
	err := eng.SyntaxPass()
	if err == nil || err.Kind != ErrorKindSyntax || err.Syntax != SyntaxBomInsideDocument {
		t.Fatalf("F4 mid-document BOM: got %+v, want SyntaxBomInsideDocument", err)
	}
}
