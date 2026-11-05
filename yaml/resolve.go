package yaml

import (
	"math"
	"math/bits"
	"strconv"
	"strings"

	"github.com/3leaps/yamlprims/yaml/internal/engine"
)

const (
	canonicalQuietNaNBits uint64 = 0x7ff8_0000_0000_0000
	yamlNamespacePrefix   string = "tag:yaml.org,2002:"
)

func canonicalNaN() float64 {
	return math.Float64frombits(canonicalQuietNaNBits)
}

type tagClassKind uint8

const (
	tagClassNone tagClassKind = iota
	tagClassNonSpecific
	tagClassCore
	tagClassMerge
	tagClassOther
)

type tagClass struct {
	kind     tagClassKind
	coreName string
	other    Tag
}

func classifyEngineTag(t *engine.Tag) tagClass {
	if t == nil {
		return tagClass{kind: tagClassNone}
	}
	if t.Handle == "" && t.Suffix == "!" {
		return tagClass{kind: tagClassNonSpecific}
	}
	if core, ok := t.CoreSuffix(); ok {
		return tagClass{kind: tagClassCore, coreName: core}
	}
	if t.Handle+t.Suffix == yamlNamespacePrefix+"merge" {
		return tagClass{kind: tagClassMerge}
	}
	return tagClass{
		kind:  tagClassOther,
		other: NewTag(t.String(), t.OriginalHandle, t.Suffix),
	}
}

func isNullLiteral(s string) bool {
	return s == "" || s == "~" || s == "null" || s == "Null" || s == "NULL"
}

func asBoolLiteral(s string) (bool, bool) {
	switch s {
	case "true", "True", "TRUE":
		return true, true
	case "false", "False", "FALSE":
		return false, true
	default:
		return false, false
	}
}

