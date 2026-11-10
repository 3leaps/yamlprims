package yaml

import (
	"math"
	"testing"
)

func TestSpecV15EmitStylesCompatQuotingAndStreamBoundaries(t *testing.T) {
	safeOpts := DefaultEmitOptions()
	coreOpts := DefaultEmitOptions()
	coreOpts.CompatQuoting = CompatQuotingCoreOnly

	if s, err := Emit(NewSyntheticNode(NullValue())); err != nil || s != "null\n" {
		t.Fatalf("Emit(Null): got %q, %v", s, err)
	}
	if s, err := Emit(NewSyntheticNode(FloatValue(math.Copysign(0, -1)))); err != nil || s != "-0.0\n" {
		t.Fatalf("Emit(-0.0): got %q, %v", s, err)
	}
	if s, err := Emit(NewSyntheticNode(FloatValue(math.Inf(1)))); err != nil || s != ".inf\n" {
		t.Fatalf("Emit(+Inf): got %q, %v", s, err)
	}
	if s, err := Emit(NewSyntheticNode(FloatValue(math.NaN()))); err != nil || s != ".nan\n" {
		t.Fatalf("Emit(NaN): got %q, %v", s, err)
	}
	if s, err := Emit(NewSyntheticNode(FloatValue(1e21))); err != nil || s != "1.0e+21\n" {
		t.Fatalf("Emit(1e21): got %q, %v", s, err)
	}
	if s, err := Emit(NewSyntheticNode(FloatValue(1.5e-7))); err != nil || s != "1.5e-7\n" {
		t.Fatalf("Emit(1.5e-7): got %q, %v", s, err)
	}

	for _, hazard := range []string{
		"y", "N", "yes", "No", "ON", "off", "0_7", "0b1010", "0x1_a",
		"1_000", "1,000", "1.2.3", "1_0.5_0", "10:20", "1_0:20", "1:05:30.5_0",
		"2026-11-09", "2026-11-09T12:00:00Z",
	} {
		n := NewSyntheticNode(StringValue(hazard))
		safeYAML, err := EmitWith(n, safeOpts)
		if err != nil || safeYAML != "'"+hazard+"'\n" {
			t.Fatalf("EmitWith(%q, Yaml11Safe): got %q, %v", hazard, safeYAML, err)
		}
		coreYAML, err := EmitWith(n, coreOpts)
		if err != nil || coreYAML != hazard+"\n" {
			t.Fatalf("EmitWith(%q, CoreOnly): got %q, %v", hazard, coreYAML, err)
		}
		docSafe, err := Load(safeYAML)
		if err != nil || !KeyEq(docSafe.Root().Value(), n.Value()) {
			t.Fatalf("reload safe %q failed: %v", hazard, err)
		}
		docCore, err := Load(coreYAML)
		if err != nil || !KeyEq(docCore.Root().Value(), n.Value()) {
			t.Fatalf("reload core %q failed: %v", hazard, err)
		}
	}

	// Stream boundaries
	if s, err := EmitAll(nil); err != nil || s != "" {
		t.Fatalf("EmitAll(nil): got %q, %v", s, err)
	}
	v12 := [2]uint32{1, 2}
	d0, _ := TryNewDocumentWithMetadata(NewSyntheticNode(StringValue("first")), &v12, true)
	d1 := NewSimpleDocument(NewSyntheticNode(NullValue()))
	d2, _ := TryNewDocumentWithMetadata(NewSyntheticNode(BoolValue(false)), &v12, true)
	stream, err := EmitAll([]Document{d0, d1, d2})
	if err != nil {
		t.Fatalf("EmitAll([d0,d1,d2]): %v", err)
	}
	wantStream := "%YAML 1.2\n---\nfirst\n---\nnull\n...\n%YAML 1.2\n---\nfalse\n"
	if stream != wantStream {
		t.Fatalf("EmitAll stream = %q, want %q", stream, wantStream)
	}
	reloaded, err := LoadAll(stream)
	if err != nil || len(reloaded) != 3 {
		t.Fatalf("LoadAll(stream): %v, len=%d", err, len(reloaded))
	}
}

