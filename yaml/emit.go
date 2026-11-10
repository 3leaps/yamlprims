package yaml

import (
	"fmt"
	"math"
	"strings"
	"unicode/utf8"
)

// CompatQuoting controls plain-scalar compatibility quoting (spec-v1.5 §10.2, ADR-0005 §2).
type CompatQuoting uint8

const (
	// CompatQuotingYaml11Safe (default) quotes YAML 1.1 scalar hazards in addition to §4.1 Core non-strings.
	CompatQuotingYaml11Safe CompatQuoting = iota
	// CompatQuotingCoreOnly quotes only §4.1 Core non-strings and YAML 1.2.2 lexical hazards.
	CompatQuotingCoreOnly
)

// EmitLimits bounds resource usage during YAML emission (spec-v1.5 §10.2).
type EmitLimits struct {
	MaxDepth          int
	MaxNodes          int
	MaxScalarBytes    int
	MaxKeyBytes       int
	MaxMappingEntries int
	MaxTagBytes       int
	MaxDocuments      int
	MaxOutputBytes    int
}

// DefaultEmitLimits returns the normative default EmitLimits (spec-v1.5 §10.2).
func DefaultEmitLimits() EmitLimits {
	return EmitLimits{
		MaxDepth:          128,
		MaxNodes:          250_000,
		MaxScalarBytes:    1024 * 1024,
		MaxKeyBytes:       64 * 1024,
		MaxMappingEntries: 100_000,
		MaxTagBytes:       1_024,
		MaxDocuments:      1_024,
		MaxOutputBytes:    8 * 1024 * 1024,
	}
}

// EmitOptions configures EmitWith, EmitAllWith, FromPlain, and Marshal (spec-v1.5 §10.2).
type EmitOptions struct {
	CompatQuoting CompatQuoting
	Limits        EmitLimits
}

// DefaultEmitOptions returns the normative default EmitOptions (spec-v1.5 §10.2).
func DefaultEmitOptions() EmitOptions {
	return EmitOptions{
		CompatQuoting: CompatQuotingYaml11Safe,
		Limits:        DefaultEmitLimits(),
	}
}

// Validate checks that all EmitOptions fields satisfy spec-v1.5 §10.2 invariants.
func (o EmitOptions) Validate() *Error {
	bad := func(msg string) *Error {
		return newNotSourceError(ErrorKindInvalidOptions, ErrorDetail{}, nil, msg)
	}
	if o.CompatQuoting != CompatQuotingYaml11Safe && o.CompatQuoting != CompatQuotingCoreOnly {
		return bad("invalid CompatQuoting policy")
	}
	l := o.Limits
	if l.MaxDepth < 1 || l.MaxDepth > 255 {
		return bad("max_depth must be in 1..=255")
	}
	for _, pair := range []struct {
		name string
		v    int
	}{
		{"max_nodes", l.MaxNodes},
		{"max_scalar_bytes", l.MaxScalarBytes},
		{"max_key_bytes", l.MaxKeyBytes},
		{"max_mapping_entries", l.MaxMappingEntries},
		{"max_tag_bytes", l.MaxTagBytes},
		{"max_documents", l.MaxDocuments},
		{"max_output_bytes", l.MaxOutputBytes},
	} {
		if pair.v < 1 {
			return bad(fmt.Sprintf("%s must be >= 1", pair.name))
		}
	}
	if l.MaxScalarBytes > l.MaxOutputBytes {
		return bad("max_scalar_bytes must be <= max_output_bytes")
	}
	if l.MaxKeyBytes > l.MaxOutputBytes {
		return bad("max_key_bytes must be <= max_output_bytes")
	}
	return nil
}

func emitSimpleErr(kind ErrorKind, msg string) *Error {
	return newNotSourceError(kind, ErrorDetail{}, nil, msg)
}

func emitLimitErr(limit LimitKind, configured int, msg string) *Error {
	return newNotSourceError(
		ErrorKindLimitExceeded,
		ErrorDetail{
			Limit:         limit,
			Configured:    uint64(configured),
			HasConfigured: true,
		},
		nil,
		fmt.Sprintf("%s (%d)", msg, configured),
	)
}

func emitInvalidUtf8Err(msg string) *Error {
	return newNotSourceError(
		ErrorKindInvalidUtf8,
		ErrorDetail{HasInvalidUtf8: true},
		nil,
		msg,
	)
}

const (
	emitPFNode uint8 = iota
	emitPFSeq
	emitPFMap
)

type emitPreflightFrame struct {
	op        uint8
	node      *Node
	seq       []Node
	entries   []MappingEntry
	idx       int
	nextVal   *Node
	depth     int
	inKey     bool
	isKeyRoot bool
}

