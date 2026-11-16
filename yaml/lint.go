package yaml

import (
	"io"
	"strings"
	"unicode/utf8"

	"github.com/3leaps/yamlprims/yaml/internal/engine"
)

// Test hook for verifying pre-clone diagnostic budget rejection.
var lintPathCloneHook func()

// LintCode identifies a closed-vocabulary YAML 1.1 compatibility hazard (spec-v1.6 §10.3, §11.7).
type LintCode string

const (
	LintCodeYaml11Boolean     LintCode = "yaml11_boolean"
	LintCodeYaml11Number      LintCode = "yaml11_number"
	LintCodeYaml11Sexagesimal LintCode = "yaml11_sexagesimal"
	LintCodeYaml11MergeKey    LintCode = "yaml11_merge_key"

	LintCodeYAML11Boolean     = LintCodeYaml11Boolean
	LintCodeYAML11Number      = LintCodeYaml11Number
	LintCodeYAML11Sexagesimal = LintCodeYaml11Sexagesimal
	LintCodeYAML11MergeKey    = LintCodeYaml11MergeKey
)

// String returns the canonical wire string of c.
func (c LintCode) String() string {
	return string(c)
}

// IsValid reports whether c is one of the 4 normative LintCode values.
func (c LintCode) IsValid() bool {
	switch c {
	case LintCodeYaml11Boolean,
		LintCodeYaml11Number,
		LintCodeYaml11Sexagesimal,
		LintCodeYaml11MergeKey:
		return true
	default:
		return false
	}
}

// LintDiagnostic represents a single YAML 1.1 compatibility hazard recorded during LintYAML11* (spec-v1.6 §10.3).
type LintDiagnostic struct {
	Code             LintCode
	Span             Span
	DocumentIndex    int
	Path             []PathSegment
	Raw              string
	YAML12Resolution string
	YAML11Note       string
	Message          string
}

// LintResult is the complete stream lint result returned by LintYAML11* (spec-v1.6 §10.3).
type LintResult struct {
	Diagnostics  []LintDiagnostic
	WarningCount int
	Truncated    bool
}

type scalarHazard uint8

const (
	hazardNone scalarHazard = iota
	hazardBooleanTruthy
	hazardBooleanFalsity
	hazardSexagesimalInt
	hazardSexagesimalFloat
	hazardLegacyOctal
	hazardBinaryInt
	hazardSignedHexInt
	hazardUnderscoreNumber
)

func classifyUntaggedPlain(raw string) scalarHazard {
	switch raw {
	case "y", "Y", "yes", "Yes", "YES", "on", "On", "ON":
		return hazardBooleanTruthy
	case "n", "N", "no", "No", "NO", "off", "Off", "OFF":
		return hazardBooleanFalsity
	}
	if isFloat, ok := matchYAML11Sexagesimal(raw); ok {
		if isFloat {
			return hazardSexagesimalFloat
		}
		return hazardSexagesimalInt
	}
	if matchLegacyOctalNoUnderscore(raw) {
		return hazardLegacyOctal
	}
	if matchBinaryInt(raw) {
		return hazardBinaryInt
	}
	if matchSignedHexInt(raw) {
		return hazardSignedHexInt
	}
	if matchUnderscoreNumber(raw) {
		return hazardUnderscoreNumber
	}
	return hazardNone
}

func isValidSexagesimalTail(parts []string) bool {
	if len(parts) == 0 {
		return false
	}
	for _, seg := range parts {
		switch len(seg) {
		case 1:
			if seg[0] < '0' || seg[0] > '9' {
				return false
			}
		case 2:
			if seg[0] < '0' || seg[0] > '5' || seg[1] < '0' || seg[1] > '9' {
				return false
			}
		default:
			return false
		}
	}
	return true
}

