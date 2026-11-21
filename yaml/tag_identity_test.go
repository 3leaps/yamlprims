package yaml

import "testing"

// Revision 1.7 resolved tag identity and %TAG separation (spec §4.4, §7.1, §11.1).
// Expected codes and coordinates are written from the specification text, not
// captured from the implementation, and match the Rust core's expectations.

type tagIdentityCase struct {
	name     string
	input    string
	reject   bool
	maxDepth int
	code     string // wire error kind, or syntax_kind for syntax errors
	byte     int
	line     int
	column   int
}

func TestSpecV17RefusedIdentitiesAndSeparation(t *testing.T) {
	const tag = "invalid_global_tag_character"
	const gap = "expected_whitespace"
	const bang = "expected_tag_directive_bang"
	const term = "invalid_tag_terminator"
	cases := []tagIdentityCase{
		{"example 6.25", "!<$:?> value\n", false, 0, tag, 0, 1, 1},
		{"digit scheme", "key: !<9ab:c> value\n", false, 0, tag, 5, 1, 6},
		{"no colon", "- !<abc> value\n", false, 0, tag, 2, 1, 3},
		{"bad scheme char", "!<a*b:c> value\n", false, 0, tag, 0, 1, 1},
		{"sequence", "!<abc> [1, 2]\n", false, 0, tag, 0, 1, 1},
		{"mapping", "!<abc> {k: v}\n", false, 0, tag, 0, 1, 1},
		{"expansion", "%TAG !n! 12\n--- !n!3 v\n", false, 0, tag, 16, 2, 5},
		{"secondary redefined", "%TAG !! 12\n--- !!3 v\n", false, 0, tag, 15, 2, 5},
		{"reject policy", "!<abc> value\n", true, 0, tag, 0, 1, 1},
		{"reject: valid unknown first, invalid later", "a: !<tag:example.com,2026:t> x\nb: !<abc> y\n", true, 0, tag, 34, 2, 4},
		{"before later syntax", "a: !<abc> 1\nb: [\n", false, 0, tag, 3, 1, 4},
		{"depth first", "[[!<abc> [1]]]\n", false, 2, "max_depth", 9, 1, 10},
		{"bom and multibyte", "\ufeff\u00e9: !<abc> x\n", false, 0, tag, 7, 1, 4},
		{"crlf", "a: 1\r\nb: !<abc> x\r\n", false, 0, tag, 9, 2, 4},
		{"gap, used", "%TAG !e!tag:example.com,2026:\n--- !e!a b\n", false, 0, gap, 8, 1, 9},
		{"gap, unused", "%TAG !e!tag:example.com,2026:\n--- b\n", false, 0, gap, 8, 1, 9},
		{"gap, secondary handle", "%TAG !!tag:example.com,2026:\n--- b\n", false, 0, gap, 7, 1, 8},
		{"gap, primary handle", "%TAG !:x\n--- b\n", false, 0, gap, 6, 1, 7},
		{"gap, second directive, crlf", "%TAG !e! tag:a,1:\r\n%TAG !f!tag:b,1:\r\n--- x\r\n", false, 0, gap, 27, 2, 9},
		{"gap, bom", "\ufeff%TAG !e!tag:example.com,2026:\n--- b\n", false, 0, gap, 11, 1, 9},
		{"gap before later syntax", "%TAG !e!tag:a,1:\n--- [\n", false, 0, gap, 8, 1, 9},
		{"gap, second document", "a\n...\n%TAG !e!tag:a,1:\n--- b\n", false, 0, gap, 14, 3, 9},
		{"handle error wins", "%TAG !e0 a:\n%TAG !f!b:\n--- x\n", false, 0, "expected_tag_directive_bang", 0, 1, 1},
		{"empty prefix unchanged", "%TAG !e!\n--- x\n", false, 0, tag, 0, 1, 1},
		{"earlier tag identity wins", "!<abc> x\n...\n%TAG !e!x:\n--- y\n", false, 0, tag, 0, 1, 1},
		{"earlier depth wins", "[[[1]]]\n...\n%TAG !e!x:\n--- y\n", false, 2, "max_depth", 2, 1, 3},
		{"earlier parser error wins", "- a\nb: c\n...\n%TAG !e!x:\n--- y\n", false, 0, "expected_block_sequence_entry", 4, 2, 1},
		{"gap before later tag identity", "%TAG !e!x:\n--- !<abc> y\n", false, 0, gap, 8, 1, 9},
		{"gap before later depth", "%TAG !e!x:\n--- [[[1]]]\n", false, 2, gap, 8, 1, 9},
		{"gap before later syntax", "%TAG !e!x:\n--- [\n", false, 0, gap, 8, 1, 9},
		{"gap then end of input", "%TAG !e!x:", false, 0, gap, 8, 1, 9},
		{"gap then NUL", "%TAG !e!x:\x00--- y\n", false, 0, gap, 8, 1, 9},
		{"terminator error wins over gap", "%TAG !e!x: ]\n--- y\n", false, 0, "invalid_directive_terminator", 0, 1, 1},
		{"gap before later duplicate key", "%TAG !e!x:\n--- a: 1\na: 2\n", false, 0, gap, 8, 1, 9},
		{"duplicate handle before gap", "%TAG !e! a:\n%TAG !e!b:\n--- x\n", false, 0, "duplicate_tag_directive", 12, 2, 1},
		{"prefix scan error wins over gap", "%TAG !e!%zz\n--- x\n", false, 0, "invalid_tag_escape", 0, 1, 1},
		{"underscore handle, used", "%TAG !a_b! tag:example.com,2026:\n--- !a_b!y z\n", false, 0, bang, 0, 1, 1},
		{"underscore handle, unused", "%TAG !a_b! tag:example.com,2026:\n--- z\n", false, 0, bang, 0, 1, 1},
		{"underscore handle and gap", "%TAG !a_b!x:\n--- z\n", false, 0, bang, 0, 1, 1},
		{"underscore handle use", "--- !a_b!y z\n", false, 0, term, 4, 1, 5},
		{"underscore handle use in flow", "k: [ !a_b!y z ]\n", false, 0, term, 5, 1, 6},
		{"verbatim bang", "!<!> x\n", false, 0, tag, 0, 1, 1},
		{"verbatim bang in sequence", "- !<!> x\n", false, 0, tag, 2, 1, 3},
		{"verbatim bang on collection", "!<!> [1]\n", false, 0, tag, 0, 1, 1},
		{"verbatim bang, reject", "!<!> x\n", true, 0, tag, 0, 1, 1},
		{"verbatim bang on collection, reject", "!<!> {a: 1}\n", true, 0, tag, 0, 1, 1},
	}
	for _, c := range cases {
		opts := DefaultOptions()
		if c.reject {
			opts.UnknownTags = UnknownTagsReject
		}
		if c.maxDepth != 0 {
			opts.Limits.MaxDepth = c.maxDepth
		}
		_, err := LoadAllWith(c.input, opts)
		if err == nil {
			t.Errorf("%s: loaded, want %s", c.name, c.code)
			continue
		}
		got := string(err.Kind)
		if err.Kind == ErrorKindSyntax {
			got = err.Detail.SyntaxKind.AsStr()
		} else if err.Kind == ErrorKindLimitExceeded {
			got = string(err.Detail.Limit)
		}
		if got != c.code {
			t.Errorf("%s: code %s, want %s", c.name, got, c.code)
		}
		loc, ok := err.Location()
		if !ok || loc.ByteOffset != c.byte || loc.Line != c.line || loc.Column != c.column {
			t.Errorf("%s: site %+v (source %v), want [%d,%d,%d]", c.name, loc, ok, c.byte, c.line, c.column)
		}
	}
}

