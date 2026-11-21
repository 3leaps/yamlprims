// SPDX-License-Identifier: MIT OR Apache-2.0

package yaml

// Native Go fuzz targets. Each target seeds from conformance/vectors/cases.jsonl
// and checks only properties the specification states:
//
//   - FuzzLoad: every error kind is in the closed vocabulary (§7.1) and is not
//     Internal (in-memory loading has no I/O, and §7.2 reserves Internal for an
//     engine that cannot supply a location); single- and multi-document
//     entrypoints agree (§5.5).
//   - FuzzDecodeWire: decode_wire then encode is byte-identical for envelopes
//     produced by encode / encode_error (§11.1 "Round trip").
//   - FuzzEmitRoundTrip: the §10.2 self-round-trip invariant.
//   - FuzzLintYAML11: lint results satisfy the §11.7 encode_lint preflight and
//     §7.2 coordinates.
//   - FuzzEncode: wire and plain output is JSON, and wire output round-trips.
//
// The seed corpora run as ordinary tests (make fuzz-replay).

import (
	"bufio"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	rtmetrics "runtime/metrics"
	"strconv"
	"sync"
	"testing"
	"time"
	"unicode/utf8"
)

// Per-input assertions for every Fuzz* body. Each input must finish within
// YAMLPRIMS_FUZZ_TIMEOUT seconds (default 10) and leave a live heap of at most
// YAMLPRIMS_FUZZ_RSS_MB MiB (default 2048); otherwise the test fails with kind
// "hang" or "memory". They are test assertions, not resource limits: the code
// under test keeps running until it returns. Failure text is
// "fuzz-guard: <kind>" and never includes the input.
var guardLimits = sync.OnceValues(func() (time.Duration, uint64) {
	timeout, heapMiB := 10.0, uint64(2048)
	if v, err := strconv.ParseFloat(os.Getenv("YAMLPRIMS_FUZZ_TIMEOUT"), 64); err == nil && v > 0 {
		timeout = v
	}
	if v, err := strconv.ParseUint(os.Getenv("YAMLPRIMS_FUZZ_RSS_MB"), 10, 64); err == nil && v > 0 {
		heapMiB = v
	}
	return time.Duration(timeout * float64(time.Second)), heapMiB << 20
})

func liveHeapBytes() uint64 {
	s := []rtmetrics.Sample{{Name: "/memory/classes/heap/objects:bytes"}}
	rtmetrics.Read(s)
	if s[0].Value.Kind() != rtmetrics.KindUint64 {
		return 0
	}
	return s[0].Value.Uint64()
}

// guarded runs one fuzz body and applies the per-input assertions. The body
// runs on its own goroutine so a slow input can be reported; t.Fatal and
// panics inside it are carried back to the test goroutine.
func guarded[T []byte | string](body func(*testing.T, T)) func(*testing.T, T) {
	return func(t *testing.T, in T) {
		t.Helper()
		timeout, heapMax := guardLimits()
		done := make(chan any, 1)
		go func() {
			var p any
			defer func() { done <- p }()
			defer func() { p = recover() }()
			body(t, in)
		}()
		timer := time.NewTimer(timeout)
		defer timer.Stop()
		select {
		case p := <-done:
			if p != nil {
				panic(fmt.Sprint("fuzz-guard: panic: ", p))
			}
		case <-timer.C:
			t.Fatal("fuzz-guard: hang")
		}
		if t.Failed() {
			t.FailNow()
		}
		if liveHeapBytes() > heapMax {
			t.Fatal("fuzz-guard: memory")
		}
	}
}