func isDigitsAndUnderscores(s string, nonZeroStart bool) bool {
	if len(s) == 0 {
		return false
	}
	if nonZeroStart {
		if s[0] < '1' || s[0] > '9' {
			return false
		}
	} else {
		if s[0] < '0' || s[0] > '9' {
			return false
		}
	}
	for i := 1; i < len(s); i++ {
		b := s[i]
		if (b < '0' || b > '9') && b != '_' {
			return false
		}
	}
	return true
}

func matchYAML11Sexagesimal(raw string) (isFloat bool, ok bool) {
	rest := raw
	if len(rest) > 0 && (rest[0] == '+' || rest[0] == '-') {
		rest = rest[1:]
	}
	if dotIdx := strings.IndexByte(rest, '.'); dotIdx >= 0 {
		beforeDot := rest[:dotIdx]
		afterDot := rest[dotIdx+1:]
		for i := 0; i < len(afterDot); i++ {
			b := afterDot[i]
			if (b < '0' || b > '9') && b != '_' {
				return false, false
			}
		}
		parts := strings.Split(beforeDot, ":")
		if len(parts) < 2 || !isDigitsAndUnderscores(parts[0], false) || !isValidSexagesimalTail(parts[1:]) {
			return false, false
		}
		return true, true
	}
	parts := strings.Split(rest, ":")
	if len(parts) < 2 || !isDigitsAndUnderscores(parts[0], true) || !isValidSexagesimalTail(parts[1:]) {
		return false, false
	}
	return false, true
}

func matchLegacyOctalNoUnderscore(raw string) bool {
	digits := raw
	if len(digits) > 0 && (digits[0] == '+' || digits[0] == '-') {
		digits = digits[1:]
	}
	if len(digits) < 3 || digits[0] != '0' {
		return false
	}
	hasMidNonZero := false
	for i := 0; i < len(digits); i++ {
		b := digits[i]
		if b < '0' || b > '7' {
			return false
		}
		if i >= 1 && i < len(digits)-1 && b >= '1' && b <= '7' {
			hasMidNonZero = true
		}
	}
	return hasMidNonZero
}

func matchBinaryInt(raw string) bool {
	rest := raw
	if len(rest) > 0 && (rest[0] == '+' || rest[0] == '-') {
		rest = rest[1:]
	}
	if !strings.HasPrefix(rest, "0b") {
		return false
	}
	bin := rest[2:]
	if len(bin) == 0 {
		return false
	}
	hasBit := false
	for i := 0; i < len(bin); i++ {
		switch bin[i] {
		case '0', '1':
			hasBit = true
		case '_':
		default:
			return false
		}
	}
	return hasBit
}

func isHexDigit(b byte) bool {
	return (b >= '0' && b <= '9') || (b >= 'a' && b <= 'f') || (b >= 'A' && b <= 'F')
}

func matchSignedHexInt(raw string) bool {
	if len(raw) == 0 || (raw[0] != '+' && raw[0] != '-') {
		return false
	}
	rest := raw[1:]
	if !strings.HasPrefix(rest, "0x") {
		return false
	}
	hex := rest[2:]
	if len(hex) == 0 {
		return false
	}
	hasHex := false
	for i := 0; i < len(hex); i++ {
		b := hex[i]
		if isHexDigit(b) {
			hasHex = true
		} else if b != '_' {
			return false
		}
	}
	return hasHex
}

