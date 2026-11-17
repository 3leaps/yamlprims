package yaml

import (
	"bytes"
	"errors"
	"io"
	"math"
	"math/big"
	"os"
	"os/exec"
	"runtime/debug"
	"strings"
	"testing"
)

type trackingLimitReader struct {
	data      []byte
	bytesRead int
	failErr   error
}

func (r *trackingLimitReader) Read(p []byte) (int, error) {
	if r.failErr != nil {
		return 0, r.failErr
	}
	if r.bytesRead >= len(r.data) {
		return 0, io.EOF
	}
	n := copy(p, r.data[r.bytesRead:])
	r.bytesRead += n
	return n, nil
}

type failingWriter struct {
	buf     bytes.Buffer
	failErr error
}

func (w *failingWriter) Write(p []byte) (int, error) {
	if w.failErr != nil {
		return 0, w.failErr
	}
	return w.buf.Write(p)
}

func TestReaderAndWriterErgonomics(t *testing.T) {
	// 1. LoadReader / LoadAllReader / UnmarshalReader cap reads at MaxInputBytes + 1.
	opts := DefaultOptions()
	opts.Limits.MaxInputBytes = 16

	trLoad := &trackingLimitReader{data: bytes.Repeat([]byte("a"), 1024)}
	_, err := LoadReaderWith(trLoad, opts)
	if err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxInputBytes {
		t.Fatalf("expected LimitExceeded(MaxInputBytes), got %+v", err)
	}
	if trLoad.bytesRead != 17 {
		t.Fatalf("LoadReaderWith read %d bytes, want 17 (MaxInputBytes+1)", trLoad.bytesRead)
	}
	if loc, ok := err.Location(); !ok || loc != (Location{ByteOffset: 0, Line: 1, Column: 1}) {
		t.Fatalf("LoadReaderWith error location = (%+v, %v), want 0:1:1", loc, ok)
	}

	trAll := &trackingLimitReader{data: bytes.Repeat([]byte("a"), 1024)}
	_, err = LoadAllReaderWith(trAll, opts)
	if err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxInputBytes {
		t.Fatalf("LoadAllReaderWith expected LimitExceeded(MaxInputBytes), got %+v", err)
	}
	if trAll.bytesRead != 17 {
		t.Fatalf("LoadAllReaderWith read %d bytes, want 17", trAll.bytesRead)
	}

	trUnm := &trackingLimitReader{data: bytes.Repeat([]byte("a"), 1024)}
	var s string
	err = UnmarshalReaderWith(trUnm, &s, opts)
	if err == nil || err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxInputBytes {
		t.Fatalf("UnmarshalReaderWith expected LimitExceeded(MaxInputBytes), got %+v", err)
	}
	if trUnm.bytesRead != 17 {
		t.Fatalf("UnmarshalReaderWith read %d bytes, want 17", trUnm.bytesRead)
	}

	// Valid LoadReader / LoadAllReader round-trip.
	doc, err := LoadReader(strings.NewReader("key: value\n"))
	if err != nil {
		t.Fatalf("LoadReader: %v", err)
	}
	m, ok := doc.Root().Value().AsMapping()
	if !ok || m.Len() != 1 {
		t.Fatalf("unexpected doc root: %+v", doc.Root())
	}
	docs, err := LoadAllReader(strings.NewReader("---\n1\n---\n2\n"))
	if err != nil || len(docs) != 2 {
		t.Fatalf("LoadAllReader: len=%d, err=%v", len(docs), err)
	}

	// Reader I/O error -> ErrorKindInternal at SiteNotSource(nil).
	ioFailReader := &trackingLimitReader{failErr: errors.New("boom")}
	if _, err := LoadReader(ioFailReader); err == nil || err.Kind != ErrorKindInternal || err.Site.Kind != SiteKindNotSource || err.Site.LastKnown != nil {
		t.Fatalf("expected ErrorKindInternal at NotSource(nil) on reader error, got %+v", err)
	}
	if _, err := LoadReader(nil); err == nil || err.Kind != ErrorKindInternal {
		t.Fatalf("expected ErrorKindInternal on nil reader, got %+v", err)
	}

	// 2. MarshalWriter / EmitWriter / EmitAllWriter write 0 bytes on OutputLimitExceeded.
	eOpts := DefaultEmitOptions()
	eOpts.Limits.MaxOutputBytes = 128
	eOpts.Limits.MaxScalarBytes = 128
	eOpts.Limits.MaxKeyBytes = 128

	bigSlice := make([]int, 60)
	for i := range bigSlice {
		bigSlice[i] = i
	}
	var outBuf bytes.Buffer
	if err := MarshalWriterWith(&outBuf, bigSlice, eOpts); err == nil || err.Kind != ErrorKindOutputLimitExceeded {
		t.Fatalf("MarshalWriterWith expected OutputLimitExceeded, got %+v", err)
	}
	if outBuf.Len() != 0 {
		t.Fatalf("MarshalWriterWith wrote %d bytes before failing, want 0", outBuf.Len())
	}

	bigNode, err := FromPlain(bigSlice)
	if err != nil {
		t.Fatalf("FromPlain: %v", err)
	}
	if err := EmitWriterWith(&outBuf, bigNode, eOpts); err == nil || err.Kind != ErrorKindOutputLimitExceeded {
		t.Fatalf("EmitWriterWith expected OutputLimitExceeded, got %+v", err)
	}
	if outBuf.Len() != 0 {
		t.Fatalf("EmitWriterWith wrote %d bytes before failing, want 0", outBuf.Len())
	}
	if err := EmitAllWriterWith(&outBuf, []Document{NewSimpleDocument(bigNode)}, eOpts); err == nil || err.Kind != ErrorKindOutputLimitExceeded {
		t.Fatalf("EmitAllWriterWith expected OutputLimitExceeded, got %+v", err)
	}
	if outBuf.Len() != 0 {
		t.Fatalf("EmitAllWriterWith wrote %d bytes before failing, want 0", outBuf.Len())
	}

	// Valid writer round-trip and writer I/O error -> ErrorKindInternal.
	if err := MarshalWriter(&outBuf, map[string]any{"enabled": "yes", "port": 8080}); err != nil {
		t.Fatalf("MarshalWriter: %v", err)
	}
	if outBuf.String() != "enabled: 'yes'\nport: 8080\n" {
		t.Fatalf("MarshalWriter output = %q", outBuf.String())
	}
	fw := &failingWriter{failErr: errors.New("disk full")}
	if err := MarshalWriter(fw, "hello"); err == nil || err.Kind != ErrorKindInternal {
		t.Fatalf("expected ErrorKindInternal on writer failure, got %+v", err)
	}
	if err := EmitWriter(nil, NewSyntheticNode(NullValue())); err == nil || err.Kind != ErrorKindInternal {
		t.Fatalf("expected ErrorKindInternal on nil writer, got %+v", err)
	}
}