func checkEmitKeyWeight(keyRoot *Node, maxKeyBytes int) *Error {
	total := uint64(0)
	maxU := uint64(maxKeyBytes)
	stack := []emitPreflightFrame{{op: emitPFNode, node: keyRoot}}
	for len(stack) > 0 {
		f := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		var n *Node
		switch f.op {
		case emitPFNode:
			n = f.node
		case emitPFSeq:
			if f.idx >= len(f.seq) {
				continue
			}
			n = &f.seq[f.idx]
			if f.idx+1 < len(f.seq) {
				f.idx++
				stack = append(stack, f)
			}
		case emitPFMap:
			if f.nextVal != nil {
				n = f.nextVal
				f.nextVal = nil
				if f.idx+1 < len(f.entries) {
					f.idx++
					stack = append(stack, f)
				}
			} else {
				if f.idx >= len(f.entries) {
					continue
				}
				e := &f.entries[f.idx]
				n = &e.Key
				f.nextVal = &e.Value
				stack = append(stack, f)
			}
		}
		total = satAdd64(total, NodeOverhead)
		if total > maxU {
			return emitLimitErr(LimitMaxKeyBytes, maxKeyBytes, "mapping key exceeds max_key_bytes")
		}
		switch n.value.kind {
		case ValueKindNull, ValueKindBool, ValueKindInt, ValueKindFloat:
		case ValueKindString:
			total = satAdd64(total, uint64(len(n.value.s)))
			if total > maxU {
				return emitLimitErr(LimitMaxKeyBytes, maxKeyBytes, "mapping key exceeds max_key_bytes")
			}
		case ValueKindSequence:
			if len(n.value.seq) > 0 {
				stack = append(stack, emitPreflightFrame{op: emitPFSeq, seq: n.value.seq})
			}
		case ValueKindMapping:
			if n.value.m != nil {
				total = satAdd64(total, satMul64(uint64(len(n.value.m.entries)), EntryOverhead))
				if total > maxU {
					return emitLimitErr(LimitMaxKeyBytes, maxKeyBytes, "mapping key exceeds max_key_bytes")
				}
				if len(n.value.m.entries) > 0 {
					stack = append(stack, emitPreflightFrame{op: emitPFMap, entries: n.value.m.entries})
				}
			}
		case ValueKindTagged:
			if n.value.tagged != nil {
				total = satAdd64(total, satMul64(uint64(len(n.value.tagged.tag.uri)), 2))
				if total > maxU {
					return emitLimitErr(LimitMaxKeyBytes, maxKeyBytes, "mapping key exceeds max_key_bytes")
				}
				stack = append(stack, emitPreflightFrame{op: emitPFNode, node: &n.value.tagged.value})
			}
		}
	}
	return nil
}

func preflightEmitDocuments(docs []Document, opts EmitOptions) *Error {
	if err := opts.Validate(); err != nil {
		return err
	}
	if len(docs) > opts.Limits.MaxDocuments {
		return emitLimitErr(LimitMaxDocuments, opts.Limits.MaxDocuments, "document count exceeds max_documents")
	}
	nodesVisited := 0
	var stack []emitPreflightFrame
	for i := range docs {
		doc := &docs[i]
		if doc.yamlVersion != nil {
			major, minor := doc.yamlVersion[0], doc.yamlVersion[1]
			if major != 1 || minor > 999_999_999 {
				return emitSimpleErr(
					ErrorKindUnsupportedYamlVersion,
					fmt.Sprintf("unsupported YAML directive version %d.%d", major, minor),
				)
			}
		}
		if err := preflightEmitNode(&doc.root, opts, &nodesVisited, &stack); err != nil {
			return err
		}
	}
	return nil
}