func matchUnderscoreNumber(raw string) bool {
	if strings.IndexByte(raw, '_') < 0 {
		return false
	}
	if strings.HasPrefix(raw, "0x") {
		hex := raw[2:]
		if len(hex) == 0 {
			return false
		}
		hasHex := false
		for i := 0; i < len(hex); i++ {
			b := hex[i]
			if isHexDigit(b) {
				hasHex = true
			} else if b != '_' {
				return false
			}
		}
		return hasHex
	}
	rest := raw
	if len(rest) > 0 && (rest[0] == '+' || rest[0] == '-') {
		rest = rest[1:]
	}
	// Legacy octal with '_'
	if len(rest) >= 2 && rest[0] == '0' {
		allOct := true
		for i := 0; i < len(rest); i++ {
			b := rest[i]
			if (b < '0' || b > '7') && b != '_' {
				allOct = false
				break
			}
		}
		if allOct {
			return true
		}
	}
	// Decimal int with '_'
	if isDigitsAndUnderscores(rest, true) {
		return true
	}
	// Decimal float with '_'
	mant := rest
	var exp string
	hasExp := false
	if eIdx := strings.IndexAny(rest, "eE"); eIdx >= 0 {
		mant = rest[:eIdx]
		exp = rest[eIdx+1:]
		hasExp = true
	}
	if hasExp {
		if len(exp) < 2 || (exp[0] != '+' && exp[0] != '-') {
			return false
		}
		for i := 1; i < len(exp); i++ {
			if exp[i] < '0' || exp[i] > '9' {
				return false
			}
		}
	}
	dotIdx := strings.IndexByte(mant, '.')
	if dotIdx < 0 {
		return false
	}
	intPart := mant[:dotIdx]
	fracPart := mant[dotIdx+1:]
	if len(intPart) == 0 {
		return isDigitsAndUnderscores(fracPart, false)
	}
	if !isDigitsAndUnderscores(intPart, false) {
		return false
	}
	for i := 0; i < len(fracPart); i++ {
		b := fracPart[i]
		if (b < '0' || b > '9') && b != '_' {
			return false
		}
	}
	return true
}

type boundedBuf struct {
	buf  []byte
	max  int
	over bool
}

func (b *boundedBuf) writeStr(t string) bool {
	if b.over {
		return false
	}
	room := b.max - len(b.buf)
	if len(t) <= room {
		b.buf = append(b.buf, t...)
		return true
	}
	i := room
	for i > 0 && !utf8.RuneStart(t[i]) {
		i--
	}
	b.buf = append(b.buf, t[:i]...)
	b.over = true
	return false
}

func (b *boundedBuf) writeJsonEscaped(s string) bool {
	if !b.writeStr(`"`) {
		return false
	}
	const hexDigits = "0123456789abcdef"
	var hexEsc = [6]byte{'\\', 'u', '0', '0', '0', '0'}
	run := 0
	for i := 0; i < len(s); i++ {
		if i-run > b.max-len(b.buf) {
			return b.writeStr(s[run:])
		}
		c := s[i]
		if c >= 0x20 && c != '"' && c != '\\' {
			continue
		}
		if run < i {
			if !b.writeStr(s[run:i]) {
				return false
			}
		}
		var esc string
		switch c {
		case '"':
			esc = `\"`
		case '\\':
			esc = `\\`
		case '\b':
			esc = `\b`
		case '\f':
			esc = `\f`
		case '\n':
			esc = `\n`
		case '\r':
			esc = `\r`
		case '\t':
			esc = `\t`
		default:
			hexEsc[4] = hexDigits[c>>4]
			hexEsc[5] = hexDigits[c&0x0f]
			if b.over {
				return false
			}
			room := b.max - len(b.buf)
			if 6 <= room {
				b.buf = append(b.buf, hexEsc[:]...)
			} else {
				b.buf = append(b.buf, hexEsc[:room]...)
				b.over = true
				return false
			}
			run = i + 1
			continue
		}
		if !b.writeStr(esc) {
			return false
		}
		run = i + 1
	}
	if run < len(s) {
		if !b.writeStr(s[run:]) {
			return false
		}
	}
	return b.writeStr(`"`)
}

func (b *boundedBuf) finish() string {
	if b.over {
		cut := b.max - len(truncatedMark)
		if cut < 0 {
			return ""
		}
		if cut > len(b.buf) {
			cut = len(b.buf)
		}
		for cut > 0 && !utf8.RuneStart(b.buf[cut]) {
			cut--
		}
		b.buf = append(b.buf[:cut], truncatedMark...)
	}
	return string(b.buf)
}

func formatStringResolution(raw string) string {
	var storage [MaxFieldBytes]byte
	b := boundedBuf{buf: storage[:0], max: MaxFieldBytes}
	b.writeStr("string (")
	b.writeJsonEscaped(raw)
	b.writeStr(")")
	return b.finish()
}

