package yaml

import (
	"fmt"
	"math"
	"math/big"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"unicode/utf8"
)

func wireFixtureDir(t *testing.T) string {
	t.Helper()
	return filepath.Join("..", "crates", "yamlprims", "tests", "fixtures", "wire")
}

func readWireInput(t *testing.T, name string) []byte {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(wireFixtureDir(t), name))
	if err != nil {
		t.Fatalf("read wire input %s: %v", name, err)
	}
	return b
}

func mustEncode(t *testing.T) func(string, *Error) string {
	return func(s string, err *Error) string {
		t.Helper()
		if err != nil {
			t.Fatalf("encoder failed: %v", err)
		}
		return s
	}
}

func loadOrErrorWire(t *testing.T, bytes []byte, o Options) string {
	t.Helper()
	docs, err := LoadAllBytes(bytes, o)
	if err != nil {
		return mustEncode(t)(EncodeError(err, o))
	}
	return mustEncode(t)(Encode(docs, o))
}

func TestWireGoldensAndDecodeWireRoundTrip(t *testing.T) {
	if os.Getenv("YAMLPRIMS_BLESS") != "" {
		t.Fatal("golden blessing is disabled")
	}
	d := DefaultOptions()
	apply := DefaultOptions()
	apply.MergeKeys = MergeKeysApply
	tiny := DefaultOptions()
	tiny.Limits.MaxOutputBytes = 64

	typedDoc, err := LoadWith(string(readWireInput(t, "typed-port.yaml")), d)
	if err != nil {
		t.Fatalf("load typed-port.yaml: %v", err)
	}
	_, typedErr := ConvertMappingField(typedDoc.IntoRoot(), "port", NodeToUint16, d)
	if typedErr == nil {
		t.Fatal("typed-port must fail")
	}
	typed := mustEncode(t)(EncodeError(typedErr, d))

	m := NewMapping()
	if insErr := m.InsertNew(NewSyntheticNode(StringValue("port")), NewSyntheticNode(StringValue("x"))); insErr != nil {
		t.Fatalf("InsertNew: %v", insErr)
	}
	_, synthErr := ConvertMappingField(NewSyntheticNode(MappingValue(m)), "port", NodeToUint16, d)
	if synthErr == nil {
		t.Fatal("synthetic must fail")
	}
	synthetic := mustEncode(t)(EncodeError(synthErr, d))

	docs, err := LoadAllBytes(readWireInput(t, "success.yaml"), d)
	if err != nil {
		t.Fatalf("load success.yaml: %v", err)
	}
	_, limitErr := Encode(docs, tiny)
	if limitErr == nil {
		t.Fatal("output limit must trip")
	}
	limit := mustEncode(t)(EncodeError(limitErr, d))

	plainOkDoc, err := LoadBytes(readWireInput(t, "plain-ok.yaml"), d)
	if err != nil {
		t.Fatalf("load plain-ok.yaml: %v", err)
	}
	plainOk := mustEncode(t)(EncodePlain(plainOkDoc.IntoRoot(), d))

	_, plainTaggedErr := EncodePlain(docs[0].IntoRoot(), d)
	if plainTaggedErr == nil {
		t.Fatal("tagged value must be refused by EncodePlain")
	}
	plainTagged := mustEncode(t)(EncodeError(plainTaggedErr, d))

	cases := []struct {
		name   string
		got    string
		isWire bool
	}{
		{"success", loadOrErrorWire(t, readWireInput(t, "success.yaml"), d), true},
		{"merge-apply", loadOrErrorWire(t, readWireInput(t, "merge-apply.yaml"), apply), true},
		{"merge-reject", loadOrErrorWire(t, readWireInput(t, "merge-apply.yaml"), d), true},
		{"duplicate", loadOrErrorWire(t, readWireInput(t, "duplicate.yaml"), d), true},
		{"invalid-utf8", loadOrErrorWire(t, readWireInput(t, "invalid-utf8.yaml"), d), true},
		{"typed-port", typed, true},
		{"synthetic", synthetic, true},
		{"output-limit", limit, true},
		{"plain-ok", plainOk, false},
		{"plain-tagged", plainTagged, true},
	}

	for _, tc := range cases {
		path := filepath.Join(wireFixtureDir(t), tc.name+".json")
		wantBytes, readErr := os.ReadFile(path)
		if readErr != nil {
			t.Fatalf("read %s: %v", path, readErr)
		}
		want := strings.TrimRight(string(wantBytes), "\n")
		if want != tc.got {
			t.Errorf("%s:\n  want %s\n  got  %s", tc.name, want, tc.got)
		}
		if tc.isWire {
			env, decErr := DecodeWire(tc.got, d)
			if decErr != nil {
				t.Fatalf("%s: DecodeWire failed: %v", tc.name, decErr)
			}
			var reencoded string
			if env.HasDocuments {
				reencoded = mustEncode(t)(Encode(env.Documents, d))
			} else {
				reencoded = mustEncode(t)(EncodeError(env.Error, d))
			}
			if reencoded != tc.got {
				t.Errorf("%s: DecodeWire round-trip mismatch:\n  want %s\n  got  %s", tc.name, tc.got, reencoded)
			}
		}
	}
}

func truthTSV(t *testing.T, name string) [][]string {
	t.Helper()
	path := filepath.Join("..", "crates", "yamlprims", "tests", "fixtures", "truth", name)
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read truth fixture %s: %v", path, err)
	}
	lines := strings.Split(strings.TrimRight(string(raw), "\r\n"), "\n")
	var rows [][]string
	for _, line := range lines[1:] {
		if line == "" {
			continue
		}
		rows = append(rows, strings.Split(line, "\t"))
	}
	return rows
}

func truthJS(s string) string {
	var o outBuf
	o.max = 1 << 20
	_ = o.string(s)
	return string(o.buf)
}

func truthUnJS(t *testing.T, s string) string {
	t.Helper()
	p := wireParser{src: s, maxDepth: 16, nodesLeft: 16}
	out, err := p.parseString()
	if err != nil {
		t.Fatalf("truthUnJS(%s): %v", s, err)
	}
	return out
}

func truthRender(n *Node) string {
	v := n.Value()
	switch v.Kind() {
	case ValueKindNull:
		return "null"
	case ValueKindBool:
		b, _ := v.AsBool()
		return fmt.Sprintf("bool:%v", b)
	case ValueKindInt:
		i, _ := v.AsInt()
		return fmt.Sprintf("int:%s", i.String())
	case ValueKindFloat:
		f, _ := v.AsFloat()
		bits := math.Float64bits(f)
		if math.IsNaN(f) {
			bits = 0x7ff8000000000000
		}
		return fmt.Sprintf("float:%016x", bits)
	case ValueKindString:
		s, _ := v.AsStr()
		return fmt.Sprintf("str:%s", truthJS(s))
	case ValueKindTagged:
		tag, _ := v.AsTagged()
		return fmt.Sprintf("tagged:%s:%s", truthJS(tag.Tag().URI()), truthRender(tag.Value()))
	case ValueKindSequence:
		seq, _ := v.AsSequence()
		return fmt.Sprintf("seq:%d", len(seq))
	case ValueKindMapping:
		m, _ := v.AsMapping()
		return fmt.Sprintf("map:%d", m.Len())
	default:
		return "?"
	}
}

