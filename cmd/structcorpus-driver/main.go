// Command structcorpus-driver is the Go consumer driver for structcorpus 0.1
// (`yaml-value/1`, typed, and node-span expectations) and the `--stdin` wire/plain
// harness used by `scripts/crosscheck.sh`.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strconv"
	"strings"

	"github.com/3leaps/yamlprims/yaml"
)

type profileSpec struct {
	id       string
	revision uint64
	status   string
	merge    yaml.MergeKeys
	single   bool
	limits   bool
	policy   string
}

const basePolicy = `"duplicate_keys":"reject","integer_range":"i128-u128","unknown_tags":"tagged-str"`

var limitsProfiles = map[string]bool{
	"yamlprims-core-limits":   true,
	"yamlprims-core-stress":   true,
	"yamlprims-core-depth128": true,
	"yamlprims-core-entries2": true,
}

type profRev struct {
	id  string
	rev uint64
}

var spec13Profiles = map[profRev]bool{
	{"yamlprims-core", 3}:              true,
	{"yamlprims-core-stream", 3}:       true,
	{"yamlprims-core-merge-apply", 3}:  true,
	{"yamlprims-core-merge-as-key", 3}: true,
	{"yamlprims-core-limits", 2}:       true,
	{"yamlprims-core-stress", 1}:       true,
}

var excludedProfiles = map[profRev]string{
	{"yaml-core", 1}: "generic profile: unbounded integers, plain-key merges, stream",
}

var nodeKinds = []string{"null", "str", "map", "tagged"}

func makeProfiles() []profileSpec {
	p := func(id string, rev uint64, status string, merge yaml.MergeKeys, mergeName string, single bool) profileSpec {
		docs := "stream"
		if single {
			docs = "single"
		}
		hasLim := limitsProfiles[id]
		strict := ""
		if rev >= 2 || hasLim {
			strict = `,"flow_indentation":"strict"`
		}
		policy := fmt.Sprintf(`{%s,"merge_keys":%q,"documents":%q%s}`, basePolicy, mergeName, docs, strict)
		return profileSpec{
			id:       id,
			revision: rev,
			status:   status,
			merge:    merge,
			single:   single,
			limits:   hasLim,
			policy:   policy,
		}
	}
	return []profileSpec{
		p("yamlprims-core-limits", 1, "target", yaml.MergeKeysReject, "reject", true),
		p("yamlprims-core", 1, "historical", yaml.MergeKeysReject, "reject", true),
		p("yamlprims-core", 2, "target", yaml.MergeKeysReject, "reject", true),
		p("yamlprims-core-stream", 2, "target", yaml.MergeKeysReject, "reject", false),
		p("yamlprims-core-merge-apply", 1, "historical", yaml.MergeKeysApply, "apply", true),
		p("yamlprims-core-merge-apply", 2, "target", yaml.MergeKeysApply, "apply", true),
		p("yamlprims-core-merge-as-key", 1, "historical", yaml.MergeKeysAsKey, "plain-key", true),
		p("yamlprims-core-merge-as-key", 2, "target", yaml.MergeKeysAsKey, "plain-key", true),
		// Spec 1.3 (DDR-0008, DDR-0009).
		p("yamlprims-core", 3, "target", yaml.MergeKeysReject, "reject", true),
		p("yamlprims-core-stream", 3, "target", yaml.MergeKeysReject, "reject", false),
		p("yamlprims-core-merge-apply", 3, "target", yaml.MergeKeysApply, "apply", true),
		p("yamlprims-core-merge-as-key", 3, "target", yaml.MergeKeysAsKey, "plain-key", true),
		p("yamlprims-core-limits", 2, "target", yaml.MergeKeysReject, "reject", true),
		p("yamlprims-core-stress", 1, "target", yaml.MergeKeysReject, "reject", true),
		// Spec 1.4 (DDR-0010, DDR-0011).
		p("yamlprims-core", 4, "target", yaml.MergeKeysReject, "reject", true),
		p("yamlprims-core-stream", 4, "target", yaml.MergeKeysReject, "reject", false),
		p("yamlprims-core-merge-apply", 4, "target", yaml.MergeKeysApply, "apply", true),
		p("yamlprims-core-merge-as-key", 4, "target", yaml.MergeKeysAsKey, "plain-key", true),
		p("yamlprims-core-depth128", 1, "target", yaml.MergeKeysReject, "reject", true),
		p("yamlprims-core-entries2", 1, "target", yaml.MergeKeysReject, "reject", true),
	}
}

type rowKey struct {
	profile string
	rev     uint64
	kind    string
}

type rowCounts struct {
	status string
	pass   int
	fail   int
}