func TestSpecV17RefusedIdentityWireBytes(t *testing.T) {
	cases := []struct{ input, want string }{
		{"!<$:?> value\n", `{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"invalid_global_tag_character"},"site":{"source":[0,1,1]},"related":[],"path":[],"message":"syntax error: resolved tag is neither a local tag nor a global tag URI"}}`},
		{"%TAG !!tag:a,1:\n--- b\n", `{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"expected_whitespace"},"site":{"source":[7,1,8]},"related":[],"path":[],"message":"syntax error: expected whitespace"}}`},
	}
	for _, c := range cases {
		_, err := LoadAllWith(c.input, DefaultOptions())
		if err == nil {
			t.Fatalf("%q: loaded", c.input)
		}
		got, encErr := EncodeError(err, DefaultOptions())
		if encErr != nil {
			t.Fatalf("%q: EncodeError: %v", c.input, encErr)
		}
		if got != c.want {
			t.Errorf("%q:\n got %s\nwant %s", c.input, got, c.want)
		}
	}
}

func TestSpecV17AcceptedIdentitiesCloseThroughWireAndEmit(t *testing.T) {
	cases := []struct{ input, uri string }{
		{"!<x:> value\n", "x:"},
		{"!<tag:example.com,2026:item> value\n", "tag:example.com,2026:item"},
		{"!local value\n", "!local"},
		{"%TAG !p! ta\n--- !p!g:example.com/x v\n", "tag:example.com/x"},
		{"!<%74ag:example.com,2026:x> v\n", "tag:example.com,2026:x"},
		{"!local%21x v\n", "!local!x"},
		{"!<tag:example.com,2026:s> [1]\n", "tag:example.com,2026:s"},
		{"--- !a_b z\n", "!a_b"},
		{"%TAG !e! tag:example.com,2026:\n--- !e!a_b z\n", "tag:example.com,2026:a_b"},
		{"!<tag:example.com,2026:a_b> z\n", "tag:example.com,2026:a_b"},
		{"%TAG !a-1! tag:example.com,2026:\n--- !a-1!y z\n", "tag:example.com,2026:y"},
		{"%TAG !e! tag:example.com,2026:\n--- !e!a b\n", "tag:example.com,2026:a"},
		{"%TAG !e!\ttag:example.com,2026:\n--- !e!a b\n", "tag:example.com,2026:a"},
	}
	opts := DefaultOptions()
	rootURI := func(t *testing.T, docs []Document, what string) string {
		t.Helper()
		tg, ok := docs[0].Root().Value().AsTagged()
		if !ok {
			t.Fatalf("%s: root is not Tagged", what)
		}
		return tg.Tag().URI()
	}
	for _, c := range cases {
		docs, err := LoadAllWith(c.input, opts)
		if err != nil {
			t.Fatalf("%q: %v", c.input, err)
		}
		if got := rootURI(t, docs, c.input); got != c.uri {
			t.Errorf("%q: uri %q, want %q", c.input, got, c.uri)
		}
		wire, encErr := Encode(docs, opts)
		if encErr != nil {
			t.Fatalf("%q: Encode: %v", c.input, encErr)
		}
		env, decErr := DecodeWire(wire, opts)
		if decErr != nil || !env.HasDocuments {
			t.Fatalf("%q: DecodeWire: %v", c.input, decErr)
		}
		again, encErr := Encode(env.Documents, opts)
		if encErr != nil || again != wire {
			t.Errorf("%q: wire round trip differs (%v)", c.input, encErr)
		}
		emitted, emitErr := EmitAll(docs)
		if emitErr != nil {
			t.Fatalf("%q: EmitAll: %v", c.input, emitErr)
		}
		reloaded, err := LoadAllWith(emitted, opts)
		if err != nil {
			t.Fatalf("%q: reload: %v", c.input, err)
		}
		if got := rootURI(t, reloaded, c.input+" reload"); got != c.uri {
			t.Errorf("%q: reload uri %q, want %q", c.input, got, c.uri)
		}
	}
	reject := DefaultOptions()
	reject.UnknownTags = UnknownTagsReject
	for _, input := range []string{"%TAG !n! 12\n--- v\n", "- !!str 1\n- ! 2\n- !!int 3\n", "! [1]\n"} {
		if input[0] == '!' {
			if _, err := LoadAllWith(input, reject); err != nil {
				t.Errorf("%q under Reject: %v", input, err)
			}
		}
		docs, err := LoadAllWith(input, opts)
		if err != nil {
			t.Fatalf("%q: %v", input, err)
		}
		wire, _ := Encode(docs, opts)
		if env, err := DecodeWire(wire, opts); err != nil || !env.HasDocuments {
			t.Errorf("%q: DecodeWire: %v", input, err)
		}
	}
}

func TestSpecV17ScannerFailureAtEndOfInputWithDirectiveText(t *testing.T) {
	for _, input := range []string{"a: '%TAG\n", "%TAG: [\n"} {
		if _, err := LoadAllWith(input, DefaultOptions()); err == nil {
			t.Errorf("%q: loaded, want an error", input)
		}
	}
}
