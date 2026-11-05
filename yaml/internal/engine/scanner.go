package engine

import (
	"math"
	"strconv"
	"strings"
	"unicode/utf8"
)

const (
	simpleKeyMaxLookahead      = 1024
	maxDirectiveBytes          = 1024
	maxReservedDirectiveParams = 16
)

type flowMarker struct {
	mark    Marker
	bracket rune
}

type flowScalarBuf struct {
	owned          bool
	buf            []byte
	start          int
	end            int
	hasPendingWS   bool
	pendingWSStart int
	pendingWSEnd   int
}

func newBorrowedFlowScalarBuf(offset int) flowScalarBuf {
	return flowScalarBuf{
		owned: false,
		start: offset,
		end:   offset,
	}
}

func (b *flowScalarBuf) notePendingWS(wsStart, wsEnd int) {
	if !b.owned {
		if !b.hasPendingWS {
			b.hasPendingWS = true
			b.pendingWSStart = wsStart
		}
		b.pendingWSEnd = wsEnd
	}
}

func (b *flowScalarBuf) commitPendingWS() {
	if !b.owned && b.hasPendingWS {
		b.end = b.pendingWSEnd
		b.hasPendingWS = false
	}
}

func (b *flowScalarBuf) discardPendingWS() {
	if !b.owned {
		b.hasPendingWS = false
		b.pendingWSStart = b.end
		b.pendingWSEnd = b.end
	}
}

type scanner struct {
	input                      strInput
	flowNestingLimit           int
	blockNestingLimit          int
	strictIndentation          bool
	mark                       Marker
	tokens                     []queuedToken
	err                        *scanError
	streamStartProduced        bool
	streamEndProduced          bool
	documentPrefixAllowed      bool
	adjacentValueAllowedAt     int
	simpleKeyAllowed           bool
	simpleKeys                 []simpleKey
	firstSimpleKey             int // -1 means None
	indent                     int
	blockLevel                 int
	indents                    []indentEntry
	flowLevel                  int
	tokensParsed               int
	tokenAvailable             bool
	leadingWhitespace          bool
	flowMappingStarted         []bool
	implicitFlowMappingStates  []implicitMappingState
	flowMarkers                []flowMarker
	interruptedPlainByComment  *Marker
	explicitKeyTabCheckPending bool
	bufWhitespaces             []byte
	bufLeadingBreak            []byte
	bufTrailingBreaks          []byte
}

func newScanner(src string, nativeDepth int) *scanner {
	return &scanner{
		input:                     newStrInput(src),
		flowNestingLimit:          nativeDepth,
		blockNestingLimit:         nativeDepth,
		strictIndentation:         true,
		mark:                      newMarker(0, true, 0, 1, 0),
		tokens:                    make([]queuedToken, 0, 64),
		documentPrefixAllowed:     true,
		adjacentValueAllowedAt:    0,
		simpleKeyAllowed:          true,
		simpleKeys:                make([]simpleKey, 0, 8),
		firstSimpleKey:            -1,
		indent:                    -1,
		indents:                   make([]indentEntry, 0, 8),
		leadingWhitespace:         true,
		flowMappingStarted:        make([]bool, 0, 4),
		implicitFlowMappingStates: make([]implicitMappingState, 0, 4),
		flowMarkers:               make([]flowMarker, 0, 4),
	}
}

func (s *scanner) scanErrorHere(kind scanErrKind) *scanError {
	return newScanError(s.mark, kind)
}

func (s *scanner) ensureCurrentCharIsPrintable() *scanError {
	ch := s.input.peek()
	if s.input.nextIsZ() || isPrintable(ch) {
		return nil
	}
	err := s.scanErrorHere(scanErrUnexpectedCharacter)
	err.ch = ch
	return err
}

func (s *scanner) tryBorrowSlice(start, end int) (string, bool) {
	return s.input.sliceBytes(start, end)
}

func (s *scanner) promoteFlowScalarBufToOwned(startMark Marker, buf *flowScalarBuf) *scanError {
	if buf.owned {
		return nil
	}
	prefix, ok := s.tryBorrowSlice(buf.start, buf.end)
	if !ok {
		return newScanError(startMark, scanErrInputOffsetsWithoutSlice)
	}
	if ch, bad := findNonPrintable(prefix); bad {
		err := newScanError(startMark, scanErrUnexpectedCharacter)
		err.ch = ch
		return err
	}
	owned := make([]byte, 0, len(prefix)+16)
	owned = append(owned, prefix...)
	*buf = flowScalarBuf{
		owned: true,
		buf:   owned,
	}
	return nil
}

func (s *scanner) nextQueuedToken() (*queuedToken, bool, *scanError) {
	if s.err != nil {
		return nil, false, nil
	}
	if s.streamEndProduced {
		return nil, false, nil
	}
	if !s.tokenAvailable {
		if err := s.fetchMoreTokens(); err != nil {
			s.err = err
			return nil, false, err
		}
	}
	if len(s.tokens) == 0 {
		return nil, false, nil
	}
	tok := s.tokens[0]
	s.tokens = s.tokens[1:]
	s.tokenAvailable = false
	s.tokensParsed++
	if tok.kind == tokenStreamEnd {
		s.streamEndProduced = true
	}
	return &tok, true, nil
}

func (s *scanner) fetchMoreTokens() *scanError {
	for {
		needMore := false
		if len(s.tokens) == 0 {
			needMore = true
		} else {
			if err := s.staleSimpleKeys(); err != nil {
				return err
			}
			if s.firstSimpleKey >= 0 && s.simpleKeys[s.firstSimpleKey].tokenNumber == s.tokensParsed {
				needMore = true
			}
		}
		if len(s.tokens) > 0 {
			lastKind := s.tokens[len(s.tokens)-1].kind
			if lastKind == tokenDocumentEnd || lastKind == tokenDocumentStart {
				break
			}
		}
		if !needMore {
			break
		}
		if err := s.fetchNextToken(); err != nil {
			return err
		}
	}
	s.tokenAvailable = true
	return nil
}

func (s *scanner) fetchNextToken() *scanError {
	return s.fetchNextTokenImpl()
}

func (s *scanner) fetchNextTokenImpl() *scanError {
	s.input.lookahead(1)

	if !s.streamStartProduced {
		s.fetchStreamStart()
		return nil
	}
	for {
		outcome, err := s.skipToNextToken()
		if err != nil {
			return err
		}
		if outcome.sawComment && len(s.tokens) > 0 {
			return nil
		}
		if !outcome.sawComment {
			break
		}
	}

	if err := s.staleSimpleKeys(); err != nil {
		return err
	}

	mark := s.mark
	s.unrollIndent(mark.col)

	s.input.lookahead(4)
	if s.input.nextIsZ() {
		return s.fetchStreamEnd()
	}

	if err := s.ensureCurrentCharIsPrintable(); err != nil {
		return err
	}

	if s.mark.col == 0 {
		if s.input.nextCharIs('%') {
			return s.fetchDirective()
		} else if s.input.nextIsDocumentStart() {
			return s.fetchDocumentIndicator(tokenDocumentStart)
		} else if s.input.nextIsDocumentEnd() {
			if err := s.fetchDocumentIndicator(tokenDocumentEnd); err != nil {
				return err
			}
			if _, err := s.skipWsToEol(skipTabsYes); err != nil {
				return err
			}
			if !s.input.nextIsBreakz() {
				return s.scanErrorHere(scanErrInvalidDocumentEnd)
			}
			return nil
		}
	}

	if s.documentPrefixAllowed {
		s.documentPrefixAllowed = false
	}

	if s.strictIndentation && s.flowLevel > 0 && s.mark.col <= s.flowBlockIndent() {
		return s.scanErrorHere(scanErrInvalidIndentation)
	}

	c := s.input.peek()
	nc := s.input.peekNth(1)
	switch {
	case c == '[':
		return s.fetchFlowCollectionStart(tokenFlowSequenceStart)
	case c == '{':
		return s.fetchFlowCollectionStart(tokenFlowMappingStart)
	case c == ']':
		return s.fetchFlowCollectionEnd(tokenFlowSequenceEnd, '[', ']')
	case c == '}':
		return s.fetchFlowCollectionEnd(tokenFlowMappingEnd, '{', '}')
	case c == ',':
		return s.fetchFlowEntry()
	case c == '-' && isBlankOrBreakz(nc):
		return s.fetchBlockEntry()
	case c == '?' && isBlankOrBreakz(nc):
		return s.fetchKey()
	case c == ':' && isBlankOrBreakz(nc):
		return s.fetchValue()
	case c == ':' && s.flowLevel > 0 && (isFlow(nc) || s.mark.charIndex == s.adjacentValueAllowedAt):
		return s.fetchFlowValue()
	case c == '*':
		return s.fetchAnchor(true)
	case c == '&':
		return s.fetchAnchor(false)
	case c == '!':
		return s.fetchTag()
	case c == '|' && s.flowLevel == 0:
		return s.fetchBlockScalar(true)
	case c == '>' && s.flowLevel == 0:
		return s.fetchBlockScalar(false)
	case c == '\'':
		return s.fetchFlowScalar(true)
	case c == '"':
		return s.fetchFlowScalar(false)
	case c == '-' && !isBlankOrBreakz(nc):
		return s.fetchPlainScalar()
	case (c == ':' || c == '?') && !isBlankOrBreakz(nc) && s.flowLevel == 0:
		return s.fetchPlainScalar()
	case isBOM(c):
		return s.scanErrorHere(scanErrBomInsideDocument)
	case c == '|' || c == '>' || c == '%' || c == '@' || c == '`':
		err := s.scanErrorHere(scanErrUnexpectedCharacter)
		err.ch = c
		return err
	default:
		return s.fetchPlainScalar()
	}
}

func (s *scanner) simpleKeyExpected(mark Marker) *scanError {
	return newScanError(mark, scanErrSimpleKeyExpected)
}