func main() {
	args := os.Args[1:]
	if len(args) > 0 && args[0] == "--stdin" {
		os.Exit(runStdin(args[1:]))
	}
	if len(args) > 0 && args[0] == "--crosscheck-jsonl" {
		os.Exit(runCrosscheckJSONL())
	}
	if len(args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: structcorpus HYDRATED_DIR SOURCE_DIR [--details N] [--case-file F]")
		os.Exit(2)
	}
	hyd := args[0]
	src := args[1]
	details := 20
	var caseFile string
	for i := 2; i < len(args); i++ {
		if args[i] == "--details" && i+1 < len(args) {
			if n, err := strconv.Atoi(args[i+1]); err == nil && n >= 0 {
				details = n
			}
			i++
		} else if args[i] == "--case-file" && i+1 < len(args) {
			caseFile = args[i+1]
			i++
		}
	}

	profiles := makeProfiles()
	var problems []string
	checkedProfiles := make(map[profRev]*yaml.Limits)
	rows := make(map[rowKey]*rowCounts)
	var failures []string
	ids := make(map[string]bool)
	cases := 0
	excluded := make(map[rowKey]int)
	conventionChecked := false
	var conventionErr error
	var withEvents, eventsOnly, eventLocations int

	var manifests []string
	if caseFile != "" {
		manifests = []string{caseFile}
	} else {
		manifests = []string{
			filepath.Join(hyd, "cases.jsonl"),
			filepath.Join(src, "manifests", "cases.jsonl"),
		}
	}

	var textBuilder strings.Builder
	for _, f := range manifests {
		b, err := os.ReadFile(f)
		if err != nil {
			problems = append(problems, fmt.Sprintf("%s: %v", f, err))
			continue
		}
		textBuilder.Write(b)
	}

	lines := strings.Split(textBuilder.String(), "\n")
	for n, line := range lines {
		if trimJSON(line) == "" {
			continue
		}
		caseObj, err := parseJSONMap(line)
		if err != nil {
			problems = append(problems, fmt.Sprintf("case line %d: %v", n+1, err))
			continue
		}
		id, ok := asString(caseObj["id"])
		if !ok {
			problems = append(problems, fmt.Sprintf("case line %d: no id", n+1))
			continue
		}
		cases++
		if ids[id] {
			problems = append(problems, fmt.Sprintf("%s: duplicate case id", id))
		}
		ids[id] = true

		input, err := readInput(caseObj, hyd, src)
		if err != nil {
			problems = append(problems, fmt.Sprintf("%s: %v", id, err))
			continue
		}

		ex, _ := caseObj["expectations"].(map[string]any)
		for _, kind := range []string{"value", "typed"} {
			list, _ := ex[kind].([]any)
			for _, rawExp := range list {
				exp, _ := rawExp.(map[string]any)
				profile, okP := asString(exp["profile"])
				rev, okR := asUint64(exp["profile_revision"])
				if !okP || !okR {
					problems = append(problems, fmt.Sprintf("%s: %s expectation without profile and revision", id, kind))
					continue
				}
				pr := profRev{id: profile, rev: rev}
				if _, isExcl := excludedProfiles[pr]; isExcl {
					rk := rowKey{profile: profile, rev: rev, kind: kind}
					if rows[rk] == nil {
						rows[rk] = &rowCounts{status: "excluded"}
					}
					excluded[rk]++
					continue
				}
				var p *profileSpec
				for i := range profiles {
					if profiles[i].id == profile && profiles[i].revision == rev {
						p = &profiles[i]
						break
					}
				}
				if p == nil {
					problems = append(problems, fmt.Sprintf("%s: unsupported profile %s r%d", id, profile, rev))
					continue
				}
				lim, seen := checkedProfiles[pr]
				if !seen {
					l, err := checkProfileFile(src, *p)
					if err != nil {
						problems = append(problems, err.Error())
						checkedProfiles[pr] = nil
						continue
					}
					lim = &l
					checkedProfiles[pr] = lim
				}
				if lim == nil {
					continue
				}
				opts := yaml.DefaultOptions()
				opts.MergeKeys = p.merge
				opts.Limits = *lim

				want, err := expectedResult(kind, exp)
				if err != nil {
					problems = append(problems, fmt.Sprintf("%s: malformed %s expectation: %v", id, kind, err))
					continue
				}
				var got any
				if kind == "value" {
					got = runValue(input, opts, p.single)
				} else {
					var terr error
					got, terr = runTyped(input, opts, exp)
					if terr != nil {
						problems = append(problems, fmt.Sprintf("%s: malformed typed expectation: %v", id, terr))
						continue
					}
				}
				rk := rowKey{profile: profile, rev: rev, kind: kind}
				rc := rows[rk]
				if rc == nil {
					rc = &rowCounts{status: p.status}
					rows[rk] = rc
				}
				if matchesResult(got, want) {
					rc.pass++
				} else {
					rc.fail++
					failures = append(failures, fmt.Sprintf("%s [%s r%d %s]\n  want %s\n  got  %s",
						id, profile, rev, kind, mustJSON(want), mustJSON(got)))
				}
			}
		}

		if ex != nil {
			if _, hasEv := ex["events"]; hasEv {
				withEvents++
				_, hasVal := ex["value"]
				_, hasTyp := ex["typed"]
				_, hasNL := ex["node_locations"]
				if !hasVal && !hasTyp && !hasNL {
					eventsOnly++
				}
			}
			if _, hasLoc := ex["locations"]; hasLoc {
				eventLocations++
			}

			if caseFile != "" {
				_, hasEv := ex["events"]
				_, hasVal := ex["value"]
				_, hasTyp := ex["typed"]
				_, hasNL := ex["node_locations"]
				if hasEv && !hasVal && !hasTyp && !hasNL {
					pr := profRev{id: "yamlprims-core", rev: 3}
					var p *profileSpec
					for i := range profiles {
						if profiles[i].id == pr.id && profiles[i].revision == pr.rev {
							p = &profiles[i]
							break
						}
					}
					lim, seen := checkedProfiles[pr]
					if !seen {
						l, err := checkProfileFile(src, *p)
						if err != nil {
							problems = append(problems, err.Error())
							checkedProfiles[pr] = nil
						} else {
							lim = &l
							checkedProfiles[pr] = lim
						}
					}
					if lim != nil {
						opts := yaml.DefaultOptions()
						opts.Limits = *lim
						var outcome any
						if p.single {
							_, lerr := yaml.LoadBytes(input, opts)
							if lerr != nil {
								outcome = errorJSON(lerr, false)
							} else {
								outcome = map[string]any{"documents": json.Number("1")}
							}
						} else {
							docs, lerr := yaml.LoadAllBytes(input, opts)
							if lerr != nil {
								outcome = errorJSON(lerr, false)
							} else {
								outcome = map[string]any{"documents": json.Number(strconv.Itoa(len(docs)))}
							}
						}
						fmt.Printf("load-only\t%s\t%s\n", id, mustJSON(outcome))
						rk := rowKey{profile: p.id, rev: 3, kind: "load-only"}
						rc := rows[rk]
						if rc == nil {
							rc = &rowCounts{status: p.status}
							rows[rk] = rc
						}
						rc.pass++
					}
				}
			}

			if rawNL, hasNL := ex["node_locations"]; hasNL {
				nl, _ := rawNL.(map[string]any)
				profile, _ := asString(nl["profile"])
				rev, _ := asUint64(nl["profile_revision"])
				convName, _ := asString(nl["convention"])
				convRev, _ := asUint64(nl["convention_revision"])
				var p *profileSpec
				for i := range profiles {
					if profiles[i].id == profile && profiles[i].revision == rev {
						p = &profiles[i]
						break
					}
				}
				if p == nil {
					problems = append(problems, fmt.Sprintf("%s: node_locations for unsupported profile %s r%d", id, profile, rev))
					continue
				}
				pr := profRev{id: profile, rev: rev}
				if !spec13Profiles[pr] {
					problems = append(problems, fmt.Sprintf("%s: node_locations bound to %s r%d, which does not cite spec 1.3", id, profile, rev))
					continue
				}
				if convName != "yamlprims-node" || convRev != 1 {
					problems = append(problems, fmt.Sprintf("%s: unsupported node convention %s r%d", id, convName, convRev))
					continue
				}
				if !conventionChecked {
					conventionChecked = true
					conventionErr = checkNodeConvention(src)
				}
				if conventionErr != nil {
					problems = append(problems, fmt.Sprintf("%s: %v", id, conventionErr))
					continue
				}
				lim, seen := checkedProfiles[pr]
				if !seen {
					l, err := checkProfileFile(src, *p)
					if err != nil {
						problems = append(problems, err.Error())
						checkedProfiles[pr] = nil
						continue
					}
					lim = &l
					checkedProfiles[pr] = lim
				}
				if lim == nil {
					continue
				}
				opts := yaml.DefaultOptions()
				opts.MergeKeys = p.merge
				opts.Limits = *lim

				var valExp map[string]any
				valList, _ := ex["value"].([]any)
				for _, rawV := range valList {
					vm, _ := rawV.(map[string]any)
					vp, _ := asString(vm["profile"])
					vr, _ := asUint64(vm["profile_revision"])
					if vp == profile && vr == rev {
						valExp = vm
						break
					}
				}
				results, err := nodeSpans(input, opts, p.single, valExp, nl)
				if err != nil {
					problems = append(problems, fmt.Sprintf("%s: malformed node_locations: %v", id, err))
				} else {
					for _, res := range results {
						rk := rowKey{profile: profile, rev: rev, kind: "node_locations"}
						rc := rows[rk]
						if rc == nil {
							rc = &rowCounts{status: p.status}
							rows[rk] = rc
						}
						if matchesResult(res.got, res.want) {
							rc.pass++
						} else {
							rc.fail++
							failures = append(failures, fmt.Sprintf("%s [%s r%d node_locations %s]\n  want %s\n  got  %s",
								id, profile, rev, res.at, mustJSON(res.want), mustJSON(res.got)))
						}
					}
				}
			}
		}
	}

	keys := make([]rowKey, 0, len(rows))
	for k := range rows {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		if keys[i].profile != keys[j].profile {
			return keys[i].profile < keys[j].profile
		}
		if keys[i].rev != keys[j].rev {
			return keys[i].rev < keys[j].rev
		}
		return keys[i].kind < keys[j].kind
	})

	fmt.Println("profile\trevision\tkind\tstatus\tpass\tfail\texcluded")
	ran := 0
	for _, k := range keys {
		rc := rows[k]
		x := excluded[k]
		ran += rc.pass + rc.fail
		fmt.Printf("%s\t%d\t%s\t%s\t%d\t%d\t%d\n", k.profile, k.rev, k.kind, rc.status, rc.pass, rc.fail, x)
	}
	fmt.Printf("not-compared\tevents\t%d cases\t%d with no other expectation\t%d with event locations\n",
		withEvents, eventsOnly, eventLocations)

	limitFails := len(failures)
	if limitFails > details {
		limitFails = details
	}
	for _, f := range failures[:limitFails] {
		fmt.Println(f)
	}
	if len(failures) > details {
		fmt.Printf("... %d more failures\n", len(failures)-details)
	}
	if cases == 0 || ran == 0 {
		problems = append(problems, fmt.Sprintf("empty inventory: %d cases, %d expectations run", cases, ran))
	}
	if len(failures) > 0 {
		problems = append(problems, fmt.Sprintf("%d expectations failed", len(failures)))
	}
	limitProbs := len(problems)
	if limitProbs > details {
		limitProbs = details
	}
	for _, p := range problems[:limitProbs] {
		fmt.Printf("problem\t%s\n", p)
	}
	if caseFile != "" {
		hwm := "unknown"
		if raw, err := os.ReadFile("/proc/self/status"); err == nil {
			for _, l := range strings.Split(string(raw), "\n") {
				if strings.HasPrefix(l, "VmHWM:") {
					fields := strings.Fields(l)
					if len(fields) >= 2 {
						hwm = fields[1]
					}
					break
				}
			}
		}
		fmt.Printf("peak_rss_kib\t%s\n", hwm)
	}
	if len(problems) == 0 {
		fmt.Printf("result\tPASS\t%d cases\t%d expectations\n", cases, ran)
		os.Exit(0)
	}
	fmt.Printf("result\tFAIL\t%d problems\n", len(problems))
	os.Exit(1)
}

