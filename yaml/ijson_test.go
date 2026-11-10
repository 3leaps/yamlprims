package yaml

import (
	"fmt"
	"strings"
	"testing"
)

func TestSpecV15CheckedConstructorsAndTagValidation(t *testing.T) {
	local, err := TryNewTag("!local")
	if err != nil {
		t.Fatalf("TryNewTag(!local): %v", err)
	}
	if local.URI() != "!local" || local.Handle() != "" || local.Suffix() != "!local" {
		t.Fatalf("unexpected local tag: %+v", local)
	}

	emptySSP, err := TryNewTag("x:")
	if err != nil || emptySSP.URI() != "x:" {
		t.Fatalf("TryNewTag(x:): %v, %+v", err, emptySSP)
	}

	commaURI, err := TryNewTag("tag:example.com,2026:v\n1")
	if err != nil || commaURI.URI() != "tag:example.com,2026:v\n1" {
		t.Fatalf("TryNewTag(comma+newline): %v", err)
	}

	longURI := "!" + strings.Repeat("a", 2050)
	longTag, err := TryNewTag(longURI)
	if err != nil || longTag.URI() != longURI {
		t.Fatalf("TryNewTag(>2KiB): %v", err)
	}

	for _, sp := range []struct {
		u, h, s string
	}{
		{"!widget", "!", "widget"},
		{"tag:yaml.org,2002:timestamp", "!!", "timestamp"},
		{"tag:example.com,2026:item", "!e!", "item"},
		{"tag:example.com,2026:item", "!", "item"},
	} {
		if _, err := TryNewTagWithSpelling(sp.u, sp.h, sp.s); err != nil {
			t.Fatalf("TryNewTagWithSpelling(%q, %q, %q): %v", sp.u, sp.h, sp.s, err)
		}
	}

	for _, bad := range []string{
		"",
		"!",
		"$:?",
		"1http:foo",
		"noscheme",
		"tag:yaml.org,2002:null",
		"tag:yaml.org,2002:bool",
		"tag:yaml.org,2002:int",
		"tag:yaml.org,2002:float",
		"tag:yaml.org,2002:str",
		"tag:yaml.org,2002:seq",
		"tag:yaml.org,2002:map",
	} {
		if _, err := TryNewTag(bad); err == nil || err.Kind != ErrorKindInvalidOptions {
			t.Fatalf("TryNewTag(%q): got %+v, want InvalidOptions", bad, err)
		}
	}

	for _, badSp := range []struct {
		u, h, s string
	}{
		{"!foo", "", "foo"},
		{"!foo", "!", ""},
		{"!foo", "!", "bar"},
		{"!foo", "!bad_handle!", "foo"},
		{"!foo", "!", "!foo"},
	} {
		if _, err := TryNewTagWithSpelling(badSp.u, badSp.h, badSp.s); err == nil || err.Kind != ErrorKindInvalidOptions {
			t.Fatalf("TryNewTagWithSpelling(%q, %q, %q): got %+v, want InvalidOptions", badSp.u, badSp.h, badSp.s, err)
		}
	}

	strNode := NewSyntheticNode(StringValue("hello"))
	seqNode := NewSyntheticNode(SequenceValue(nil))
	mapNode := NewSyntheticNode(MappingValue(NewMapping()))

	if _, err := TryNewTagged(local, strNode); err != nil {
		t.Fatalf("TryNewTagged(string): %v", err)
	}
	if _, err := TryNewTagged(local, seqNode); err != nil {
		t.Fatalf("TryNewTagged(seq): %v", err)
	}
	if _, err := TryNewTagged(local, mapNode); err != nil {
		t.Fatalf("TryNewTagged(map): %v", err)
	}

	mergeTag, _ := TryNewTag("tag:yaml.org,2002:merge")
	if _, err := TryNewTagged(mergeTag, strNode); err != nil {
		t.Fatalf("TryNewTagged(merge, string): %v", err)
	}
	if _, err := TryNewTagged(mergeTag, seqNode); err == nil || err.Kind != ErrorKindTagKindMismatch {
		t.Fatalf("TryNewTagged(merge, seq): got %+v, want TagKindMismatch", err)
	}
	if _, err := TryNewTagged(mergeTag, mapNode); err == nil || err.Kind != ErrorKindTagKindMismatch {
		t.Fatalf("TryNewTagged(merge, map): got %+v, want TagKindMismatch", err)
	}

	innerTagged, _ := TryNewTagged(local, strNode)
	for _, badInner := range []Node{
		NewSyntheticNode(NullValue()),
		NewSyntheticNode(BoolValue(true)),
		NewSyntheticNode(IntValue(IntegerFromInt64(1))),
		NewSyntheticNode(FloatValue(1.0)),
		NewSyntheticNode(TaggedValue(innerTagged)),
	} {
		if _, err := TryNewTagged(local, badInner); err == nil || err.Kind != ErrorKindTagKindMismatch {
			t.Fatalf("TryNewTagged(badInner): got %+v, want TagKindMismatch", err)
		}
	}

	v12 := [2]uint32{1, 2}
	if _, err := TryNewDocumentWithMetadata(strNode, &v12, true); err != nil {
		t.Fatalf("TryNewDocumentWithMetadata(1.2): %v", err)
	}
	v20 := [2]uint32{2, 0}
	if _, err := TryNewDocumentWithMetadata(strNode, &v20, false); err == nil || err.Kind != ErrorKindUnsupportedYamlVersion {
		t.Fatalf("TryNewDocumentWithMetadata(2.0): got %+v, want UnsupportedYamlVersion", err)
	}
	vBigMinor := [2]uint32{1, 1_000_000_000}
	if _, err := TryNewDocumentWithMetadata(strNode, &vBigMinor, false); err == nil || err.Kind != ErrorKindUnsupportedYamlVersion {
		t.Fatalf("TryNewDocumentWithMetadata(1.1000000000): got %+v, want UnsupportedYamlVersion", err)
	}

	// Verify 12,000-deep caller-constructed Node.Clone() succeeds iteratively.
	cur := NewSyntheticNode(StringValue("leaf"))
	for i := 0; i < 12_000; i++ {
		if i%2 == 0 {
			cur = NewSyntheticNode(SequenceValue([]Node{cur}))
		} else {
			m := NewMapping()
			_ = m.InsertNew(NewSyntheticNode(StringValue("k")), cur)
			cur = NewSyntheticNode(MappingValue(m))
		}
	}
	cloned := cur.Clone()
	if cloned.Value().Kind() != cur.Value().Kind() {
		t.Fatalf("cloned root kind mismatch")
	}
}