// fuzzSeeds returns the input of every conformance vector.
func fuzzSeeds(tb testing.TB) []string {
	tb.Helper()
	f, err := os.Open(filepath.Join("..", "conformance", "vectors", "cases.jsonl"))
	if err != nil {
		tb.Fatalf("open conformance vectors: %v", err)
	}
	defer f.Close()
	var seeds []string
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 1<<20), 16<<20)
	for sc.Scan() {
		if len(sc.Bytes()) == 0 {
			continue
		}
		var rec struct {
			Input    *string `json:"input"`
			InputHex *string `json:"input_hex"`
		}
		if err := json.Unmarshal(sc.Bytes(), &rec); err != nil {
			tb.Fatalf("conformance vector: %v", err)
		}
		switch {
		case rec.Input != nil:
			seeds = append(seeds, *rec.Input)
		case rec.InputHex != nil:
			b, err := hex.DecodeString(*rec.InputHex)
			if err != nil {
				tb.Fatalf("conformance vector input_hex: %v", err)
			}
			seeds = append(seeds, string(b))
		}
	}
	if err := sc.Err(); err != nil {
		tb.Fatalf("read conformance vectors: %v", err)
	}
	if len(seeds) == 0 {
		tb.Fatal("no conformance vectors")
	}
	return seeds
}

// wireSeeds returns the committed wire fixtures.
func wireSeeds(tb testing.TB) []string {
	tb.Helper()
	paths, err := filepath.Glob(filepath.Join("..", "crates", "yamlprims", "tests", "fixtures", "wire", "*.json"))
	if err != nil || len(paths) == 0 {
		tb.Fatalf("wire fixtures: %v (%d found)", err, len(paths))
	}
	var seeds []string
	for _, p := range paths {
		b, err := os.ReadFile(p)
		if err != nil {
			tb.Fatalf("read %s: %v", p, err)
		}
		seeds = append(seeds, string(b))
	}
	return seeds
}

// fuzzOptions picks the load options from the first byte, so one corpus covers
// the default, the I-JSON default, and both opt-in merge-key policies. The
// selector byte is not part of the YAML input.
func fuzzOptions(data []byte) (Options, []byte) {
	if len(data) == 0 {
		return DefaultOptions(), data
	}
	opts := DefaultOptions()
	switch data[0] % 4 {
	case 1:
		opts = DefaultIJSONOptions()
	case 2:
		opts.MergeKeys = MergeKeysAsKey
	case 3:
		opts.MergeKeys = MergeKeysApply
	}
	return opts, data[1:]
}

func addSelectedSeeds(f *testing.F) {
	for _, s := range fuzzSeeds(f) {
		for sel := byte(0); sel < 4; sel++ {
			f.Add(append([]byte{sel}, s...))
		}
	}
}

func checkKind(t *testing.T, what string, e *Error) {
	t.Helper()
	if !e.Kind.IsValid() {
		t.Fatalf("%s: error kind %q is outside the closed vocabulary: %v", what, e.Kind, e)
	}
}

func checkNotInternal(t *testing.T, what string, e *Error) {
	t.Helper()
	checkKind(t, what, e)
	if e.Kind == ErrorKindInternal {
		t.Fatalf("%s: internal error: %v", what, e)
	}
}

// valueEq is §5.1 key_eq extended as §10.2 requires for the self-round-trip:
// NaN equals NaN and -0.0 keeps its sign bit. Mapping entries are compared in
// order (Mapping preserves insertion order and the emitter writes it).
func valueEq(a, b Value) bool {
	type pair struct{ a, b Value }
	stack := []pair{{a, b}}
	for len(stack) > 0 {
		p := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if p.a.kind != p.b.kind {
			return false
		}
		switch p.a.kind {
		case ValueKindNull:
		case ValueKindBool:
			if p.a.b != p.b.b {
				return false
			}
		case ValueKindInt:
			if p.a.i != p.b.i {
				return false
			}
		case ValueKindFloat:
			if math.Float64bits(p.a.f) != math.Float64bits(p.b.f) {
				return false
			}
		case ValueKindString:
			if p.a.s != p.b.s {
				return false
			}
		case ValueKindTagged:
			if (p.a.tagged == nil) != (p.b.tagged == nil) {
				return false
			}
			if p.a.tagged == nil {
				continue
			}
			if p.a.tagged.tag.uri != p.b.tagged.tag.uri {
				return false
			}
			stack = append(stack, pair{p.a.tagged.value.value, p.b.tagged.value.value})
		case ValueKindSequence:
			if len(p.a.seq) != len(p.b.seq) {
				return false
			}
			for i := range p.a.seq {
				stack = append(stack, pair{p.a.seq[i].value, p.b.seq[i].value})
			}
		case ValueKindMapping:
			if p.a.m.Len() != p.b.m.Len() {
				return false
			}
			for i := 0; i < p.a.m.Len(); i++ {
				ea, eb := p.a.m.entries[i], p.b.m.entries[i]
				stack = append(stack, pair{ea.Key.value, eb.Key.value}, pair{ea.Value.value, eb.Value.value})
			}
		default:
			return false
		}
	}
	return true
}