func isJSONWS(c byte) bool {
	return c == ' ' || c == '\t' || c == '\n' || c == '\r'
}

func trimJSONStart(s string) string {
	i := 0
	for i < len(s) && isJSONWS(s[i]) {
		i++
	}
	return s[i:]
}

func trimJSON(s string) string {
	s = trimJSONStart(s)
	j := len(s)
	for j > 0 && isJSONWS(s[j-1]) {
		j--
	}
	return s[:j]
}

func parseJSONAny(s string) (any, error) {
	rest := trimJSON(s)
	v, err := parseDeepJSON(&rest, 0)
	if err != nil {
		return nil, err
	}
	if trimJSON(rest) != "" {
		return nil, fmt.Errorf("trailing characters after JSON value")
	}
	return v, nil
}

func parseDeepJSON(s *string, depth int) (any, error) {
	if depth > 512 {
		return nil, fmt.Errorf("JSON nesting exceeds 512")
	}
	*s = trimJSONStart(*s)
	if len(*s) == 0 {
		return nil, fmt.Errorf("unexpected EOF")
	}
	switch (*s)[0] {
	case '{':
		*s = trimJSONStart((*s)[1:])
		m := make(map[string]any)
		if strings.HasPrefix(*s, "}") {
			*s = (*s)[1:]
			return m, nil
		}
		for {
			k, err := parseJSONStr(s)
			if err != nil {
				return nil, err
			}
			*s = trimJSONStart(*s)
			if !strings.HasPrefix(*s, ":") {
				return nil, fmt.Errorf("expected ':'")
			}
			*s = (*s)[1:]
			v, err := parseDeepJSON(s, depth+1)
			if err != nil {
				return nil, err
			}
			m[k] = v
			*s = trimJSONStart(*s)
			if strings.HasPrefix(*s, ",") {
				*s = trimJSONStart((*s)[1:])
			} else if strings.HasPrefix(*s, "}") {
				*s = (*s)[1:]
				return m, nil
			} else {
				return nil, fmt.Errorf("expected ',' or '}'")
			}
		}
	case '[':
		*s = trimJSONStart((*s)[1:])
		items := make([]any, 0)
		if strings.HasPrefix(*s, "]") {
			*s = (*s)[1:]
			return items, nil
		}
		for {
			v, err := parseDeepJSON(s, depth+1)
			if err != nil {
				return nil, err
			}
			items = append(items, v)
			*s = trimJSONStart(*s)
			if strings.HasPrefix(*s, ",") {
				*s = trimJSONStart((*s)[1:])
			} else if strings.HasPrefix(*s, "]") {
				*s = (*s)[1:]
				return items, nil
			} else {
				return nil, fmt.Errorf("expected ',' or ']'")
			}
		}
	case '"':
		return parseJSONStr(s)
	default:
		end := len(*s)
		for i := 0; i < len(*s); i++ {
			b := (*s)[i]
			if b == ',' || b == ']' || b == '}' || isJSONWS(b) {
				end = i
				break
			}
		}
		tok := (*s)[:end]
		*s = (*s)[end:]
		dec := json.NewDecoder(strings.NewReader(tok))
		dec.UseNumber()
		var v any
		if err := dec.Decode(&v); err != nil {
			return nil, err
		}
		var extra any
		if err := dec.Decode(&extra); err != io.EOF {
			return nil, fmt.Errorf("trailing token in JSON primitive")
		}
		return v, nil
	}
}

func parseJSONStr(s *string) (string, error) {
	*s = trimJSONStart(*s)
	if !strings.HasPrefix(*s, `"`) {
		return "", fmt.Errorf("expected string")
	}
	bytes := *s
	i := 1
	for i < len(bytes) {
		switch bytes[i] {
		case '\\':
			i += 2
		case '"':
			tok := bytes[:i+1]
			*s = bytes[i+1:]
			var out string
			if err := json.Unmarshal([]byte(tok), &out); err != nil {
				return "", err
			}
			return out, nil
		default:
			i++
		}
	}
	return "", fmt.Errorf("unterminated string")
}