func truthKind(e *Error) string {
	if e.Kind == ErrorKindSyntax {
		return "syntax"
	}
	return fmt.Sprintf("err:%s", e.Kind.Name())
}

func truthFirstKey(n *Node) *Node {
	m, ok := n.Value().AsMapping()
	if !ok || m.Len() == 0 {
		return nil
	}
	k, _ := m.At(0)
	return &k
}

func truthProbe(root *Node, position string) (string, bool) {
	var n *Node
	switch position {
	case "block_value", "flow_value":
		m, ok := root.Value().AsMapping()
		if !ok {
			return "", false
		}
		v, ok := m.Get("k")
		if !ok {
			return "", false
		}
		n = v
	case "block_implicit_key", "block_explicit_key", "flow_key":
		n = truthFirstKey(root)
		if n == nil {
			return "", false
		}
	case "block_seq", "flow_seq":
		seq, ok := root.Value().AsSequence()
		if !ok || len(seq) == 0 {
			return "", false
		}
		n = &seq[0]
	case "top":
		n = root
	default:
		return "", false
	}
	return truthRender(n), true
}

func truthTyped[T any](yamlSrc string, opts Options, fn func(Node, ...Options) (T, *Error), fmtVal func(T) string) string {
	doc, err := LoadWith(yamlSrc, opts)
	if err != nil {
		return truthKind(err)
	}
	v, err := ConvertMappingField(doc.IntoRoot(), "k", fn, opts)
	if err != nil {
		return truthKind(err)
	}
	return fmtVal(v)
}