func formatOctalResolution(raw string, resolvedVal Value) string {
	var storage [MaxFieldBytes]byte
	b := boundedBuf{buf: storage[:0], max: MaxFieldBytes}
	b.writeStr("int (")
	if resolvedVal.kind == ValueKindInt {
		b.writeStr(resolvedVal.i.String())
	} else {
		neg := strings.HasPrefix(raw, "-")
		digits := raw
		if len(digits) > 0 && (digits[0] == '+' || digits[0] == '-') {
			digits = digits[1:]
		}
		digits = strings.TrimLeft(digits, "0")
		if digits == "" {
			b.writeStr("0")
		} else {
			if neg {
				b.writeStr("-")
			}
			b.writeStr(digits)
		}
	}
	b.writeStr(")")
	return b.finish()
}

func formatLintMessage(code LintCode, rawInput, yaml12Resolution, yaml11Note string) string {
	var storage [MaxMessageBytes]byte
	b := boundedBuf{buf: storage[:0], max: MaxMessageBytes}
	b.writeStr("YAML 1.1 compatibility hazard (")
	b.writeStr(code.String())
	b.writeStr("): unquoted ")
	b.writeJsonEscaped(rawInput)
	b.writeStr(" resolves as ")
	b.writeStr(yaml12Resolution)
	b.writeStr(" in YAML 1.2 Core (")
	b.writeStr(yaml11Note)
	b.writeStr(")")
	return b.finish()
}

type lintCollector struct {
	result LintResult
}

func (c *lintCollector) admitRetentionAndPath(l *loader, span Span) (bool, *Error) {
	if c.result.WarningCount < int(^uint(0)>>1) {
		c.result.WarningCount++
	}
	if len(c.result.Diagnostics) >= l.opts.Limits.MaxWarnings {
		c.result.Truncated = true
		return false, nil
	}
	// Step 1: Charge each PathSegment::Key in l.path before formatting or cloning.
	for i := range l.path {
		seg := &l.path[i]
		if seg.Kind == PathSegmentIndex {
			continue
		}
		kNodes, kWeight, limitKind := measurePathKey(&seg.Key, l.opts.Limits)
		if limitKind != "" {
			configured := l.opts.Limits.MaxKeyBytes
			if limitKind == LimitMaxDepth {
				configured = l.opts.Limits.MaxDepth
			}
			err := errLimit(limitKind, configured, span.Start)
			err.Path = l.path
			l.path = nil
			return false, err
		}
		if kind, configured, ok := l.budget.charge(kNodes, kWeight, false); !ok {
			err := errLimit(kind, configured, span.Start)
			err.Path = l.path
			l.path = nil
			return false, err
		}
	}
	return true, nil
}

func (c *lintCollector) recordMergeKey(l *loader, span Span) *Error {
	retained, err := c.admitRetentionAndPath(l, span)
	if err != nil || !retained {
		return err
	}
	var res string
	switch l.opts.MergeKeys {
	case MergeKeysReject:
		res = `rejected (MergeKeys::Reject default; literal string "<<" under MergeKeys::AsKey)`
	case MergeKeysApply:
		res = `merge applied (MergeKeys::Apply opt-in; not part of YAML 1.2.2 Core)`
	case MergeKeysAsKey:
		res = `string ("<<" under MergeKeys::AsKey)`
	}
	note := "YAML 1.1 merge key specification (https://yaml.org/type/merge.html)"
	return c.commitRetained(l, LintCodeYaml11MergeKey, span, "<<", res, note)
}