func TestUnmarshalTargetsAndStrictness(t *testing.T) {
	// 1. Non-nil pointer requirement.
	if err := Unmarshal([]byte("1"), nil); err == nil || err.Kind != ErrorKindType || err.Site.Kind != SiteKindNotSource {
		t.Fatalf("expected ErrorKindType on nil target, got %+v", err)
	}
	if err := Unmarshal([]byte("1"), 42); err == nil || err.Kind != ErrorKindType || err.Site.Kind != SiteKindNotSource {
		t.Fatalf("expected ErrorKindType on non-pointer target, got %+v", err)
	}
	var nilIntPtr *int
	if err := Unmarshal([]byte("1"), nilIntPtr); err == nil || err.Kind != ErrorKindType || err.Site.Kind != SiteKindNotSource {
		t.Fatalf("expected ErrorKindType on typed nil pointer target, got %+v", err)
	}

	// 2. Zero-document stream -> implicit Null; multi-document -> MultipleDocuments.
	var optInt *int
	n := 99
	optInt = &n
	if err := Unmarshal([]byte("# comment only\n"), &optInt); err != nil || optInt != nil {
		t.Fatalf("empty stream into **int: optInt=%v, err=%v", optInt, err)
	}
	var singleInt int
	if err := Unmarshal([]byte("---\n1\n---\n2\n"), &singleInt); err == nil || err.Kind != ErrorKindMultipleDocuments {
		t.Fatalf("expected MultipleDocuments, got %+v", err)
	}

	// 3. *Node, *Value, *Document, *Tagged.
	var gotDoc Document
	if err := Unmarshal([]byte("%YAML 1.2\n---\nhello\n"), &gotDoc); err != nil {
		t.Fatalf("Unmarshal *Document: %v", err)
	}
	maj, min, hasVer := gotDoc.YamlVersion()
	if !hasVer || maj != 1 || min != 2 || !gotDoc.ExplicitStart() {
		t.Fatalf("Document metadata lost: ver=(%d,%d,%v) explicit=%v", maj, min, hasVer, gotDoc.ExplicitStart())
	}
	var gotDocFromNode Document
	if err := UnmarshalNode(NewSyntheticNode(StringValue("hi")), &gotDocFromNode); err != nil {
		t.Fatalf("UnmarshalNode *Document: %v", err)
	}
	if _, _, hasVer := gotDocFromNode.YamlVersion(); hasVer || gotDocFromNode.ExplicitStart() {
		t.Fatalf("expected default metadata from UnmarshalNode into *Document")
	}

	var gotTagged Tagged
	if err := Unmarshal([]byte("!custom payload\n"), &gotTagged); err != nil {
		t.Fatalf("Unmarshal *Tagged: %v", err)
	}
	if gotTagged.Tag().URI() != "!custom" {
		t.Fatalf("got tag %q, want !custom", gotTagged.Tag().URI())
	}
	if err := Unmarshal([]byte("untagged\n"), &gotTagged); err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "tagged value" {
		t.Fatalf("expected ErrorKindType on untagged into *Tagged, got %+v", err)
	}

	// Untagged-only requirement for standard Go targets.
	var strTarget string
	if err := Unmarshal([]byte("!custom payload\n"), &strTarget); err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "string" || err.Detail.Found != "tagged value" {
		t.Fatalf("expected ErrorKindType(expected=string, found=tagged value), got %+v", err)
	}

	// 4. *any delegates to ToPlain.
	var anyTarget any
	if err := Unmarshal([]byte("a: [true, 42, 3.5, null]\n"), &anyTarget); err != nil {
		t.Fatalf("Unmarshal *any: %v", err)
	}
	anyMap, ok := anyTarget.(map[string]any)
	if !ok {
		t.Fatalf("expected map[string]any, got %T", anyTarget)
	}
	anySeq, ok := anyMap["a"].([]any)
	if !ok || len(anySeq) != 4 || anySeq[0] != true || anySeq[1] != int64(42) || anySeq[2] != 3.5 || anySeq[3] != nil {
		t.Fatalf("unexpected *any sequence: %#v", anyMap["a"])
	}

	// 5. Exact integers, yaml.Integer, and math/big.Int (including 128-bit and >128-bit boundaries).
	var u8 uint8
	if err := Unmarshal([]byte("256\n"), &u8); err == nil || err.Kind != ErrorKindIntegerOutOfRange {
		t.Fatalf("expected IntegerOutOfRange for 256 into uint8, got %+v", err)
	}
	if err := Unmarshal([]byte("-1\n"), &u8); err == nil || err.Kind != ErrorKindIntegerOutOfRange {
		t.Fatalf("expected IntegerOutOfRange for -1 into uint8, got %+v", err)
	}
	if err := Unmarshal([]byte("\"42\"\n"), &u8); err == nil || err.Kind != ErrorKindType {
		t.Fatalf("expected ErrorKindType for string into uint8, got %+v", err)
	}

	var bigVal big.Int
	if err := Unmarshal([]byte("340282366920938463463374607431768211455\n"), &bigVal); err != nil {
		t.Fatalf("Unmarshal u128::MAX into *big.Int: %v", err)
	}
	if bigVal.String() != "340282366920938463463374607431768211455" {
		t.Fatalf("big.Int = %s, want u128::MAX", bigVal.String())
	}
	marshaledBig, err := Marshal(&bigVal)
	if err != nil || marshaledBig != "340282366920938463463374607431768211455\n" {
		t.Fatalf("Marshal *big.Int(u128::MAX) = (%q, %v)", marshaledBig, err)
	}
	tooBig := new(big.Int).Lsh(big.NewInt(1), 128)
	if _, err := Marshal(tooBig); err == nil || err.Kind != ErrorKindIntegerOutOfRange {
		t.Fatalf("Marshal 2^128 big.Int expected IntegerOutOfRange, got %+v", err)
	}

	// 6. Floats: exact Int accepted, inexact Int -> IntegerOutOfRange, float32 overflow -> FloatOutOfRange.
	var f64 float64
	if err := Unmarshal([]byte("9007199254740992\n"), &f64); err != nil || f64 != 9007199254740992.0 {
		t.Fatalf("Unmarshal 2^53 into float64: got (%v, %v)", f64, err)
	}
	if err := Unmarshal([]byte("9007199254740993\n"), &f64); err == nil || err.Kind != ErrorKindIntegerOutOfRange {
		t.Fatalf("Unmarshal 2^53+1 into float64 expected IntegerOutOfRange, got %+v", err)
	}
	var f32 float32
	if err := Unmarshal([]byte("16777217\n"), &f32); err == nil || err.Kind != ErrorKindIntegerOutOfRange {
		t.Fatalf("Unmarshal 2^24+1 into float32 expected IntegerOutOfRange, got %+v", err)
	}
	if err := Unmarshal([]byte("1e300\n"), &f32); err == nil || err.Kind != ErrorKindFloatOutOfRange {
		t.Fatalf("Unmarshal 1e300 into float32 expected FloatOutOfRange, got %+v", err)
	}

	// 7. Slices and fixed arrays.
	var sliceTarget []int
	sliceTarget = []int{1, 2}
	if err := Unmarshal([]byte("null\n"), &sliceTarget); err != nil || sliceTarget != nil {
		t.Fatalf("Null into *[]int: got (%v, %v), want nil", sliceTarget, err)
	}
	var arrTarget [2]int
	if err := Unmarshal([]byte("null\n"), &arrTarget); err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "sequence of length 2" {
		t.Fatalf("Null into *[2]int expected ErrorKindType(sequence of length 2), got %+v", err)
	}
	if err := Unmarshal([]byte("[1, 2, 3]\n"), &arrTarget); err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "sequence of length 2" {
		t.Fatalf("[1,2,3] into *[2]int expected ErrorKindType(sequence of length 2), got %+v", err)
	}
	if err := Unmarshal([]byte("[10, 20]\n"), &arrTarget); err != nil || arrTarget != [2]int{10, 20} {
		t.Fatalf("[10,20] into *[2]int: got (%v, %v)", arrTarget, err)
	}

	// 8. Maps and structs (strict unknown field rejection, non-string key rejection, and Go key collision).
	type serviceConfig struct {
		Host    string `yaml:"host"`
		Port    uint16 `yaml:"port"`
		Ignored string `yaml:"-"`
		Default bool
	}
	var cfg serviceConfig
	if err := UnmarshalReader(strings.NewReader("host: localhost\nport: 8080\nDefault: true\n"), &cfg); err != nil {
		t.Fatalf("UnmarshalReader struct: %v", err)
	}
	if cfg.Host != "localhost" || cfg.Port != 8080 || !cfg.Default {
		t.Fatalf("unexpected struct: %+v", cfg)
	}

	// Unknown field fails closed with UnknownField at offending key's Span.Start.
	err = Unmarshal([]byte("host: localhost\ntypo_field: 1\n"), &cfg)
	if err == nil || err.Kind != ErrorKindUnknownField || err.Detail.Field != "typo_field" {
		t.Fatalf("expected UnknownField(typo_field), got %+v", err)
	}
	if loc, ok := err.Location(); !ok || loc.Line != 2 || loc.Column != 1 {
		t.Fatalf("UnknownField location = (%+v, %v), want line 2 col 1", loc, ok)
	}

	// Ignored field `yaml:"-"` is treated as unknown if supplied in input.
	err = Unmarshal([]byte("Ignored: secret\n"), &cfg)
	if err == nil || err.Kind != ErrorKindUnknownField || err.Detail.Field != "Ignored" {
		t.Fatalf("expected UnknownField(Ignored) for yaml:\"-\" field, got %+v", err)
	}

	// Non-string key into struct fails closed with ErrorKindType.
	err = Unmarshal([]byte("123: localhost\n"), &cfg)
	if err == nil || err.Kind != ErrorKindType || err.Detail.Expected != "string" || err.Detail.Found != "int" {
		t.Fatalf("expected ErrorKindType on non-string struct key, got %+v", err)
	}

	// Nested path and alias/merge Related provenance on Unmarshal error.
	type outerConfig struct {
		BadPort int             `yaml:"bad_port"`
		Items   []serviceConfig `yaml:"items"`
	}
	var outer outerConfig
	aliasInput := "bad_port: &bp 99999\nitems:\n  - host: ok\n    port: *bp\n"
	err = Unmarshal([]byte(aliasInput), &outer)
	if err == nil || err.Kind != ErrorKindIntegerOutOfRange {
		t.Fatalf("expected IntegerOutOfRange via alias, got %+v", err)
	}
	if len(err.Path) != 3 ||
		err.Path[0].Kind != PathSegmentKey || !KeyEq(err.Path[0].Key, StringValue("items")) ||
		err.Path[1].Kind != PathSegmentIndex || err.Path[1].Index != 0 ||
		err.Path[2].Kind != PathSegmentKey || !KeyEq(err.Path[2].Key, StringValue("port")) {
		t.Fatalf("unexpected error path: %+v", err.Path)
	}
	if len(err.Related) != 1 || err.Related[0].Role != RelatedRoleAliasUse {
		t.Fatalf("expected RelatedRoleAliasUse on aliased field error, got %+v", err.Related)
	}

	// Go map key collision when two distinct YAML keys (-0.0 and 0.0, or float 1.0 and int 1 into float64 key) map to same Go key.
	// In YAML §5.1, Int(1) and Float(1.0) are distinct keys, but both unmarshal to float64(1.0) in map[float64]string!
	var floatKeyMap map[float64]string
	err = Unmarshal([]byte("1: int_one\n1.0: float_one\n"), &floatKeyMap)
	if err == nil || err.Kind != ErrorKindDuplicateKey {
		t.Fatalf("expected ErrorKindDuplicateKey when distinct YAML keys 1 and 1.0 collide in map[float64]string, got %+v", err)
	}
	if len(err.Related) != 1 || err.Related[0].Role != RelatedRoleFirstDefinition {
		t.Fatalf("expected RelatedRoleFirstDefinition on Go map key collision, got %+v", err.Related)
	}

	// Null into *map[K]V sets map to nil.
	floatKeyMap = map[float64]string{1.0: "x"}
	if err := Unmarshal([]byte("null\n"), &floatKeyMap); err != nil || floatKeyMap != nil {
		t.Fatalf("Null into *map: got (%v, %v), want nil", floatKeyMap, err)
	}

	// Synthetic invalid UTF-8 string into *string fails with ErrorKindInvalidUtf8.
	var badStr string
	if err := UnmarshalNode(NewSyntheticNode(StringValue("\xff")), &badStr); err == nil || err.Kind != ErrorKindInvalidUtf8 {
		t.Fatalf("expected ErrorKindInvalidUtf8 on invalid UTF-8 node into *string, got %+v", err)
	}
	_ = math.MaxFloat32
}