func (s *scanner) staleSimpleKeys() *scanError {
	for s.firstSimpleKey >= 0 {
		index := s.firstSimpleKey
		sk := &s.simpleKeys[index]
		isLineStale := s.flowLevel == 0 && sk.mark.line < s.mark.line
		isLengthStale := s.mark.charIndex-sk.mark.charIndex > simpleKeyMaxLookahead
		if !isLineStale && !isLengthStale {
			break
		}
		if sk.required {
			return s.simpleKeyExpected(sk.mark)
		}
		sk.possible = false
		nextFirst := -1
		for i := index + 1; i < len(s.simpleKeys); i++ {
			if s.simpleKeys[i].possible {
				nextFirst = i
				break
			}
		}
		s.firstSimpleKey = nextFirst
	}
	return nil
}

func (s *scanner) skipToNextToken() (scanToNextTokenStatus, *scanError) {
	consumeLinebreak := func() {
		s.input.lookahead(2)
		s.skipLinebreak()
		if s.flowLevel == 0 {
			s.allowSimpleKey()
		}
	}

	var outcome scanToNextTokenStatus
	for {
		ch := s.input.lookCh()
		if s.explicitKeyTabCheckPending {
			switch ch {
			case '\t':
				if err := s.skipExplicitKeyTab(); err != nil {
					return outcome, err
				}
				continue
			case ' ', '\n', '\r', '#':
			case '&', '!':
				if !s.strictIndentation {
					s.explicitKeyTabCheckPending = false
				}
			default:
				s.explicitKeyTabCheckPending = false
			}
		}

		switch {
		case ch == '\t':
			indent := s.indent
			if s.strictIndentation && s.flowLevel > 0 {
				indent = s.flowBlockIndent() + 1
			}
			if s.isWithinBlock() && s.leadingWhitespace && s.mark.col < indent {
				charsConsumed, _ := s.input.skipWsToEolBlanks(skipTabsYes)
				s.mark.col += charsConsumed
				s.mark.charIndex += charsConsumed
				s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()

				if s.input.lookCh() == '#' {
					continue
				}
				if !s.input.nextIsBreakz() {
					return outcome, s.scanErrorHere(scanErrTabInBlockIndentation)
				}
				if look := s.input.lookCh(); look == '\n' || look == '\r' {
					consumeLinebreak()
				}
			} else {
				s.skipBlank()
			}
		case ch == ' ':
			s.skipBlank()
		case ch == '\n' || ch == '\r':
			consumeLinebreak()
		case isBOM(ch) && s.documentPrefixAllowed && s.flowLevel == 0 && s.mark.col == 0:
			s.skipBom()
		case ch == '#':
			outcome.sawComment = true
			if err := s.skipComment(); err != nil {
				return outcome, err
			}
			if look := s.input.lookCh(); look == '\n' || look == '\r' {
				consumeLinebreak()
			}
			return outcome, nil
		default:
			goto afterLoop
		}
	}
afterLoop:
	if s.interruptedPlainByComment != nil {
		errMark := *s.interruptedPlainByComment
		s.interruptedPlainByComment = nil
		isImmediateNextLine := s.mark.line == errMark.line+1
		if s.flowLevel == 0 && isImmediateNextLine && s.mark.col > s.indent {
			s.input.lookahead(4)
			if !s.input.nextIsZ() && !s.input.nextIsDocumentIndicator() && s.input.nextCanBePlainScalar(false) {
				return outcome, newScanError(errMark, scanErrCommentInterceptedScalar)
			}
		}
	}
	return outcome, nil
}

func (s *scanner) skipExplicitKeyTab() *scanError {
	if s.leadingWhitespace && s.mark.col <= s.indent {
		tabMark := s.mark
		charsConsumed, _ := s.input.skipWsToEolBlanks(skipTabsYes)
		s.mark.col += charsConsumed
		s.mark.charIndex += charsConsumed
		s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
		if s.input.lookCh() != '#' && !s.input.nextIsBreakz() {
			return newScanError(tabMark, scanErrTabNotAllowed)
		}
	} else {
		s.skipBlank()
	}
	s.disallowSimpleKey()
	return nil
}

func (s *scanner) skipComment() *scanError {
	s.skipCommentChar()
	n := s.input.skipWhileNonBreakz()
	s.mark.charIndex += n
	s.mark.col += n
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
	return s.ensureCurrentCharIsPrintable()
}

func (s *scanner) skipWsToEol(skipTabsMode skipTabs) (whitespaceResult, *scanError) {
	charsConsumed, ws := s.input.skipWsToEolBlanks(skipTabsMode)
	s.mark.col += charsConsumed
	s.mark.charIndex += charsConsumed
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()

	if s.input.lookCh() != '#' {
		return ws, nil
	}
	if !ws.foundTabs() && !ws.hasValidYamlWs() {
		return ws, s.scanErrorHere(scanErrCommentNotSeparated)
	}
	if err := s.skipComment(); err != nil {
		return ws, err
	}
	return ws, nil
}

func (s *scanner) skipYamlWhitespace() (bool, *scanError) {
	needWhitespace := true
	for {
		switch s.input.lookCh() {
		case ' ':
			s.skipBlank()
			needWhitespace = false
		case '\t':
			if err := s.skipExplicitKeyTab(); err != nil {
				return false, err
			}
			needWhitespace = false
		case '\n', '\r':
			s.input.lookahead(2)
			s.skipLinebreak()
			if s.flowLevel == 0 {
				s.allowSimpleKey()
			}
			needWhitespace = false
		case '#':
			if needWhitespace {
				if err := s.skipComment(); err != nil {
					return false, err
				}
			} else {
				if err := s.skipComment(); err != nil {
					return false, err
				}
				return true, nil
			}
		default:
			goto done
		}
	}
done:
	if needWhitespace && !s.input.nextIsZ() {
		return false, s.scanErrorHere(scanErrExpectedWhitespace)
	}
	return false, nil
}

func (s *scanner) readBreak(dst *[]byte) {
	s.skipBreak()
	*dst = append(*dst, '\n')
}

func (s *scanner) insertToken(pos int, tok queuedToken) {
	oldLen := len(s.tokens)
	if pos > oldLen {
		panic("insertToken position out of bounds")
	}
	s.tokens = append(s.tokens, queuedToken{})
	copy(s.tokens[pos+1:], s.tokens[pos:oldLen])
	s.tokens[pos] = tok
}

func (s *scanner) simpleKeyTokenIndex(key *simpleKey, errorMark Marker) (int, *scanError) {
	if key.tokenNumber < s.tokensParsed {
		return 0, newScanError(errorMark, scanErrInvalidSimpleKey)
	}
	tokenIndex := key.tokenNumber - s.tokensParsed
	if tokenIndex > len(s.tokens) {
		return 0, newScanError(errorMark, scanErrInvalidSimpleKey)
	}
	return tokenIndex, nil
}

func (s *scanner) allowSimpleKey() {
	s.simpleKeyAllowed = true
}

func (s *scanner) disallowSimpleKey() {
	s.simpleKeyAllowed = false
}

func (s *scanner) skipBlank() {
	s.input.skip()
	s.mark.charIndex++
	s.mark.col++
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
}

func (s *scanner) skipNonBlank() {
	s.input.skip()
	s.mark.charIndex++
	s.mark.col++
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
	s.leadingWhitespace = false
}

func (s *scanner) skipBom() {
	s.input.skip()
	s.mark.charIndex++
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
}

func (s *scanner) skipCommentChar() {
	s.input.skip()
	s.mark.charIndex++
	s.mark.col++
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
}

func (s *scanner) skipNNonBlank(count int) {
	for i := 0; i < count; i++ {
		s.input.skip()
		s.mark.charIndex++
		s.mark.col++
	}
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
	s.leadingWhitespace = false
}

func (s *scanner) skipNl() {
	s.input.skip()
	s.mark.charIndex++
	s.mark.col = 0
	s.mark.line++
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
	s.leadingWhitespace = true
}

func (s *scanner) skipBreak() {
	c := s.input.peek()
	nc := s.input.peekNth(1)
	if c == '\r' && nc == '\n' {
		s.skipBlank()
	}
	s.skipNl()
}

func (s *scanner) skipLinebreak() {
	if s.input.next2Are('\r', '\n') {
		s.skipBlank()
		s.skipNl()
	} else if s.input.nextIsBreak() {
		s.skipNl()
	}
}

func (s *scanner) fetchStreamStart() {
	mark := s.mark
	s.indent = -1
	s.streamStartProduced = true
	s.allowSimpleKey()
	s.tokens = append(s.tokens, queuedToken{
		span: emptyRawSpan(mark),
		kind: tokenStreamStart,
	})
	s.simpleKeys = append(s.simpleKeys, newSimpleKey())
}

func (s *scanner) fetchStreamEnd() *scanError {
	if len(s.flowMarkers) > 0 {
		open := s.flowMarkers[len(s.flowMarkers)-1]
		s.flowMarkers = s.flowMarkers[:len(s.flowMarkers)-1]
		err := newScanError(open.mark, scanErrUnclosedFlowCollection)
		err.closing = open.bracket
		return err
	}
	for i := range s.simpleKeys {
		sk := &s.simpleKeys[i]
		if sk.required && sk.possible {
			return s.simpleKeyExpected(sk.mark)
		}
		sk.possible = false
	}
	s.firstSimpleKey = -1

	s.unrollIndent(-1)
	if err := s.removeSimpleKey(); err != nil {
		return err
	}
	s.disallowSimpleKey()

	s.tokens = append(s.tokens, queuedToken{
		span: emptyRawSpan(s.mark),
		kind: tokenStreamEnd,
	})
	return nil
}

func (s *scanner) fetchDirective() *scanError {
	s.unrollIndent(-1)
	if err := s.removeSimpleKey(); err != nil {
		return err
	}
	s.disallowSimpleKey()

	tokenIndex := len(s.tokens)
	tok, err := s.scanDirective()
	if err != nil {
		return err
	}
	s.insertToken(tokenIndex, tok)
	return nil
}