func preflightEmitNode(root *Node, opts EmitOptions, nodesVisited *int, stack *[]emitPreflightFrame) *Error {
	*stack = (*stack)[:0]
	*stack = append(*stack, emitPreflightFrame{
		op:        emitPFNode,
		node:      root,
		depth:     0,
		inKey:     false,
		isKeyRoot: false,
	})

	for len(*stack) > 0 {
		f := (*stack)[len(*stack)-1]
		*stack = (*stack)[:len(*stack)-1]

		var node *Node
		var depth int
		var inKey, isKeyRoot bool
		switch f.op {
		case emitPFNode:
			node, depth, inKey, isKeyRoot = f.node, f.depth, f.inKey, f.isKeyRoot
		case emitPFSeq:
			if f.idx >= len(f.seq) {
				continue
			}
			node, depth, inKey, isKeyRoot = &f.seq[f.idx], f.depth, f.inKey, false
			if f.idx+1 < len(f.seq) {
				f.idx++
				*stack = append(*stack, f)
			}
		case emitPFMap:
			if f.nextVal != nil {
				node, depth, inKey, isKeyRoot = f.nextVal, f.depth, f.inKey, false
				f.nextVal = nil
				if f.idx+1 < len(f.entries) {
					f.idx++
					*stack = append(*stack, f)
				}
			} else {
				if f.idx >= len(f.entries) {
					continue
				}
				e := &f.entries[f.idx]
				node, depth, inKey, isKeyRoot = &e.Key, f.depth, true, true
				f.nextVal = &e.Value
				*stack = append(*stack, f)
			}
		}

		*nodesVisited++
		if *nodesVisited > opts.Limits.MaxNodes {
			return emitLimitErr(LimitMaxNodes, opts.Limits.MaxNodes, "node count exceeds max_nodes")
		}
		if isKeyRoot {
			if err := checkEmitKeyWeight(node, opts.Limits.MaxKeyBytes); err != nil {
				return err
			}
		}

		v := &node.value
		switch v.kind {
		case ValueKindNull, ValueKindBool, ValueKindInt:
		case ValueKindFloat:
			if inKey && math.IsNaN(v.f) {
				return emitSimpleErr(ErrorKindInvalidKey, "NaN float is not valid inside a mapping key")
			}
		case ValueKindString:
			if !utf8.ValidString(v.s) {
				return emitInvalidUtf8Err("string value is not valid UTF-8")
			}
			if len(v.s) > opts.Limits.MaxScalarBytes {
				return emitLimitErr(LimitMaxScalarBytes, opts.Limits.MaxScalarBytes, "scalar byte length exceeds max_scalar_bytes")
			}
		case ValueKindTagged:
			if v.tagged == nil {
				return emitSimpleErr(ErrorKindInvalidOptions, "nil Tagged payload")
			}
			t := v.tagged
			if !utf8.ValidString(t.tag.uri) || !utf8.ValidString(t.tag.handle) || !utf8.ValidString(t.tag.suffix) {
				return emitInvalidUtf8Err("Tag contains invalid UTF-8")
			}
			if len(t.tag.uri) > opts.Limits.MaxTagBytes || !isValidNonCoreTagURI(t.tag.uri) {
				return emitSimpleErr(ErrorKindInvalidOptions, "Tagged node contains invalid or oversized Tag.URI")
			}
			innerKind := t.value.value.kind
			if t.tag.uri == "tag:yaml.org,2002:merge" {
				if inKey || innerKind != ValueKindString {
					return emitSimpleErr(ErrorKindTagKindMismatch, "tag:yaml.org,2002:merge may only wrap a non-key String node")
				}
			} else if innerKind != ValueKindString && innerKind != ValueKindSequence && innerKind != ValueKindMapping {
				return emitSimpleErr(ErrorKindTagKindMismatch, "Tagged node must wrap String, Sequence, or Mapping")
			}
			*stack = append(*stack, emitPreflightFrame{
				op:        emitPFNode,
				node:      &t.value,
				depth:     depth,
				inKey:     inKey,
				isKeyRoot: false,
			})
		case ValueKindSequence:
			nextDepth := depth + 1
			if nextDepth > opts.Limits.MaxDepth {
				return emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "collection nesting exceeds max_depth")
			}
			if len(v.seq) > 0 {
				*stack = append(*stack, emitPreflightFrame{
					op:    emitPFSeq,
					seq:   v.seq,
					idx:   0,
					depth: nextDepth,
					inKey: inKey,
				})
			}
		case ValueKindMapping:
			nextDepth := depth + 1
			if nextDepth > opts.Limits.MaxDepth {
				return emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "collection nesting exceeds max_depth")
			}
			if v.m != nil {
				if len(v.m.entries) > opts.Limits.MaxMappingEntries {
					return emitLimitErr(LimitMaxMappingEntries, opts.Limits.MaxMappingEntries, "mapping entry count exceeds max_mapping_entries")
				}
				if len(v.m.entries) > 0 {
					*stack = append(*stack, emitPreflightFrame{
						op:      emitPFMap,
						entries: v.m.entries,
						idx:     0,
						depth:   nextDepth,
						inKey:   inKey,
					})
				}
			}
		}
	}
	return nil
}

func formatYAMLFloat(f float64) string {
	if math.IsNaN(f) {
		return ".nan"
	}
	if math.IsInf(f, 0) {
		if f < 0 {
			return "-.inf"
		}
		return ".inf"
	}
	if f == 0 {
		if math.Signbit(f) {
			return "-0.0"
		}
		return "0.0"
	}
	var b strings.Builder
	b.Grow(24)
	if math.Signbit(f) {
		b.WriteByte('-')
	}
	digits, k := decomposeShortestFloat(math.Abs(f))
	n := len(digits)
	sciExp := k - 1
	if sciExp >= -6 && sciExp < 21 {
		if k >= n {
			b.WriteString(digits)
			for i := 0; i < k-n; i++ {
				b.WriteByte('0')
			}
			b.WriteString(".0")
		} else if k > 0 {
			b.WriteString(digits[:k])
			b.WriteByte('.')
			b.WriteString(digits[k:])
		} else {
			b.WriteString("0.")
			for i := 0; i < -k; i++ {
				b.WriteByte('0')
			}
			b.WriteString(digits)
		}
	} else {
		b.WriteByte(digits[0])
		b.WriteByte('.')
		if len(digits) == 1 {
			b.WriteByte('0')
		} else {
			b.WriteString(digits[1:])
		}
		if sciExp >= 0 {
			fmt.Fprintf(&b, "e+%d", sciExp)
		} else {
			fmt.Fprintf(&b, "e%d", sciExp)
		}
	}
	return b.String()
}

func isEscapedCodepoint(r rune) bool {
	return (r <= 0x1F && r != 0x0A) ||
		(r >= 0x7F && r <= 0x9F) ||
		r == 0x2028 || r == 0x2029 || r == 0xFEFF || r == 0xFFFE || r == 0xFFFF
}

func needsDoubleQuotedStyle1(s string, isKey bool, enclosingCollections int) bool {
	for _, r := range s {
		if isEscapedCodepoint(r) {
			return true
		}
	}
	if len(s) > 0 {
		allNewlines := true
		for i := 0; i < len(s); i++ {
			if s[i] != '\n' {
				allNewlines = false
				break
			}
		}
		if allNewlines {
			return true
		}
	}
	if strings.IndexByte(s, '\n') >= 0 {
		if isKey {
			return true
		}
		if enclosingCollections == 0 && (s[0] == ' ' || s[0] == '\n') {
			return true
		}
	}
	return false
}