func TestTruthTables(t *testing.T) {
	t.Run("booleans", func(t *testing.T) {
		opts := DefaultOptions()
		for _, row := range truthTSV(t, "booleans.tsv") {
			id, pos, yamlSrc, expected := row[0], row[2], truthUnJS(t, row[6]), row[7]
			doc, err := LoadWith(yamlSrc, opts)
			var got string
			if err != nil {
				got = truthKind(err)
			} else if p, ok := truthProbe(doc.Root(), pos); ok {
				got = p
			} else {
				got = "probe-failed"
			}
			if got != expected {
				t.Errorf("%s [value]: got %s, want %s (%q)", id, got, expected, yamlSrc)
			}
			if row[8] != "-" {
				b := truthTyped(yamlSrc, opts, NodeToBool, func(v bool) string { return fmt.Sprintf("bool:%v", v) })
				if b != row[8] {
					t.Errorf("%s [typed_bool]: got %s, want %s", id, b, row[8])
				}
				s := truthTyped(yamlSrc, opts, NodeToString, func(v string) string { return fmt.Sprintf("str:%s", truthJS(v)) })
				if s != row[9] {
					t.Errorf("%s [typed_string]: got %s, want %s", id, s, row[9])
				}
				o := truthTyped(yamlSrc, opts, NodeToOptionalBool, func(v *bool) string {
					if v == nil {
						return "none"
					}
					return fmt.Sprintf("bool:%v", *v)
				})
				if o != row[10] {
					t.Errorf("%s [typed_option_bool]: got %s, want %s", id, o, row[10])
				}
			}
		}
	})

	t.Run("collisions", func(t *testing.T) {
		opts := DefaultOptions()
		for _, row := range truthTSV(t, "collisions.tsv") {
			yamlSrc := truthUnJS(t, row[5])
			_, err := LoadWith(yamlSrc, opts)
			got := "ok"
			if err != nil {
				got = truthKind(err)
			}
			if got != row[6] {
				t.Errorf("%s [collision]: got %s, want %s (%q)", row[0], got, row[6], yamlSrc)
			}
		}
	})

	t.Run("numbers", func(t *testing.T) {
		opts := DefaultOptions()
		targets := []string{"u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "f32", "f64"}
		for _, row := range truthTSV(t, "numbers.tsv") {
			yamlSrc := truthUnJS(t, row[4])
			doc, err := LoadWith(yamlSrc, opts)
			var got string
			if err != nil {
				got = truthKind(err)
			} else {
				got, _ = truthProbe(doc.Root(), "block_value")
			}
			if got != row[5] {
				t.Errorf("%s [value]: got %s, want %s (%q)", row[0], got, row[5], yamlSrc)
			}
			for i, tgt := range targets {
				var tgot string
				switch tgt {
				case "u8":
					tgot = truthTyped(yamlSrc, opts, NodeToUint8, func(v uint8) string { return fmt.Sprintf("int:%d", v) })
				case "u16":
					tgot = truthTyped(yamlSrc, opts, NodeToUint16, func(v uint16) string { return fmt.Sprintf("int:%d", v) })
				case "u32":
					tgot = truthTyped(yamlSrc, opts, NodeToUint32, func(v uint32) string { return fmt.Sprintf("int:%d", v) })
				case "u64":
					tgot = truthTyped(yamlSrc, opts, NodeToUint64, func(v uint64) string { return fmt.Sprintf("int:%d", v) })
				case "u128":
					tgot = truthTyped(yamlSrc, opts, NodeToUint128, func(v Integer) string { return fmt.Sprintf("int:%s", v.String()) })
				case "i8":
					tgot = truthTyped(yamlSrc, opts, NodeToInt8, func(v int8) string { return fmt.Sprintf("int:%d", v) })
				case "i16":
					tgot = truthTyped(yamlSrc, opts, NodeToInt16, func(v int16) string { return fmt.Sprintf("int:%d", v) })
				case "i32":
					tgot = truthTyped(yamlSrc, opts, NodeToInt32, func(v int32) string { return fmt.Sprintf("int:%d", v) })
				case "i64":
					tgot = truthTyped(yamlSrc, opts, NodeToInt64, func(v int64) string { return fmt.Sprintf("int:%d", v) })
				case "i128":
					tgot = truthTyped(yamlSrc, opts, NodeToInt128, func(v Integer) string { return fmt.Sprintf("int:%s", v.String()) })
				case "f32":
					tgot = truthTyped(yamlSrc, opts, NodeToFloat32, func(v float32) string {
						bits := math.Float32bits(v)
						if math.IsNaN(float64(v)) {
							bits = 0x7fc00000
						}
						return fmt.Sprintf("f32:%08x", bits)
					})
				case "f64":
					tgot = truthTyped(yamlSrc, opts, NodeToFloat64, func(v float64) string {
						bits := math.Float64bits(v)
						if math.IsNaN(v) {
							bits = 0x7ff8000000000000
						}
						return fmt.Sprintf("float:%016x", bits)
					})
				}
				if tgot != row[6+i] {
					t.Errorf("%s [typed_%s]: got %s, want %s (%q)", row[0], tgt, tgot, row[6+i], yamlSrc)
				}
			}
		}
	})

	t.Run("golden", func(t *testing.T) {
		for _, row := range truthTSV(t, "golden.tsv") {
			id, yamlSrc, probe, mode, expected := row[0], truthUnJS(t, row[1]), row[2], row[3], row[4]
			opts := DefaultOptions()
			switch mode {
			case "apply":
				opts.MergeKeys = MergeKeysApply
			case "as_key":
				opts.MergeKeys = MergeKeysAsKey
			default:
				opts.MergeKeys = MergeKeysReject
			}
			var got string
			switch probe {
			case "typed_u16":
				got = truthTyped(yamlSrc, opts, NodeToUint16, func(v uint16) string { return fmt.Sprintf("int:%d", v) })
			case "typed_bool":
				got = truthTyped(yamlSrc, opts, NodeToBool, func(v bool) string { return fmt.Sprintf("bool:%v", v) })
			case "typed_string":
				got = truthTyped(yamlSrc, opts, NodeToString, func(v string) string { return fmt.Sprintf("str:%s", truthJS(v)) })
			case "typed_option_bool":
				got = truthTyped(yamlSrc, opts, NodeToOptionalBool, func(v *bool) string {
					if v == nil {
						return "none"
					}
					return fmt.Sprintf("bool:%v", *v)
				})
			default:
				doc, err := LoadWith(yamlSrc, opts)
				if err != nil {
					got = truthKind(err)
				} else if expected == "ok" && probe == "root" {
					got = "ok"
				} else if strings.HasPrefix(expected, "keys:") {
					n := doc.Root()
					for _, part := range strings.Split(probe, "/") {
						key := strings.TrimPrefix(part, "k=")
						if m, ok := n.Value().AsMapping(); ok {
							if child, ok := m.Get(key); ok {
								n = child
							}
						}
					}
					m, _ := n.Value().AsMapping()
					ks := make([]string, m.Len())
					for i := 0; i < m.Len(); i++ {
						k, _ := m.At(i)
						if s, ok := k.Value().AsStr(); ok {
							if s == "<<" {
								ks[i] = `"<<"`
							} else {
								ks[i] = s
							}
						} else {
							ks[i] = truthRender(&k)
						}
					}
					got = "keys:" + strings.Join(ks, ",")
				} else if p, ok := truthGoldenProbe(doc.Root(), probe); ok {
					got = p
				} else {
					got = "probe-failed"
				}
			}
			if got != expected {
				t.Errorf("%s [%s]: got %s, want %s (%q)", id, probe, got, expected, yamlSrc)
			}
		}
	})
}

func truthGoldenProbe(root *Node, probe string) (string, bool) {
	if probe == "root" {
		return truthRender(root), true
	}
	if probe == "key0" {
		k := truthFirstKey(root)
		if k == nil {
			return "", false
		}
		return truthRender(k), true
	}
	if strings.HasPrefix(probe, "[") && strings.HasSuffix(probe, "]") {
		idx, err := strconv.Atoi(probe[1 : len(probe)-1])
		if err != nil {
			return "", false
		}
		seq, ok := root.Value().AsSequence()
		if !ok || idx < 0 || idx >= len(seq) {
			return "", false
		}
		return truthRender(&seq[idx]), true
	}
	n := root
	for _, part := range strings.Split(probe, "/") {
		if !strings.HasPrefix(part, "k=") {
			return "", false
		}
		key := strings.TrimPrefix(part, "k=")
		m, ok := n.Value().AsMapping()
		if !ok {
			return "", false
		}
		child, ok := m.Get(key)
		if !ok {
			return "", false
		}
		n = child
	}
	return truthRender(n), true
}

func TestMergeAdmission(t *testing.T) {
	var copies, insertions int
	mergeCopyStartedHook = func(stored, capacity int) {
		if stored >= capacity {
			t.Fatalf("logical copy past result capacity: stored=%d capacity=%d", stored, capacity)
		}
		copies++
	}
	mergeInsertStartedHook = func(stored, capacity int) {
		if stored >= capacity {
			t.Fatalf("insertion past result capacity: stored=%d capacity=%d", stored, capacity)
		}
		insertions++
	}
	defer func() {
		mergeCopyStartedHook = nil
		mergeInsertStartedHook = nil
	}()

	t.Run("capacity_stops_before_logical_copy_and_insertion", func(t *testing.T) {
		cases := []struct {
			input      string
			capacity   int
			wantCopies int
			wantIns    int
			wantLimit  LimitKind
		}{
			{"{x: 0, <<: {a: 1}}", 1, 0, 0, LimitMaxMappingEntries},
			{"{x: 0, <<: {a: 1}}", 2, 1, 1, ""},
			{"{x: 0, <<: [{a: 1}, {b: 2}]}", 2, 1, 1, LimitMaxMappingEntries},
			{"{x: 0, <<: {x: 1}}", 1, 0, 0, ""},
			{"{<<: [{a: 1}, {a: 2}]}", 1, 1, 1, ""},
			{"{x: 0, <<: {<<: {a: 1}}}", 1, 1, 1, LimitMaxMappingEntries},
		}
		for _, tc := range cases {
			opts := DefaultOptions()
			opts.MergeKeys = MergeKeysApply
			opts.Limits.MaxMappingEntries = tc.capacity
			copies, insertions = 0, 0
			_, err := LoadWith(tc.input, opts)
			if copies != tc.wantCopies || insertions != tc.wantIns {
				t.Errorf("%s: got (copies=%d, ins=%d), want (%d, %d)", tc.input, copies, insertions, tc.wantCopies, tc.wantIns)
			}
			if tc.wantLimit != "" {
				if err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != tc.wantLimit || err.Detail.Configured != uint64(tc.capacity) {
					t.Errorf("%s: unexpected error %+v", tc.input, err)
				}
			} else if err != nil {
				t.Errorf("%s: unexpected error %v", tc.input, err)
			}
		}
	})

	t.Run("all_sources_validate_before_any_logical_copy_or_insertion", func(t *testing.T) {
		opts := DefaultOptions()
		opts.MergeKeys = MergeKeysApply
		opts.Limits.MaxMappingEntries = 1
		copies, insertions = 0, 0
		_, err := LoadWith("{x: 0, <<: [{a: 1}, 0]}", opts)
		if err == nil || err.Kind != ErrorKindMergeSource {
			t.Fatalf("expected MergeSource error, got %+v", err)
		}
		if copies != 0 || insertions != 0 {
			t.Fatalf("expected (0, 0) operations, got (%d, %d)", copies, insertions)
		}
	})

	t.Run("logical_copy_budget_rejects_before_insertion", func(t *testing.T) {
		opts := DefaultOptions()
		opts.MergeKeys = MergeKeysApply
		opts.Limits.MaxMappingEntries = 2
		opts.Limits.MaxNodes = 8
		copies, insertions = 0, 0
		_, err := LoadWith("{x: 0, <<: {a: 1}}", opts)
		if err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxNodes || err.Detail.Configured != 8 {
			t.Fatalf("expected MaxNodes(8) error, got %+v", err)
		}
		if copies != 1 || insertions != 0 {
			t.Fatalf("expected (1, 0) operations, got (%d, %d)", copies, insertions)
		}
	})
}

func TestRegistrationAdmission(t *testing.T) {
	var copies int
	registrationCopyHook = func() {
		copies++
	}
	defer func() {
		registrationCopyHook = nil
	}()

	makeOpts := func(owned int) Options {
		o := DefaultOptions()
		o.MergeKeys = MergeKeysReject
		o.UnknownTags = UnknownTagsPreserve
		o.Limits.MaxInputBytes = 4096
		o.Limits.MaxDepth = 16
		o.Limits.MaxNodes = 128
		o.Limits.MaxExpandedNodes = 64
		o.Limits.MaxAliasEvents = 32
		o.Limits.MaxOwnedBytes = owned
		o.Limits.MaxScalarBytes = 16
		o.Limits.MaxKeyBytes = 16
		o.Limits.MaxMappingEntries = 16
		o.Limits.MaxDocuments = 4
		o.Limits.MaxOutputBytes = 4096
		o.Limits.MaxWarnings = 1
		if err := o.Validate(); err != nil {
			t.Fatalf("Validate: %v", err)
		}
		return o
	}

	loc := func(b, l, c int) *Location {
		return &Location{ByteOffset: b, Line: l, Column: c}
	}

	cases := []struct {
		name       string
		input      string
		owned      int
		stream     bool
		wantCopies int
		wantAt     *Location
	}{
		{"scalar_definition_registration_1_below", "[&a 1]", 191, false, 0, loc(4, 1, 5)},
		{"scalar_definition_registration_1_at", "[&a 1]", 192, false, 1, nil},
		{"scalar_definition_registration_1_above", "[&a 1]", 193, false, 1, nil},
		{"scalar_alias_registration_1_below", "[&a 1, *a]", 191, false, 0, loc(4, 1, 5)},
		{"scalar_alias_registration_1_at", "[&a 1, *a]", 192, false, 1, loc(7, 1, 8)},
		{"scalar_alias_registration_1_above", "[&a 1, *a]", 193, false, 1, loc(7, 1, 8)},
		{"redefinition_registration_1_below", "[&a 1, &a 2, *a]", 191, false, 0, loc(4, 1, 5)},
		{"redefinition_registration_1_at", "[&a 1, &a 2, *a]", 192, false, 1, loc(10, 1, 11)},
		{"redefinition_registration_1_above", "[&a 1, &a 2, *a]", 193, false, 1, loc(10, 1, 11)},
		{"redefinition_registration_2_below", "[&a 1, &a 2, *a]", 319, false, 1, loc(10, 1, 11)},
		{"redefinition_registration_2_at", "[&a 1, &a 2, *a]", 320, false, 2, loc(13, 1, 14)},
		{"redefinition_registration_2_above", "[&a 1, &a 2, *a]", 321, false, 2, loc(13, 1, 14)},
		{"nested_distinct_registration_1_below", "[&a [&b 1], *a]", 255, false, 0, loc(8, 1, 9)},
		{"nested_distinct_registration_1_at", "[&a [&b 1], *a]", 256, false, 1, loc(4, 1, 5)},
		{"nested_distinct_registration_1_above", "[&a [&b 1], *a]", 257, false, 1, loc(4, 1, 5)},
		{"nested_distinct_registration_2_below", "[&a [&b 1], *a]", 383, false, 1, loc(4, 1, 5)},
		{"nested_distinct_registration_2_at", "[&a [&b 1], *a]", 384, false, 2, loc(12, 1, 13)},
		{"nested_distinct_registration_2_above", "[&a [&b 1], *a]", 385, false, 2, loc(12, 1, 13)},
		{"nested_same_name_registration_1_below", "[&a [&a 1], *a]", 255, false, 0, loc(8, 1, 9)},
		{"nested_same_name_registration_1_at", "[&a [&a 1], *a]", 256, false, 1, loc(4, 1, 5)},
		{"nested_same_name_registration_1_above", "[&a [&a 1], *a]", 257, false, 1, loc(4, 1, 5)},
		{"nested_same_name_registration_2_below", "[&a [&a 1], *a]", 383, false, 1, loc(4, 1, 5)},
		{"nested_same_name_registration_2_at", "[&a [&a 1], *a]", 384, false, 2, loc(12, 1, 13)},
		{"nested_same_name_registration_2_above", "[&a [&a 1], *a]", 385, false, 2, loc(12, 1, 13)},
		{"retained_scalar_alias_registration_1_below", "[&a !t x, *a]", 329, false, 0, loc(7, 1, 8)},
		{"retained_scalar_alias_registration_1_at", "[&a !t x, *a]", 330, false, 1, loc(10, 1, 11)},
		{"retained_scalar_alias_registration_1_above", "[&a !t x, *a]", 331, false, 1, loc(10, 1, 11)},
		{"stream_registration_1_below", "--- &a 1\n--- &a 2\n", 127, true, 0, loc(7, 1, 8)},
		{"stream_registration_1_at", "--- &a 1\n--- &a 2\n", 128, true, 1, loc(16, 2, 8)},
		{"stream_registration_1_above", "--- &a 1\n--- &a 2\n", 129, true, 1, loc(16, 2, 8)},
		{"stream_registration_2_below", "--- &a 1\n--- &a 2\n", 255, true, 1, loc(16, 2, 8)},
		{"stream_registration_2_at", "--- &a 1\n--- &a 2\n", 256, true, 2, nil},
		{"stream_registration_2_above", "--- &a 1\n--- &a 2\n", 257, true, 2, nil},
		{"root_scalar_registration_1_below", "&a 1", 127, false, 0, loc(3, 1, 4)},
		{"root_scalar_registration_1_at", "&a 1", 128, false, 1, nil},
		{"root_scalar_registration_1_above", "&a 1", 129, false, 1, nil},
		{"core_scalar_registration_1_below", "&a !!int 1", 127, false, 0, loc(9, 1, 10)},
		{"core_scalar_registration_1_at", "&a !!int 1", 128, false, 1, nil},
		{"core_scalar_registration_1_above", "&a !!int 1", 129, false, 1, nil},
		{"empty_sequence_registration_1_below", "&a []", 127, false, 0, loc(3, 1, 4)},
		{"empty_sequence_registration_1_at", "&a []", 128, false, 1, nil},
		{"empty_sequence_registration_1_above", "&a []", 129, false, 1, nil},
		{"empty_mapping_registration_1_below", "&a {}", 127, false, 0, loc(3, 1, 4)},
		{"empty_mapping_registration_1_at", "&a {}", 128, false, 1, nil},
		{"empty_mapping_registration_1_above", "&a {}", 129, false, 1, nil},
		{"retained_scalar_registration_1_below", "&a !t x", 265, false, 0, loc(6, 1, 7)},
		{"retained_scalar_registration_1_at", "&a !t x", 266, false, 1, nil},
		{"retained_scalar_registration_1_above", "&a !t x", 267, false, 1, nil},
		{"utf8_scalar_registration_1_below", "&a \"é🙂\"", 139, false, 0, loc(3, 1, 4)},
		{"utf8_scalar_registration_1_at", "&a \"é🙂\"", 140, false, 1, nil},
		{"utf8_scalar_registration_1_above", "&a \"é🙂\"", 141, false, 1, nil},
		{"sequence_registration_1_below", "&a [1]", 255, false, 0, loc(3, 1, 4)},
		{"sequence_registration_1_at", "&a [1]", 256, false, 1, nil},
		{"sequence_registration_1_above", "&a [1]", 257, false, 1, nil},
		{"core_sequence_registration_1_below", "&a !!seq [1]", 255, false, 0, loc(9, 1, 10)},
		{"core_sequence_registration_1_at", "&a !!seq [1]", 256, false, 1, nil},
		{"core_sequence_registration_1_above", "&a !!seq [1]", 257, false, 1, nil},
		{"retained_sequence_registration_1_below", "&a !t [1]", 391, false, 0, loc(6, 1, 7)},
		{"retained_sequence_registration_1_at", "&a !t [1]", 392, false, 1, nil},
		{"retained_sequence_registration_1_above", "&a !t [1]", 393, false, 1, nil},
		{"collection_redefinition_registration_1_below", "[&a [], &a [2], *a]", 191, false, 0, loc(4, 1, 5)},
		{"collection_redefinition_registration_1_at", "[&a [], &a [2], *a]", 192, false, 1, loc(11, 1, 12)},
		{"collection_redefinition_registration_1_above", "[&a [], &a [2], *a]", 193, false, 1, loc(11, 1, 12)},
		{"collection_redefinition_registration_2_below", "[&a [], &a [2], *a]", 447, false, 1, loc(11, 1, 12)},
		{"collection_redefinition_registration_2_at", "[&a [], &a [2], *a]", 448, false, 2, loc(16, 1, 17)},
		{"collection_redefinition_registration_2_above", "[&a [], &a [2], *a]", 449, false, 2, loc(16, 1, 17)},
		{"bom_registration_1_below", "\uFEFF[&a 1]", 191, false, 0, loc(7, 1, 5)},
		{"bom_registration_1_at", "\uFEFF[&a 1]", 192, false, 1, nil},
		{"bom_registration_1_above", "\uFEFF[&a 1]", 193, false, 1, nil},
		{"unicode_prefix_registration_1_below", "[\"é🙂\", &a 1]", 261, false, 0, loc(14, 1, 11)},
		{"unicode_prefix_registration_1_at", "[\"é🙂\", &a 1]", 262, false, 1, nil},
		{"unicode_prefix_registration_1_above", "[\"é🙂\", &a 1]", 263, false, 1, nil},
		{"lf_registration_1_below", "[\n &a 1\n]", 191, false, 0, loc(6, 2, 5)},
		{"lf_registration_1_at", "[\n &a 1\n]", 192, false, 1, nil},
		{"lf_registration_1_above", "[\n &a 1\n]", 193, false, 1, nil},
		{"crlf_registration_1_below", "[\r\n &a 1\r\n]", 191, false, 0, loc(7, 2, 5)},
		{"crlf_registration_1_at", "[\r\n &a 1\r\n]", 192, false, 1, nil},
		{"crlf_registration_1_above", "[\r\n &a 1\r\n]", 193, false, 1, nil},
		{"cr_registration_1_below", "[\r &a 1\r]", 191, false, 0, loc(6, 2, 5)},
		{"cr_registration_1_at", "[\r &a 1\r]", 192, false, 1, nil},
		{"cr_registration_1_above", "[\r &a 1\r]", 193, false, 1, nil},
		{"bom_unicode_crlf_registration_1_below", "\uFEFF[\"é🙂\",\r\n &a !!int 1\r\n]", 261, false, 0, loc(25, 2, 11)},
		{"bom_unicode_crlf_registration_1_at", "\uFEFF[\"é🙂\",\r\n &a !!int 1\r\n]", 262, false, 1, nil},
		{"bom_unicode_crlf_registration_1_above", "\uFEFF[\"é🙂\",\r\n &a !!int 1\r\n]", 263, false, 1, nil},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			o := makeOpts(tc.owned)
			copies = 0
			var err *Error
			if tc.stream {
				_, err = LoadAllWith(tc.input, o)
			} else {
				_, err = LoadWith(tc.input, o)
			}
			if copies != tc.wantCopies {
				t.Errorf("registration clone attempts: got %d, want %d", copies, tc.wantCopies)
			}
			if tc.wantAt != nil {
				if err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxOwnedBytes || err.Detail.Configured != uint64(tc.owned) {
					t.Fatalf("expected MaxOwnedBytes(%d), got %+v", tc.owned, err)
				}
				gotLoc, ok := err.Location()
				if !ok || gotLoc != *tc.wantAt {
					t.Errorf("location: got (%v, %v), want %v", gotLoc, ok, *tc.wantAt)
				}
			} else if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
		})
	}
}