func (s *scanner) spendDirectiveBytes(budget *int, bytes int, startMark Marker) *scanError {
	if *budget < bytes {
		return newScanError(startMark, scanErrDirectiveByteLimitExceeded)
	}
	*budget -= bytes
	return nil
}

func (s *scanner) fetchDirectiveWord(out *strings.Builder, budget *int) (int, bool) {
	nChars := 0
	for {
		c := s.input.lookCh()
		if !isYamlNonSpace(c) || isZ(c) {
			break
		}
		cLen := utf8.RuneLen(c)
		if cLen > *budget {
			return 0, false
		}
		*budget -= cLen
		out.WriteRune(c)
		s.input.skip()
		nChars++
	}
	return nChars, true
}

func (s *scanner) scanDirective() (queuedToken, *scanError) {
	startMark := s.mark
	s.skipNonBlank() // eat '%'

	budget := maxDirectiveBytes
	name, err := s.scanDirectiveName(&budget)
	if err != nil {
		return queuedToken{}, err
	}
	separatedComment := false
	var tok queuedToken
	switch name {
	case "YAML":
		tok, err = s.scanVersionDirectiveValue(&startMark)
	case "TAG":
		tok, err = s.scanTagDirectiveValue(&startMark, &budget)
	default:
		var params []string
		for s.input.nextIsBlank() {
			nBlanks := s.input.skipWhileBlank()
			s.mark.charIndex += nBlanks
			s.mark.col += nBlanks
			s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()

			if s.input.peek() == '#' {
				separatedComment = true
				break
			}
			if !isBlankOrBreakz(s.input.peek()) {
				if err := s.spendDirectiveBytes(&budget, nBlanks, startMark); err != nil {
					return queuedToken{}, err
				}
				if len(params) >= maxReservedDirectiveParams {
					return queuedToken{}, newScanError(startMark, scanErrTooManyReservedDirectiveParams)
				}
				var param strings.Builder
				nChars, ok := s.fetchDirectiveWord(&param, &budget)
				if !ok {
					return queuedToken{}, newScanError(startMark, scanErrDirectiveByteLimitExceeded)
				}
				s.mark.charIndex += nChars
				s.mark.col += nChars
				s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
				params = append(params, param.String())
			}
		}
		tok = queuedToken{
			span:   newRawSpan(startMark, s.mark),
			kind:   tokenReservedDirective,
			text1:  name,
			params: params,
		}
	}
	if err != nil {
		return queuedToken{}, err
	}

	if separatedComment {
		if err := s.skipComment(); err != nil {
			return queuedToken{}, err
		}
	} else {
		if _, err := s.skipWsToEol(skipTabsYes); err != nil {
			return queuedToken{}, err
		}
	}

	if s.input.nextIsBreakz() {
		s.input.lookahead(2)
		s.skipLinebreak()
		return tok, nil
	}
	return queuedToken{}, newScanError(startMark, scanErrInvalidDirectiveTerminator)
}

func (s *scanner) scanVersionDirectiveValue(startMark *Marker) (queuedToken, *scanError) {
	nBlanks := s.input.skipWhileBlank()
	s.mark.charIndex += nBlanks
	s.mark.col += nBlanks
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()

	major, err := s.scanVersionDirectiveNumber(startMark)
	if err != nil {
		return queuedToken{}, err
	}
	if s.input.peek() != '.' {
		return queuedToken{}, newScanError(*startMark, scanErrMissingYamlVersionSeparator)
	}
	s.skipNonBlank()
	minor, err := s.scanVersionDirectiveNumber(startMark)
	if err != nil {
		return queuedToken{}, err
	}
	return queuedToken{
		span:  newRawSpan(*startMark, s.mark),
		kind:  tokenVersionDirective,
		major: major,
		minor: minor,
	}, nil
}

func (s *scanner) scanDirectiveName(budget *int) (string, *scanError) {
	startMark := s.mark
	var out strings.Builder
	nChars, ok := s.fetchDirectiveWord(&out, budget)
	if !ok {
		return "", newScanError(startMark, scanErrDirectiveByteLimitExceeded)
	}
	s.mark.charIndex += nChars
	s.mark.col += nChars
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()

	if out.Len() == 0 {
		return "", newScanError(startMark, scanErrMissingDirectiveName)
	}
	if !isBlankOrBreakz(s.input.peek()) {
		return "", newScanError(startMark, scanErrInvalidDirectiveName)
	}
	return out.String(), nil
}

func (s *scanner) scanVersionDirectiveNumber(startMark *Marker) (uint32, *scanError) {
	val := uint32(0)
	length := 0
	for {
		ch := s.input.lookCh()
		if !isDigit(ch) {
			break
		}
		if length+1 > 9 {
			return 0, newScanError(*startMark, scanErrYamlVersionTooLong)
		}
		length++
		val = val*10 + uint32(ch-'0')
		s.skipNonBlank()
	}
	if length == 0 {
		return 0, newScanError(*startMark, scanErrMissingYamlVersion)
	}
	return val, nil
}

func (s *scanner) scanTagDirectiveValue(startMark *Marker, budget *int) (queuedToken, *scanError) {
	nBlanks := s.input.skipWhileBlank()
	s.mark.charIndex += nBlanks
	s.mark.col += nBlanks
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
	if err := s.spendDirectiveBytes(budget, nBlanks, *startMark); err != nil {
		return queuedToken{}, err
	}

	handle, err := s.scanTagHandleDirective(startMark, budget)
	if err != nil {
		return queuedToken{}, err
	}

	nBlanks = s.input.skipWhileBlank()
	s.mark.charIndex += nBlanks
	s.mark.col += nBlanks
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
	if err := s.spendDirectiveBytes(budget, nBlanks, *startMark); err != nil {
		return queuedToken{}, err
	}

	prefix, err := s.scanTagPrefixDirective(startMark, budget)
	if err != nil {
		return queuedToken{}, err
	}

	s.input.lookahead(1)
	if s.input.nextIsBlankOrBreakz() {
		return queuedToken{
			span:  newRawSpan(*startMark, s.mark),
			kind:  tokenTagDirective,
			text1: handle,
			text2: prefix,
		}, nil
	}
	return queuedToken{}, newScanError(*startMark, scanErrInvalidTagDirectiveTerminator)
}

func (s *scanner) scanTagHandleDirective(mark *Marker, budget *int) (string, *scanError) {
	var out strings.Builder
	if s.input.lookCh() != '!' {
		return "", newScanError(*mark, scanErrExpectedTagBang)
	}
	if err := s.spendDirectiveBytes(budget, 1, *mark); err != nil {
		return "", err
	}
	out.WriteByte('!')
	s.skipNonBlank()

	s.input.lookahead(1)
	for s.input.nextIsAlpha() {
		if err := s.spendDirectiveBytes(budget, 1, *mark); err != nil {
			return "", err
		}
		out.WriteRune(s.input.peek())
		s.skipNonBlank()
		s.input.lookahead(1)
	}

	if s.input.peek() == '!' {
		if err := s.spendDirectiveBytes(budget, 1, *mark); err != nil {
			return "", err
		}
		out.WriteByte('!')
		s.skipNonBlank()
	} else if out.String() != "!" {
		return "", newScanError(*mark, scanErrExpectedTagDirectiveBang)
	}
	return out.String(), nil
}

func (s *scanner) scanTagPrefixDirective(startMark *Marker, budget *int) (string, *scanError) {
	var out strings.Builder
	if s.input.lookCh() == '!' {
		if err := s.spendDirectiveBytes(budget, 1, *startMark); err != nil {
			return "", err
		}
		out.WriteByte('!')
		s.skipNonBlank()
	} else if !isTagChar(s.input.peek()) {
		err := newScanError(*startMark, scanErrInvalidGlobalTagCharacter)
		err.ch = s.input.peek()
		return "", err
	} else if s.input.peek() == '%' {
		ch, err := s.scanURIEscapesWithBudget(startMark, budget)
		if err != nil {
			return "", err
		}
		out.WriteRune(ch)
	} else {
		c := s.input.peek()
		if err := s.spendDirectiveBytes(budget, utf8.RuneLen(c), *startMark); err != nil {
			return "", err
		}
		out.WriteRune(c)
		s.skipNonBlank()
	}

	for isURIChar(s.input.lookCh()) {
		if s.input.peek() == '%' {
			ch, err := s.scanURIEscapesWithBudget(startMark, budget)
			if err != nil {
				return "", err
			}
			out.WriteRune(ch)
		} else {
			c := s.input.peek()
			if err := s.spendDirectiveBytes(budget, utf8.RuneLen(c), *startMark); err != nil {
				return "", err
			}
			out.WriteRune(c)
			s.skipNonBlank()
		}
	}
	return out.String(), nil
}

func (s *scanner) fetchTag() *scanError {
	s.saveSimpleKey()
	s.disallowSimpleKey()
	tok, err := s.scanTag()
	if err != nil {
		return err
	}
	s.tokens = append(s.tokens, tok)
	return nil
}

func (s *scanner) scanTag() (queuedToken, *scanError) {
	startMark := s.mark
	s.input.lookahead(2)

	var handle, suffix string
	var err *scanError
	if s.input.nthCharIs(1, '<') {
		suffix, err = s.scanVerbatimTag(&startMark)
		if err != nil {
			return queuedToken{}, err
		}
	} else {
		handle, err = s.scanTagHandle(false, &startMark)
		if err != nil {
			return queuedToken{}, err
		}
		if len(handle) >= 2 && strings.HasPrefix(handle, "!") && strings.HasSuffix(handle, "!") {
			suffix, err = s.scanTagShorthandSuffix(&startMark, "")
			if err != nil {
				return queuedToken{}, err
			}
		} else {
			suffix, err = s.scanTagShorthandSuffix(&startMark, handle)
			if err != nil {
				return queuedToken{}, err
			}
			handle = "!"
			if suffix == "" {
				handle = ""
				suffix = "!"
			}
		}
	}

	if isBlankOrBreakz(s.input.lookCh()) || (s.flowLevel > 0 && isFlow(s.input.peek()) && s.input.peek() != '[' && s.input.peek() != '{') {
		return queuedToken{
			span:  newRawSpan(startMark, s.mark),
			kind:  tokenTag,
			text1: handle,
			text2: suffix,
		}, nil
	}
	return queuedToken{}, newScanError(startMark, scanErrInvalidTagTerminator)
}