func renderDoubleQuoted(s string) string {
	var b strings.Builder
	b.Grow(len(s) + 2)
	b.WriteByte('"')
	for _, r := range s {
		switch r {
		case '"':
			b.WriteString(`\"`)
		case '\\':
			b.WriteString(`\\`)
		case '\b':
			b.WriteString(`\b`)
		case '\t':
			b.WriteString(`\t`)
		case '\n':
			b.WriteString(`\n`)
		case '\f':
			b.WriteString(`\f`)
		case '\r':
			b.WriteString(`\r`)
		default:
			if isEscapedCodepoint(r) {
				fmt.Fprintf(&b, `\u%04x`, uint32(r))
			} else {
				b.WriteRune(r)
			}
		}
	}
	b.WriteByte('"')
	return b.String()
}

func isUnicodeWhitespaceRune(r rune) bool {
	return (r >= 0x0009 && r <= 0x000D) ||
		r == 0x0020 ||
		r == 0x0085 ||
		r == 0x00A0 ||
		r == 0x1680 ||
		(r >= 0x2000 && r <= 0x200A) ||
		r == 0x2028 ||
		r == 0x2029 ||
		r == 0x202F ||
		r == 0x205F ||
		r == 0x3000
}

func isAllASCIIDigits(s string) bool {
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

func matchesCoreNonString(s string) bool {
	switch s {
	case "", "~", "null", "Null", "NULL", "true", "True", "TRUE", "false", "False", "FALSE", ".nan", ".NaN", ".NAN":
		return true
	}
	if strings.HasPrefix(s, "0o") {
		r := s[2:]
		if len(r) > 0 {
			ok := true
			for i := 0; i < len(r); i++ {
				if r[i] < '0' || r[i] > '7' {
					ok = false
					break
				}
			}
			if ok {
				return true
			}
		}
	}
	if strings.HasPrefix(s, "0x") {
		r := s[2:]
		if len(r) > 0 {
			ok := true
			for i := 0; i < len(r); i++ {
				c := r[i]
				if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
					ok = false
					break
				}
			}
			if ok {
				return true
			}
		}
	}
	u := s
	if len(u) > 0 && (u[0] == '-' || u[0] == '+') {
		u = u[1:]
	}
	switch u {
	case ".inf", ".Inf", ".INF":
		return true
	}
	mant := u
	hasExp := false
	exp := ""
	if idx := strings.IndexAny(u, "eE"); idx >= 0 {
		mant = u[:idx]
		exp = u[idx+1:]
		hasExp = true
	}
	if hasExp {
		ed := exp
		if len(ed) > 0 && (ed[0] == '-' || ed[0] == '+') {
			ed = ed[1:]
		}
		if !isAllASCIIDigits(ed) {
			return false
		}
	}
	if dotIdx := strings.IndexByte(mant, '.'); dotIdx >= 0 {
		intPart := mant[:dotIdx]
		fracPart := mant[dotIdx+1:]
		if len(intPart) == 0 {
			return isAllASCIIDigits(fracPart)
		}
		if !isAllASCIIDigits(intPart) {
			return false
		}
		for i := 0; i < len(fracPart); i++ {
			if fracPart[i] < '0' || fracPart[i] > '9' {
				return false
			}
		}
		return true
	}
	return isAllASCIIDigits(mant)
}

