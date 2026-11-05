package engine

import "unicode/utf8"

func isZ(c rune) bool {
	return c == 0
}

func isBreak(c rune) bool {
	return c == '\n' || c == '\r'
}

func isBreakz(c rune) bool {
	return isBreak(c) || isZ(c)
}

func isBlank(c rune) bool {
	return c == ' ' || c == '\t'
}

func isBlankOrBreak(c rune) bool {
	return isBlank(c) || isBreak(c)
}

func isBlankOrBreakz(c rune) bool {
	return isBlank(c) || isBreakz(c)
}

func isDigit(c rune) bool {
	return c >= '0' && c <= '9'
}

func isAlpha(c rune) bool {
	return (c >= '0' && c <= '9') ||
		(c >= 'a' && c <= 'z') ||
		(c >= 'A' && c <= 'Z') ||
		c == '_' || c == '-'
}

func isHex(c rune) bool {
	return (c >= '0' && c <= '9') ||
		(c >= 'a' && c <= 'f') ||
		(c >= 'A' && c <= 'F')
}

func asHex(c rune) uint32 {
	switch {
	case c >= '0' && c <= '9':
		return uint32(c - '0')
	case c >= 'a' && c <= 'f':
		return uint32(c-'a') + 10
	case c >= 'A' && c <= 'F':
		return uint32(c-'A') + 10
	default:
		panic("unreachable non-hex digit")
	}
}

func isFlow(c rune) bool {
	return c == ',' || c == '[' || c == ']' || c == '{' || c == '}'
}

func isBOM(c rune) bool {
	return c == '\uFEFF'
}

// isPrintable implements YAML 1.2.2 rule [1] c-printable.
func isPrintable(c rune) bool {
	return c == 0x09 ||
		c == 0x0A ||
		c == 0x0D ||
		(c >= 0x20 && c <= 0x7E) ||
		c == 0x85 ||
		(c >= 0xA0 && c <= 0xD7FF) ||
		(c >= 0xE000 && c <= 0xFFFD) ||
		(c >= 0x10000 && c <= 0x10FFFF)
}

func findNonPrintable(s string) (rune, bool) {
	for i := 0; i < len(s); {
		b := s[i]
		if b < 0x80 {
			if (b >= 0x20 && b <= 0x7E) || b == 0x09 || b == 0x0A || b == 0x0D {
				i++
				continue
			}
			return rune(b), true
		}
		r, width := utf8.DecodeRuneInString(s[i:])
		if !isPrintable(r) {
			return r, true
		}
		i += width
	}
	return 0, false
}

func isYamlNonBreak(c rune) bool {
	return isPrintable(c) && !isBreakz(c) && !isBOM(c)
}

func isYamlNonSpace(c rune) bool {
	return isYamlNonBreak(c) && !isBlank(c)
}

func isAnchorChar(c rune) bool {
	return isYamlNonSpace(c) && !isFlow(c)
}

func isWordChar(c rune) bool {
	return isAlpha(c) && c != '_'
}

func isURIChar(c rune) bool {
	if isWordChar(c) {
		return true
	}
	switch c {
	case '#', ';', '/', '?', ':', '@', '&', '=', '+', '$', ',', '_', '.', '!', '~', '*', '\'', '(', ')', '[', ']', '%':
		return true
	default:
		return false
	}
}

func isTagChar(c rune) bool {
	return isURIChar(c) && !isFlow(c) && c != '!'
}

const inputBufCap = 16

type strInput struct {
	full    string
	src     string
	bytePos int
}

func newStrInput(s string) strInput {
	return strInput{
		full:    s,
		src:     s,
		bytePos: 0,
	}
}

func (in *strInput) bufmaxlen() int {
	return inputBufCap
}

func (in *strInput) bufIsEmpty() bool {
	return len(in.src) == 0
}

func (in *strInput) rawReadCh() rune {
	if len(in.src) == 0 {
		return 0
	}
	r, width := utf8.DecodeRuneInString(in.src)
	in.src = in.src[width:]
	in.bytePos += width
	return r
}

func (in *strInput) skip() {
	if len(in.src) == 0 {
		return
	}
	_, width := utf8.DecodeRuneInString(in.src)
	in.src = in.src[width:]
	in.bytePos += width
}