func (s *scanner) scanTagHandle(directive bool, mark *Marker) (string, *scanError) {
	var out strings.Builder
	if s.input.lookCh() != '!' {
		return "", newScanError(*mark, scanErrExpectedTagBang)
	}
	out.WriteRune(s.input.peek())
	s.skipNonBlank()

	for s.input.nextIsAlpha() {
		out.WriteRune(s.input.peek())
		s.skipNonBlank()
		s.input.lookahead(1)
	}

	if s.input.peek() == '!' {
		out.WriteRune(s.input.peek())
		s.skipNonBlank()
	} else if directive && out.String() != "!" {
		return "", newScanError(*mark, scanErrExpectedTagDirectiveBang)
	}
	return out.String(), nil
}

func (s *scanner) scanVerbatimTag(startMark *Marker) (string, *scanError) {
	s.skipNonBlank()
	s.skipNonBlank()

	var out strings.Builder
	for isURIChar(s.input.lookCh()) {
		if s.input.peek() == '%' {
			ch, err := s.scanURIEscapesWithBudget(startMark, nil)
			if err != nil {
				return "", err
			}
			out.WriteRune(ch)
		} else {
			out.WriteRune(s.input.peek())
			s.skipNonBlank()
		}
	}
	if out.Len() == 0 {
		return "", newScanError(*startMark, scanErrMissingTagUri)
	}
	if s.input.peek() != '>' {
		return "", newScanError(*startMark, scanErrUnclosedVerbatimTag)
	}
	s.skipNonBlank()
	return out.String(), nil
}

func (s *scanner) scanTagShorthandSuffix(mark *Marker, head string) (string, *scanError) {
	length := len(head)
	var out strings.Builder
	if length > 1 {
		out.WriteString(head[1:])
	}
	for isTagChar(s.input.lookCh()) {
		if s.input.peek() == '%' {
			ch, err := s.scanURIEscapesWithBudget(mark, nil)
			if err != nil {
				return "", err
			}
			out.WriteRune(ch)
		} else {
			out.WriteRune(s.input.peek())
			s.skipNonBlank()
		}
		length++
	}
	if length == 0 {
		return "", newScanError(*mark, scanErrMissingTagUri)
	}
	return out.String(), nil
}

func (s *scanner) scanURIEscapesWithBudget(startMark *Marker, budget *int) (rune, *scanError) {
	width := 0
	var bytes [4]byte
	bytesLen := 0
	for {
		s.input.lookahead(3)
		c := s.input.peekNth(1)
		nc := s.input.peekNth(2)
		if !(s.input.peek() == '%' && isHex(c) && isHex(nc)) {
			return 0, newScanError(*startMark, scanErrInvalidTagEscape)
		}
		if budget != nil {
			if err := s.spendDirectiveBytes(budget, 3, *startMark); err != nil {
				return 0, err
			}
		}
		b := byte((asHex(c) << 4) + asHex(nc))
		if width == 0 {
			switch {
			case b&0x80 == 0x00:
				width = 1
			case b&0xE0 == 0xC0:
				width = 2
			case b&0xF0 == 0xE0:
				width = 3
			case b&0xF8 == 0xF0:
				width = 4
			default:
				return 0, newScanError(*startMark, scanErrInvalidTagUtf8LeadingByte)
			}
		} else if b&0xC0 != 0x80 {
			return 0, newScanError(*startMark, scanErrInvalidTagUtf8TrailingByte)
		}
		bytes[bytesLen] = b
		bytesLen++
		s.skipNNonBlank(3)
		width--
		if width == 0 {
			break
		}
	}
	if !utf8.Valid(bytes[:bytesLen]) {
		return 0, newScanError(*startMark, scanErrInvalidTagUtf8)
	}
	ch, _ := utf8.DecodeRune(bytes[:bytesLen])
	return ch, nil
}

func (s *scanner) fetchAnchor(alias bool) *scanError {
	s.saveSimpleKey()
	s.disallowSimpleKey()
	tok, err := s.scanAnchor(alias)
	if err != nil {
		return err
	}
	s.tokens = append(s.tokens, tok)
	return nil
}

func (s *scanner) scanAnchor(alias bool) (queuedToken, *scanError) {
	startMark := s.mark
	s.skipNonBlank()

	if start, ok := s.input.byteOffset(); ok {
		for isAnchorChar(s.input.lookCh()) {
			s.skipNonBlank()
		}
		end, _ := s.input.byteOffset()
		if start == end {
			return queuedToken{}, newScanError(startMark, scanErrMissingAnchorOrAliasName)
		}
		slice, ok := s.tryBorrowSlice(start, end)
		if !ok {
			return queuedToken{}, newScanError(startMark, scanErrInputOffsetsWithoutSlice)
		}
		kind := tokenAnchor
		if alias {
			kind = tokenAlias
		}
		return queuedToken{
			span:  newRawSpan(startMark, s.mark),
			kind:  kind,
			text1: slice,
		}, nil
	}

	var name strings.Builder
	for isAnchorChar(s.input.lookCh()) {
		name.WriteRune(s.input.peek())
		s.skipNonBlank()
	}
	if name.Len() == 0 {
		return queuedToken{}, newScanError(startMark, scanErrMissingAnchorOrAliasName)
	}
	kind := tokenAnchor
	if alias {
		kind = tokenAlias
	}
	return queuedToken{
		span:  newRawSpan(startMark, s.mark),
		kind:  kind,
		text1: name.String(),
	}, nil
}

func (s *scanner) fetchFlowCollectionStart(tok tokenKind) *scanError {
	s.saveSimpleKey()

	startMark := s.mark
	indicator := s.input.peek()
	s.flowMarkers = append(s.flowMarkers, flowMarker{mark: startMark, bracket: indicator})

	s.rollOneColIndent()
	if err := s.increaseFlowLevel(); err != nil {
		return err
	}
	s.allowSimpleKey()

	s.skipNonBlank()
	endMark := s.mark

	if tok == tokenFlowMappingStart {
		s.flowMappingStarted = append(s.flowMappingStarted, true)
	} else {
		s.flowMappingStarted = append(s.flowMappingStarted, false)
		s.implicitFlowMappingStates = append(s.implicitFlowMappingStates, implicitMappingState{})
	}

	tokenIndex := len(s.tokens)
	if _, err := s.skipWsToEol(skipTabsYes); err != nil {
		return err
	}

	s.insertToken(tokenIndex, queuedToken{
		span: newRawSpan(startMark, endMark),
		kind: tok,
	})
	return nil
}

func (s *scanner) fetchFlowCollectionEnd(tok tokenKind, expectedOpen, actualClose rune) *scanError {
	if s.flowLevel == 0 || len(s.flowMarkers) == 0 {
		err := s.scanErrorHere(scanErrMisplacedFlowCollectionEnd)
		err.closing = actualClose
		return err
	}
	open := s.flowMarkers[len(s.flowMarkers)-1]
	s.flowMarkers = s.flowMarkers[:len(s.flowMarkers)-1]
	if open.bracket != expectedOpen {
		err := newScanError(open.mark, scanErrMismatchedFlowCollectionEnd)
		err.closing = actualClose
		err.expected = open.bracket
		err.openingLine = open.mark.line
		err.openingCol = open.mark.col + 1
		return err
	}

	flowLevel := s.flowLevel
	if err := s.removeSimpleKey(); err != nil {
		return err
	}
	if tok == tokenFlowSequenceEnd {
		s.endImplicitMapping(s.mark, flowLevel)
		if len(s.implicitFlowMappingStates) > 0 {
			s.implicitFlowMappingStates = s.implicitFlowMappingStates[:len(s.implicitFlowMappingStates)-1]
		}
	}
	if len(s.flowMappingStarted) > 0 {
		s.flowMappingStarted = s.flowMappingStarted[:len(s.flowMappingStarted)-1]
	}

	s.decreaseFlowLevel()
	s.disallowSimpleKey()

	startMark := s.mark
	s.skipNonBlank()
	endMark := s.mark
	tokenIndex := len(s.tokens)
	if _, err := s.skipWsToEol(skipTabsYes); err != nil {
		return err
	}
	if s.flowLevel > 0 {
		s.adjacentValueAllowedAt = s.mark.charIndex
	}

	s.insertToken(tokenIndex, queuedToken{
		span: newRawSpan(startMark, endMark),
		kind: tok,
	})
	return nil
}

func (s *scanner) fetchFlowEntry() *scanError {
	if err := s.removeSimpleKey(); err != nil {
		return err
	}
	s.allowSimpleKey()

	s.endImplicitMapping(s.mark, s.flowLevel)
	if s.currentFlowCollectionIsSequence() {
		s.setCurrentFlowMappingStarted(false)
	}

	startMark := s.mark
	s.skipNonBlank()
	endMark := s.mark
	tokenIndex := len(s.tokens)
	if _, err := s.skipWsToEol(skipTabsYes); err != nil {
		return err
	}

	s.insertToken(tokenIndex, queuedToken{
		span: newRawSpan(startMark, endMark),
		kind: tokenFlowEntry,
	})
	return nil
}