func matchesYAML11Hazard(s string) bool {
	for _, kw := range []string{"y", "n", "yes", "no", "on", "off", "true", "false", "null"} {
		if strings.EqualFold(s, kw) {
			return true
		}
	}
	if strings.EqualFold(s, ".nan") {
		return true
	}
	u := s
	if len(u) > 0 && (u[0] == '-' || u[0] == '+') {
		u = u[1:]
	}
	if strings.EqualFold(u, ".inf") {
		return true
	}

	// Class 2: radix, underscore/comma-grouped, or multi-dot numeric tokens
	if strings.HasPrefix(u, "0b") {
		r := u[2:]
		if len(r) > 0 {
			ok := true
			for i := 0; i < len(r); i++ {
				if r[i] != '0' && r[i] != '1' && r[i] != '_' {
					ok = false
					break
				}
			}
			if ok {
				return true
			}
		}
	}
	if strings.HasPrefix(u, "0x") {
		r := u[2:]
		if len(r) > 0 {
			ok := true
			for i := 0; i < len(r); i++ {
				c := r[i]
				if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F') || c == '_') {
					ok = false
					break
				}
			}
			if ok {
				return true
			}
		}
	}
	if strings.HasPrefix(u, "0") {
		r := u[1:]
		if len(r) > 0 {
			ok := true
			for i := 0; i < len(r); i++ {
				c := r[i]
				if !((c >= '0' && c <= '9') || c == '_') {
					ok = false
					break
				}
			}
			if ok {
				return true
			}
		}
	}
	mant := u
	expOk := true
	if idx := strings.IndexAny(u, "eE"); idx >= 0 {
		mant = u[:idx]
		ed := u[idx+1:]
		if len(ed) > 0 && (ed[0] == '-' || ed[0] == '+') {
			ed = ed[1:]
		}
		if len(ed) == 0 {
			expOk = false
		} else {
			for i := 0; i < len(ed); i++ {
				c := ed[i]
				if !((c >= '0' && c <= '9') || c == '_') {
					expOk = false
					break
				}
			}
		}
	}
	if expOk {
		hasDigit := false
		for i := 0; i < len(mant); i++ {
			if mant[i] >= '0' && mant[i] <= '9' {
				hasDigit = true
				break
			}
		}
		if strings.IndexByte(mant, '.') >= 0 && hasDigit {
			dotOk := false
			if strings.HasPrefix(mant, ".") {
				rest := mant[1:]
				if len(rest) > 0 && ((rest[0] >= '0' && rest[0] <= '9') || rest[0] == '_') {
					dotOk = true
					for i := 0; i < len(rest); i++ {
						c := rest[i]
						if !((c >= '0' && c <= '9') || c == '_' || c == '.') {
							dotOk = false
							break
						}
					}
				}
			} else if dotIdx := strings.IndexByte(mant, '.'); dotIdx > 0 {
				head := mant[:dotIdx]
				tail := mant[dotIdx+1:]
				dotOk = true
				for i := 0; i < len(head); i++ {
					c := head[i]
					if !((c >= '0' && c <= '9') || c == '_') {
						dotOk = false
						break
					}
				}
				if dotOk {
					for i := 0; i < len(tail); i++ {
						c := tail[i]
						if !((c >= '0' && c <= '9') || c == '_' || c == '.') {
							dotOk = false
							break
						}
					}
				}
			}
			if dotOk {
				return true
			}
		}
		if len(mant) > 0 && mant[0] >= '0' && mant[0] <= '9' {
			intOk := true
			for i := 1; i < len(mant); i++ {
				c := mant[i]
				if !((c >= '0' && c <= '9') || c == '_' || c == ',') {
					intOk = false
					break
				}
			}
			if intOk {
				return true
			}
		}
	}

	// Class 3: sexagesimal [+-]?[0-9][0-9_]*(:[0-5]?[0-9])+([.][0-9_]*)?
	if strings.IndexByte(u, ':') >= 0 {
		sexInt := u
		sexFracOk := true
		if dotIdx := strings.IndexByte(u, '.'); dotIdx >= 0 {
			sexInt = u[:dotIdx]
			frac := u[dotIdx+1:]
			for i := 0; i < len(frac); i++ {
				c := frac[i]
				if !((c >= '0' && c <= '9') || c == '_') {
					sexFracOk = false
					break
				}
			}
		}
		if sexFracOk {
			parts := strings.Split(sexInt, ":")
			if len(parts) >= 2 {
				first := parts[0]
				firstOk := len(first) > 0 && first[0] >= '0' && first[0] <= '9'
				for i := 1; firstOk && i < len(first); i++ {
					c := first[i]
					if !((c >= '0' && c <= '9') || c == '_') {
						firstOk = false
					}
				}
				if firstOk {
					allOk := true
					for _, seg := range parts[1:] {
						segOk := (len(seg) == 1 && seg[0] >= '0' && seg[0] <= '9') ||
							(len(seg) == 2 && seg[0] >= '0' && seg[0] <= '5' && seg[1] >= '0' && seg[1] <= '9')
						if !segOk {
							allOk = false
							break
						}
					}
					if allOk {
						return true
					}
				}
			}
		}
	}

	// Class 4: YAML 1.1 timestamp [0-9]{4}-[0-9]{1,2}-[0-9]{1,2}([Tt \t].*)?
	if len(s) >= 8 && isAllASCIIDigits(s[0:4]) && s[4] == '-' {
		idx := 5
		mStart := idx
		for idx < len(s) && idx-mStart < 2 && s[idx] >= '0' && s[idx] <= '9' {
			idx++
		}
		if idx > mStart && idx < len(s) && s[idx] == '-' {
			idx++
			dStart := idx
			for idx < len(s) && idx-dStart < 2 && s[idx] >= '0' && s[idx] <= '9' {
				idx++
			}
			if idx > dStart && (idx == len(s) || s[idx] == 'T' || s[idx] == 't' || s[idx] == ' ' || s[idx] == '\t') {
				return true
			}
		}
	}

	return false
}