func allDecDigits(s string) bool {
	if len(s) == 0 {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return true
}

func allOctalDigits(s string) bool {
	if len(s) == 0 {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '7' {
			return false
		}
	}
	return true
}

func allHexDigits(s string) bool {
	if len(s) == 0 {
		return false
	}
	for i := 0; i < len(s); i++ {
		b := s[i]
		if !((b >= '0' && b <= '9') || (b >= 'a' && b <= 'f') || (b >= 'A' && b <= 'F')) {
			return false
		}
	}
	return true
}

type intParseKind uint8

const (
	intParseNotMatch intParseKind = iota
	intParseValue
	intParseOverflow
)

type intParseResult struct {
	kind intParseKind
	val  Integer
}

func parseIntLiteral(t string) intParseResult {
	if strings.HasPrefix(t, "0o") {
		r := t[2:]
		if !allOctalDigits(r) {
			return intParseResult{kind: intParseNotMatch}
		}
		return parseRadix128(false, r, 8)
	}
	if strings.HasPrefix(t, "0x") {
		r := t[2:]
		if !allHexDigits(r) {
			return intParseResult{kind: intParseNotMatch}
		}
		return parseRadix128(false, r, 16)
	}
	neg := false
	digits := t
	if strings.HasPrefix(t, "-") {
		neg = true
		digits = t[1:]
	} else if strings.HasPrefix(t, "+") {
		digits = t[1:]
	}
	if !allDecDigits(digits) {
		return intParseResult{kind: intParseNotMatch}
	}
	return parseRadix128(neg, digits, 10)
}

func parseRadix128(neg bool, digits string, radix uint64) intParseResult {
	var accHi, accLo uint64
	for i := 0; i < len(digits); i++ {
		b := digits[i]
		var d uint64
		switch {
		case b >= '0' && b <= '9':
			d = uint64(b - '0')
		case b >= 'a' && b <= 'f':
			d = uint64(b-'a') + 10
		case b >= 'A' && b <= 'F':
			d = uint64(b-'A') + 10
		default:
			return intParseResult{kind: intParseNotMatch}
		}
		p0Hi, p0Lo := bits.Mul64(accLo, radix)
		p1Hi, p1Lo := bits.Mul64(accHi, radix)
		if p1Hi != 0 {
			return intParseResult{kind: intParseOverflow}
		}
		midHi, carry1 := bits.Add64(p0Hi, p1Lo, 0)
		if carry1 != 0 {
			return intParseResult{kind: intParseOverflow}
		}
		newLo, carry2 := bits.Add64(p0Lo, d, 0)
		newHi, carry3 := bits.Add64(midHi, 0, carry2)
		if carry3 != 0 {
			return intParseResult{kind: intParseOverflow}
		}
		accHi, accLo = newHi, newLo
	}
	v, ok := NewSignedMagnitudeInteger(neg, accHi, accLo)
	if !ok {
		return intParseResult{kind: intParseOverflow}
	}
	return intParseResult{kind: intParseValue, val: v}
}

type floatParseKind uint8

const (
	floatParseNotMatch floatParseKind = iota
	floatParseValue
	floatParseOverflow
)

type floatParseResult struct {
	kind floatParseKind
	val  float64
}

func parseFloatLiteral(t string) floatParseResult {
	unsigned := t
	if strings.HasPrefix(t, "-") || strings.HasPrefix(t, "+") {
		unsigned = t[1:]
	}
	switch unsigned {
	case ".inf", ".Inf", ".INF":
		if strings.HasPrefix(t, "-") {
			return floatParseResult{kind: floatParseValue, val: math.Inf(-1)}
		}
		return floatParseResult{kind: floatParseValue, val: math.Inf(1)}
	}
	switch t {
	case ".nan", ".NaN", ".NAN":
		return floatParseResult{kind: floatParseValue, val: canonicalNaN()}
	}
	// [-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?
	eIdx := strings.IndexAny(unsigned, "eE")
	mant := unsigned
	hasExp := false
	var exp string
	if eIdx >= 0 {
		mant = unsigned[:eIdx]
		exp = unsigned[eIdx+1:]
		hasExp = true
	}
	mantOK := false
	if dotIdx := strings.IndexByte(mant, '.'); dotIdx >= 0 {
		intPart := mant[:dotIdx]
		fracPart := mant[dotIdx+1:]
		if intPart == "" {
			mantOK = allDecDigits(fracPart)
		} else {
			mantOK = allDecDigits(intPart) && (fracPart == "" || allDecDigits(fracPart))
		}
	} else {
		mantOK = allDecDigits(mant)
	}
	expOK := true
	if hasExp {
		eDigits := exp
		if strings.HasPrefix(exp, "-") || strings.HasPrefix(exp, "+") {
			eDigits = exp[1:]
		}
		expOK = allDecDigits(eDigits)
	}
	if !mantOK || !expOK {
		return floatParseResult{kind: floatParseNotMatch}
	}
	v, err := strconv.ParseFloat(t, 64)
	if err != nil {
		if math.IsInf(v, 0) {
			return floatParseResult{kind: floatParseOverflow}
		}
		if math.IsNaN(v) {
			return floatParseResult{kind: floatParseNotMatch}
		}
		return floatParseResult{kind: floatParseValue, val: v}
	}
	if math.IsInf(v, 0) {
		return floatParseResult{kind: floatParseOverflow}
	}
	return floatParseResult{kind: floatParseValue, val: v}
}

func corePlain(t string) (Value, ErrorKind) {
	if isNullLiteral(t) {
		return NullValue(), ""
	}
	if b, ok := asBoolLiteral(t); ok {
		return BoolValue(b), ""
	}
	intRes := parseIntLiteral(t)
	switch intRes.kind {
	case intParseValue:
		return IntValue(intRes.val), ""
	case intParseOverflow:
		return Value{}, ErrorKindIntegerOutOfRange
	}
	floatRes := parseFloatLiteral(t)
	switch floatRes.kind {
	case floatParseValue:
		return FloatValue(floatRes.val), ""
	case floatParseOverflow:
		return Value{}, ErrorKindFloatOutOfRange
	}
	return StringValue(t), ""
}

type resolvedScalar struct {
	isTagged bool
	tag      Tag
	value    Value
}

func resolveScalarValue(text string, style engine.ScalarStyle, class tagClass) (resolvedScalar, ErrorKind) {
	plain := style == engine.ScalarStylePlain
	switch class.kind {
	case tagClassNone:
		if plain {
			v, ek := corePlain(text)
			if ek != "" {
				return resolvedScalar{}, ek
			}
			return resolvedScalar{value: v}, ""
		}
		return resolvedScalar{value: StringValue(text)}, ""

	case tagClassNonSpecific:
		return resolvedScalar{value: StringValue(text)}, ""

	case tagClassCore:
		switch class.coreName {
		case "str":
			return resolvedScalar{value: StringValue(text)}, ""
		case "null":
			if isNullLiteral(text) {
				return resolvedScalar{value: NullValue()}, ""
			}
			return resolvedScalar{}, ErrorKindTagValueMismatch
		case "bool":
			if b, ok := asBoolLiteral(text); ok {
				return resolvedScalar{value: BoolValue(b)}, ""
			}
			return resolvedScalar{}, ErrorKindTagValueMismatch
		case "int":
			res := parseIntLiteral(text)
			switch res.kind {
			case intParseValue:
				return resolvedScalar{value: IntValue(res.val)}, ""
			case intParseOverflow:
				return resolvedScalar{}, ErrorKindIntegerOutOfRange
			default:
				return resolvedScalar{}, ErrorKindTagValueMismatch
			}
		case "float":
			if isIntLiteral(text) {
				if f, ok := intLiteralToF64Exact(text); ok {
					return resolvedScalar{value: FloatValue(f)}, ""
				}
				return resolvedScalar{}, ErrorKindTagValueMismatch
			}
			res := parseFloatLiteral(text)
			switch res.kind {
			case floatParseValue:
				return resolvedScalar{value: FloatValue(res.val)}, ""
			case floatParseOverflow:
				return resolvedScalar{}, ErrorKindFloatOutOfRange
			default:
				return resolvedScalar{}, ErrorKindTagValueMismatch
			}
		default:
			return resolvedScalar{}, ErrorKindTagKindMismatch
		}

	case tagClassMerge:
		return resolvedScalar{
			isTagged: true,
			tag:      NewTag(yamlNamespacePrefix+"merge", "!!", "merge"),
			value:    StringValue(text),
		}, ""

	case tagClassOther:
		return resolvedScalar{
			isTagged: true,
			tag:      class.other,
			value:    StringValue(text),
		}, ""
	}
	return resolvedScalar{}, ErrorKindInternal
}

func isIntLiteral(t string) bool {
	if strings.HasPrefix(t, "0o") {
		return allOctalDigits(t[2:])
	}
	if strings.HasPrefix(t, "0x") {
		return allHexDigits(t[2:])
	}
	d := t
	if strings.HasPrefix(t, "-") || strings.HasPrefix(t, "+") {
		d = t[1:]
	}
	return allDecDigits(d)
}

func intLiteralToF64Exact(t string) (float64, bool) {
	var bitsPer int
	var digits string
	if strings.HasPrefix(t, "0o") {
		bitsPer = 3
		digits = t[2:]
	} else if strings.HasPrefix(t, "0x") {
		bitsPer = 4
		digits = t[2:]
	} else {
		neg := strings.HasPrefix(t, "-")
		d := t
		if strings.HasPrefix(t, "-") || strings.HasPrefix(t, "+") {
			d = t[1:]
		}
		d = strings.TrimLeft(d, "0")
		if d == "" {
			d = "0"
		}
		f, err := strconv.ParseFloat(d, 64)
		if err != nil || math.IsInf(f, 0) || math.IsNaN(f) {
			return 0, false
		}
		if strconv.FormatFloat(f, 'f', 0, 64) != d {
			return 0, false
		}
		if neg && f != 0.0 {
			return -f, true
		}
		return f, true
	}

	var bitBuf strings.Builder
	for i := 0; i < len(digits); i++ {
		c := digits[i]
		var v uint8
		switch {
		case c >= '0' && c <= '9':
			v = c - '0'
		case c >= 'a' && c <= 'f':
			v = c - 'a' + 10
		case c >= 'A' && c <= 'F':
			v = c - 'A' + 10
		default:
			return 0, false
		}
		if int(v) >= (1 << bitsPer) {
			return 0, false
		}
		for bit := bitsPer - 1; bit >= 0; bit-- {
			if (v>>bit)&1 == 1 {
				bitBuf.WriteByte('1')
			} else {
				bitBuf.WriteByte('0')
			}
		}
	}
	bitStr := strings.TrimLeft(bitBuf.String(), "0")
	if bitStr == "" {
		return 0.0, true
	}
	trimmedRight := strings.TrimRight(bitStr, "0")
	span := len(trimmedRight)
	if span > 53 || len(bitStr) > 1024 {
		return 0, false
	}
	mant, err := strconv.ParseUint(bitStr[:span], 2, 64)
	if err != nil {
		return 0, false
	}
	return math.Ldexp(float64(mant), len(bitStr)-span), true
}