func TestValueAndKeyImmutability(t *testing.T) {
	// 1. Integer.BigInt() returns a defensive copy; mutating it never mutates Integer or Mapping keys.
	i := IntegerFromInt64(42)
	b := i.BigInt()
	b.SetInt64(999999)
	if i.String() != "42" {
		t.Fatalf("Integer mutated via BigInt(): got %s, want 42", i.String())
	}

	m := NewMapping()
	keyNode := NewSyntheticNode(IntValue(i))
	valNode := NewSyntheticNode(StringValue("answer"))
	if insErr := m.InsertNew(keyNode, valNode); insErr != nil {
		t.Fatalf("InsertNew: %v", insErr)
	}
	k0, _ := m.At(0)
	kb, _ := k0.Value().AsInt()
	kb.BigInt().SetInt64(-12345)
	found, ok := m.GetValue(IntValue(IntegerFromInt64(42)))
	if !ok || found == nil {
		t.Fatal("Mapping lookup failed after mutating BigInt() returned from key")
	}

	// 2. Mapping.Entries() returns a defensive copy; mutating the slice does not mutate Mapping.
	entries := m.Entries()
	entries[0].Key = NewSyntheticNode(StringValue("tampered"))
	kAfter, _ := m.At(0)
	if kAfter.Value().Kind() != ValueKindInt {
		t.Fatalf("Mapping mutated via Entries() slice: got kind %v", kAfter.Value().Kind())
	}

	// 3. Negative zero is normalized to +0 in NewSignedMagnitudeInteger and IntegerFromBigInt.
	z1, ok := NewSignedMagnitudeInteger(true, 0, 0)
	if !ok || z1.IsNegative() || z1.String() != "0" {
		t.Fatalf("negative zero not normalized: %+v", z1)
	}
	z2, ok := IntegerFromBigInt(big.NewInt(0))
	if !ok || z2.IsNegative() || z2.String() != "0" {
		t.Fatalf("zero big.Int not normalized: %+v", z2)
	}

	// 4. Loaded composite mapping keys (sequence, mapping, tagged) are deep-isolated across At, Entries, GetValue, AsSequence, AsMapping, AsTagged.
	compDoc, err := Load("? [a]\n: value\n? {k: v1}\n: mapval\n? !custom [x]\n: tagval\n")
	if err != nil {
		t.Fatalf("Load composite keys: %v", err)
	}
	cm, _ := compDoc.Root().Value().AsMapping()
	seqKey, _ := cm.At(0)
	seqSlice, _ := seqKey.Value().AsSequence()
	seqSlice[0] = NewSyntheticNode(StringValue("b"))
	reSeqKey, _ := cm.At(0)
	reSeqSlice, _ := reSeqKey.Value().AsSequence()
	reSeqElem, _ := reSeqSlice[0].Value().AsStr()
	if reSeqElem != "a" {
		t.Fatalf("loaded sequence key mutated via AsSequence: got %q, want \"a\"", reSeqElem)
	}
	if _, found := cm.GetValue(reSeqKey.Value()); !found {
		t.Fatal("loaded sequence key lookup failed after attempting mutation via AsSequence")
	}

	entriesCopy := cm.Entries()
	entSeq, _ := entriesCopy[0].Key.Value().AsSequence()
	entSeq[0] = NewSyntheticNode(StringValue("c"))
	if _, found := cm.GetValue(reSeqKey.Value()); !found {
		t.Fatal("loaded sequence key lookup failed after mutating Entries() element")
	}

	mapKey, _ := cm.At(1)
	innerMap, _ := mapKey.Value().AsMapping()
	_ = innerMap.InsertNew(NewSyntheticNode(StringValue("extra")), NewSyntheticNode(StringValue("v2")))
	reMapKey, _ := cm.At(1)
	reInnerMap, _ := reMapKey.Value().AsMapping()
	if reInnerMap.Len() != 1 {
		t.Fatalf("loaded mapping key mutated via AsMapping.InsertNew: len=%d, want 1", reInnerMap.Len())
	}
	if _, found := cm.GetValue(reMapKey.Value()); !found {
		t.Fatal("loaded mapping key lookup failed after mutating returned inner Mapping")
	}

	tagKey, _ := cm.At(2)
	innerTag, _ := tagKey.Value().AsTagged()
	*innerTag.Value() = NewSyntheticNode(StringValue("mutated"))
	reTagKey, _ := cm.At(2)
	if _, found := cm.GetValue(reTagKey.Value()); !found {
		t.Fatal("loaded tagged key lookup failed after mutating Tagged.Value()")
	}

	// 5. Caller-constructed composite keys and duplicate-error Existing retain deep ownership.
	callerSlice := []Node{NewSyntheticNode(StringValue("orig"))}
	callerSeqVal := SequenceValue(callerSlice)
	callerSlice[0] = NewSyntheticNode(StringValue("mut1"))
	callerMap := NewMapping()
	callerKey := NewSyntheticNode(callerSeqVal)
	if insErr := callerMap.InsertNew(callerKey, NewSyntheticNode(StringValue("v"))); insErr != nil {
		t.Fatalf("callerMap.InsertNew: %v", insErr)
	}
	dupErr := callerMap.InsertNew(callerKey, NewSyntheticNode(StringValue("v2")))
	if dupErr == nil || dupErr.Kind != InsertErrorDuplicate {
		t.Fatalf("expected duplicate key error, got %+v", dupErr)
	}
	dupSeq, _ := dupErr.Existing.Value().AsSequence()
	dupSeq[0] = NewSyntheticNode(StringValue("mut2"))
	storedCallerKey, _ := callerMap.At(0)
	storedCallerSeq, _ := storedCallerKey.Value().AsSequence()
	storedStr, _ := storedCallerSeq[0].Value().AsStr()
	if storedStr != "orig" {
		t.Fatalf("caller-constructed sequence key mutated: got %q, want \"orig\"", storedStr)
	}
	if _, found := callerMap.GetValue(storedCallerKey.Value()); !found {
		t.Fatal("caller-constructed sequence key lookup failed after mutating inputs and duplicate Existing")
	}

	// 6. Value-copied Mapping and Clone() detach on mutation without corrupting or leaking across handles.
	origMap := NewMapping()
	if insErr := origMap.InsertNew(NewSyntheticNode(StringValue("a")), NewSyntheticNode(NullValue())); insErr != nil {
		t.Fatalf("origMap.InsertNew(a): %v", insErr)
	}
	copiedMap := *origMap
	if insErr := copiedMap.InsertNew(NewSyntheticNode(StringValue("b")), NewSyntheticNode(NullValue())); insErr != nil {
		t.Fatalf("copiedMap.InsertNew(b): %v", insErr)
	}
	if _, found := origMap.Get("b"); found || origMap.Len() != 1 {
		t.Fatalf("copy insertion leaked into origMap: found=%v len=%d", found, origMap.Len())
	}
	if _, found := copiedMap.Get("b"); !found || copiedMap.Len() != 2 {
		t.Fatalf("copiedMap missing inserted key b: found=%v len=%d", found, copiedMap.Len())
	}

	copiedBeforeOrigMut := *origMap
	if insErr := origMap.InsertNew(NewSyntheticNode(StringValue("c")), NewSyntheticNode(NullValue())); insErr != nil {
		t.Fatalf("origMap.InsertNew(c): %v", insErr)
	}
	if _, found := copiedBeforeOrigMut.Get("c"); found || copiedBeforeOrigMut.Len() != 1 {
		t.Fatalf("origMap insertion leaked into earlier value copy: found=%v len=%d", found, copiedBeforeOrigMut.Len())
	}
	if insErr := copiedBeforeOrigMut.InsertNew(NewSyntheticNode(StringValue("c")), NewSyntheticNode(StringValue("from-copy"))); insErr != nil {
		t.Fatalf("copiedBeforeOrigMut.InsertNew(c) failed after origMap inserted c: %v", insErr)
	}

	clonedMap := origMap.Clone()
	if insErr := clonedMap.InsertNew(NewSyntheticNode(StringValue("d")), NewSyntheticNode(NullValue())); insErr != nil {
		t.Fatalf("clonedMap.InsertNew(d): %v", insErr)
	}
	if _, found := origMap.Get("d"); found || origMap.Len() != 2 {
		t.Fatalf("clonedMap insertion leaked into origMap: found=%v len=%d", found, origMap.Len())
	}
}