func parseJSONMap(s string) (map[string]any, error) {
	v, err := parseJSONAny(s)
	if err != nil {
		return nil, err
	}
	m, ok := v.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("expected JSON object")
	}
	return m, nil
}

func mustJSON(v any) string {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	_ = enc.Encode(v)
	return strings.TrimSuffix(buf.String(), "\n")
}

func asString(v any) (string, bool) {
	s, ok := v.(string)
	return s, ok
}

func asUint64(v any) (uint64, bool) {
	switch x := v.(type) {
	case json.Number:
		u, err := strconv.ParseUint(x.String(), 10, 64)
		return u, err == nil
	case float64:
		if x >= 0 && math.Trunc(x) == x {
			return uint64(x), true
		}
	}
	return 0, false
}

func checkProfileFile(src string, p profileSpec) (yaml.Limits, error) {
	f := filepath.Join(src, "manifests", "profiles", fmt.Sprintf("%s.r%d.json", p.id, p.revision))
	raw, err := os.ReadFile(f)
	if err != nil {
		return yaml.Limits{}, fmt.Errorf("%s: %v", f, err)
	}
	doc, err := parseJSONMap(string(raw))
	if err != nil {
		return yaml.Limits{}, fmt.Errorf("%s: %v", f, err)
	}
	wantPolicy, err := parseJSONAny(p.policy)
	if err != nil {
		return yaml.Limits{}, fmt.Errorf("driver policy JSON: %v", err)
	}
	id, _ := asString(doc["id"])
	rev, _ := asUint64(doc["revision"])
	if id != p.id || rev != p.revision || !reflect.DeepEqual(doc["policy"], wantPolicy) {
		return yaml.Limits{}, fmt.Errorf("%s: policy %s differs from the driver's %s", f, mustJSON(doc["policy"]), mustJSON(wantPolicy))
	}
	rawLimits, hasLimits := doc["limits"]
	if !hasLimits && !p.limits {
		return yaml.DefaultLimits(), nil
	}
	if hasLimits && p.limits {
		l, err := parseLimits(rawLimits)
		if err != nil {
			return yaml.Limits{}, fmt.Errorf("%s: limits: %v", f, err)
		}
		return l, nil
	}
	return yaml.Limits{}, fmt.Errorf("%s: limits %v where the driver expects %v", f, hasLimits, p.limits)
}

func parseLimits(raw any) (yaml.Limits, error) {
	obj, ok := raw.(map[string]any)
	if !ok {
		return yaml.Limits{}, fmt.Errorf("not an object")
	}
	out := yaml.DefaultLimits()
	type slot struct {
		name string
		dst  *int
	}
	fields := []slot{
		{"max_input_bytes", &out.MaxInputBytes},
		{"max_depth", &out.MaxDepth},
		{"max_nodes", &out.MaxNodes},
		{"max_expanded_nodes", &out.MaxExpandedNodes},
		{"max_alias_events", &out.MaxAliasEvents},
		{"max_owned_bytes", &out.MaxOwnedBytes},
		{"max_scalar_bytes", &out.MaxScalarBytes},
		{"max_key_bytes", &out.MaxKeyBytes},
		{"max_mapping_entries", &out.MaxMappingEntries},
		{"max_documents", &out.MaxDocuments},
		{"max_output_bytes", &out.MaxOutputBytes},
		{"max_warnings", &out.MaxWarnings},
	}
	known := make(map[string]bool, len(fields))
	for _, f := range fields {
		known[f.name] = true
	}
	for k := range obj {
		if !known[k] {
			return yaml.Limits{}, fmt.Errorf("unknown limit %s", k)
		}
	}
	for _, f := range fields {
		v, exists := obj[f.name]
		if !exists {
			return yaml.Limits{}, fmt.Errorf("missing %s", f.name)
		}
		u, ok := asUint64(v)
		if !ok || u > uint64(math.MaxInt) {
			return yaml.Limits{}, fmt.Errorf("%s is not a non-negative integer", f.name)
		}
		*f.dst = int(u)
	}
	o := yaml.DefaultOptions()
	o.Limits = out
	if err := o.Validate(); err != nil {
		return yaml.Limits{}, fmt.Errorf("%s", err.Message)
	}
	return out, nil
}

func checkNodeConvention(src string) error {
	f := filepath.Join(src, "manifests", "span-conventions", "yamlprims-node.r1.json")
	raw, err := os.ReadFile(f)
	if err != nil {
		return fmt.Errorf("%s: %v", f, err)
	}
	doc, err := parseJSONMap(string(raw))
	if err != nil {
		return fmt.Errorf("%s: %v", f, err)
	}
	id, _ := asString(doc["id"])
	rev, _ := asUint64(doc["revision"])
	offsetUnit, _ := asString(doc["offset_unit"])
	interval, _ := asString(doc["interval"])
	selects, _ := asString(doc["selects"])
	rawNodes, _ := doc["defined_nodes"].([]any)
	var gotNodes []string
	for _, rn := range rawNodes {
		if s, ok := asString(rn); ok {
			gotNodes = append(gotNodes, s)
		}
	}
	_, hasEvents := doc["defined_events"]
	if id != "yamlprims-node" || rev != 1 || offsetUnit != "utf8-byte" || interval != "half-open" ||
		selects != "value-node" || !reflect.DeepEqual(gotNodes, nodeKinds) || hasEvents {
		return fmt.Errorf("%s: convention differs from the driver's", f)
	}
	return nil
}

type spanCheckResult struct {
	at   string
	got  any
	want any
}