func sameVersion(a, b Document) bool {
	amaj, amin, aok := a.YamlVersion()
	bmaj, bmin, bok := b.YamlVersion()
	return aok == bok && amaj == bmaj && amin == bmin
}

func FuzzLoad(f *testing.F) {
	addSelectedSeeds(f)
	f.Fuzz(guarded(func(t *testing.T, data []byte) {
		opts, input := fuzzOptions(data)
		docs, errAll := LoadAllBytesWith(input, opts)
		one, errOne := LoadBytesWith(input, opts)
		if errAll != nil {
			checkNotInternal(t, "LoadAllBytesWith", errAll)
		}
		if errOne != nil {
			checkNotInternal(t, "LoadBytesWith", errOne)
		}
		if errAll != nil {
			return
		}
		// §5.5: the multi-document call succeeded, so the single-document call
		// agrees with it.
		switch {
		case len(docs) == 0:
			if errOne != nil {
				t.Fatalf("stream with no documents: LoadBytesWith failed: %v", errOne)
			}
			if one.root.value.kind != ValueKindNull {
				t.Fatalf("stream with no documents: root kind %v, want null", one.root.value.kind)
			}
		case len(docs) == 1:
			if errOne != nil {
				t.Fatalf("one document: LoadBytesWith failed: %v", errOne)
			}
			if !valueEq(one.root.value, docs[0].root.value) || !sameVersion(one, docs[0]) {
				t.Fatal("one document: LoadBytesWith and LoadAllBytesWith disagree")
			}
		default:
			if errOne == nil || errOne.Kind != ErrorKindMultipleDocuments {
				t.Fatalf("%d documents: LoadBytesWith returned %v, want multiple_documents", len(docs), errOne)
			}
		}
	}))
}

// checkWireRoundTrip asserts the §11.1 round trip for an envelope that the
// encoder produced, when it fits the decoder's input budget.
func checkWireRoundTrip(t *testing.T, wire string) {
	t.Helper()
	opts := DefaultOptions()
	if len(wire) > opts.Limits.MaxInputBytes {
		return
	}
	env, err := DecodeWire(wire, opts)
	if err != nil {
		t.Fatalf("DecodeWire rejected encoder output: %v", err)
	}
	var again string
	if env.HasDocuments {
		again, err = Encode(env.Documents, opts)
	} else {
		again, err = EncodeError(env.Error, opts)
	}
	if err != nil {
		t.Fatalf("re-encode of decoded encoder output failed: %v", err)
	}
	if again != wire {
		t.Fatal("decode_wire then encode is not byte-identical")
	}
}

func FuzzDecodeWire(f *testing.F) {
	for _, s := range fuzzSeeds(f) {
		docs, lerr := LoadAll(s)
		var wire string
		var err *Error
		if lerr != nil {
			wire, err = EncodeError(lerr, DefaultOptions())
		} else {
			wire, err = Encode(docs, DefaultOptions())
		}
		if err == nil {
			f.Add(wire)
		}
	}
	for _, w := range wireSeeds(f) {
		f.Add(w)
	}
	f.Fuzz(guarded(func(t *testing.T, text string) {
		if !utf8.ValidString(text) {
			return // the Rust twin takes &str; keep both targets on the same domain
		}
		opts := DefaultOptions()
		env, err := DecodeWire(text, opts)
		if err != nil {
			checkKind(t, "DecodeWire", err)
			return
		}
		var wire string
		if env.HasDocuments {
			wire, err = Encode(env.Documents, opts)
		} else {
			wire, err = EncodeError(env.Error, opts)
		}
		if err != nil {
			if err.Kind != ErrorKindOutputLimitExceeded {
				t.Fatalf("encode of a decoded envelope failed: %v", err)
			}
			return
		}
		checkWireRoundTrip(t, wire)
	}))
}