func TestClosedSyntaxVocabularyAndUTF8Boundaries(t *testing.T) {
	// 1. Fabricated or zero SyntaxKind values are rejected by EncodeError; valid SyntaxKind values round-trip.
	_, synErr := Load("[")
	if synErr == nil || synErr.Kind != ErrorKindSyntax {
		t.Fatalf("expected syntax error on '[', got %+v", synErr)
	}
	badZero := *synErr
	badZero.Detail.SyntaxKind = 0
	if _, err := EncodeError(&badZero, DefaultOptions()); err == nil || err.Kind != ErrorKindInternal {
		t.Fatalf("expected ErrorKindInternal on zero SyntaxKind, got %+v", err)
	}
	badFabricated := *synErr
	badFabricated.Detail.SyntaxKind = SyntaxKind(255)
	if _, err := EncodeError(&badFabricated, DefaultOptions()); err == nil || err.Kind != ErrorKindInternal {
		t.Fatalf("expected ErrorKindInternal on fabricated SyntaxKind(255), got %+v", err)
	}
	if _, ok := badFabricated.SyntaxKind(); ok {
		t.Fatal("Error.SyntaxKind() returned true for fabricated SyntaxKind(255)")
	}
	// DecodeWire rejects unknown syntax_kind strings on the wire.
	unknownWire := `{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"made_up_code"},"site":{"source":[0,1,1]},"related":[],"path":[],"message":"bad"}}`
	if _, err := DecodeWire(unknownWire, DefaultOptions()); err == nil {
		t.Fatal("DecodeWire accepted unknown syntax_kind code")
	}

	// 2. Caller-constructed invalid UTF-8 strings are rejected across Encode, EncodePlain, ToPlain, NodeToString, and EncodeError.
	badStrNode := NewSyntheticNode(StringValue("\xff"))
	badDoc := NewDocument(badStrNode, nil, false)
	if w, err := Encode([]Document{badDoc}, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 || w != "" {
		t.Fatalf("Encode invalid UTF-8 scalar: got (%q, %+v), want ErrorKindInvalidUtf8", w, err)
	}
	if p, err := EncodePlain(badStrNode, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 || p != "" {
		t.Fatalf("EncodePlain invalid UTF-8 scalar: got (%q, %+v), want ErrorKindInvalidUtf8", p, err)
	}
	if v, err := ToPlain(badStrNode); err == nil || err.Kind != ErrorKindInvalidUtf8 || v != nil {
		t.Fatalf("ToPlain invalid UTF-8 scalar: got (%v, %+v), want ErrorKindInvalidUtf8", v, err)
	}
	if s, err := NodeToString(badStrNode); err == nil || err.Kind != ErrorKindInvalidUtf8 || s != "" {
		t.Fatalf("NodeToString invalid UTF-8 scalar: got (%q, %+v), want ErrorKindInvalidUtf8", s, err)
	}

	// Invalid UTF-8 in mapping key across Encode, EncodePlain, ToPlain.
	badMap := NewMapping()
	_ = badMap.InsertNew(NewSyntheticNode(StringValue("k\x80")), NewSyntheticNode(StringValue("ok")))
	badMapNode := NewSyntheticNode(MappingValue(badMap))
	if _, err := Encode([]Document{NewDocument(badMapNode, nil, false)}, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("Encode invalid UTF-8 key: got %+v, want ErrorKindInvalidUtf8", err)
	}
	if _, err := EncodePlain(badMapNode, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("EncodePlain invalid UTF-8 key: got %+v, want ErrorKindInvalidUtf8", err)
	}
	if _, err := ToPlain(badMapNode); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("ToPlain invalid UTF-8 key: got %+v, want ErrorKindInvalidUtf8", err)
	}

	// Invalid UTF-8 in tag URI/handle/suffix across Encode.
	badTagNode := NewSyntheticNode(TaggedValue(NewTagged(NewTag("!\xff", "!", "ok"), NewSyntheticNode(StringValue("ok")))))
	if _, err := Encode([]Document{NewDocument(badTagNode, nil, false)}, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("Encode invalid UTF-8 tag URI: got %+v, want ErrorKindInvalidUtf8", err)
	}
	badTagSuffixNode := NewSyntheticNode(TaggedValue(NewTagged(NewTag("!ok", "!", "\xff"), NewSyntheticNode(StringValue("ok")))))
	if _, err := Encode([]Document{NewDocument(badTagSuffixNode, nil, false)}, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("Encode invalid UTF-8 tag suffix: got %+v, want ErrorKindInvalidUtf8", err)
	}

	// Invalid UTF-8 in Error message, detail fields, and path keys across EncodeError.
	badMsgErr := &Error{Kind: ErrorKindCustom, Site: SiteNotSource(nil), Message: "bad \xff msg"}
	if _, err := EncodeError(badMsgErr, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("EncodeError invalid UTF-8 message: got %+v, want ErrorKindInvalidUtf8", err)
	}
	badTypeErr := &Error{
		Kind:    ErrorKindType,
		Detail:  ErrorDetail{Expected: "\xff", Found: "int", HasType: true},
		Site:    SiteNotSource(nil),
		Message: "type mismatch",
	}
	if _, err := EncodeError(badTypeErr, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("EncodeError invalid UTF-8 detail.Expected: got %+v, want ErrorKindInvalidUtf8", err)
	}
	badPathErr := &Error{
		Kind:    ErrorKindCustom,
		Site:    SiteNotSource(nil),
		Path:    []PathSegment{PathKey(StringValue("\xff"))},
		Message: "ok",
	}
	if _, err := EncodeError(badPathErr, DefaultOptions()); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("EncodeError invalid UTF-8 path key: got %+v, want ErrorKindInvalidUtf8", err)
	}

	// 3. Valid U+FFFD replacement character is preserved losslessly across Encode, DecodeWire, EncodePlain, ToPlain, and NodeToString.
	fffdNode := NewSyntheticNode(StringValue("valid \ufffd replacement"))
	fffdDoc := NewDocument(fffdNode, nil, false)
	w, err := Encode([]Document{fffdDoc}, DefaultOptions())
	if err != nil || !utf8.ValidString(w) {
		t.Fatalf("Encode U+FFFD failed: err=%v validUTF8=%v", err, utf8.ValidString(w))
	}
	env, err := DecodeWire(w, DefaultOptions())
	if err != nil || !env.HasDocuments || len(env.Documents) != 1 {
		t.Fatalf("DecodeWire U+FFFD failed: %v", err)
	}
	gotFFFD, err := NodeToString(env.Documents[0].IntoRoot())
	if err != nil || gotFFFD != "valid \ufffd replacement" {
		t.Fatalf("round-trip U+FFFD: got (%q, %v)", gotFFFD, err)
	}
	plainFFFD, err := EncodePlain(fffdNode, DefaultOptions())
	if err != nil || plainFFFD != "\"valid \ufffd replacement\"" || !utf8.ValidString(plainFFFD) {
		t.Fatalf("EncodePlain U+FFFD: got (%q, %v)", plainFFFD, err)
	}
	toPlainFFFD, err := ToPlain(fffdNode)
	if err != nil || toPlainFFFD != "valid \ufffd replacement" {
		t.Fatalf("ToPlain U+FFFD: got (%v, %v)", toPlainFFFD, err)
	}

	// 4. DecodeWire rejects nested Tagged-in-Tagged wire nodes and yaml_version components > math.MaxUint32
	// while accepting identical valid-envelope controls without the targeted fault.
	validTaggedWire := `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"tagged","tag":{"uri":"!a","handle":"!","suffix":"a"},"value":{"kind":"null","span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null}}]}`
	if _, err := DecodeWire(validTaggedWire, DefaultOptions()); err != nil {
		t.Fatalf("DecodeWire rejected valid single-tagged control envelope: %v", err)
	}
	nestedTaggedWire := `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"tagged","tag":{"uri":"!a","handle":"!","suffix":"a"},"value":{"kind":"tagged","tag":{"uri":"!b","handle":"!","suffix":"b"},"value":{"kind":"null","span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null}}]}`
	if _, err := DecodeWire(nestedTaggedWire, DefaultOptions()); err == nil {
		t.Fatal("DecodeWire accepted nested Tagged-in-Tagged wire node")
	}
	validMaxVerWire := `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":[4294967295,4294967295],"explicit_start":false,"root":{"kind":"null","span":null,"via":null,"tag_start":null}}]}`
	if _, err := DecodeWire(validMaxVerWire, DefaultOptions()); err != nil {
		t.Fatalf("DecodeWire rejected valid max-uint32 yaml_version control envelope: %v", err)
	}
	overflowMajorVerWire := `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":[4294967296,0],"explicit_start":false,"root":{"kind":"null","span":null,"via":null,"tag_start":null}}]}`
	if _, err := DecodeWire(overflowMajorVerWire, DefaultOptions()); err == nil {
		t.Fatal("DecodeWire accepted yaml_version major > math.MaxUint32")
	}
	overflowMinorVerWire := `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":[0,4294967296],"explicit_start":false,"root":{"kind":"null","span":null,"via":null,"tag_start":null}}]}`
	if _, err := DecodeWire(overflowMinorVerWire, DefaultOptions()); err == nil {
		t.Fatal("DecodeWire accepted yaml_version minor > math.MaxUint32")
	}

	// 5. Exact MaxOwnedBytes W / W-1 boundaries for NodeOverhead (64), EntryOverhead (24), Tag weight, and MergeKeysApply metrics().
	checkBudgetBoundary := func(name, input string, opts Options, exactW int) {
		t.Helper()
		passOpts := opts
		passOpts.Limits.MaxScalarBytes = 256
		passOpts.Limits.MaxKeyBytes = 256
		passOpts.Limits.MaxOwnedBytes = exactW
		if _, err := LoadWith(input, passOpts); err != nil {
			t.Fatalf("%s at exact W=%d failed: %v", name, exactW, err)
		}
		failOpts := passOpts
		failOpts.Limits.MaxOwnedBytes = exactW - 1
		if _, err := LoadWith(input, failOpts); err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxOwnedBytes {
			t.Fatalf("%s at W-1=%d: got %+v, want LimitExceeded(MaxOwnedBytes)", name, exactW-1, err)
		}
	}
	checkBudgetBoundary("mapping-entries", "a: b\nc: d\n", DefaultOptions(), 372)
	tagOpts := DefaultOptions()
	tagOpts.UnknownTags = UnknownTagsPreserve
	checkBudgetBoundary("tagged-key-val", "!tk key: !tv val\n", tagOpts, 362)
	mergeOpts := DefaultOptions()
	mergeOpts.MergeKeys = MergeKeysApply
	checkBudgetBoundary("merge-apply-metrics", "src: &s\n  k: hello\ndst:\n  <<: *s\n", mergeOpts, 1224)
}