func (c *lintCollector) recordScalar(l *loader, hazard scalarHazard, raw string, resolvedVal Value, span Span) *Error {
	retained, err := c.admitRetentionAndPath(l, span)
	if err != nil || !retained {
		return err
	}
	var code LintCode
	var res, note string
	switch hazard {
	case hazardBooleanTruthy:
		code = LintCodeYaml11Boolean
		res = formatStringResolution(raw)
		note = "YAML 1.1 boolean literal (true)"
	case hazardBooleanFalsity:
		code = LintCodeYaml11Boolean
		res = formatStringResolution(raw)
		note = "YAML 1.1 boolean literal (false)"
	case hazardSexagesimalInt:
		code = LintCodeYaml11Sexagesimal
		res = formatStringResolution(raw)
		note = "YAML 1.1 sexagesimal integer literal"
	case hazardSexagesimalFloat:
		code = LintCodeYaml11Sexagesimal
		res = formatStringResolution(raw)
		note = "YAML 1.1 sexagesimal float literal"
	case hazardLegacyOctal:
		code = LintCodeYaml11Number
		res = formatOctalResolution(raw, resolvedVal)
		note = "YAML 1.1 base-8 (octal) integer literal; use 0o prefix for octal in YAML 1.2 or quote as string"
	case hazardBinaryInt:
		code = LintCodeYaml11Number
		res = formatStringResolution(raw)
		note = "YAML 1.1 base-2 (binary) integer literal"
	case hazardSignedHexInt:
		code = LintCodeYaml11Number
		res = formatStringResolution(raw)
		note = "YAML 1.1 signed hexadecimal integer literal"
	case hazardUnderscoreNumber:
		code = LintCodeYaml11Number
		res = formatStringResolution(raw)
		note = "YAML 1.1 underscore-grouped numeric literal"
	default:
		return nil
	}
	return c.commitRetained(l, code, span, raw, res, note)
}

func (c *lintCollector) commitRetained(
	l *loader,
	code LintCode,
	span Span,
	rawInput string,
	yaml12Resolution string,
	yaml11Note string,
) *Error {
	// Step 2: Format bounded strings and charge the diagnostic itself before cloning path.
	raw := boundedString(rawInput, MaxFieldBytes)
	msg := formatLintMessage(code, rawInput, yaml12Resolution, yaml11Note)
	diagWeight := satAdd64(
		64,
		satAdd64(
			satMul64(8, uint64(len(l.path))),
			uint64(len(raw)+len(yaml12Resolution)+len(yaml11Note)+len(msg)),
		),
	)
	if kind, configured, ok := l.budget.charge(1, diagWeight, false); !ok {
		err := errLimit(kind, configured, span.Start)
		err.Path = l.path
		l.path = nil
		return err
	}
	if lintPathCloneHook != nil {
		lintPathCloneHook()
	}
	clonedPath := cloneLintPath(l.path)
	c.result.Diagnostics = append(c.result.Diagnostics, LintDiagnostic{
		Code:             code,
		Span:             span,
		DocumentIndex:    l.docIndex,
		Path:             clonedPath,
		Raw:              raw,
		YAML12Resolution: yaml12Resolution,
		YAML11Note:       yaml11Note,
		Message:          msg,
	})
	return nil
}