func TestSpecV15DecodeWireLimitsAndHvsP(t *testing.T) {
	opts := DefaultOptions()
	longTagYAML := fmt.Sprintf("!%s hello\n", strings.Repeat("a", 2050))
	longDocs, err := LoadAllWith(longTagYAML, opts)
	if err != nil {
		t.Fatalf("LoadAllWith(>2KiB tag): %v", err)
	}
	longWire, err := Encode(longDocs, opts)
	if err != nil {
		t.Fatalf("Encode(>2KiB tag): %v", err)
	}
	env, err := DecodeWire(longWire, opts)
	if err != nil || !env.HasDocuments {
		t.Fatalf("DecodeWire(>2KiB tag): %v", err)
	}
	re, err := Encode(env.Documents, opts)
	if err != nil || re != longWire {
		t.Fatalf("round-trip mismatch on >2KiB tag")
	}

	// Document root [[]] has H = 2 (fails at MaxDepth = 1), whereas error.path[*].key [[]] has P = 1 (passes at MaxDepth = 1).
	emptyNestedDocs, err := LoadAllWith("[[]]\n", opts)
	if err != nil {
		t.Fatalf("LoadAllWith([[]]): %v", err)
	}
	docWire, _ := Encode(emptyNestedDocs, opts)
	d1Opts := DefaultOptions()
	d1Opts.Limits.MaxDepth = 1
	if _, err := DecodeWire(docWire, d1Opts); err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxDepth {
		t.Fatalf("DecodeWire(doc [[]], MaxDepth=1): got %+v, want LimitExceeded(MaxDepth)", err)
	}

	pathKeyWire := `{"format":"yamlprims-value","version":1,"error":{"kind":"custom","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[{"key":{"kind":"sequence","items":[{"kind":"sequence","items":[],"span":null,"via":null,"tag_start":null}],"span":null,"via":null,"tag_start":null}}],"message":"ok"}}`
	if _, err := DecodeWire(pathKeyWire, d1Opts); err != nil {
		t.Fatalf("DecodeWire(error path key [[]], MaxDepth=1) failed: %v", err)
	}

	n1Opts := DefaultOptions()
	n1Opts.Limits.MaxNodes = 1
	if _, err := DecodeWire(pathKeyWire, n1Opts); err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxNodes {
		t.Fatalf("DecodeWire(error path key, MaxNodes=1): got %+v, want LimitExceeded(MaxNodes)", err)
	}
}