func TestTypedConversionsAndToPlain(t *testing.T) {
	// Zero-value Options{} fails Validate with ErrorKindInvalidOptions.
	if _, err := LoadWith("a: 1", Options{}); err == nil || err.Kind != ErrorKindInvalidOptions {
		t.Fatalf("expected ErrorKindInvalidOptions on zero Options{}, got %+v", err)
	}

	// 128-bit boundaries: i128::MIN, i128::MAX, i128::MAX + 1, u128::MAX
	doc, err := Load("[-170141183460469231731687303715884105728, 170141183460469231731687303715884105727, 170141183460469231731687303715884105728, 340282366920938463463374607431768211455]")
	if err != nil {
		t.Fatalf("Load 128-bit boundaries: %v", err)
	}
	seq, _ := doc.Root().Value().AsSequence()

	// i128::MIN -> valid i128, out of range for u128 and int64
	if _, e := NodeToInt128(seq[0]); e != nil {
		t.Errorf("NodeToInt128(i128::MIN) failed: %v", e)
	}
	if _, e := NodeToUint128(seq[0]); e == nil || e.Kind != ErrorKindIntegerOutOfRange {
		t.Errorf("NodeToUint128(i128::MIN) expected IntegerOutOfRange, got %+v", e)
	}
	if _, e := NodeToInt64(seq[0]); e == nil || e.Kind != ErrorKindIntegerOutOfRange {
		t.Errorf("NodeToInt64(i128::MIN) expected IntegerOutOfRange, got %+v", e)
	}

	// i128::MAX -> valid i128 and u128
	if _, e := NodeToInt128(seq[1]); e != nil {
		t.Errorf("NodeToInt128(i128::MAX) failed: %v", e)
	}
	if _, e := NodeToUint128(seq[1]); e != nil {
		t.Errorf("NodeToUint128(i128::MAX) failed: %v", e)
	}

	// i128::MAX + 1 -> out of range for i128, valid for u128
	if _, e := NodeToInt128(seq[2]); e == nil || e.Kind != ErrorKindIntegerOutOfRange {
		t.Errorf("NodeToInt128(i128::MAX+1) expected IntegerOutOfRange, got %+v", e)
	}
	if _, e := NodeToUint128(seq[2]); e != nil {
		t.Errorf("NodeToUint128(i128::MAX+1) failed: %v", e)
	}

	// u128::MAX -> out of range for i128, valid for u128
	if _, e := NodeToInt128(seq[3]); e == nil || e.Kind != ErrorKindIntegerOutOfRange {
		t.Errorf("NodeToInt128(u128::MAX) expected IntegerOutOfRange, got %+v", e)
	}
	if _, e := NodeToUint128(seq[3]); e != nil {
		t.Errorf("NodeToUint128(u128::MAX) failed: %v", e)
	}

	// ToPlain loss matrix: int64 fits -> int64; outside int64 -> ErrorKindType
	plainDoc, err := Load("9223372036854775807")
	if err != nil {
		t.Fatalf("Load int64::MAX: %v", err)
	}
	pv, perr := ToPlain(plainDoc.IntoRoot())
	if perr != nil || pv != int64(math.MaxInt64) {
		t.Errorf("ToPlain(int64::MAX): got (%v, %v)", pv, perr)
	}
	if _, perr := ToPlain(seq[0]); perr == nil || perr.Kind != ErrorKindType {
		t.Errorf("ToPlain(i128::MIN) expected ErrorKindType, got %+v", perr)
	}
}