func measurePathKey(k *Value, l Limits) (uint64, uint64, LimitKind) {
	type stackItem struct {
		val   *Value
		depth int
	}
	nodes := uint64(1)
	weight := NodeOverhead
	if weight > uint64(l.MaxKeyBytes) {
		return 0, 0, LimitMaxKeyBytes
	}
	stack := []stackItem{{val: k, depth: 0}}
	for len(stack) > 0 {
		top := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		v := top.val
		depth := top.depth

		var moreNodes, moreWeight uint64
		var children int
		switch v.kind {
		case ValueKindString:
			moreWeight = uint64(len(v.s))
		case ValueKindTagged:
			if v.tagged != nil {
				moreNodes = 1
				moreWeight = satAdd64(tagBytes(v.tagged.tag), NodeOverhead)
				children = 1
			}
		case ValueKindSequence:
			moreNodes = uint64(len(v.seq))
			moreWeight = satMul64(NodeOverhead, uint64(len(v.seq)))
			children = len(v.seq)
		case ValueKindMapping:
			n := v.m.Len()
			moreNodes = satMul64(2, uint64(n))
			moreWeight = satMul64(2*NodeOverhead+EntryOverhead, uint64(n))
			children = 2 * n
		}
		nodes = satAdd64(nodes, moreNodes)
		weight = satAdd64(weight, moreWeight)
		if weight > uint64(l.MaxKeyBytes) {
			return 0, 0, LimitMaxKeyBytes
		}
		if children > 0 && v.kind != ValueKindTagged && depth+1 > l.MaxDepth {
			return 0, 0, LimitMaxDepth
		}
		switch v.kind {
		case ValueKindTagged:
			if v.tagged != nil {
				stack = append(stack, stackItem{val: &v.tagged.value.value, depth: depth})
			}
		case ValueKindSequence:
			for j := range v.seq {
				stack = append(stack, stackItem{val: &v.seq[j].value, depth: depth + 1})
			}
		case ValueKindMapping:
			if v.m != nil {
				for j := range v.m.entries {
					stack = append(stack, stackItem{val: &v.m.entries[j].Key.value, depth: depth + 1})
					stack = append(stack, stackItem{val: &v.m.entries[j].Value.value, depth: depth + 1})
				}
			}
		}
	}
	return nodes, weight, ""
}

func cloneLintPath(path []PathSegment) []PathSegment {
	if len(path) == 0 {
		return []PathSegment{}
	}
	out := make([]PathSegment, len(path))
	for i := range path {
		if path[i].Kind == PathSegmentIndex {
			out[i] = PathIndex(path[i].Index)
		} else {
			out[i] = PathSegment{
				Kind: PathSegmentKey,
				Key:  stripValueLocations(path[i].Key),
			}
		}
	}
	return out
}

func stripValueLocations(v Value) Value {
	switch v.kind {
	case ValueKindNull, ValueKindBool, ValueKindInt, ValueKindFloat, ValueKindString:
		return v
	default:
		n := stripNodeLocations(newSyntheticNodeOwned(v))
		return n.value
	}
}

func stripNodeLocations(root Node) Node {
	const (
		opNode uint8 = iota
		opFinishSeq
		opFinishMap
		opFinishTagged
	)
	type workFrame struct {
		op         uint8
		node       Node
		count      int
		tag        Tag
		innerPlain bool
	}
	work := []workFrame{{op: opNode, node: root}}
	var results []Node
	for len(work) > 0 {
		f := work[len(work)-1]
		work = work[:len(work)-1]
		switch f.op {
		case opNode:
			v := f.node.value
			switch v.kind {
			case ValueKindNull, ValueKindBool, ValueKindInt, ValueKindFloat, ValueKindString:
				results = append(results, newSyntheticNodeOwned(v))
			case ValueKindSequence:
				work = append(work, workFrame{op: opFinishSeq, count: len(v.seq)})
				for i := len(v.seq) - 1; i >= 0; i-- {
					work = append(work, workFrame{op: opNode, node: v.seq[i]})
				}
			case ValueKindMapping:
				var entries []MappingEntry
				if v.m != nil {
					entries = v.m.entries
				}
				work = append(work, workFrame{op: opFinishMap, count: len(entries)})
				for i := len(entries) - 1; i >= 0; i-- {
					work = append(work,
						workFrame{op: opNode, node: entries[i].Value},
						workFrame{op: opNode, node: entries[i].Key},
					)
				}
			case ValueKindTagged:
				if v.tagged == nil {
					results = append(results, newSyntheticNodeOwned(NullValue()))
				} else {
					work = append(work,
						workFrame{op: opFinishTagged, tag: v.tagged.tag, innerPlain: v.tagged.innerPlain},
						workFrame{op: opNode, node: v.tagged.value},
					)
				}
			}
		case opFinishSeq:
			start := len(results) - f.count
			items := make([]Node, f.count)
			copy(items, results[start:])
			results = results[:start]
			results = append(results, newSyntheticNodeOwned(sequenceValueOwned(items)))
		case opFinishMap:
			start := len(results) - 2*f.count
			tail := results[start:]
			results = results[:start]
			m := NewMapping()
			for i := 0; i < len(tail); i += 2 {
				_ = m.insertNewOwned(tail[i], tail[i+1])
			}
			results = append(results, newSyntheticNodeOwned(mappingValueOwned(m)))
		case opFinishTagged:
			inner := results[len(results)-1]
			results = results[:len(results)-1]
			results = append(results, newSyntheticNodeOwned(taggedValueOwned(newTaggedWithPlain(f.tag, inner, f.innerPlain))))
		}
	}
	return results[len(results)-1]
}