func nodeSpans(input []byte, opts yaml.Options, single bool, valueExp map[string]any, nl map[string]any) ([]spanCheckResult, error) {
	if valueExp == nil {
		return nil, fmt.Errorf("no value expectation with documents for this profile")
	}
	wantDocs, ok := valueExp["documents"].([]any)
	if !ok {
		return nil, fmt.Errorf("no value expectation with documents for this profile")
	}
	spans, ok := nl["spans"].([]any)
	if !ok || len(spans) == 0 {
		return nil, fmt.Errorf("no spans")
	}
	var docs []yaml.Document
	var loadErr *yaml.Error
	if single {
		d, err := yaml.LoadBytes(input, opts)
		if err != nil {
			loadErr = err
		} else {
			docs = []yaml.Document{d}
		}
	} else {
		docs, loadErr = yaml.LoadAllBytes(input, opts)
	}

	var out []spanCheckResult
	for _, rawS := range spans {
		s, _ := rawS.(map[string]any)
		dIdx, ok := asUint64(s["document"])
		if !ok {
			return nil, fmt.Errorf("span without a document index")
		}
		steps, ok := s["path"].([]any)
		if !ok {
			return nil, fmt.Errorf("span without a path")
		}
		start, okS := asUint64(s["start"])
		end, okE := asUint64(s["end"])
		if !okS || !okE {
			return nil, fmt.Errorf("span without start and end")
		}
		if start > end || end > uint64(len(input)) {
			return nil, fmt.Errorf("span [%d, %d) outside the input", start, end)
		}
		if int(dIdx) >= len(wantDocs) {
			return nil, fmt.Errorf("document index outside the expected value")
		}
		w := wantDocs[int(dIdx)]
		for _, st := range steps {
			var err error
			w, err = expectedStep(w, st)
			if err != nil {
				return nil, err
			}
		}
		wm, _ := w.(map[string]any)
		kind, ok := asString(wm["t"])
		if !ok {
			return nil, fmt.Errorf("expected node without a kind")
		}
		validKind := false
		for _, nk := range nodeKinds {
			if nk == kind {
				validKind = true
				break
			}
		}
		if !validKind {
			return nil, fmt.Errorf("node kind %s has no span in yamlprims-node r1", kind)
		}
		at := fmt.Sprintf("document %d path %s", dIdx, mustJSON(steps))
		var got any
		if loadErr != nil {
			got = errorJSON(loadErr, false)
		} else if int(dIdx) >= len(docs) {
			got = map[string]any{"missing": at}
		} else {
			n := docs[int(dIdx)].Root()
			for _, st := range steps {
				n = loadedStep(n, st)
				if n == nil {
					break
				}
			}
			if n == nil {
				got = map[string]any{"missing": at}
			} else {
				not := notation(n)
				tStr := not["t"]
				if sp, ok := n.Span(); ok {
					got = map[string]any{
						"t":     tStr,
						"start": json.Number(strconv.Itoa(sp.Start.ByteOffset)),
						"end":   json.Number(strconv.Itoa(sp.End.ByteOffset)),
					}
				} else {
					got = map[string]any{
						"t":    tStr,
						"span": "synthetic",
					}
				}
			}
		}
		want := map[string]any{
			"t":     kind,
			"start": json.Number(strconv.FormatUint(start, 10)),
			"end":   json.Number(strconv.FormatUint(end, 10)),
		}
		out = append(out, spanCheckResult{at: at, got: got, want: want})
	}
	return out, nil
}

func parseStep(st any) (string, int, error) {
	o, ok := st.(map[string]any)
	if !ok || len(o) != 1 {
		return "", 0, fmt.Errorf("path step %s is not a single step", mustJSON(st))
	}
	for k, v := range o {
		switch k {
		case "key", "value", "item":
			idx, ok := asUint64(v)
			if !ok {
				return "", 0, fmt.Errorf("path step %s: bad index", mustJSON(st))
			}
			return k, int(idx), nil
		case "inner":
			if b, ok := v.(bool); ok && b {
				return "inner", 0, nil
			}
		}
	}
	return "", 0, fmt.Errorf("unknown path step %s", mustJSON(st))
}

func expectedStep(w any, st any) (any, error) {
	k, i, err := parseStep(st)
	if err != nil {
		return nil, err
	}
	wm, _ := w.(map[string]any)
	tStr, _ := asString(wm["t"])
	switch {
	case (k == "key" || k == "value") && tStr == "map":
		entries, _ := wm["entries"].([]any)
		if i < len(entries) {
			pair, _ := entries[i].([]any)
			if len(pair) == 2 {
				if k == "key" {
					return pair[0], nil
				}
				return pair[1], nil
			}
		}
	case k == "item" && tStr == "seq":
		items, _ := wm["items"].([]any)
		if i < len(items) {
			return items[i], nil
		}
	case k == "inner" && tStr == "tagged":
		if inner, ok := wm["inner"]; ok {
			return inner, nil
		}
	}
	return nil, fmt.Errorf("path step %s does not resolve in the expected value", mustJSON(st))
}

func loadedStep(n *yaml.Node, st any) *yaml.Node {
	k, i, err := parseStep(st)
	if err != nil {
		return nil
	}
	v := n.Value()
	switch k {
	case "key":
		if m, ok := v.AsMapping(); ok && i < m.Len() {
			keyNode, _ := m.At(i)
			return &keyNode
		}
	case "value":
		if m, ok := v.AsMapping(); ok && i < m.Len() {
			_, valNode := m.At(i)
			return &valNode
		}
	case "item":
		if seq, ok := v.AsSequence(); ok && i < len(seq) {
			return &seq[i]
		}
	case "inner":
		if tag, ok := v.AsTagged(); ok {
			return tag.Value()
		}
	}
	return nil
}

func expectedResult(kind string, exp map[string]any) (map[string]any, error) {
	if rawErr, hasErr := exp["error"]; hasErr {
		e, ok := rawErr.(map[string]any)
		if !ok {
			return nil, fmt.Errorf("error is not an object")
		}
		phase, ok := asString(e["phase"])
		if !ok {
			return nil, fmt.Errorf("error without phase")
		}
		switch phase {
		case "input", "syntax", "resolution", "structure", "limit", "typed":
		default:
			return nil, fmt.Errorf("unknown phase %s", phase)
		}
		if phase == "limit" {
			if _, hasLim := e["limit"]; !hasLim {
				return nil, fmt.Errorf("limit error without a limit name")
			}
		}
		return map[string]any{"error": e}, nil
	}
	if kind == "value" {
		docs, ok := exp["documents"].([]any)
		if !ok {
			return nil, fmt.Errorf("no documents or error")
		}
		return map[string]any{"documents": docs}, nil
	}
	res, ok := exp["result"].(map[string]any)
	if !ok {
		return nil, fmt.Errorf("no result or error")
	}
	return res, nil
}

func readInput(caseObj map[string]any, hyd, src string) ([]byte, error) {
	inputObj, _ := caseObj["input"].(map[string]any)
	var path string
	if p, ok := asString(inputObj["path"]); ok {
		cand := filepath.Join(hyd, p)
		if _, err := os.Stat(cand); err == nil {
			path = cand
		} else {
			path = filepath.Join(src, p)
		}
	} else {
		id, _ := asString(caseObj["id"])
		idx := strings.LastIndexByte(id, '.')
		if idx < 0 {
			return nil, fmt.Errorf("recipe case id without a suffix")
		}
		recipe, suffix := id[:idx], id[idx+1:]
		path = filepath.Join(hyd, "inputs", "yaml", recipe, suffix+".yaml")
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("%s: %v", path, err)
	}
	wantBytes, ok := asUint64(inputObj["bytes"])
	if !ok || uint64(len(b)) != wantBytes {
		return nil, fmt.Errorf("%s: %d bytes, record says %v", path, len(b), inputObj["bytes"])
	}
	return b, nil
}

func runValue(input []byte, opts yaml.Options, single bool) map[string]any {
	var docs []yaml.Document
	var err *yaml.Error
	if single {
		d, lerr := yaml.LoadBytes(input, opts)
		if lerr != nil {
			err = lerr
		} else {
			docs = []yaml.Document{d}
		}
	} else {
		docs, err = yaml.LoadAllBytes(input, opts)
	}
	if err != nil {
		return errorJSON(err, false)
	}
	outDocs := make([]any, len(docs))
	for i := range docs {
		outDocs[i] = notation(docs[i].Root())
	}
	return map[string]any{"documents": outDocs}
}