type recursivePointerSelf *recursivePointerSelf
type recursivePointerMutualA *recursivePointerMutualB
type recursivePointerMutualB *recursivePointerMutualA

func TestUnmarshalRecursiveAndFinitePointerChains(t *testing.T) {
	if os.Getenv("YAMLPRIMS_POINTER_CYCLE_CHILD") == "1" {
		debug.SetMaxStack(128 << 10)
		var selfPtr recursivePointerSelf
		if err := Unmarshal([]byte("1\n"), &selfPtr); err == nil || err.Kind != ErrorKindType {
			t.Fatalf("expected ErrorKindType on direct recursive pointer target, got %+v", err)
		}
		var mutualPtr recursivePointerMutualA
		if err := Unmarshal([]byte("1\n"), &mutualPtr); err == nil || err.Kind != ErrorKindType {
			t.Fatalf("expected ErrorKindType on mutual recursive pointer target, got %+v", err)
		}
		return
	}

	// Direct and mutual recursive pointer types fail closed with ErrorKindType without allocating or overflowing stack.
	var selfPtr recursivePointerSelf
	err := Unmarshal([]byte("1\n"), &selfPtr)
	if err == nil || err.Kind != ErrorKindType || selfPtr != nil {
		t.Fatalf("direct recursive pointer: got (%v, %+v), want nil and ErrorKindType", selfPtr, err)
	}

	var mutualPtr recursivePointerMutualA
	err = Unmarshal([]byte("1\n"), &mutualPtr)
	if err == nil || err.Kind != ErrorKindType || mutualPtr != nil {
		t.Fatalf("mutual recursive pointer: got (%v, %+v), want nil and ErrorKindType", mutualPtr, err)
	}

	// Null into recursive pointer type sets target to nil cleanly.
	var nonNilSelf recursivePointerSelf
	nonNilSelf = &nonNilSelf
	if err := Unmarshal([]byte("null\n"), &nonNilSelf); err != nil || nonNilSelf != nil {
		t.Fatalf("null into recursive pointer: got (%v, %+v), want nil", nonNilSelf, err)
	}

	// Finite multi-pointer chain succeeds without charging YAML collection depth.
	var chain ****int
	opts := DefaultOptions()
	opts.Limits.MaxDepth = 1
	if err := UnmarshalWith([]byte("42\n"), &chain, opts); err != nil {
		t.Fatalf("finite pointer chain at max_depth=1 failed: %+v", err)
	}
	if chain == nil || *chain == nil || **chain == nil || ***chain == nil || ****chain != 42 {
		t.Fatalf("unexpected finite pointer chain value: %+v", chain)
	}

	// Subprocess verification under 128 KiB max stack.
	child := exec.Command(os.Args[0], "-test.run=^TestUnmarshalRecursiveAndFinitePointerChains$")
	child.Env = append(os.Environ(), "YAMLPRIMS_POINTER_CYCLE_CHILD=1")
	out, runErr := child.CombinedOutput()
	if runErr != nil {
		t.Fatalf("recursive pointer subprocess under 128 KiB stack failed: %v\n%s", runErr, string(out))
	}
}