// LintYAML11 inspects input for unquoted YAML 1.1 compatibility hazards using DefaultOptions() (spec-v1.6 §10.3).
func LintYAML11(input string) (LintResult, *Error) {
	return LintYAML11With(input, DefaultOptions())
}

// LintYAML11With inspects input for unquoted YAML 1.1 compatibility hazards using opts (spec-v1.6 §10.3).
func LintYAML11With(input string, opts Options) (LintResult, *Error) {
	return LintYAML11Bytes([]byte(input), opts)
}

// LintYAML11Bytes inspects raw bytes for unquoted YAML 1.1 compatibility hazards using opts (spec-v1.6 §10.3).
func LintYAML11Bytes(input []byte, opts Options) (LintResult, *Error) {
	if err := opts.Validate(); err != nil {
		return LintResult{}, err
	}
	eOpts := engine.DefaultOptions()
	eOpts.Limits.MaxInputBytes = opts.Limits.MaxInputBytes
	eOpts.Limits.MaxDepth = opts.Limits.MaxDepth
	s, bom, ee := engine.GateBytes(input, eOpts)
	if ee != nil {
		return LintResult{}, fromEngineError(ee)
	}
	l := newLoader(s, bom, opts)
	l.lint = &lintCollector{
		result: LintResult{
			Diagnostics: []LintDiagnostic{},
		},
	}
	if _, err := l.loadStream(false, true); err != nil {
		return LintResult{}, err
	}
	return l.lint.result, nil
}

// LintYAML11Reader reads from r and inspects the stream for unquoted YAML 1.1 compatibility hazards using DefaultOptions() (spec-v1.6 §10.3).
func LintYAML11Reader(r io.Reader) (LintResult, *Error) {
	return LintYAML11ReaderWith(r, DefaultOptions())
}

// LintYAML11ReaderWith reads from r up to MaxInputBytes+1 bytes and inspects the stream for unquoted YAML 1.1 compatibility hazards using opts (spec-v1.6 §10.3).
func LintYAML11ReaderWith(r io.Reader, opts Options) (LintResult, *Error) {
	buf, err := readBounded(r, opts)
	if err != nil {
		return LintResult{}, err
	}
	return LintYAML11Bytes(buf, opts)
}

func validateLintLoc(loc Location) bool {
	return loc.ByteOffset >= 0 && uint64(loc.ByteOffset) <= MaxWireInt &&
		loc.Line >= 1 && uint64(loc.Line) <= MaxWireInt &&
		loc.Column >= 1 && uint64(loc.Column) <= MaxWireInt
}

func validateLintSpan(span Span) bool {
	if !validateLintLoc(span.Start) || !validateLintLoc(span.End) {
		return false
	}
	if span.Start.ByteOffset > span.End.ByteOffset {
		return false
	}
	return span.Start.Line < span.End.Line ||
		(span.Start.Line == span.End.Line && span.Start.Column <= span.End.Column)
}

// EncodeLint serializes result into a canonical yamlprims-lint/1 JSON envelope using DefaultOptions() (spec-v1.6 §11.7).
func EncodeLint(result LintResult) (string, *Error) {
	return EncodeLintWith(result, DefaultOptions())
}