func runTyped(input []byte, opts yaml.Options, exp map[string]any) (map[string]any, error) {
	doc, lerr := yaml.LoadBytes(input, opts)
	if lerr != nil {
		return errorJSON(lerr, false), nil
	}
	n := doc.Root()
	pathList, ok := exp["path"].([]any)
	if !ok {
		return nil, fmt.Errorf("no path")
	}
	for _, seg := range pathList {
		v := n.Value()
		switch s := seg.(type) {
		case string:
			m, ok := v.AsMapping()
			if !ok {
				return nil, fmt.Errorf("path %q does not apply", s)
			}
			child, ok := m.Get(s)
			if !ok {
				return nil, fmt.Errorf("path key %q missing", s)
			}
			n = child
		case json.Number:
			seq, ok := v.AsSequence()
			if !ok {
				return nil, fmt.Errorf("path %s does not apply", s.String())
			}
			idx, err := strconv.Atoi(s.String())
			if err != nil || idx < 0 || idx >= len(seq) {
				return nil, fmt.Errorf("path index %s missing", s.String())
			}
			n = &seq[idx]
		default:
			return nil, fmt.Errorf("path %v does not apply", seg)
		}
	}

	target, ok := asString(exp["target"])
	if !ok {
		return nil, fmt.Errorf("no target")
	}
	nodeVal := *n
	switch target {
	case "bool":
		v, err := yaml.NodeToBool(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "bool", "v": v}, nil
	case "option-bool":
		v, err := yaml.NodeToOptionalBool(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		if v == nil {
			return map[string]any{"t": "none"}, nil
		}
		return map[string]any{"t": "bool", "v": *v}, nil
	case "string":
		v, err := yaml.NodeToString(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "str", "v": v}, nil
	case "u8":
		v, err := yaml.NodeToUint8(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatUint(uint64(v), 10)}, nil
	case "u16":
		v, err := yaml.NodeToUint16(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatUint(uint64(v), 10)}, nil
	case "u32":
		v, err := yaml.NodeToUint32(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatUint(uint64(v), 10)}, nil
	case "u64":
		v, err := yaml.NodeToUint64(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatUint(v, 10)}, nil
	case "u128":
		v, err := yaml.NodeToUint128(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": v.String()}, nil
	case "i8":
		v, err := yaml.NodeToInt8(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatInt(int64(v), 10)}, nil
	case "i16":
		v, err := yaml.NodeToInt16(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatInt(int64(v), 10)}, nil
	case "i32":
		v, err := yaml.NodeToInt32(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatInt(int64(v), 10)}, nil
	case "i64":
		v, err := yaml.NodeToInt64(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": strconv.FormatInt(v, 10)}, nil
	case "i128":
		v, err := yaml.NodeToInt128(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "int", "v": v.String()}, nil
	case "f32":
		v, err := yaml.NodeToFloat32(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		return map[string]any{"t": "float32", "bits": fmt.Sprintf("%08x", math.Float32bits(v))}, nil
	case "f64":
		v, err := yaml.NodeToFloat64(nodeVal, opts)
		if err != nil {
			return errorJSON(err, true), nil
		}
		bits := math.Float64bits(v)
		if math.IsNaN(v) {
			bits = 0x7ff8000000000000
		}
		return map[string]any{"t": "float", "bits": fmt.Sprintf("%016x", bits)}, nil
	default:
		return nil, fmt.Errorf("unknown target %s", target)
	}
}

func notation(n *yaml.Node) map[string]any {
	v := n.Value()
	switch v.Kind() {
	case yaml.ValueKindNull:
		return map[string]any{"t": "null"}
	case yaml.ValueKindBool:
		b, _ := v.AsBool()
		return map[string]any{"t": "bool", "v": b}
	case yaml.ValueKindInt:
		i, _ := v.AsInt()
		return map[string]any{"t": "int", "v": i.String()}
	case yaml.ValueKindFloat:
		f, _ := v.AsFloat()
		bits := math.Float64bits(f)
		if math.IsNaN(f) {
			bits = 0x7ff8000000000000
		}
		return map[string]any{"t": "float", "bits": fmt.Sprintf("%016x", bits)}
	case yaml.ValueKindString:
		s, _ := v.AsStr()
		return map[string]any{"t": "str", "v": s}
	case yaml.ValueKindSequence:
		seq, _ := v.AsSequence()
		items := make([]any, len(seq))
		for i := range seq {
			items[i] = notation(&seq[i])
		}
		return map[string]any{"t": "seq", "items": items}
	case yaml.ValueKindMapping:
		m, _ := v.AsMapping()
		entries := make([]any, m.Len())
		for i := 0; i < m.Len(); i++ {
			k, val := m.At(i)
			entries[i] = []any{notation(&k), notation(&val)}
		}
		return map[string]any{"t": "map", "entries": entries}
	case yaml.ValueKindTagged:
		tag, _ := v.AsTagged()
		return map[string]any{"t": "tagged", "tag": tag.Tag().URI(), "inner": notation(tag.Value())}
	default:
		panic("unknown value kind")
	}
}

func errorJSON(e *yaml.Error, typed bool) map[string]any {
	kind := e.Kind.Name()
	var phase string
	switch {
	case typed:
		phase = "typed"
	case e.Kind == yaml.ErrorKindInvalidUtf8 ||
		(e.Kind == yaml.ErrorKindLimitExceeded && e.Detail.Limit == yaml.LimitMaxInputBytes):
		phase = "input"
	case e.Kind == yaml.ErrorKindSyntax:
		phase = "syntax"
	case e.Kind == yaml.ErrorKindLimitExceeded:
		phase = "limit"
	case e.Kind == yaml.ErrorKindDuplicateKey ||
		e.Kind == yaml.ErrorKindInvalidKey ||
		e.Kind == yaml.ErrorKindUnknownAnchor ||
		e.Kind == yaml.ErrorKindRecursiveAlias ||
		e.Kind == yaml.ErrorKindMergeRejected ||
		e.Kind == yaml.ErrorKindMergeSource ||
		e.Kind == yaml.ErrorKindMultipleDocuments:
		phase = "structure"
	default:
		phase = "resolution"
	}
	out := map[string]any{
		"phase": phase,
		"kind":  kind,
	}
	if e.Kind == yaml.ErrorKindLimitExceeded {
		out["limit"] = e.Detail.Limit.FieldName()
		out["configured"] = json.Number(strconv.FormatUint(e.Detail.Configured, 10))
	}
	return map[string]any{"error": out}
}

func matchesResult(got, want any) bool {
	gm, gOk := got.(map[string]any)
	wm, wOk := want.(map[string]any)
	if gOk && wOk {
		gErr, gHasErr := gm["error"].(map[string]any)
		wErr, wHasErr := wm["error"].(map[string]any)
		if gHasErr && wHasErr {
			for k, wv := range wErr {
				gv, exists := gErr[k]
				if !exists || !reflect.DeepEqual(gv, wv) {
					return false
				}
			}
			return true
		}
		if gHasErr != wHasErr {
			return false
		}
	}
	return reflect.DeepEqual(got, want)
}

func runStdin(args []string) int {
	format := "wire"
	single := false
	opts := yaml.DefaultOptions()
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--format":
			if i+1 < len(args) {
				format = args[i+1]
				i++
			}
		case "--single":
			single = true
		case "--merge":
			if i+1 < len(args) {
				switch args[i+1] {
				case "apply":
					opts.MergeKeys = yaml.MergeKeysApply
				case "as-key", "plain-key":
					opts.MergeKeys = yaml.MergeKeysAsKey
				default:
					opts.MergeKeys = yaml.MergeKeysReject
				}
				i++
			}
		case "--unknown-tags":
			if i+1 < len(args) {
				if args[i+1] == "reject" {
					opts.UnknownTags = yaml.UnknownTagsReject
				} else {
					opts.UnknownTags = yaml.UnknownTagsPreserve
				}
				i++
			}
		case "--limits":
			if i+1 < len(args) {
				for _, kv := range strings.Split(args[i+1], ",") {
					parts := strings.SplitN(kv, "=", 2)
					if len(parts) != 2 {
						continue
					}
					n, err := strconv.Atoi(parts[1])
					if err != nil {
						return 2
					}
					switch parts[0] {
					case "max_input_bytes":
						opts.Limits.MaxInputBytes = n
					case "max_depth":
						opts.Limits.MaxDepth = n
					case "max_nodes":
						opts.Limits.MaxNodes = n
					case "max_expanded_nodes":
						opts.Limits.MaxExpandedNodes = n
					case "max_alias_events":
						opts.Limits.MaxAliasEvents = n
					case "max_owned_bytes":
						opts.Limits.MaxOwnedBytes = n
					case "max_scalar_bytes":
						opts.Limits.MaxScalarBytes = n
					case "max_key_bytes":
						opts.Limits.MaxKeyBytes = n
					case "max_mapping_entries":
						opts.Limits.MaxMappingEntries = n
					case "max_documents":
						opts.Limits.MaxDocuments = n
					case "max_output_bytes":
						opts.Limits.MaxOutputBytes = n
					case "max_warnings":
						opts.Limits.MaxWarnings = n
					}
				}
				i++
			}
		}
	}

	input, err := io.ReadAll(os.Stdin)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}

	defOpts := yaml.DefaultOptions()
	switch format {
	case "wire":
		var docs []yaml.Document
		var lerr *yaml.Error
		if single {
			d, err := yaml.LoadBytes(input, opts)
			if err != nil {
				lerr = err
			} else {
				docs = []yaml.Document{d}
			}
		} else {
			docs, lerr = yaml.LoadAllBytes(input, opts)
		}
		if lerr != nil {
			out, encErr := yaml.EncodeError(lerr, defOpts)
			if encErr != nil {
				fmt.Fprintln(os.Stderr, encErr)
				return 2
			}
			fmt.Println(out)
			return 0
		}
		out, encErr := yaml.Encode(docs, opts)
		if encErr != nil {
			errOut, e2 := yaml.EncodeError(encErr, defOpts)
			if e2 != nil {
				fmt.Fprintln(os.Stderr, e2)
				return 2
			}
			fmt.Println(errOut)
			return 0
		}
		fmt.Println(out)
		return 0

	case "plain":
		doc, lerr := yaml.LoadBytes(input, opts)
		if lerr != nil {
			out, encErr := yaml.EncodeError(lerr, defOpts)
			if encErr != nil {
				fmt.Fprintln(os.Stderr, encErr)
				return 2
			}
			fmt.Println(out)
			return 0
		}
		out, perr := yaml.EncodePlain(doc.IntoRoot(), opts)
		if perr != nil {
			errOut, e2 := yaml.EncodeError(perr, defOpts)
			if e2 != nil {
				fmt.Fprintln(os.Stderr, e2)
				return 2
			}
			fmt.Println(errOut)
			return 0
		}
		fmt.Println(out)
		return 0

	case "roundtrip":
		env, derr := yaml.DecodeWire(string(input), opts)
		if derr != nil {
			fmt.Fprintln(os.Stderr, derr)
			return 1
		}
		var out string
		var encErr *yaml.Error
		if env.HasDocuments {
			out, encErr = yaml.Encode(env.Documents, opts)
		} else {
			out, encErr = yaml.EncodeError(env.Error, opts)
		}
		if encErr != nil {
			fmt.Fprintln(os.Stderr, encErr)
			return 1
		}
		fmt.Println(out)
		return 0

	default:
		fmt.Fprintf(os.Stderr, "unknown format %s\n", format)
		return 2
	}
}

func decodeHex(h string) ([]byte, error) {
	if len(h)%2 != 0 {
		return nil, fmt.Errorf("odd hex length")
	}
	out := make([]byte, len(h)/2)
	for i := 0; i < len(h); i += 2 {
		b, err := strconv.ParseUint(h[i:i+2], 16, 8)
		if err != nil {
			return nil, err
		}
		out[i/2] = byte(b)
	}
	return out, nil
}

func runCrosscheckJSONL() int {
	raw, err := io.ReadAll(os.Stdin)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	defOpts := yaml.DefaultOptions()
	for n, line := range strings.Split(string(raw), "\n") {
		if trimJSON(line) == "" {
			continue
		}
		rec, err := parseJSONMap(line)
		if err != nil {
			fmt.Fprintf(os.Stderr, "line %d: %v\n", n+1, err)
			return 2
		}
		id, _ := asString(rec["id"])
		if code, ok := asString(rec["syntax_code"]); ok {
			sk, valid := yaml.SyntaxKindFromCode(code)
			if !valid {
				fmt.Fprintf(os.Stderr, "%s: unknown syntax_code %s\n", id, code)
				return 2
			}
			synErr := &yaml.Error{
				Kind:    yaml.ErrorKindSyntax,
				Detail:  yaml.ErrorDetail{SyntaxKind: sk},
				Site:    yaml.SiteSource(yaml.Location{ByteOffset: 0, Line: 1, Column: 1}),
				Message: "syntax error: " + code,
			}
			wire, encErr := yaml.EncodeError(synErr, defOpts)
			if encErr != nil {
				fmt.Fprintf(os.Stderr, "%s: EncodeError: %v\n", id, encErr)
				return 2
			}
			env, decErr := yaml.DecodeWire(wire, defOpts)
			if decErr != nil {
				fmt.Fprintf(os.Stderr, "%s: DecodeWire: %v\n", id, decErr)
				return 2
			}
			rt, encErr := yaml.EncodeError(env.Error, defOpts)
			if encErr != nil {
				fmt.Fprintf(os.Stderr, "%s: roundtrip EncodeError: %v\n", id, encErr)
				return 2
			}
			fmt.Println(mustJSON(map[string]any{
				"id":        id,
				"wire":      wire,
				"roundtrip": rt,
				"plain":     "",
			}))
			continue
		}

		var input []byte
		if hexStr, ok := asString(rec["input_hex"]); ok {
			input, err = decodeHex(hexStr)
			if err != nil {
				fmt.Fprintf(os.Stderr, "%s: %v\n", id, err)
				return 2
			}
		} else if s, ok := asString(rec["input"]); ok {
			input = []byte(s)
		} else {
			fmt.Fprintf(os.Stderr, "%s: missing input or input_hex\n", id)
			return 2
		}

		single, _ := rec["single"].(bool)
		checkPlain, _ := rec["check_plain"].(bool)
		opts := yaml.DefaultOptions()
		if m, ok := asString(rec["merge"]); ok {
			switch m {
			case "apply":
				opts.MergeKeys = yaml.MergeKeysApply
			case "as-key", "plain-key":
				opts.MergeKeys = yaml.MergeKeysAsKey
			default:
				opts.MergeKeys = yaml.MergeKeysReject
			}
		}
		if u, ok := asString(rec["unknown_tags"]); ok {
			if u == "reject" {
				opts.UnknownTags = yaml.UnknownTagsReject
			} else {
				opts.UnknownTags = yaml.UnknownTagsPreserve
			}
		}
		if limObj, ok := rec["limits"].(map[string]any); ok {
			for k, v := range limObj {
				u, ok := asUint64(v)
				if !ok {
					continue
				}
				n := int(u)
				switch k {
				case "max_input_bytes":
					opts.Limits.MaxInputBytes = n
				case "max_depth":
					opts.Limits.MaxDepth = n
				case "max_nodes":
					opts.Limits.MaxNodes = n
				case "max_expanded_nodes":
					opts.Limits.MaxExpandedNodes = n
				case "max_alias_events":
					opts.Limits.MaxAliasEvents = n
				case "max_owned_bytes":
					opts.Limits.MaxOwnedBytes = n
				case "max_scalar_bytes":
					opts.Limits.MaxScalarBytes = n
				case "max_key_bytes":
					opts.Limits.MaxKeyBytes = n
				case "max_mapping_entries":
					opts.Limits.MaxMappingEntries = n
				case "max_documents":
					opts.Limits.MaxDocuments = n
				case "max_output_bytes":
					opts.Limits.MaxOutputBytes = n
				case "max_warnings":
					opts.Limits.MaxWarnings = n
				}
			}
		}

		var docs []yaml.Document
		var lerr *yaml.Error
		if single {
			d, err := yaml.LoadBytes(input, opts)
			if err != nil {
				lerr = err
			} else {
				docs = []yaml.Document{d}
			}
		} else {
			docs, lerr = yaml.LoadAllBytes(input, opts)
		}

		var wire string
		if lerr != nil {
			w, e2 := yaml.EncodeError(lerr, defOpts)
			if e2 != nil {
				fmt.Fprintf(os.Stderr, "%s: EncodeError: %v\n", id, e2)
				return 2
			}
			wire = w
		} else {
			w, encErr := yaml.Encode(docs, opts)
			if encErr != nil {
				w2, e2 := yaml.EncodeError(encErr, defOpts)
				if e2 != nil {
					fmt.Fprintf(os.Stderr, "%s: EncodeError(encErr): %v\n", id, e2)
					return 2
				}
				wire = w2
			} else {
				wire = w
			}
		}

		rtOpts := defOpts
		if opts.Validate() == nil {
			if opts.Limits.MaxInputBytes > rtOpts.Limits.MaxInputBytes {
				rtOpts.Limits.MaxInputBytes = opts.Limits.MaxInputBytes
			}
			if opts.Limits.MaxDepth > rtOpts.Limits.MaxDepth {
				rtOpts.Limits.MaxDepth = opts.Limits.MaxDepth
			}
			if opts.Limits.MaxNodes > rtOpts.Limits.MaxNodes {
				rtOpts.Limits.MaxNodes = opts.Limits.MaxNodes
			}
			if opts.Limits.MaxExpandedNodes > rtOpts.Limits.MaxExpandedNodes {
				rtOpts.Limits.MaxExpandedNodes = opts.Limits.MaxExpandedNodes
			}
			if opts.Limits.MaxAliasEvents > rtOpts.Limits.MaxAliasEvents {
				rtOpts.Limits.MaxAliasEvents = opts.Limits.MaxAliasEvents
			}
			if opts.Limits.MaxOwnedBytes > rtOpts.Limits.MaxOwnedBytes {
				rtOpts.Limits.MaxOwnedBytes = opts.Limits.MaxOwnedBytes
			}
			if opts.Limits.MaxScalarBytes > rtOpts.Limits.MaxScalarBytes {
				rtOpts.Limits.MaxScalarBytes = opts.Limits.MaxScalarBytes
			}
			if opts.Limits.MaxKeyBytes > rtOpts.Limits.MaxKeyBytes {
				rtOpts.Limits.MaxKeyBytes = opts.Limits.MaxKeyBytes
			}
			if opts.Limits.MaxMappingEntries > rtOpts.Limits.MaxMappingEntries {
				rtOpts.Limits.MaxMappingEntries = opts.Limits.MaxMappingEntries
			}
			if opts.Limits.MaxDocuments > rtOpts.Limits.MaxDocuments {
				rtOpts.Limits.MaxDocuments = opts.Limits.MaxDocuments
			}
			if opts.Limits.MaxOutputBytes > rtOpts.Limits.MaxOutputBytes {
				rtOpts.Limits.MaxOutputBytes = opts.Limits.MaxOutputBytes
			}
			if opts.Limits.MaxWarnings > rtOpts.Limits.MaxWarnings {
				rtOpts.Limits.MaxWarnings = opts.Limits.MaxWarnings
			}
		}

		env, decErr := yaml.DecodeWire(wire, rtOpts)
		if decErr != nil {
			fmt.Fprintf(os.Stderr, "%s: DecodeWire(%s): %v\n", id, wire, decErr)
			return 2
		}
		var rt string
		var rtErr *yaml.Error
		if env.HasDocuments {
			rt, rtErr = yaml.Encode(env.Documents, rtOpts)
		} else {
			rt, rtErr = yaml.EncodeError(env.Error, rtOpts)
		}
		if rtErr != nil {
			fmt.Fprintf(os.Stderr, "%s: roundtrip Encode: %v\n", id, rtErr)
			return 2
		}

		var plain string
		if checkPlain {
			doc, perr := yaml.LoadBytes(input, opts)
			if perr != nil {
				p, e2 := yaml.EncodeError(perr, defOpts)
				if e2 != nil {
					fmt.Fprintf(os.Stderr, "%s: plain EncodeError: %v\n", id, e2)
					return 2
				}
				plain = p
			} else {
				p, perr := yaml.EncodePlain(doc.IntoRoot(), opts)
				if perr != nil {
					p2, e2 := yaml.EncodeError(perr, defOpts)
					if e2 != nil {
						fmt.Fprintf(os.Stderr, "%s: EncodePlain error: %v\n", id, e2)
						return 2
					}
					plain = p2
				} else {
					plain = p
				}
			}
		}

		fmt.Println(mustJSON(map[string]any{
			"id":        id,
			"wire":      wire,
			"roundtrip": rt,
			"plain":     plain,
		}))
	}
	return 0
}