func (s *scanner) increaseFlowLevel() *scanError {
	if s.flowLevel >= s.flowNestingLimit {
		return s.scanErrorHere(scanErrRecursionLimitExceeded)
	}
	s.simpleKeys = append(s.simpleKeys, newSimpleKey())
	s.flowLevel++
	return nil
}

func (s *scanner) decreaseFlowLevel() {
	if s.flowLevel > 0 {
		s.flowLevel--
		s.simpleKeys = s.simpleKeys[:len(s.simpleKeys)-1]
	}
}

func (s *scanner) fetchBlockEntry() *scanError {
	if s.flowLevel > 0 {
		return s.scanErrorHere(scanErrBlockEntryInFlowCollection)
	}
	if !s.simpleKeyAllowed {
		return s.scanErrorHere(scanErrBlockSequenceEntryNotAllowed)
	}

	mark := s.mark
	s.skipNonBlank()

	if err := s.rollIndent(mark.col, nil, tokenBlockSequenceStart, mark); err != nil {
		return err
	}
	tokenIndex := len(s.tokens)
	ws, err := s.skipWsToEol(skipTabsYes)
	if err != nil {
		return err
	}
	foundTabs := ws.foundTabs()
	s.input.lookahead(2)
	if foundTabs && s.input.nextCharIs('-') && isBlankOrBreakz(s.input.peekNth(1)) {
		return s.scanErrorHere(scanErrInvalidBlockEntryWhitespace)
	}

	if _, err := s.skipWsToEol(skipTabsNo); err != nil {
		return err
	}
	s.input.lookahead(1)
	if s.input.nextIsBreak() || s.input.nextIsFlow() {
		s.rollOneColIndent()
	}

	if err := s.removeSimpleKey(); err != nil {
		return err
	}
	s.allowSimpleKey()

	s.insertToken(tokenIndex, queuedToken{
		span: emptyRawSpan(s.mark),
		kind: tokenBlockEntry,
	})
	return nil
}

func (s *scanner) fetchDocumentIndicator(t tokenKind) *scanError {
	if len(s.flowMarkers) > 0 {
		open := s.flowMarkers[len(s.flowMarkers)-1]
		s.flowMarkers = s.flowMarkers[:len(s.flowMarkers)-1]
		err := newScanError(open.mark, scanErrUnclosedFlowCollection)
		err.closing = open.bracket
		return err
	}

	s.unrollIndent(-1)
	if err := s.removeSimpleKey(); err != nil {
		return err
	}
	s.disallowSimpleKey()

	mark := s.mark
	s.skipNNonBlank(3)

	s.documentPrefixAllowed = t == tokenDocumentEnd
	s.tokens = append(s.tokens, queuedToken{
		span: newRawSpan(mark, s.mark),
		kind: t,
	})
	return nil
}

func (s *scanner) fetchBlockScalar(literal bool) *scanError {
	s.saveSimpleKey()
	s.allowSimpleKey()
	tokenIndex := len(s.tokens)
	tok, err := s.scanBlockScalar(literal)
	if err != nil {
		return err
	}
	s.insertToken(tokenIndex, tok)
	return nil
}

func (s *scanner) scanBlockScalar(literal bool) (queuedToken, *scanError) {
	startMark := s.mark
	chomp := chompingClip
	increment := 0
	indent := 0
	leadingBlank := false
	trailingBlank := false
	style := ScalarStyleFolded
	if literal {
		style = ScalarStyleLiteral
	}

	var out []byte
	var leadingBreak []byte
	var trailingBreaks []byte
	var chompingBreak []byte

	// skip '|' or '>'
	s.skipNonBlank()
	s.unrollNonBlockIndents()

	s.input.lookahead(1)
	if s.input.lookCh() == '+' || s.input.peek() == '-' {
		if s.input.peek() == '+' {
			chomp = chompingKeep
		} else {
			chomp = chompingStrip
		}
		s.skipNonBlank()
		s.input.lookahead(1)
		if s.input.nextIsDigit() {
			if s.input.peek() == '0' {
				return queuedToken{}, newScanError(startMark, scanErrZeroBlockScalarIndent)
			}
			increment = int(s.input.peek() - '0')
			s.skipNonBlank()
		}
	} else if s.input.nextIsDigit() {
		if s.input.peek() == '0' {
			return queuedToken{}, newScanError(startMark, scanErrZeroBlockScalarIndent)
		}
		increment = int(s.input.peek() - '0')
		s.skipNonBlank()
		s.input.lookahead(1)
		if s.input.peek() == '+' || s.input.peek() == '-' {
			if s.input.peek() == '+' {
				chomp = chompingKeep
			} else {
				chomp = chompingStrip
			}
			s.skipNonBlank()
		}
	}

	if _, err := s.skipWsToEol(skipTabsYes); err != nil {
		return queuedToken{}, err
	}
	s.input.lookahead(1)
	if err := s.ensureCurrentCharIsPrintable(); err != nil {
		return queuedToken{}, err
	}
	if !s.input.nextIsBreakz() {
		return queuedToken{}, newScanError(startMark, scanErrInvalidBlockScalarHeader)
	}
	if s.input.nextIsBreak() {
		s.input.lookahead(2)
		s.readBreak(&chompingBreak)
	}
	if err := s.ensureCurrentCharIsPrintable(); err != nil {
		return queuedToken{}, err
	}
	if s.input.lookCh() == '\t' {
		return queuedToken{}, newScanError(startMark, scanErrTabAtBlockScalarStart)
	}

	if increment > 0 {
		if s.indent >= 0 {
			indent = s.indent + increment
		} else {
			indent = increment
		}
	}

	if indent == 0 {
		s.skipBlockScalarFirstLineIndent(&indent, &trailingBreaks)
	} else {
		s.skipBlockScalarIndent(indent, &trailingBreaks)
	}
	if err := s.ensureCurrentCharIsPrintable(); err != nil {
		return queuedToken{}, err
	}

	if s.input.nextIsZ() {
		var contents string
		switch {
		case chomp == chompingStrip:
			contents = ""
		case s.mark.line == startMark.line:
			contents = ""
		case chomp == chompingClip:
			contents = ""
		case chomp == chompingKeep && len(trailingBreaks) == 0 && s.mark.col > 0:
			contents = string(chompingBreak)
		case chomp == chompingKeep:
			contents = string(trailingBreaks)
		}
		return queuedToken{
			span:  newRawSpan(startMark, s.mark),
			kind:  tokenScalar,
			style: style,
			text1: contents,
		}, nil
	}

	if s.mark.col < indent && s.mark.col > s.indent {
		if s.indent < 0 && s.mark.col == 0 {
			s.input.lookahead(4)
			if s.input.nextIsDocumentStart() || s.input.nextIsDocumentEnd() || s.input.peek() == '#' {
				// Terminated rather than under-indented at root level.
			} else {
				return queuedToken{}, s.scanErrorHere(scanErrInvalidBlockScalarIndent)
			}
		} else {
			return queuedToken{}, s.scanErrorHere(scanErrInvalidBlockScalarIndent)
		}
	}

	startMark = s.mark
	for s.mark.col == indent && !s.input.nextIsZ() {
		if err := s.ensureCurrentCharIsPrintable(); err != nil {
			return queuedToken{}, err
		}
		if indent == 0 {
			s.input.lookahead(4)
			if s.input.nextIsDocumentIndicator() {
				break
			}
		}
		trailingBlank = s.input.nextIsBlank()
		if !literal && len(leadingBreak) > 0 && !leadingBlank && !trailingBlank {
			out = append(out, trailingBreaks...)
			if len(trailingBreaks) == 0 {
				out = append(out, ' ')
			}
		} else {
			out = append(out, leadingBreak...)
			out = append(out, trailingBreaks...)
		}
		leadingBreak = leadingBreak[:0]
		trailingBreaks = trailingBreaks[:0]

		leadingBlank = s.input.nextIsBlank()
		s.scanBlockScalarContentLine(&out)

		s.input.lookahead(2)
		if s.input.nextIsZ() {
			break
		}
		if err := s.ensureCurrentCharIsPrintable(); err != nil {
			return queuedToken{}, err
		}
		s.readBreak(&leadingBreak)
		s.skipBlockScalarIndent(indent, &trailingBreaks)
	}

	if err := s.ensureCurrentCharIsPrintable(); err != nil {
		return queuedToken{}, err
	}

	if chomp != chompingStrip {
		minIndent := indent
		if minIndent < 1 {
			minIndent = 1
		}
		if len(leadingBreak) == 0 && s.input.nextIsZ() && s.mark.col >= minIndent {
			out = append(out, '\n')
		}
		out = append(out, leadingBreak...)
	}
	if chomp == chompingKeep {
		out = append(out, trailingBreaks...)
	}

	str := string(out)
	if ch, bad := findNonPrintable(str); bad {
		err := newScanError(startMark, scanErrUnexpectedCharacter)
		err.ch = ch
		return queuedToken{}, err
	}

	span := newRawSpan(startMark, s.mark)
	if strings.TrimSpace(str) != "" {
		span = span.withIndent(indent, true)
	}
	return queuedToken{
		span:  span,
		kind:  tokenScalar,
		style: style,
		text1: str,
	}, nil
}

func (s *scanner) scanBlockScalarContentLine(out *[]byte) {
	charsConsumed := s.input.fetchBlockScalarLine(out)
	s.mark.col += charsConsumed
	s.mark.charIndex += charsConsumed
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
}

func (s *scanner) skipBlockScalarIndent(indent int, breaks *[]byte) {
	for {
		maxLen := s.input.bufmaxlen()
		if indent < maxLen-2 {
			s.input.lookahead(maxLen)
			for s.mark.col < indent && s.input.peek() == ' ' {
				s.skipBlank()
			}
		} else {
			for {
				s.input.lookahead(maxLen)
				for !s.input.bufIsEmpty() && s.mark.col < indent && s.input.peek() == ' ' {
					s.skipBlank()
				}
				if s.mark.col == indent || (!s.input.bufIsEmpty() && s.input.peek() != ' ') {
					break
				}
			}
			s.input.lookahead(2)
		}
		if s.input.nextIsBreak() {
			s.readBreak(breaks)
		} else {
			break
		}
	}
}