// EncodeLintWith serializes result into a canonical yamlprims-lint/1 JSON envelope using opts (spec-v1.6 §11.7).
func EncodeLintWith(result LintResult, opts Options) (string, *Error) {
	if err := opts.Validate(); err != nil {
		return "", err
	}
	if len(result.Diagnostics) > opts.Limits.MaxWarnings {
		return "", errInvalidOptions("LintResult diagnostics length exceeds max_warnings")
	}
	if result.Truncated {
		if result.WarningCount <= len(result.Diagnostics) {
			return "", errInvalidOptions("truncated LintResult must have warning_count > len(diagnostics)")
		}
	} else if result.WarningCount != len(result.Diagnostics) {
		return "", errInvalidOptions("non-truncated LintResult must have warning_count == len(diagnostics)")
	}
	if result.WarningCount < 0 || uint64(result.WarningCount) > MaxWireInt {
		return "", errInvalidOptions("LintResult warning_count out of range")
	}
	for i := range result.Diagnostics {
		d := &result.Diagnostics[i]
		if !d.Code.IsValid() {
			return "", errInvalidOptions("invalid LintCode")
		}
		if d.DocumentIndex < 0 || uint64(d.DocumentIndex) > MaxWireInt {
			return "", errInvalidOptions("LintDiagnostic document_index out of range")
		}
		if !validateLintSpan(d.Span) {
			return "", errInvalidOptions("LintDiagnostic span has invalid coordinates")
		}
		for j := range d.Path {
			seg := &d.Path[j]
			if seg.Kind == PathSegmentIndex && (seg.Index < 0 || uint64(seg.Index) > MaxWireInt) {
				return "", errInvalidOptions("LintDiagnostic path index out of range")
			}
		}
		if err := preflightPath(d.Path, opts.Limits); err != nil {
			return "", err
		}
		if !utf8.ValidString(d.Raw) || !utf8.ValidString(d.YAML12Resolution) ||
			!utf8.ValidString(d.YAML11Note) || !utf8.ValidString(d.Message) {
			return "", newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
		}
	}

	o := newOutBuf(opts.Limits.MaxOutputBytes)
	if err := o.push(`{"format":"yamlprims-lint","version":1,"warning_count":`); err != nil {
		return "", err
	}
	if err := o.pushInt(result.WarningCount); err != nil {
		return "", err
	}
	if result.Truncated {
		if err := o.push(`,"truncated":true,"diagnostics":[`); err != nil {
			return "", err
		}
	} else {
		if err := o.push(`,"truncated":false,"diagnostics":[`); err != nil {
			return "", err
		}
	}
	for i := range result.Diagnostics {
		if i > 0 {
			if err := o.push(","); err != nil {
				return "", err
			}
		}
		d := &result.Diagnostics[i]
		if err := o.push(`{"code":`); err != nil {
			return "", err
		}
		if err := o.string(d.Code.String()); err != nil {
			return "", err
		}
		if err := o.push(`,"span":`); err != nil {
			return "", err
		}
		if err := o.span(d.Span); err != nil {
			return "", err
		}
		if err := o.push(`,"document_index":`); err != nil {
			return "", err
		}
		if err := o.pushInt(d.DocumentIndex); err != nil {
			return "", err
		}
		if err := o.push(`,"path":[`); err != nil {
			return "", err
		}
		if err := writePathSegments(&o, d.Path); err != nil {
			return "", err
		}
		if err := o.push(`],"raw":`); err != nil {
			return "", err
		}
		if err := o.boundedString(d.Raw, MaxFieldBytes); err != nil {
			return "", err
		}
		if err := o.push(`,"yaml12_resolution":`); err != nil {
			return "", err
		}
		if err := o.boundedString(d.YAML12Resolution, MaxFieldBytes); err != nil {
			return "", err
		}
		if err := o.push(`,"yaml11_note":`); err != nil {
			return "", err
		}
		if err := o.boundedString(d.YAML11Note, MaxFieldBytes); err != nil {
			return "", err
		}
		if err := o.push(`,"message":`); err != nil {
			return "", err
		}
		if err := o.boundedString(d.Message, MaxMessageBytes); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	}
	if err := o.push("]}"); err != nil {
		return "", err
	}
	return string(o.buf), nil
}