func (in *strInput) skipN(count int) {
	if count == 0 {
		return
	}
	bytes := in.src
	if count <= len(bytes) {
		asciiPrefix := true
		for i := 0; i < count; i++ {
			if bytes[i] >= 0x80 {
				asciiPrefix = false
				break
			}
		}
		if asciiPrefix {
			in.src = in.src[count:]
			in.bytePos += count
			return
		}
	}
	consumed := 0
	s := in.src
	for i := 0; i < count && len(s) > 0; i++ {
		_, width := utf8.DecodeRuneInString(s)
		s = s[width:]
		consumed += width
	}
	in.src = in.src[consumed:]
	in.bytePos += consumed
}

func (in *strInput) lookahead(_ int) {
	// Direct string slice needs no lookahead buffering.
}

func (in *strInput) peek() rune {
	if len(in.src) == 0 {
		return 0
	}
	b := in.src[0]
	if b < 0x80 {
		return rune(b)
	}
	r, _ := utf8.DecodeRuneInString(in.src)
	return r
}

func (in *strInput) lookCh() rune {
	return in.peek()
}

func (in *strInput) peekNth(n int) rune {
	if n == 0 {
		return in.peek()
	}
	if len(in.src) > n {
		asciiPrefix := true
		for i := 0; i <= n; i++ {
			if in.src[i] >= 0x80 {
				asciiPrefix = false
				break
			}
		}
		if asciiPrefix {
			return rune(in.src[n])
		}
	}
	s := in.src
	for i := 0; i < n; i++ {
		if len(s) == 0 {
			return 0
		}
		_, width := utf8.DecodeRuneInString(s)
		s = s[width:]
	}
	if len(s) == 0 {
		return 0
	}
	r, _ := utf8.DecodeRuneInString(s)
	return r
}

func (in *strInput) byteOffset() (int, bool) {
	return in.bytePos, true
}

func (in *strInput) sliceBytes(start, end int) (string, bool) {
	if start < 0 || end < start || end > len(in.full) {
		return "", false
	}
	return in.full[start:end], true
}

func (in *strInput) fetchBlockScalarLine(out *[]byte) int {
	bytes := in.src
	for i := 0; i < len(bytes); i++ {
		b := bytes[i]
		if b == '\n' || b == '\r' || b == 0 {
			if i > 0 {
				*out = append(*out, bytes[:i]...)
				in.src = in.src[i:]
				in.bytePos += i
			}
			return utf8.RuneCountInString(bytes[:i])
		}
	}
	n := len(bytes)
	if n == 0 {
		return 0
	}
	*out = append(*out, bytes...)
	in.src = ""
	in.bytePos += n
	return utf8.RuneCountInString(bytes)
}

func (in *strInput) takeQuotedScalarAsciiChunk(single bool) string {
	bytes := in.src
	i := 0
	if single {
		for i < len(bytes) {
			b := bytes[i]
			if b < 0x21 || b > 0x7E || b == '\'' {
				break
			}
			i++
		}
	} else {
		for i < len(bytes) {
			b := bytes[i]
			if b < 0x21 || b > 0x7E || b == '"' || b == '\\' {
				break
			}
			i++
		}
	}
	chunk := in.src[:i]
	in.src = in.src[i:]
	in.bytePos += i
	return chunk
}

func canContinuePlainScalar(ch, next rune, inFlow bool) bool {
	if isBlankOrBreakz(ch) {
		return false
	}
	if ch == ':' && (isBlankOrBreakz(next) || (inFlow && isFlow(next))) {
		return false
	}
	return !(inFlow && isFlow(ch))
}

func scanPlainScalarChunk(s string, maxChars int, inFlow bool) (stop bool, charsConsumed int, bytesConsumed int) {
	rem := s
	for charsConsumed < maxChars {
		if len(rem) == 0 {
			return true, charsConsumed, bytesConsumed
		}
		ch, width := utf8.DecodeRuneInString(rem)
		next := rune(0)
		if len(rem) > width {
			next, _ = utf8.DecodeRuneInString(rem[width:])
		}
		if !canContinuePlainScalar(ch, next, inFlow) {
			return true, charsConsumed, bytesConsumed
		}
		rem = rem[width:]
		charsConsumed++
		bytesConsumed += width
	}
	return false, charsConsumed, bytesConsumed
}