func canEmitPlain(s string, quoting CompatQuoting) bool {
	if len(s) == 0 || len(s) > 1024 || strings.IndexByte(s, '\n') >= 0 {
		return false
	}
	firstRune, _ := utf8.DecodeRuneInString(s)
	lastRune, _ := utf8.DecodeLastRuneInString(s)
	if isUnicodeWhitespaceRune(firstRune) || isUnicodeWhitespaceRune(lastRune) {
		return false
	}
	switch s[0] {
	case ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`':
		return false
	case '-', '?', ':':
		if len(s) == 1 || s[1] == ' ' || s[1] == '\t' {
			return false
		}
	}
	for _, marker := range []string{"---", "..."} {
		if strings.HasPrefix(s, marker) {
			rest := s[len(marker):]
			if len(rest) == 0 {
				return false
			}
			r, _ := utf8.DecodeRuneInString(rest)
			if isUnicodeWhitespaceRune(r) {
				return false
			}
		}
	}
	if strings.Contains(s, ": ") || strings.Contains(s, ":\t") || strings.Contains(s, " #") || strings.Contains(s, "\t#") {
		return false
	}
	if s[len(s)-1] == ':' || s == "<<" || s == "=" {
		return false
	}
	if matchesCoreNonString(s) {
		return false
	}
	if quoting == CompatQuotingYaml11Safe && matchesYAML11Hazard(s) {
		return false
	}
	return true
}

func renderString(s string, isKey bool, enclosingCollections int, quoting CompatQuoting) (singleLine string, blockHeader string, isBlock bool) {
	if needsDoubleQuotedStyle1(s, isKey, enclosingCollections) {
		return renderDoubleQuoted(s), "", false
	}
	if strings.IndexByte(s, '\n') >= 0 {
		needsIndentDigit := enclosingCollections >= 1 && (s[0] == ' ' || s[0] == '\n')
		var header string
		if s[len(s)-1] != '\n' {
			if needsIndentDigit {
				header = "|2-"
			} else {
				header = "|-"
			}
		} else if strings.HasSuffix(s, "\n\n") {
			if needsIndentDigit {
				header = "|2+"
			} else {
				header = "|+"
			}
		} else if needsIndentDigit {
			header = "|2"
		} else {
			header = "|"
		}
		return "", header, true
	}
	if canEmitPlain(s, quoting) {
		return s, "", false
	}
	if strings.IndexByte(s, '\'') < 0 {
		return "'" + s + "'", "", false
	}
	return renderDoubleQuoted(s), "", false
}

const emitSpacesChunk = "                                                                "

func pushEmitSpaces(out *outBuf, count int) *Error {
	rem := count
	for rem > 0 {
		chunk := rem
		if chunk > len(emitSpacesChunk) {
			chunk = len(emitSpacesChunk)
		}
		if err := out.push(emitSpacesChunk[:chunk]); err != nil {
			return err
		}
		rem -= chunk
	}
	return nil
}

func writeLiteralBlock(out *outBuf, header string, body string, bodyIndent int) *Error {
	if err := out.push(header); err != nil {
		return err
	}
	if err := out.push("\n"); err != nil {
		return err
	}
	trimmed := strings.TrimSuffix(body, "\n")
	lines := strings.Split(trimmed, "\n")
	for _, line := range lines {
		if len(line) > 0 {
			if err := pushEmitSpaces(out, bodyIndent); err != nil {
				return err
			}
			if err := out.push(line); err != nil {
				return err
			}
		}
		if err := out.push("\n"); err != nil {
			return err
		}
	}
	return nil
}

func isNsTagCharNoPct(b byte) bool {
	if (b >= '0' && b <= '9') || (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z') {
		return true
	}
	switch b {
	case '#', ';', '/', '?', ':', '@', '&', '=', '+', '$', '_', '.', '~', '*', '\'', '(', ')', '-':
		return true
	}
	return false
}

func isVerbatimURICharNoPct(b byte) bool {
	if (b >= '0' && b <= '9') || (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z') {
		return true
	}
	switch b {
	case ';', '/', '?', ':', '@', '&', '=', '+', '$', '_', '.', '~', '*', '\'', '(', ')', '-', ',', '!':
		return true
	}
	return false
}

func percentEncodeInto(b *strings.Builder, s string, allow func(byte) bool) {
	const hexUpper = "0123456789ABCDEF"
	for i := 0; i < len(s); i++ {
		c := s[i]
		if allow(c) {
			b.WriteByte(c)
		} else {
			b.WriteByte('%')
			b.WriteByte(hexUpper[c>>4])
			b.WriteByte(hexUpper[c&0x0F])
		}
	}
}

func renderTag(tag Tag) string {
	uri := tag.uri
	var b strings.Builder
	if strings.HasPrefix(uri, "!") {
		b.Grow(len(uri) + 4)
		b.WriteByte('!')
		percentEncodeInto(&b, uri[1:], isNsTagCharNoPct)
		return b.String()
	}
	if strings.HasPrefix(uri, "tag:yaml.org,2002:") {
		rest := uri[len("tag:yaml.org,2002:"):]
		if len(rest) > 0 {
			b.Grow(len(rest) + 6)
			b.WriteString("!!")
			percentEncodeInto(&b, rest, isNsTagCharNoPct)
			return b.String()
		}
	}
	b.Grow(len(uri) + 6)
	b.WriteString("!<")
	percentEncodeInto(&b, uri, isVerbatimURICharNoPct)
	b.WriteByte('>')
	return b.String()
}

func renderScalarSingleLine(v *Value, isKey bool, enclosingCollections int, quoting CompatQuoting) (string, bool) {
	switch v.kind {
	case ValueKindNull:
		return "null", true
	case ValueKindBool:
		if v.b {
			return "true", true
		}
		return "false", true
	case ValueKindInt:
		return v.i.String(), true
	case ValueKindFloat:
		return formatYAMLFloat(v.f), true
	case ValueKindString:
		single, _, isBlock := renderString(v.s, isKey, enclosingCollections, quoting)
		if isBlock {
			return "", false
		}
		return single, true
	case ValueKindSequence:
		if len(v.seq) == 0 {
			return "[]", true
		}
	case ValueKindMapping:
		if v.m == nil || len(v.m.entries) == 0 {
			return "{}", true
		}
	}
	return "", false
}

func renderImplicitKey(key *Node, enclosing int, quoting CompatQuoting) (string, bool) {
	switch key.value.kind {
	case ValueKindNull:
		return "null", true
	case ValueKindBool:
		if key.value.b {
			return "true", true
		}
		return "false", true
	case ValueKindInt:
		return key.value.i.String(), true
	case ValueKindFloat:
		return formatYAMLFloat(key.value.f), true
	case ValueKindString:
		if strings.IndexByte(key.value.s, '\n') < 0 {
			single, _, isBlock := renderString(key.value.s, true, enclosing, quoting)
			if !isBlock && len(single) <= 1024 {
				return single, true
			}
		}
	}
	return "", false
}

type emitFrameKind uint8

const (
	emitFrameSeq emitFrameKind = iota
	emitFrameMap
	emitFrameExplicitMapValue
)

type emitFrame struct {
	kind      emitFrameKind
	seq       []Node
	entries   []MappingEntry
	val       *Node
	idx       int
	indent    int
	enclosing int
}

func emitAfterPrefix(
	out *outBuf,
	stack *[]emitFrame,
	indent int,
	prefix string,
	node *Node,
	enclosing int,
	isKey bool,
	quoting CompatQuoting,
) *Error {
	if err := pushEmitSpaces(out, indent); err != nil {
		return err
	}
	if err := out.push(prefix); err != nil {
		return err
	}

	if single, ok := renderScalarSingleLine(&node.value, isKey, enclosing, quoting); ok {
		if err := out.push(" "); err != nil {
			return err
		}
		if err := out.push(single); err != nil {
			return err
		}
		return out.push("\n")
	}

	switch node.value.kind {
	case ValueKindString:
		single, header, isBlock := renderString(node.value.s, isKey, enclosing, quoting)
		if !isBlock {
			if err := out.push(" "); err != nil {
				return err
			}
			if err := out.push(single); err != nil {
				return err
			}
			return out.push("\n")
		}
		if err := out.push(" "); err != nil {
			return err
		}
		return writeLiteralBlock(out, header, node.value.s, indent+2)
	case ValueKindSequence:
		if err := out.push("\n"); err != nil {
			return err
		}
		*stack = append(*stack, emitFrame{
			kind:      emitFrameSeq,
			seq:       node.value.seq,
			idx:       0,
			indent:    indent + 2,
			enclosing: enclosing + 1,
		})
		return nil
	case ValueKindMapping:
		if err := out.push("\n"); err != nil {
			return err
		}
		var entries []MappingEntry
		if node.value.m != nil {
			entries = node.value.m.entries
		}
		*stack = append(*stack, emitFrame{
			kind:      emitFrameMap,
			entries:   entries,
			idx:       0,
			indent:    indent + 2,
			enclosing: enclosing + 1,
		})
		return nil
	case ValueKindTagged:
		t := node.value.tagged
		tagStr := renderTag(t.tag)
		if err := out.push(" "); err != nil {
			return err
		}
		if err := out.push(tagStr); err != nil {
			return err
		}
		if single, ok := renderScalarSingleLine(&t.value.value, isKey, enclosing, quoting); ok {
			if err := out.push(" "); err != nil {
				return err
			}
			if err := out.push(single); err != nil {
				return err
			}
			return out.push("\n")
		}
		switch t.value.value.kind {
		case ValueKindString:
			single, header, isBlock := renderString(t.value.value.s, isKey, enclosing, quoting)
			if !isBlock {
				if err := out.push(" "); err != nil {
					return err
				}
				if err := out.push(single); err != nil {
					return err
				}
				return out.push("\n")
			}
			if err := out.push(" "); err != nil {
				return err
			}
			return writeLiteralBlock(out, header, t.value.value.s, indent+2)
		case ValueKindSequence:
			if err := out.push("\n"); err != nil {
				return err
			}
			*stack = append(*stack, emitFrame{
				kind:      emitFrameSeq,
				seq:       t.value.value.seq,
				idx:       0,
				indent:    indent + 2,
				enclosing: enclosing + 1,
			})
			return nil
		case ValueKindMapping:
			if err := out.push("\n"); err != nil {
				return err
			}
			var entries []MappingEntry
			if t.value.value.m != nil {
				entries = t.value.value.m.entries
			}
			*stack = append(*stack, emitFrame{
				kind:      emitFrameMap,
				entries:   entries,
				idx:       0,
				indent:    indent + 2,
				enclosing: enclosing + 1,
			})
			return nil
		}
	}
	return nil
}

func emitRootNode(out *outBuf, root *Node, quoting CompatQuoting) *Error {
	if single, ok := renderScalarSingleLine(&root.value, false, 0, quoting); ok {
		if err := out.push(single); err != nil {
			return err
		}
		return out.push("\n")
	}

	var stack []emitFrame
	switch root.value.kind {
	case ValueKindString:
		single, header, isBlock := renderString(root.value.s, false, 0, quoting)
		if !isBlock {
			if err := out.push(single); err != nil {
				return err
			}
			return out.push("\n")
		}
		return writeLiteralBlock(out, header, root.value.s, 2)
	case ValueKindSequence:
		stack = append(stack, emitFrame{
			kind:      emitFrameSeq,
			seq:       root.value.seq,
			idx:       0,
			indent:    0,
			enclosing: 1,
		})
	case ValueKindMapping:
		var entries []MappingEntry
		if root.value.m != nil {
			entries = root.value.m.entries
		}
		stack = append(stack, emitFrame{
			kind:      emitFrameMap,
			entries:   entries,
			idx:       0,
			indent:    0,
			enclosing: 1,
		})
	case ValueKindTagged:
		t := root.value.tagged
		tagStr := renderTag(t.tag)
		if err := out.push(tagStr); err != nil {
			return err
		}
		if single, ok := renderScalarSingleLine(&t.value.value, false, 0, quoting); ok {
			if err := out.push(" "); err != nil {
				return err
			}
			if err := out.push(single); err != nil {
				return err
			}
			return out.push("\n")
		}
		switch t.value.value.kind {
		case ValueKindString:
			single, header, isBlock := renderString(t.value.value.s, false, 0, quoting)
			if !isBlock {
				if err := out.push(" "); err != nil {
					return err
				}
				if err := out.push(single); err != nil {
					return err
				}
				return out.push("\n")
			}
			if err := out.push(" "); err != nil {
				return err
			}
			return writeLiteralBlock(out, header, t.value.value.s, 2)
		case ValueKindSequence:
			if err := out.push("\n"); err != nil {
				return err
			}
			stack = append(stack, emitFrame{
				kind:      emitFrameSeq,
				seq:       t.value.value.seq,
				idx:       0,
				indent:    0,
				enclosing: 1,
			})
		case ValueKindMapping:
			if err := out.push("\n"); err != nil {
				return err
			}
			var entries []MappingEntry
			if t.value.value.m != nil {
				entries = t.value.value.m.entries
			}
			stack = append(stack, emitFrame{
				kind:      emitFrameMap,
				entries:   entries,
				idx:       0,
				indent:    0,
				enclosing: 1,
			})
		}
	}

	for len(stack) > 0 {
		frame := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		switch frame.kind {
		case emitFrameSeq:
			if frame.idx < len(frame.seq) {
				if frame.idx+1 < len(frame.seq) {
					stack = append(stack, emitFrame{
						kind:      emitFrameSeq,
						seq:       frame.seq,
						idx:       frame.idx + 1,
						indent:    frame.indent,
						enclosing: frame.enclosing,
					})
				}
				if err := emitAfterPrefix(out, &stack, frame.indent, "-", &frame.seq[frame.idx], frame.enclosing, false, quoting); err != nil {
					return err
				}
			}
		case emitFrameMap:
			if frame.idx < len(frame.entries) {
				if frame.idx+1 < len(frame.entries) {
					stack = append(stack, emitFrame{
						kind:      emitFrameMap,
						entries:   frame.entries,
						idx:       frame.idx + 1,
						indent:    frame.indent,
						enclosing: frame.enclosing,
					})
				}
				entry := &frame.entries[frame.idx]
				if keyStr, ok := renderImplicitKey(&entry.Key, frame.enclosing, quoting); ok {
					if err := emitAfterPrefix(out, &stack, frame.indent, keyStr+":", &entry.Value, frame.enclosing, false, quoting); err != nil {
						return err
					}
				} else {
					stack = append(stack, emitFrame{
						kind:      emitFrameExplicitMapValue,
						val:       &entry.Value,
						indent:    frame.indent,
						enclosing: frame.enclosing,
					})
					if err := emitAfterPrefix(out, &stack, frame.indent, "?", &entry.Key, frame.enclosing, true, quoting); err != nil {
						return err
					}
				}
			}
		case emitFrameExplicitMapValue:
			if err := emitAfterPrefix(out, &stack, frame.indent, ":", frame.val, frame.enclosing, false, quoting); err != nil {
				return err
			}
		}
	}
	return nil
}

// Emit serializes a single Node to canonical YAML 1.2.2 Core text under DefaultEmitOptions() (spec-v1.5 §10.2).
func Emit(node Node) (string, *Error) {
	return EmitWith(node, DefaultEmitOptions())
}

// EmitWith serializes a single Node to canonical YAML 1.2.2 Core text under opts (spec-v1.5 §10.2).
func EmitWith(node Node, opts EmitOptions) (string, *Error) {
	if err := opts.Validate(); err != nil {
		return "", err
	}
	nodesVisited := 0
	var stack []emitPreflightFrame
	if err := preflightEmitNode(&node, opts, &nodesVisited, &stack); err != nil {
		return "", err
	}
	out := &outBuf{max: opts.Limits.MaxOutputBytes}
	if err := emitRootNode(out, &node, opts.CompatQuoting); err != nil {
		return "", err
	}
	return string(out.buf), nil
}

// EmitAll serializes a slice of Documents to a canonical YAML 1.2.2 Core stream under DefaultEmitOptions() (spec-v1.5 §10.2).
func EmitAll(docs []Document) (string, *Error) {
	return EmitAllWith(docs, DefaultEmitOptions())
}

// EmitAllWith serializes a slice of Documents to a canonical YAML 1.2.2 Core stream under opts (spec-v1.5 §10.2).
func EmitAllWith(docs []Document, opts EmitOptions) (string, *Error) {
	if err := preflightEmitDocuments(docs, opts); err != nil {
		return "", err
	}
	if len(docs) == 0 {
		return "", nil
	}
	out := &outBuf{max: opts.Limits.MaxOutputBytes}
	for idx := range docs {
		doc := &docs[idx]
		if idx == 0 {
			if doc.yamlVersion != nil {
				if err := out.push(fmt.Sprintf("%%YAML 1.%d\n---\n", doc.yamlVersion[1])); err != nil {
					return "", err
				}
			} else if doc.explicitStart {
				if err := out.push("---\n"); err != nil {
					return "", err
				}
			}
		} else if doc.yamlVersion != nil {
			if err := out.push(fmt.Sprintf("...\n%%YAML 1.%d\n---\n", doc.yamlVersion[1])); err != nil {
				return "", err
			}
		} else {
			if err := out.push("---\n"); err != nil {
				return "", err
			}
		}
		if err := emitRootNode(out, &doc.root, opts.CompatQuoting); err != nil {
			return "", err
		}
	}
	return string(out.buf), nil
}