func TestSpecV15FromPlainAndMarshalDeterministicMapSort(t *testing.T) {
	type sampleStruct struct {
		Name   string            `yaml:"name"`
		Flags  map[string]string `yaml:"flags"`
		OmitMe int               `yaml:"omit_me,omitempty"`
		SkipMe string            `yaml:"-"`
	}
	in := sampleStruct{
		Name: "yes",
		Flags: map[string]string{
			"yes": "1",
			"a":   "2",
			"10":  "3",
		},
		OmitMe: 0,
		SkipMe: "ignored",
	}
	out, err := Marshal(in)
	if err != nil {
		t.Fatalf("Marshal: %v", err)
	}
	// Under CompatQuotingCoreOnly key sort:
	// "10" -> "'10'\n", "a" -> "a\n", "yes" -> "yes\n"
	// byte order: "'10'\n" < "a\n" < "yes\n", and under Yaml11Safe emission "yes" is quoted as 'yes'.
	want := "name: 'yes'\nflags:\n  '10': '3'\n  a: '2'\n  'yes': '1'\n"
	if out != want {
		t.Fatalf("Marshal = %q, want %q", out, want)
	}

	// NewTag(uri, "", "") and caller Mapping ownership preservation across Marshal
	tagNode := NewSyntheticNode(TaggedValue(NewTagged(NewTag("!foo", "", ""), NewSyntheticNode(StringValue("bar")))))
	tagYAML, err := Marshal(tagNode)
	if err != nil || tagYAML != "!foo bar\n" {
		t.Fatalf("Marshal(NewTag): got %q, %v", tagYAML, err)
	}

	emptyCoreTag := NewSyntheticNode(TaggedValue(NewTagged(NewTag("tag:yaml.org,2002:", "", ""), NewSyntheticNode(StringValue("v")))))
	emptyCoreYAML, err := Emit(emptyCoreTag)
	if err != nil || emptyCoreYAML != "!<tag:yaml.org,2002:> v\n" {
		t.Fatalf("Emit(tag:yaml.org,2002:): got %q, %v", emptyCoreYAML, err)
	}

	// Self-referential interface/pointer cycle bounded by MaxDepth
	var cyc any
	cyc = &cyc
	if _, err := FromPlain(cyc, DefaultEmitOptions()); err == nil {
		t.Fatalf("expected error on cyclic interface pointer")
	}

	// Reordered JSON object members in DecodeWire
	reorderedWire := `{"documents":[{"root":{"value":"ok","tag_start":null,"via":null,"span":null,"kind":"string"},"explicit_start":false,"yaml_version":null}],"version":1,"format":"yamlprims-value"}`
	env, err := DecodeWire(reorderedWire, DefaultOptions())
	if err != nil || env.Error != nil || len(env.Documents) != 1 {
		t.Fatalf("DecodeWire(reordered): got %+v, %v", env, err)
	}

	// FromPlain on a loaded Document.Root() strips Span, Via, and TagStart at all depths
	// without mutating the caller's original Node tree.
	loadedDoc, err := Load("items:\n  - !custom 'hello'\n")
	if err != nil {
		t.Fatalf("Load for FromPlain metadata strip test: %v", err)
	}
	origRoot := loadedDoc.Root()
	synthNode, err := FromPlain(origRoot, DefaultEmitOptions())
	if err != nil {
		t.Fatalf("FromPlain(origRoot): %v", err)
	}
	hasSpan := func(n Node) bool {
		_, ok := n.Span()
		return ok
	}
	if hasSpan(synthNode) || synthNode.Via() != nil || synthNode.TagStart() != nil {
		t.Fatalf("expected root metadata cleared on FromPlain result")
	}
	synthMap, _ := synthNode.Value().AsMapping()
	synthEntry := synthMap.Entries()[0]
	if hasSpan(synthEntry.Key) || synthEntry.Key.Via() != nil || hasSpan(synthEntry.Value) {
		t.Fatalf("expected mapping entry metadata cleared on FromPlain result")
	}
	synthSeq, _ := synthEntry.Value.Value().AsSequence()
	synthTaggedNode := synthSeq[0]
	if hasSpan(synthTaggedNode) || synthTaggedNode.TagStart() != nil {
		t.Fatalf("expected tagged node metadata cleared on FromPlain result")
	}
	synthTagged, _ := synthTaggedNode.Value().AsTagged()
	if hasSpan(*synthTagged.Value()) || synthTagged.Value().Via() != nil {
		t.Fatalf("expected tagged inner node metadata cleared on FromPlain result")
	}
	// Caller's original tree still retains its spans
	if !hasSpan(*origRoot) {
		t.Fatalf("caller origRoot Span was mutated")
	}
}
