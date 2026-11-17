package yaml

import (
	"encoding/json"
	"runtime"
	"strings"
	"testing"
)

func TestLintYAML11AllHazardClassesAndPriority(t *testing.T) {
	src := strings.Join([]string{
		"bool_yes: yes",
		"bool_no: OFF",
		"sex_int: 12:30:00",
		"sex_flt: -0:15.5",
		"sex_over_underscore: 1_0:20",
		"oct_legacy: 0755",
		"bin_int: 0b1010",
		"shex_int: -0x1a",
		"under_int: 1_000",
		"under_flt: 3.14_15",
		"under_hex: 0xdead_beef",
		"under_oct: 07_55",
		"base: &b",
		"  k: 1",
		"merged:",
		"  <<: *b",
		"",
	}, "\n")

	res, err := LintYAML11(src)
	if err != nil {
		t.Fatalf("LintYAML11 failed: %v", err)
	}
	if res.Truncated {
		t.Fatalf("expected Truncated=false, got true")
	}
	if res.WarningCount != 13 || len(res.Diagnostics) != 13 {
		t.Fatalf("expected 13 diagnostics, got WarningCount=%d len=%d", res.WarningCount, len(res.Diagnostics))
	}

	wantCodes := []LintCode{
		LintCodeYaml11Boolean,
		LintCodeYaml11Boolean,
		LintCodeYaml11Sexagesimal,
		LintCodeYaml11Sexagesimal,
		LintCodeYaml11Sexagesimal,
		LintCodeYaml11Number,
		LintCodeYaml11Number,
		LintCodeYaml11Number,
		LintCodeYaml11Number,
		LintCodeYaml11Number,
		LintCodeYaml11Number,
		LintCodeYaml11Number,
		LintCodeYaml11MergeKey,
	}
	for i, w := range wantCodes {
		if res.Diagnostics[i].Code != w {
			t.Errorf("diag[%d]: got code %q, want %q (raw=%q)", i, res.Diagnostics[i].Code, w, res.Diagnostics[i].Raw)
		}
	}
	if res.Diagnostics[5].YAML12Resolution != "int (755)" {
		t.Errorf("oct_legacy YAML12Resolution = %q, want %q", res.Diagnostics[5].YAML12Resolution, "int (755)")
	}
	if res.Diagnostics[12].YAML12Resolution != `rejected (MergeKeys::Reject default; literal string "<<" under MergeKeys::AsKey)` {
		t.Errorf("merge_key YAML12Resolution = %q", res.Diagnostics[12].YAML12Resolution)
	}

	// MergeKeysApply and MergeKeysAsKey also emit yaml11_merge_key with mode-appropriate resolution strings.
	optsApply := DefaultOptions()
	optsApply.MergeKeys = MergeKeysApply
	resApply, err := LintYAML11With("base: &b {a: 1}\nm:\n  <<: *b\n", optsApply)
	if err != nil {
		t.Fatalf("LintYAML11With(Apply) failed: %v", err)
	}
	if len(resApply.Diagnostics) != 1 || resApply.Diagnostics[0].YAML12Resolution != `merge applied (MergeKeys::Apply opt-in; not part of YAML 1.2.2 Core)` {
		t.Fatalf("unexpected Apply diag: %+v", resApply.Diagnostics)
	}

	optsAsKey := DefaultOptions()
	optsAsKey.MergeKeys = MergeKeysAsKey
	resAsKey, err := LintYAML11With("m:\n  <<: 1\n", optsAsKey)
	if err != nil {
		t.Fatalf("LintYAML11With(AsKey) failed: %v", err)
	}
	if len(resAsKey.Diagnostics) != 1 || resAsKey.Diagnostics[0].YAML12Resolution != `string ("<<" under MergeKeys::AsKey)` {
		t.Fatalf("unexpected AsKey diag: %+v", resAsKey.Diagnostics)
	}
}

func TestLintYAML11QuotedAndTaggedNegativeCases(t *testing.T) {
	src := strings.Join([]string{
		`quoted_yes: "yes"`,
		`single_no: 'no'`,
		`tagged_str: !!str on`,
		`tagged_int: !!int 010`,
		`core_bool: true`,
		`core_oct: 0o755`,
		`core_hex: 0x1a`,
		`zero: 0`,
		`double_zero: 00`,
		`non_octal: 0800`,
		`non_sexagesimal: 12:60`,
		`quoted_merge:`,
		`  "<<": 1`,
		"",
	}, "\n")

	res, err := LintYAML11(src)
	if err != nil {
		t.Fatalf("LintYAML11 failed: %v", err)
	}
	if res.WarningCount != 0 || len(res.Diagnostics) != 0 || res.Truncated {
		t.Fatalf("expected 0 diagnostics, got %+v", res)
	}
}