func (s *scanner) skipBlockScalarFirstLineIndent(indent *int, breaks *[]byte) {
	maxIndent := 0
	for {
		for s.input.lookCh() == ' ' {
			s.skipBlank()
		}
		if s.mark.col > maxIndent {
			maxIndent = s.mark.col
		}
		if s.input.nextIsBreak() {
			s.input.lookahead(2)
			s.readBreak(breaks)
		} else {
			break
		}
	}
	minIndent := s.indent + 1
	if maxIndent < minIndent {
		maxIndent = minIndent
	}
	*indent = maxIndent
}

func (s *scanner) fetchFlowScalar(single bool) *scanError {
	s.saveSimpleKey()
	s.disallowSimpleKey()

	tokenIndex := len(s.tokens)
	tok, err := s.scanFlowScalar(single)
	if err != nil {
		return err
	}
	status, err := s.skipToNextToken()
	if err != nil {
		return err
	}
	if status.sawComment {
		s.adjacentValueAllowedAt = math.MaxInt
	} else {
		s.adjacentValueAllowedAt = s.mark.charIndex
	}
	s.insertToken(tokenIndex, tok)
	return nil
}

func (s *scanner) scanFlowScalar(single bool) (queuedToken, *scanError) {
	startMark := s.mark
	off, _ := s.input.byteOffset()
	buf := newBorrowedFlowScalarBuf(off + utf8.RuneLen(s.input.peek()))
	var breakScratch []byte

	// eat left quote
	s.skipNonBlank()

	for {
		s.input.lookahead(4)
		if s.mark.col == 0 && s.input.nextIsDocumentIndicator() {
			return queuedToken{}, newScanError(startMark, scanErrDocumentIndicatorInQuotedScalar)
		}
		if s.input.nextIsZ() {
			return queuedToken{}, newScanError(startMark, scanErrUnclosedQuotedScalar)
		}
		if err := s.ensureCurrentCharIsPrintable(); err != nil {
			return queuedToken{}, err
		}

		leadingBlanks := false
		if err := s.consumeFlowScalarNonWhitespaceChars(single, &buf, &leadingBlanks, &startMark); err != nil {
			return queuedToken{}, err
		}

		look := s.input.lookCh()
		if (look == '\'' && single && !leadingBlanks) || (look == '"' && !single && !leadingBlanks) {
			break
		}

		trailingWSStart := -1
		hasLeadingBreak := false
		hasTrailingBreaks := false
		pendingWSStart := -1

		for s.input.nextIsBlank() || s.input.nextIsBreak() {
			if s.input.nextIsBlank() {
				if leadingBlanks {
					indent := s.indent
					if s.strictIndentation && s.flowLevel > 0 {
						indent = s.flowBlockIndent() + 1
					}
					if s.input.peek() == '\t' && s.mark.col < indent {
						return queuedToken{}, s.scanErrorHere(scanErrTabInIndentation)
					}
					s.skipBlank()
				} else {
					if buf.owned {
						if trailingWSStart < 0 {
							trailingWSStart = len(buf.buf)
						}
						buf.buf = utf8.AppendRune(buf.buf, s.input.peek())
					} else if pendingWSStart < 0 {
						if bo, ok := s.input.byteOffset(); ok {
							pendingWSStart = bo
						}
					}
					s.skipBlank()
					if !buf.owned && pendingWSStart >= 0 {
						if wsEnd, ok := s.input.byteOffset(); ok {
							buf.notePendingWS(pendingWSStart, wsEnd)
						}
					}
				}
			} else {
				s.input.lookahead(2)
				if leadingBlanks {
					if !buf.owned {
						if err := s.promoteFlowScalarBufToOwned(startMark, &buf); err != nil {
							return queuedToken{}, err
						}
					}
					s.readBreak(&buf.buf)
					hasTrailingBreaks = true
				} else {
					if trailingWSStart >= 0 && buf.owned {
						buf.buf = buf.buf[:trailingWSStart]
					}
					trailingWSStart = -1
					if pendingWSStart >= 0 {
						pendingWSStart = -1
						if !buf.owned {
							if err := s.promoteFlowScalarBufToOwned(startMark, &buf); err != nil {
								return queuedToken{}, err
							}
						}
						buf.discardPendingWS()
					} else {
						buf.commitPendingWS()
					}
					breakScratch = breakScratch[:0]
					s.readBreak(&breakScratch)
					hasLeadingBreak = true
					leadingBlanks = true
				}
			}
			s.input.lookahead(1)
		}

		if leadingBlanks && s.strictIndentation && s.flowLevel > 0 && s.mark.col <= s.flowBlockIndent() {
			return queuedToken{}, s.scanErrorHere(scanErrInvalidIndentation)
		}
		if leadingBlanks && hasLeadingBreak && s.flowLevel == 0 {
			nextCh := s.input.peek()
			isClosingQuote := (single && nextCh == '\'') || (!single && nextCh == '"')
			if (!isClosingQuote || s.strictIndentation) && s.mark.col <= s.flowBlockIndent() {
				return queuedToken{}, s.scanErrorHere(scanErrInvalidQuotedScalarIndent)
			}
		}
		if leadingBlanks && !hasLeadingBreak && s.strictIndentation && s.flowLevel == 0 && s.mark.col <= s.flowBlockIndent() {
			return queuedToken{}, s.scanErrorHere(scanErrInvalidIndentation)
		}
		if leadingBlanks && hasLeadingBreak && !hasTrailingBreaks {
			if !buf.owned {
				if err := s.promoteFlowScalarBufToOwned(startMark, &buf); err != nil {
					return queuedToken{}, err
				}
			}
			buf.buf = append(buf.buf, ' ')
		}
	}

	// eat right quote
	s.skipNonBlank()
	endMark := s.mark

	if _, err := s.skipWsToEol(skipTabsYes); err != nil {
		return queuedToken{}, err
	}
	if err := s.ensureCurrentCharIsPrintable(); err != nil {
		return queuedToken{}, err
	}
	c := s.input.peek()
	switch {
	case (c == ',' || c == '}' || c == ']') && s.flowLevel > 0:
	case isBreakz(c):
	case c == ':' && s.flowLevel == 0 && startMark.line == s.mark.line:
	case c == ':' && s.flowLevel > 0:
	default:
		kind := scanErrInvalidTrailingDoubleQuotedScalar
		if single {
			kind = scanErrInvalidTrailingSingleQuotedScalar
		}
		return queuedToken{}, s.scanErrorHere(kind)
	}

	style := ScalarStyleDoubleQuoted
	if single {
		style = ScalarStyleSingleQuoted
	}
	var contents string
	if buf.owned {
		contents = string(buf.buf)
	} else {
		end := buf.end
		if buf.hasPendingWS {
			end = buf.pendingWSEnd
		}
		slice, ok := s.tryBorrowSlice(buf.start, end)
		if !ok {
			return queuedToken{}, newScanError(startMark, scanErrInputOffsetsWithoutSlice)
		}
		contents = slice
	}

	return queuedToken{
		span:  newRawSpan(startMark, endMark),
		kind:  tokenScalar,
		style: style,
		text1: contents,
	}, nil
}

func (s *scanner) consumeFlowScalarAsciiChunk(single bool, buf *flowScalarBuf) bool {
	chunk := s.input.takeQuotedScalarAsciiChunk(single)
	consumed := len(chunk)
	if consumed == 0 {
		return false
	}
	if buf.owned {
		buf.buf = append(buf.buf, chunk...)
	} else {
		buf.commitPendingWS()
	}
	s.mark.charIndex += consumed
	s.mark.col += consumed
	s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
	s.leadingWhitespace = false
	if !buf.owned && s.mark.hasByteOffset {
		buf.end = s.mark.byteOffset
	}
	return true
}

func (s *scanner) consumeFlowScalarNonWhitespaceChars(
	single bool,
	buf *flowScalarBuf,
	leadingBlanks *bool,
	startMark *Marker,
) *scanError {
	s.input.lookahead(2)
	for {
		ch := s.input.peek()
		if isBlankOrBreakz(ch) || !isPrintable(ch) {
			break
		}
		switch {
		case ch == '\'' && s.input.peekNth(1) == '\'' && single:
			if !buf.owned {
				buf.commitPendingWS()
				if err := s.promoteFlowScalarBufToOwned(*startMark, buf); err != nil {
					return err
				}
			}
			buf.buf = append(buf.buf, '\'')
			s.skipNNonBlank(2)
		case ch == '\'' && single:
			return nil
		case ch == '"' && !single:
			return nil
		case ch == '\\' && !single && isBreak(s.input.peekNth(1)):
			s.input.lookahead(3)
			if !buf.owned {
				buf.commitPendingWS()
				if err := s.promoteFlowScalarBufToOwned(*startMark, buf); err != nil {
					return err
				}
			}
			s.skipNonBlank()
			s.skipLinebreak()
			*leadingBlanks = true
			return nil
		case ch == '\\' && !single:
			if !buf.owned {
				buf.commitPendingWS()
				if err := s.promoteFlowScalarBufToOwned(*startMark, buf); err != nil {
					return err
				}
			}
			esc, err := s.resolveFlowScalarEscapeSequence(startMark)
			if err != nil {
				return err
			}
			buf.buf = utf8.AppendRune(buf.buf, esc)
		default:
			if ch < 0x80 && s.consumeFlowScalarAsciiChunk(single, buf) {
				s.input.lookahead(2)
				continue
			}
			if buf.owned {
				buf.buf = utf8.AppendRune(buf.buf, ch)
			} else {
				buf.commitPendingWS()
			}
			s.skipNonBlank()
			if !buf.owned {
				if newEnd, ok := s.input.byteOffset(); ok {
					buf.end = newEnd
				}
			}
		}
		s.input.lookahead(2)
	}
	return nil
}