func FuzzEmitRoundTrip(f *testing.F) {
	addSelectedSeeds(f)
	f.Fuzz(guarded(func(t *testing.T, data []byte) {
		opts, input := fuzzOptions(data)
		docs, err := LoadAllBytesWith(input, opts)
		if err != nil {
			return
		}
		emitted, err := EmitAll(docs)
		if err != nil {
			checkNotInternal(t, "EmitAll", err)
			return
		}
		// §10.2: accepted by default emit_all, reloads under default load_all.
		again, err := LoadAll(emitted)
		if err != nil {
			t.Fatalf("emitted YAML does not reload: %v", err)
		}
		if len(again) != len(docs) {
			t.Fatalf("reloaded %d documents, emitted %d", len(again), len(docs))
		}
		for i := range docs {
			if !sameVersion(docs[i], again[i]) {
				t.Fatalf("document %d: %%YAML version changed on reload", i)
			}
			if !valueEq(docs[i].root.value, again[i].root.value) {
				t.Fatalf("document %d: value changed on reload", i)
			}
		}
	}))
}

func FuzzLintYAML11(f *testing.F) {
	for _, s := range fuzzSeeds(f) {
		f.Add([]byte(s))
	}
	f.Fuzz(guarded(func(t *testing.T, data []byte) {
		opts := DefaultOptions()
		res, err := LintYAML11Bytes(data, opts)
		if err != nil {
			checkNotInternal(t, "LintYAML11Bytes", err)
			return
		}
		if res.WarningCount < len(res.Diagnostics) {
			t.Fatalf("warning_count %d < %d diagnostics", res.WarningCount, len(res.Diagnostics))
		}
		for i, d := range res.Diagnostics {
			s := d.Span
			if s.Start.Line < 1 || s.Start.Column < 1 || s.End.Line < 1 || s.End.Column < 1 {
				t.Fatalf("diagnostic %d: coordinates are 1-based: %+v", i, s)
			}
			if s.Start.ByteOffset > s.End.ByteOffset || s.End.ByteOffset > len(data) {
				t.Fatalf("diagnostic %d: span %+v out of bounds for %d bytes", i, s, len(data))
			}
		}
		// §11.7: encode_lint's preflight accepts what the linter produced.
		out, err := EncodeLintWith(res, opts)
		if err != nil {
			if err.Kind != ErrorKindOutputLimitExceeded {
				t.Fatalf("EncodeLintWith rejected the linter's result: %v", err)
			}
			return
		}
		if !json.Valid([]byte(out)) {
			t.Fatal("lint envelope is not JSON")
		}
	}))
}

func FuzzEncode(f *testing.F) {
	addSelectedSeeds(f)
	f.Fuzz(guarded(func(t *testing.T, data []byte) {
		opts, input := fuzzOptions(data)
		docs, lerr := LoadAllBytesWith(input, opts)
		if lerr != nil {
			wire, err := EncodeError(lerr, opts)
			if err != nil {
				if err.Kind != ErrorKindOutputLimitExceeded {
					t.Fatalf("EncodeError failed: %v", err)
				}
				return
			}
			if !json.Valid([]byte(wire)) {
				t.Fatal("error envelope is not JSON")
			}
			checkWireRoundTrip(t, wire)
			return
		}
		wire, err := Encode(docs, opts)
		if err != nil {
			if err.Kind != ErrorKindOutputLimitExceeded {
				t.Fatalf("Encode failed: %v", err)
			}
		} else {
			if !json.Valid([]byte(wire)) {
				t.Fatal("wire envelope is not JSON")
			}
			checkWireRoundTrip(t, wire)
		}
		for i := range docs {
			plain, err := EncodePlain(docs[i].root, opts)
			if err != nil {
				checkNotInternal(t, "EncodePlain", err)
				continue
			}
			if !json.Valid([]byte(plain)) {
				t.Fatalf("document %d: plain output is not JSON", i)
			}
		}
	}))
}