func TestSpecV15IJSONSixRulesAndAll66Noncharacters(t *testing.T) {
	// Rule 1: single non-empty-stream document & §7.1 precedence
	for _, empty := range []string{"", "   \n  ", "# comment\n", "...\n"} {
		_, err := LoadIJSON(empty)
		if err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "single YAML document" || err.Detail.Found != "empty_stream" {
			t.Fatalf("LoadIJSON(%q): got %+v, want empty_stream Type error", empty, err)
		}
	}
	for _, nullIn := range []string{"---\n", "null\n", "~\n"} {
		got, err := LoadIJSON(nullIn)
		if err != nil || got != "null" {
			t.Fatalf("LoadIJSON(%q) = (%q, %v), want (\"null\", nil)", nullIn, got, err)
		}
	}

	multiOpts := DefaultIJSONOptions()
	multiOpts.Limits.MaxDocuments = 10
	if _, err := LoadIJSONWith("null\n---\n1\n", multiOpts); err == nil || err.Kind != ErrorKindMultipleDocuments {
		t.Fatalf("LoadIJSONWith(multi): got %+v, want MultipleDocuments", err)
	}
	if _, err := LoadIJSONWith("null\n---\n[\n", multiOpts); err == nil || err.Kind != ErrorKindSyntax {
		t.Fatalf("LoadIJSONWith(multi+syntax): got %+v, want Syntax", err)
	}
	d2Opts := DefaultIJSONOptions()
	d2Opts.Limits.MaxDepth = 2
	if _, err := LoadIJSONWith("null\n---\n[[[]]]\n", d2Opts); err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxDepth {
		t.Fatalf("LoadIJSONWith(multi+depth): got %+v, want LimitExceeded(MaxDepth)", err)
	}

	// Rule 2: IEEE 754 safe integers ±(2^53 - 1)
	if got, err := LoadIJSON("9007199254740991\n"); err != nil || got != "9007199254740991" {
		t.Fatalf("LoadIJSON(2^53-1): (%q, %v)", got, err)
	}
	if got, err := LoadIJSON("-9007199254740991\n"); err != nil || got != "-9007199254740991" {
		t.Fatalf("LoadIJSON(-(2^53-1)): (%q, %v)", got, err)
	}
	for _, unsafeInt := range []string{
		"9007199254740992\n",
		"-9007199254740992\n",
		"-9223372036854775808\n",
		"18446744073709551615\n",
	} {
		if _, err := LoadIJSON(unsafeInt); err == nil || err.Kind != ErrorKindIntegerOutOfRange {
			t.Fatalf("LoadIJSON(%q): got %+v, want IntegerOutOfRange", unsafeInt, err)
		}
	}

	// Rule 3: finite floats and RFC 8785 formatting
	for _, nf := range []string{".nan\n", ".inf\n", "-.inf\n"} {
		if _, err := LoadIJSON(nf); err == nil || err.Kind != ErrorKindFloatOutOfRange {
			t.Fatalf("LoadIJSON(%q): got %+v, want FloatOutOfRange", nf, err)
		}
	}
	if got, err := LoadIJSON("-0.0\n"); err != nil || got != "0" {
		t.Fatalf("LoadIJSON(-0.0) = (%q, %v)", got, err)
	}
	if got, err := LoadIJSON("[0.1, 1.5e-7, 123456789.0, 1e20, 1e21, -2.5]\n"); err != nil || got != "[0.1,1.5e-7,123456789,100000000000000000000,1e+21,-2.5]" {
		t.Fatalf("LoadIJSON(floats) = (%q, %v)", got, err)
	}

	// Rule 4: String-only mapping keys
	for _, badKey := range []string{
		"true: 1\n",
		"1: a\n",
		"1.5: a\n",
		"null: a\n",
		"? [1]\n: a\n",
		"? {a: 1}\n: b\n",
	} {
		_, err := LoadIJSON(badKey)
		if err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "I-JSON string key" || err.Detail.Found != "non_string_key" {
			t.Fatalf("LoadIJSON(%q): got %+v, want non_string_key", badKey, err)
		}
	}

	// Rule 5: UnknownTagsReject forced; Tagged key fails with UnknownTag before non_string_key; MergeKeys
	presOpts := DefaultOptions()
	presOpts.UnknownTags = UnknownTagsPreserve
	if _, err := LoadIJSONWith("!custom hello\n", presOpts); err == nil || err.Kind != ErrorKindUnknownTag {
		t.Fatalf("LoadIJSONWith(preserve): got %+v, want UnknownTag", err)
	}

	tkTag, _ := TryNewTag("!k")
	tkInner, _ := TryNewTagged(tkTag, NewSyntheticNode(SequenceValue(nil)))
	m := NewMapping()
	_ = m.InsertNew(NewSyntheticNode(TaggedValue(tkInner)), NewSyntheticNode(NullValue()))
	if _, err := EncodeIJSON(NewSyntheticNode(MappingValue(m))); err == nil || err.Kind != ErrorKindUnknownTag {
		t.Fatalf("EncodeIJSON(tagged key): got %+v, want UnknownTag", err)
	}

	mergeYAML := "base: &b\n  x: 1\n  y: 2\nderived:\n  <<: *b\n  y: 3\n"
	if _, err := LoadIJSON(mergeYAML); err == nil || err.Kind != ErrorKindMergeRejected {
		t.Fatalf("LoadIJSON(merge): got %+v, want MergeRejected", err)
	}
	applyOpts := DefaultIJSONOptions()
	applyOpts.MergeKeys = MergeKeysApply
	if got, err := LoadIJSONWith(mergeYAML, applyOpts); err != nil || got != `{"base":{"x":1,"y":2},"derived":{"x":1,"y":3}}` {
		t.Fatalf("LoadIJSONWith(MergeKeysApply) = (%q, %v)", got, err)
	}
	asKeyOpts := DefaultIJSONOptions()
	asKeyOpts.MergeKeys = MergeKeysAsKey
	if _, err := LoadIJSONWith(mergeYAML, asKeyOpts); err == nil || err.Kind != ErrorKindInvalidOptions {
		t.Fatalf("LoadIJSONWith(MergeKeysAsKey): got %+v, want InvalidOptions", err)
	}

	// Rule 6: All 66 Unicode noncharacters rejected in both values and keys + ill-formed UTF-8 rejected
	var nonchars []rune
	for cp := rune(0xFDD0); cp <= 0xFDEF; cp++ {
		nonchars = append(nonchars, cp)
	}
	for plane := uint32(0); plane <= 16; plane++ {
		base := plane << 16
		nonchars = append(nonchars, rune(base|0xFFFE), rune(base|0xFFFF))
	}
	if len(nonchars) != 66 {
		t.Fatalf("expected 66 noncharacters, got %d", len(nonchars))
	}
	for _, ch := range nonchars {
		vn := NewSyntheticNode(StringValue("a" + string(ch) + "b"))
		_, err := EncodeIJSON(vn)
		if err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "RFC 7493 I-JSON string" || err.Detail.Found != "noncharacter" {
			t.Fatalf("EncodeIJSON(nonchar U+%04X): got %+v", ch, err)
		}
		km := NewMapping()
		_ = km.InsertNew(NewSyntheticNode(StringValue(string(ch))), NewSyntheticNode(NullValue()))
		_, kerr := EncodeIJSON(NewSyntheticNode(MappingValue(km)))
		if kerr == nil || kerr.Kind != ErrorKindType || kerr.Detail.Found != "noncharacter" {
			t.Fatalf("EncodeIJSON(key nonchar U+%04X): got %+v", ch, kerr)
		}
	}

	if _, err := EncodeIJSON(NewSyntheticNode(StringValue("\xff"))); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("EncodeIJSON(invalid UTF-8): got %+v, want InvalidUtf8", err)
	}
}