func (in *strInput) fetchPlainScalarChunk(out *[]byte, maxChars int, inFlow bool) (bool, int) {
	stop, charsConsumed, bytesConsumed := scanPlainScalarChunk(in.src, maxChars, inFlow)
	if bytesConsumed > 0 {
		*out = append(*out, in.src[:bytesConsumed]...)
		in.src = in.src[bytesConsumed:]
		in.bytePos += bytesConsumed
	}
	return stop, charsConsumed
}

func (in *strInput) skipPlainScalarChunk(maxChars int, inFlow bool) (bool, int) {
	stop, charsConsumed, bytesConsumed := scanPlainScalarChunk(in.src, maxChars, inFlow)
	if bytesConsumed > 0 {
		in.src = in.src[bytesConsumed:]
		in.bytePos += bytesConsumed
	}
	return stop, charsConsumed
}

func (in *strInput) skipWsToEolBlanks(skipTabsMode skipTabs) (int, whitespaceResult) {
	bytes := in.src
	i := 0
	encounteredTab := false
	hasYamlWs := false
	if skipTabsMode == skipTabsYes {
		for i < len(bytes) {
			switch bytes[i] {
			case ' ':
				hasYamlWs = true
				i++
			case '\t':
				encounteredTab = true
				i++
			default:
				goto done
			}
		}
	} else {
		for i < len(bytes) && bytes[i] == ' ' {
			hasYamlWs = true
			i++
		}
	}
done:
	in.src = in.src[i:]
	in.bytePos += i
	return i, whitespaceResult{
		encounteredTab: encounteredTab,
		hasYamlWs:      hasYamlWs,
	}
}

func (in *strInput) skipWhileNonBreakz() int {
	bytePos := 0
	charsConsumed := 0
	for i, c := range in.src {
		if isBreakz(c) || !isPrintable(c) {
			break
		}
		bytePos = i + utf8.RuneLen(c)
		charsConsumed++
	}
	in.src = in.src[bytePos:]
	in.bytePos += bytePos
	return charsConsumed
}

func (in *strInput) skipWhileBlank() int {
	bytes := in.src
	i := 0
	for i < len(bytes) {
		if bytes[i] == ' ' || bytes[i] == '\t' {
			i++
		} else {
			break
		}
	}
	in.src = in.src[i:]
	in.bytePos += i
	return i
}

func (in *strInput) nextCharIs(c rune) bool {
	return in.peek() == c
}

func (in *strInput) nthCharIs(n int, c rune) bool {
	return in.peekNth(n) == c
}

func (in *strInput) next2Are(c1, c2 rune) bool {
	return in.peek() == c1 && in.peekNth(1) == c2
}

func (in *strInput) nextIsZ() bool {
	return len(in.src) == 0
}

func (in *strInput) nextIsBreak() bool {
	return isBreak(in.peek())
}

func (in *strInput) nextIsBreakz() bool {
	return isBreakz(in.peek())
}

func (in *strInput) nextIsBlank() bool {
	return isBlank(in.peek())
}

func (in *strInput) nextIsBlankOrBreak() bool {
	return isBlankOrBreak(in.peek())
}

func (in *strInput) nextIsBlankOrBreakz() bool {
	return isBlankOrBreakz(in.peek())
}

func (in *strInput) nextIsFlow() bool {
	return isFlow(in.peek())
}

func (in *strInput) nextIsDigit() bool {
	return isDigit(in.peek())
}

func (in *strInput) nextIsAlpha() bool {
	return isAlpha(in.peek())
}

func (in *strInput) nextIsDocumentStart() bool {
	return in.peekNth(0) == '-' &&
		in.peekNth(1) == '-' &&
		in.peekNth(2) == '-' &&
		isBlankOrBreakz(in.peekNth(3))
}

func (in *strInput) nextIsDocumentEnd() bool {
	return in.peekNth(0) == '.' &&
		in.peekNth(1) == '.' &&
		in.peekNth(2) == '.' &&
		isBlankOrBreakz(in.peekNth(3))
}

func (in *strInput) nextIsDocumentIndicator() bool {
	return in.nextIsDocumentStart() || in.nextIsDocumentEnd()
}

func (in *strInput) nextCanBePlainScalar(inFlow bool) bool {
	c := in.peek()
	nc := in.peekNth(1)
	if c == ':' && (isBlankOrBreakz(nc) || (inFlow && isFlow(nc))) {
		return false
	}
	if inFlow && isFlow(c) {
		return false
	}
	return true
}