func TestLintYAML11TruncationAndMultiDoc(t *testing.T) {
	src := "---\na: yes\nb: no\n---\nc: on\nd: off\n"
	opts := DefaultOptions()
	opts.Limits.MaxWarnings = 2

	res, err := LintYAML11With(src, opts)
	if err != nil {
		t.Fatalf("LintYAML11With failed: %v", err)
	}
	if !res.Truncated || res.WarningCount != 4 || len(res.Diagnostics) != 2 {
		t.Fatalf("unexpected truncation result: %+v", res)
	}
	if res.Diagnostics[0].DocumentIndex != 0 || res.Diagnostics[1].DocumentIndex != 0 {
		t.Fatalf("unexpected DocumentIndex: %+v", res.Diagnostics)
	}

	wire, err := EncodeLintWith(res, opts)
	if err != nil {
		t.Fatalf("EncodeLintWith failed: %v", err)
	}
	var parsed map[string]any
	if jerr := json.Unmarshal([]byte(wire), &parsed); jerr != nil {
		t.Fatalf("invalid JSON: %v", jerr)
	}
	if parsed["format"] != "yamlprims-lint" || parsed["truncated"] != true || parsed["warning_count"] != float64(4) {
		t.Fatalf("unexpected wire envelope: %v", parsed)
	}
}

func TestLintYAML11ReaderAndEncodeLintValidation(t *testing.T) {
	res, err := LintYAML11Reader(strings.NewReader("k: yes\n"))
	if err != nil {
		t.Fatalf("LintYAML11Reader failed: %v", err)
	}
	if res.WarningCount != 1 {
		t.Fatalf("expected 1 warning, got %d", res.WarningCount)
	}

	// Synthetic invalid LintResult (WarningCount != len(Diagnostics) when Truncated == false)
	bad := LintResult{
		Diagnostics:  res.Diagnostics,
		WarningCount: 2,
		Truncated:    false,
	}
	if _, err := EncodeLint(bad); err == nil || err.Kind != ErrorKindInvalidOptions {
		t.Fatalf("expected InvalidOptions for mismatched WarningCount, got %v", err)
	}

	// MaxOutputBytes enforcement on EncodeLintWith
	smallOpts := DefaultOptions()
	smallOpts.Limits.MaxOutputBytes = 32
	if _, err := EncodeLintWith(res, smallOpts); err == nil || err.Kind != ErrorKindOutputLimitExceeded {
		t.Fatalf("expected OutputLimitExceeded error, got %v", err)
	}
}

func TestDiagnosticBudgetRejectsBeforeCloningPath(t *testing.T) {
	src := "outer_key_with_bytes:\n  inner_key_with_bytes:\n    - yes\n    - no\n"
	opts := DefaultOptions()
	opts.Limits.MaxOwnedBytes = 900
	opts.Limits.MaxScalarBytes = 256
	opts.Limits.MaxKeyBytes = 256

	clones := 0
	lintPathCloneHook = func() {
		clones++
	}
	defer func() {
		lintPathCloneHook = nil
	}()

	_, err := LintYAML11With(src, opts)
	if err == nil {
		t.Fatal("expected MaxOwnedBytes error, got nil")
	}
	if err.Kind != ErrorKindLimitExceeded || err.Detail.Limit != LimitMaxOwnedBytes {
		t.Fatalf("expected MaxOwnedBytes LimitExceeded, got %v", err)
	}
	if clones != 1 {
		t.Fatalf("expected 1 path clone before budget rejection, got %d", clones)
	}
	if len(err.Path) != 3 {
		t.Fatalf("expected error path len 3, got %d (%+v)", len(err.Path), err.Path)
	}
}

func TestOmittedLintDoesNotFormatLargeScalar(t *testing.T) {
	raw := "0b" + strings.Repeat("1", 1<<20)
	opts := DefaultOptions()
	opts.Limits.MaxWarnings = 1
	c := lintCollector{
		result: LintResult{
			Diagnostics:  make([]LintDiagnostic, 1),
			WarningCount: 1,
		},
	}
	l := loader{opts: opts, budget: newBudget(opts.Limits)}
	runtime.GC()
	var before, after runtime.MemStats
	runtime.ReadMemStats(&before)
	err := c.recordScalar(&l, hazardBinaryInt, raw, Value{}, Span{})
	runtime.ReadMemStats(&after)
	if err != nil {
		t.Fatal(err)
	}
	if c.result.WarningCount != 2 || !c.result.Truncated {
		t.Fatalf("unexpected count/truncation: %+v", c.result)
	}
	allocated := after.TotalAlloc - before.TotalAlloc
	if allocated > 64<<10 {
		t.Fatalf("omitted diagnostic allocated input-sized temporary: %d bytes", allocated)
	}
}

func TestRetainedLintFormatsWithinFixedBuffers(t *testing.T) {
	raw := "0b" + strings.Repeat("1", 1<<20)
	opts := DefaultOptions()
	c := lintCollector{}
	l := loader{opts: opts, budget: newBudget(opts.Limits)}
	runtime.GC()
	var before, after runtime.MemStats
	runtime.ReadMemStats(&before)
	err := c.recordScalar(&l, hazardBinaryInt, raw, Value{}, Span{})
	runtime.ReadMemStats(&after)
	if err != nil {
		t.Fatal(err)
	}
	d := c.result.Diagnostics[0]
	if len(d.Raw) != MaxFieldBytes || len(d.YAML12Resolution) != MaxFieldBytes || len(d.Message) != MaxMessageBytes {
		t.Fatalf("expected bounded field lengths %d/%d/%d, got %d/%d/%d",
			MaxFieldBytes, MaxFieldBytes, MaxMessageBytes,
			len(d.Raw), len(d.YAML12Resolution), len(d.Message))
	}
	allocated := after.TotalAlloc - before.TotalAlloc
	if allocated > 64<<10 {
		t.Fatalf("bounded diagnostic formatted through input-sized temporaries: %d bytes", allocated)
	}
}
