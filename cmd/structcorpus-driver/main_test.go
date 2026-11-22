package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func layoutRoot(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	for _, d := range []string{"manifests/yaml/profiles", "manifests/yaml/span-conventions"} {
		if err := os.MkdirAll(filepath.Join(dir, filepath.FromSlash(d)), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func yamlEntry() map[string]any {
	return map[string]any{
		"cases":            "manifests/yaml/cases.jsonl",
		"profiles":         "manifests/yaml/profiles",
		"span_conventions": "manifests/yaml/span-conventions",
	}
}

func writeRegistry(t *testing.T, dir string, doc any) {
	t.Helper()
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "manifests", "formats.json"), raw, 0o644); err != nil {
		t.Fatal(err)
	}
}

func registryDoc(yaml map[string]any) map[string]any {
	return map[string]any{"schema_version": "formats/v1", "formats": map[string]any{"yaml": yaml}}
}

func TestLayoutLegacyWithoutRegistry(t *testing.T) {
	dir := layoutRoot(t)
	l, err := resolveLayout(dir)
	if err != nil {
		t.Fatal(err)
	}
	want := sourceLayout{
		cases:           filepath.Join(dir, "manifests", "cases.jsonl"),
		profiles:        filepath.Join(dir, "manifests", "profiles"),
		spanConventions: filepath.Join(dir, "manifests", "span-conventions"),
	}
	if l != want {
		t.Fatalf("got %+v, want %+v", l, want)
	}
}

func TestLayoutFromRegistry(t *testing.T) {
	dir := layoutRoot(t)
	writeRegistry(t, dir, registryDoc(yamlEntry()))
	l, err := resolveLayout(dir)
	if err != nil {
		t.Fatal(err)
	}
	want := sourceLayout{
		cases:           filepath.Join(dir, "manifests", "yaml", "cases.jsonl"),
		profiles:        filepath.Join(dir, "manifests", "yaml", "profiles"),
		spanConventions: filepath.Join(dir, "manifests", "yaml", "span-conventions"),
		registry:        true,
	}
	if l != want {
		t.Fatalf("got %+v, want %+v", l, want)
	}
}

func TestLayoutRegistryFailsClosed(t *testing.T) {
	dir := layoutRoot(t)
	writeRegistry(t, dir, map[string]any{"schema_version": "formats/v2", "formats": map[string]any{"yaml": yamlEntry()}})
	if _, err := resolveLayout(dir); err == nil || !strings.Contains(err.Error(), "schema_version") {
		t.Fatalf("bad schema_version: %v", err)
	}
	writeRegistry(t, dir, map[string]any{"schema_version": "formats/v1", "formats": map[string]any{"json": yamlEntry()}})
	if _, err := resolveLayout(dir); err == nil || !strings.Contains(err.Error(), "no yaml") {
		t.Fatalf("missing yaml entry: %v", err)
	}
	for _, bad := range []any{"../x", "/abs", "manifests/./yaml", "manifests//yaml", "", `a\b`, "c:x", 1} {
		e := yamlEntry()
		e["profiles"] = bad
		writeRegistry(t, dir, registryDoc(e))
		if _, err := resolveLayout(dir); err == nil || !strings.Contains(err.Error(), "yaml.profiles") {
			t.Fatalf("%v: %v", bad, err)
		}
	}
}

func TestLayoutRegistryLinkOutsideFails(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks need privileges on Windows")
	}
	dir := layoutRoot(t)
	if err := os.Symlink(t.TempDir(), filepath.Join(dir, "manifests", "out")); err != nil {
		t.Fatal(err)
	}
	e := yamlEntry()
	e["profiles"] = "manifests/out"
	writeRegistry(t, dir, registryDoc(e))
	if _, err := resolveLayout(dir); err == nil || !strings.Contains(err.Error(), "outside") {
		t.Fatalf("link outside: %v", err)
	}
}

func TestYAMLRecordFilter(t *testing.T) {
	for _, tc := range []struct {
		rec      map[string]any
		registry bool
		run      bool
		fails    bool
	}{
		{map[string]any{"format": "yaml"}, true, true, false},
		{map[string]any{"format": "json"}, true, false, false},
		{map[string]any{"format": "json"}, false, false, false},
		{map[string]any{}, false, true, false},
		{map[string]any{}, true, false, true},
		{map[string]any{"format": 1.0}, false, false, true},
	} {
		run, err := yamlRecord(tc.rec, tc.registry)
		if run != tc.run || (err != nil) != tc.fails {
			t.Fatalf("%v registry=%v: run=%v err=%v", tc.rec, tc.registry, run, err)
		}
	}
}