func (s *scanner) resolveFlowScalarEscapeSequence(startMark *Marker) (rune, *scanError) {
	codeLen := 0
	ret := rune(0)
	switch s.input.peekNth(1) {
	case '0':
		ret = '\x00'
	case 'a':
		ret = '\x07'
	case 'b':
		ret = '\x08'
	case 't', '\t':
		ret = '\t'
	case 'n':
		ret = '\n'
	case 'v':
		ret = '\x0b'
	case 'f':
		ret = '\x0c'
	case 'r':
		ret = '\x0d'
	case 'e':
		ret = '\x1b'
	case ' ':
		ret = '\x20'
	case '"':
		ret = '"'
	case '/':
		ret = '/'
	case '\\':
		ret = '\\'
	case 'N':
		ret = 0x85
	case '_':
		ret = 0xA0
	case 'L':
		ret = 0x2028
	case 'P':
		ret = 0x2029
	case 'x':
		codeLen = 2
	case 'u':
		codeLen = 4
	case 'U':
		codeLen = 8
	default:
		return 0, newScanError(*startMark, scanErrUnknownQuotedScalarEscape)
	}
	s.skipNNonBlank(2)

	if codeLen > 0 {
		s.input.lookahead(codeLen)
		val := uint32(0)
		for i := 0; i < codeLen; i++ {
			c := s.input.peekNth(i)
			if !isHex(c) {
				return 0, newScanError(*startMark, scanErrInvalidQuotedScalarHexEscape)
			}
			val = (val << 4) + asHex(c)
		}
		s.skipNNonBlank(codeLen)

		if codeLen == 4 && val >= 0xD800 && val <= 0xDBFF {
			s.input.lookahead(2)
			if s.input.peek() == '\\' && s.input.peekNth(1) == 'u' {
				s.skipNNonBlank(2)
				s.input.lookahead(4)
				lowVal := uint32(0)
				for i := 0; i < 4; i++ {
					c := s.input.peekNth(i)
					if !isHex(c) {
						return 0, newScanError(*startMark, scanErrInvalidLowSurrogateHexEscape)
					}
					lowVal = (lowVal << 4) + asHex(c)
				}
				if lowVal >= 0xDC00 && lowVal <= 0xDFFF {
					val = 0x10000 + (((val - 0xD800) << 10) | (lowVal - 0xDC00))
					s.skipNNonBlank(4)
				} else {
					return 0, newScanError(*startMark, scanErrInvalidLowSurrogate)
				}
			} else {
				return 0, newScanError(*startMark, scanErrMissingLowSurrogate)
			}
		} else if codeLen == 4 && val >= 0xDC00 && val <= 0xDFFF {
			return 0, newScanError(*startMark, scanErrUnpairedLowSurrogate)
		}

		if val > 0x10FFFF || !utf8.ValidRune(rune(val)) {
			return 0, newScanError(*startMark, scanErrInvalidUnicodeEscape)
		}
		ret = rune(val)
	}
	return ret, nil
}

func (s *scanner) fetchPlainScalar() *scanError {
	s.saveSimpleKey()
	s.disallowSimpleKey()
	tokenIndex := len(s.tokens)
	tok, err := s.scanPlainScalar()
	if err != nil {
		return err
	}
	s.insertToken(tokenIndex, tok)
	return nil
}

func (s *scanner) scanPlainScalar() (queuedToken, *scanError) {
	s.unrollNonBlockIndents()
	indent := s.indent + 1
	startMark := s.mark

	borrowStart := startMark.byteOffset
	canBorrow := startMark.hasByteOffset
	if canBorrow {
		_, canBorrow = s.tryBorrowSlice(borrowStart, borrowStart)
	}
	var out []byte
	isOwned := !canBorrow
	hasContent := false
	s.bufWhitespaces = s.bufWhitespaces[:0]
	s.bufLeadingBreak = s.bufLeadingBreak[:0]
	s.bufTrailingBreaks = s.bufTrailingBreaks[:0]
	endMark := s.mark

	for {
		s.input.lookahead(4)
		if (s.mark.col == 0 && s.input.nextIsDocumentIndicator()) || s.input.peek() == '#' {
			if s.input.peek() == '#' && hasContent && len(s.bufWhitespaces) > 0 && s.flowLevel == 0 {
				cp := s.mark
				s.interruptedPlainByComment = &cp
			}
			break
		}
		if !hasContent && s.flowLevel > 0 && s.input.peek() == '-' && isFlow(s.input.peekNth(1)) {
			return queuedToken{}, s.scanErrorHere(scanErrPlainScalarStartsWithDashFlowIndicator)
		}
		if !s.input.nextIsBlankOrBreakz() && s.input.nextCanBePlainScalar(s.flowLevel > 0) {
			if s.leadingWhitespace {
				if hasContent && !isOwned {
					if !endMark.hasByteOffset {
						return queuedToken{}, newScanError(startMark, scanErrInputOffsetsWithoutSlice)
					}
					prefix, ok := s.tryBorrowSlice(borrowStart, endMark.byteOffset)
					if !ok {
						return queuedToken{}, newScanError(startMark, scanErrInputOffsetsWithoutSlice)
					}
					out = append(out[:0], prefix...)
					isOwned = true
				}
				if len(s.bufLeadingBreak) == 0 {
					if isOwned {
						out = append(out, s.bufLeadingBreak...)
						out = append(out, s.bufTrailingBreaks...)
					}
					s.bufTrailingBreaks = s.bufTrailingBreaks[:0]
					s.bufLeadingBreak = s.bufLeadingBreak[:0]
				} else {
					if len(s.bufTrailingBreaks) == 0 {
						if isOwned {
							out = append(out, ' ')
						}
					} else {
						if isOwned {
							out = append(out, s.bufTrailingBreaks...)
						}
						s.bufTrailingBreaks = s.bufTrailingBreaks[:0]
					}
					s.bufLeadingBreak = s.bufLeadingBreak[:0]
				}
				s.leadingWhitespace = false
			} else if len(s.bufWhitespaces) > 0 {
				if isOwned {
					out = append(out, s.bufWhitespaces...)
				}
				s.bufWhitespaces = s.bufWhitespaces[:0]
			}

			hasContent = true
			if isOwned {
				out = utf8.AppendRune(out, s.input.peek())
			}
			s.skipNonBlank()

			end := false
			for !end {
				maxLen := s.input.bufmaxlen()
				s.input.lookahead(maxLen)
				chunkLen := maxLen - 1
				if chunkLen < 1 {
					chunkLen = 1
				}
				var charsConsumed int
				if isOwned {
					end, charsConsumed = s.input.fetchPlainScalarChunk(&out, chunkLen, s.flowLevel > 0)
				} else {
					end, charsConsumed = s.input.skipPlainScalarChunk(chunkLen, s.flowLevel > 0)
				}
				s.mark.charIndex += charsConsumed
				s.mark.col += charsConsumed
				s.mark.byteOffset, s.mark.hasByteOffset = s.input.byteOffset()
			}
			endMark = s.mark
		}

		if !(s.input.nextIsBlank() || s.input.nextIsBreak()) {
			break
		}

		s.input.lookahead(2)
		for s.input.nextIsBlankOrBreak() {
			if s.input.nextIsBlank() {
				if !s.leadingWhitespace {
					s.bufWhitespaces = utf8.AppendRune(s.bufWhitespaces, s.input.peek())
					s.skipBlank()
				} else if s.mark.col < indent && s.input.peek() == '\t' {
					if _, err := s.skipWsToEol(skipTabsYes); err != nil {
						return queuedToken{}, err
					}
					if !s.input.nextIsBreakz() {
						return queuedToken{}, newScanError(startMark, scanErrTabInPlainScalar)
					}
				} else {
					s.skipBlank()
				}
			} else {
				if s.leadingWhitespace {
					s.skipBreak()
					s.bufTrailingBreaks = append(s.bufTrailingBreaks, '\n')
				} else {
					s.bufWhitespaces = s.bufWhitespaces[:0]
					s.skipBreak()
					s.bufLeadingBreak = append(s.bufLeadingBreak, '\n')
					s.leadingWhitespace = true
				}
			}
			s.input.lookahead(2)
		}

		if (s.flowLevel == 0 || s.strictIndentation) && s.mark.col < indent {
			break
		}
	}

	if s.leadingWhitespace {
		s.allowSimpleKey()
	}

	var scalarText string
	if !isOwned {
		if !endMark.hasByteOffset {
			return queuedToken{}, newScanError(startMark, scanErrInputOffsetsWithoutSlice)
		}
		slice, ok := s.tryBorrowSlice(borrowStart, endMark.byteOffset)
		if !ok {
			return queuedToken{}, newScanError(startMark, scanErrInputOffsetsWithoutSlice)
		}
		scalarText = slice
	} else {
		scalarText = string(out)
	}
	if ch, bad := findNonPrintable(scalarText); bad {
		err := newScanError(startMark, scanErrUnexpectedCharacter)
		err.ch = ch
		return queuedToken{}, err
	}
	if err := s.ensureCurrentCharIsPrintable(); err != nil {
		return queuedToken{}, err
	}
	if hasContent {
		return queuedToken{
			span:  newRawSpan(startMark, endMark),
			kind:  tokenScalar,
			style: ScalarStylePlain,
			text1: scalarText,
		}, nil
	}
	return queuedToken{}, newScanError(startMark, scanErrUnexpectedEndOfPlainScalar)
}

func (s *scanner) fetchKey() *scanError {
	startMark := s.mark
	if s.flowLevel == 0 {
		if !s.simpleKeyAllowed {
			return s.scanErrorHere(scanErrMappingKeyNotAllowed)
		}
		if err := s.rollIndent(startMark.col, nil, tokenBlockMappingStart, startMark); err != nil {
			return err
		}
	} else {
		s.setCurrentFlowMappingStarted(true)
	}
	if err := s.removeSimpleKey(); err != nil {
		return err
	}
	if s.flowLevel == 0 {
		s.allowSimpleKey()
	} else {
		s.disallowSimpleKey()
	}
	s.skipNonBlank()
	endMark := s.mark
	tokenIndex := len(s.tokens)
	s.explicitKeyTabCheckPending = false
	if s.flowLevel == 0 {
		stoppedAfterComment, err := s.skipYamlWhitespace()
		if err != nil {
			return err
		}
		s.explicitKeyTabCheckPending = stoppedAfterComment ||
			(s.strictIndentation && (s.input.peek() == '&' || s.input.peek() == '!'))
	}
	s.insertToken(tokenIndex, queuedToken{
		span: newRawSpan(startMark, endMark),
		kind: tokenKey,
	})
	return nil
}

func (s *scanner) fetchFlowValue() *scanError {
	nc := s.input.peekNth(1)
	if s.mark.charIndex != s.adjacentValueAllowedAt && (nc == '[' || nc == '{') {
		return s.scanErrorHere(scanErrFlowMappingValueAdjacentCollection)
	}
	return s.fetchValue()
}

func (s *scanner) fetchValue() *scanError {
	sk := s.simpleKeys[len(s.simpleKeys)-1]
	startMark := s.mark
	isImplicitFlowMapping := s.currentFlowCollectionIsSequence() &&
		!s.currentFlowMappingStarted() &&
		len(s.implicitFlowMappingStates) > 0
	if isImplicitFlowMapping {
		s.implicitFlowMappingStates[len(s.implicitFlowMappingStates)-1] = implicitMappingState{
			inside:    true,
			flowLevel: s.flowLevel,
		}
	}

	// Skip over ':'
	s.skipNonBlank()
	var trailingTokens []queuedToken
	foundTabs := false
	if look := s.input.lookCh(); look == ' ' || look == '\t' {
		trailingTokenIndex := len(s.tokens)
		status, err := s.skipWsToEol(skipTabsYes)
		if err != nil {
			return err
		}
		foundTabs = status.foundTabs()
		if trailingTokenIndex < len(s.tokens) {
			trailingTokens = append(trailingTokens, s.tokens[trailingTokenIndex:]...)
			s.tokens = s.tokens[:trailingTokenIndex]
		}
	}

	if sk.possible {
		blockIndent := s.flowBlockIndent()
		if s.flowLevel > 0 && sk.mark.line < startMark.line && sk.mark.col <= blockIndent {
			return newScanError(startMark, scanErrInvalidColonPlacement)
		}
		tokenIndex, err := s.simpleKeyTokenIndex(&sk, startMark)
		if err != nil {
			return err
		}
		if err := s.validateFlowKeyIndentation(&sk, tokenIndex, startMark); err != nil {
			return err
		}
		s.insertToken(tokenIndex, queuedToken{
			span: emptyRawSpan(sk.mark),
			kind: tokenKey,
		})
		if isImplicitFlowMapping {
			if sk.mark.line < startMark.line {
				return newScanError(startMark, scanErrInvalidColonPlacement)
			}
			s.insertToken(tokenIndex, queuedToken{
				span: emptyRawSpan(sk.mark),
				kind: tokenFlowMappingStart,
			})
		}
		tokNum := sk.tokenNumber
		if err := s.rollIndent(sk.mark.col, &tokNum, tokenBlockMappingStart, sk.mark); err != nil {
			return err
		}
		s.rollOneColIndent()
		s.clearSimpleKey()
		s.disallowSimpleKey()
	} else {
		if isImplicitFlowMapping {
			s.tokens = append(s.tokens, queuedToken{
				span: emptyRawSpan(startMark),
				kind: tokenFlowMappingStart,
			})
		}
		if s.flowLevel == 0 {
			if !s.simpleKeyAllowed {
				return newScanError(startMark, scanErrMappingValueNotAllowed)
			}
			if err := s.rollIndent(startMark.col, nil, tokenBlockMappingStart, startMark); err != nil {
				return err
			}
		}
		s.rollOneColIndent()
		if s.flowLevel == 0 && !foundTabs {
			s.allowSimpleKey()
		} else {
			s.disallowSimpleKey()
		}
	}

	s.tokens = append(s.tokens, queuedToken{
		span: emptyRawSpan(startMark),
		kind: tokenValue,
	})
	if len(trailingTokens) > 0 {
		s.tokens = append(s.tokens, trailingTokens...)
	}
	return nil
}

func (s *scanner) validateFlowKeyIndentation(key *simpleKey, tokenIndex int, colon Marker) *scanError {
	if s.flowLevel == 0 || key.mark.line == colon.line {
		return nil
	}
	blockIndent := s.flowBlockIndent()
	underIndented := key.mark.col <= blockIndent || colon.col <= blockIndent
	for i := tokenIndex; i < len(s.tokens); i++ {
		tok := &s.tokens[i]
		if tok.span.start.col <= blockIndent {
			underIndented = true
		}
		if tok.kind != tokenAnchor && tok.kind != tokenTag {
			break
		}
	}
	if underIndented {
		return newScanError(colon, scanErrInvalidColonPlacement)
	}
	return nil
}

func (s *scanner) flowBlockIndent() int {
	if len(s.indents) > 0 {
		last := s.indents[len(s.indents)-1]
		if !last.needsBlockEnd {
			return last.indent
		}
	}
	return s.indent
}

func (s *scanner) rollIndent(col int, number *int, tok tokenKind, mark Marker) *scanError {
	if s.flowLevel > 0 {
		return nil
	}
	if s.indent <= col && len(s.indents) > 0 {
		last := s.indents[len(s.indents)-1]
		if !last.needsBlockEnd {
			s.indent = last.indent
			s.indents = s.indents[:len(s.indents)-1]
		}
	}
	if s.indent < col {
		if s.blockLevel >= s.blockNestingLimit {
			return newScanError(mark, scanErrRecursionLimitExceeded)
		}
		s.indents = append(s.indents, indentEntry{
			indent:        s.indent,
			needsBlockEnd: true,
		})
		s.blockLevel++
		s.indent = col
		qt := queuedToken{
			span: emptyRawSpan(mark),
			kind: tok,
		}
		if number != nil {
			s.insertToken(*number-s.tokensParsed, qt)
		} else {
			s.tokens = append(s.tokens, qt)
		}
	}
	return nil
}

func (s *scanner) unrollIndent(col int) {
	if s.flowLevel > 0 {
		return
	}
	for s.indent > col {
		last := s.indents[len(s.indents)-1]
		s.indents = s.indents[:len(s.indents)-1]
		s.indent = last.indent
		if last.needsBlockEnd {
			s.blockLevel--
			s.tokens = append(s.tokens, queuedToken{
				span: emptyRawSpan(s.mark),
				kind: tokenBlockEnd,
			})
		}
	}
}

func (s *scanner) rollOneColIndent() {
	if s.flowLevel == 0 && len(s.indents) > 0 && s.indents[len(s.indents)-1].needsBlockEnd {
		s.indents = append(s.indents, indentEntry{
			indent:        s.indent,
			needsBlockEnd: false,
		})
		s.indent++
	}
}

func (s *scanner) unrollNonBlockIndents() {
	for len(s.indents) > 0 {
		last := s.indents[len(s.indents)-1]
		if last.needsBlockEnd {
			break
		}
		s.indent = last.indent
		s.indents = s.indents[:len(s.indents)-1]
	}
}

func (s *scanner) saveSimpleKey() {
	if s.simpleKeyAllowed {
		required := s.flowLevel == 0 &&
			s.indent == s.mark.col &&
			len(s.indents) > 0 &&
			s.indents[len(s.indents)-1].needsBlockEnd
		if len(s.simpleKeys) > 0 {
			s.simpleKeys[len(s.simpleKeys)-1] = simpleKey{
				mark:        s.mark,
				possible:    true,
				required:    required,
				tokenNumber: s.tokensParsed + len(s.tokens),
			}
			if s.firstSimpleKey < 0 {
				s.firstSimpleKey = s.flowLevel
			}
		}
	}
}

func (s *scanner) removeSimpleKey() *scanError {
	last := s.simpleKeys[len(s.simpleKeys)-1]
	if last.possible && last.required {
		return s.simpleKeyExpected(last.mark)
	}
	s.clearSimpleKey()
	return nil
}

func (s *scanner) clearSimpleKey() {
	s.simpleKeys[len(s.simpleKeys)-1].possible = false
	if s.firstSimpleKey == s.flowLevel {
		s.firstSimpleKey = -1
	}
}

func (s *scanner) isWithinBlock() bool {
	return len(s.indents) > 0
}

func (s *scanner) endImplicitMapping(mark Marker, flowLevel int) {
	if len(s.implicitFlowMappingStates) > 0 {
		last := &s.implicitFlowMappingStates[len(s.implicitFlowMappingStates)-1]
		if last.inside && last.flowLevel == flowLevel {
			*last = implicitMappingState{}
			s.setCurrentFlowMappingStarted(false)
			s.tokens = append(s.tokens, queuedToken{
				span: emptyRawSpan(mark),
				kind: tokenFlowMappingEnd,
			})
		}
	}
}

func (s *scanner) currentFlowCollectionIsSequence() bool {
	return len(s.flowMarkers) > 0 && s.flowMarkers[len(s.flowMarkers)-1].bracket == '['
}

func (s *scanner) currentFlowMappingStarted() bool {
	return len(s.flowMappingStarted) > 0 && s.flowMappingStarted[len(s.flowMappingStarted)-1]
}

func (s *scanner) setCurrentFlowMappingStarted(started bool) {
	if len(s.flowMappingStarted) > 0 {
		s.flowMappingStarted[len(s.flowMappingStarted)-1] = started
	}
}

var _ = strconv.Itoa
