package md_tmpl

// Regression tests for "delimiters inside quoted strings" in frontmatter
// default values.
//
// In a default declaration (`name = type := literal`), delimiters — commas
// `,` and the bracket family `()[]{}<>` — that appear inside quoted string
// literals ("..." or '...') must be treated as literal characters, NOT as
// field/element separators.
//
// The Go binding does not parse default literals itself: FromSource crosses
// the cgo/FFI boundary into the Rust core, which owns the quote-aware
// splitting (crates/md-tmpl-core/.../params.rs::split_at_depth_zero). These
// tests therefore lock in the behaviour end-to-end: Go inherits the fix, and
// any regression in the core (or the FFI marshalling of parsed defaults) will
// break them.
//
// The cases mirror the canonical regression matrix (T1–T8). Defaults() decodes
// the core's JSON, so scalars arrive as: string -> string, int -> float64,
// list -> []any, struct/record -> map[string]any.

import (
	"strings"
	"testing"
)

// asAnySlice fails the test unless v is a []any of the expected length.
func asAnySlice(t *testing.T, name string, v any, wantLen int) []any {
	t.Helper()
	s, ok := v.([]any)
	if !ok {
		t.Fatalf("%s: default is %T, want []any", name, v)
	}
	if len(s) != wantLen {
		t.Fatalf("%s: got %d items, want %d (%#v)", name, len(s), wantLen, s)
	}
	return s
}

// asAnyMap fails the test unless v is a map[string]any.
func asAnyMap(t *testing.T, name string, v any) map[string]any {
	t.Helper()
	m, ok := v.(map[string]any)
	if !ok {
		t.Fatalf("%s: default is %T, want map[string]any", name, v)
	}
	return m
}

// asString fails the test unless v is a string equal to want.
func wantString(t *testing.T, name string, v any, want string) {
	t.Helper()
	s, ok := v.(string)
	if !ok {
		t.Fatalf("%s: default element is %T, want string", name, v)
	}
	if s != want {
		t.Fatalf("%s: got %q, want %q", name, s, want)
	}
}

// parseMatrixDefaults compiles a template containing every T1–T8 default
// declaration and returns the parsed defaults map. Parsing succeeds only if
// the Rust core split each literal quote-aware; a regression would surface
// here as a FromSource error or a wrong element count/value.
func parseMatrixDefaults(t *testing.T) map[string]any {
	t.Helper()
	// Each param carries an embedded-delimiter default. allow_unused keeps the
	// body minimal; we only care about default parsing, not rendering.
	src := `---
params:
  - t1 = list(str) := ["a, b", "c, d"]
  - t2 = struct(msg = str, n = int) := {msg = "a, b", n = 1}
  - t3 = list(name = str, note = str) := [{name = "x", note = "p, q, r"}, {name = "y", note = "s"}]
  - t4 = list(str) := ["a[b]c", "d{e}f", "g(h)i"]
  - t5 = list(str) := ['a, b', 'c']
  - t6 = list(str) := ["Theory — not a finding", "✅ done, ok"]
  - t7 = list(name = str, note = str) := [{name = "", note = ""}, {name = "a", note = "y, z"}]

allow_unused: true
---
body`

	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource with embedded-delimiter defaults failed: %v", err)
	}
	t.Cleanup(func() { tmpl.Close() })

	defaults := tmpl.Defaults()
	if defaults == nil {
		t.Fatal("Defaults() returned nil for template with defaults")
	}
	return defaults
}

// TestDefaultsMatrixParses is the top-level end-to-end guard: a single template
// carrying all T1–T8 defaults must compile via FromSource (i.e. the Rust core
// parsed every literal without choking on embedded delimiters).
func TestDefaultsMatrixParses(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	for _, name := range []string{"t1", "t2", "t3", "t4", "t5", "t6", "t7"} {
		if _, ok := defaults[name]; !ok {
			t.Errorf("missing default for %q; got keys %v", name, keysOf(defaults))
		}
	}
}

func keysOf(m map[string]any) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	return ks
}

// T1: commas inside double-quoted list-of-str elements stay literal.
func TestDefaultCommaInDoubleQuotedListItems(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	items := asAnySlice(t, "t1", defaults["t1"], 2)
	wantString(t, "t1[0]", items[0], "a, b")
	wantString(t, "t1[1]", items[1], "c, d")
}

// T2: a comma inside a struct string field does not split the record.
func TestDefaultCommaInStructStringField(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	m := asAnyMap(t, "t2", defaults["t2"])
	wantString(t, "t2.msg", m["msg"], "a, b")
	if n, ok := m["n"].(float64); !ok || n != 1 {
		t.Fatalf("t2.n: got %#v, want 1", m["n"])
	}
}

// T3: a list of records where one record's string field contains commas.
func TestDefaultCommasInListOfRecords(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	recs := asAnySlice(t, "t3", defaults["t3"], 2)
	r0 := asAnyMap(t, "t3[0]", recs[0])
	wantString(t, "t3[0].name", r0["name"], "x")
	wantString(t, "t3[0].note", r0["note"], "p, q, r")
	r1 := asAnyMap(t, "t3[1]", recs[1])
	wantString(t, "t3[1].name", r1["name"], "y")
	wantString(t, "t3[1].note", r1["note"], "s")
}

// T4: the bracket family ()[]{}  inside quoted strings stays literal.
func TestDefaultBracketsInQuotedListItems(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	items := asAnySlice(t, "t4", defaults["t4"], 3)
	wantString(t, "t4[0]", items[0], "a[b]c")
	wantString(t, "t4[1]", items[1], "d{e}f")
	wantString(t, "t4[2]", items[2], "g(h)i")
}

// T5: single-quoted strings are also quote-aware for embedded commas.
func TestDefaultCommaInSingleQuotedListItems(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	items := asAnySlice(t, "t5", defaults["t5"], 2)
	wantString(t, "t5[0]", items[0], "a, b")
	wantString(t, "t5[1]", items[1], "c")
}

// T6: unicode (em-dash, emoji) plus an embedded comma survive intact.
func TestDefaultUnicodeAndCommaInListItems(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	items := asAnySlice(t, "t6", defaults["t6"], 2)
	wantString(t, "t6[0]", items[0], "Theory — not a finding")
	wantString(t, "t6[1]", items[1], "✅ done, ok")
}

// T7: empty quoted strings must not collapse or mis-split records.
func TestDefaultEmptyStringsInListOfRecords(t *testing.T) {
	defaults := parseMatrixDefaults(t)
	recs := asAnySlice(t, "t7", defaults["t7"], 2)
	r0 := asAnyMap(t, "t7[0]", recs[0])
	wantString(t, "t7[0].name", r0["name"], "")
	wantString(t, "t7[0].note", r0["note"], "")
	r1 := asAnyMap(t, "t7[1]", recs[1])
	wantString(t, "t7[1].name", r1["name"], "a")
	wantString(t, "t7[1].note", r1["note"], "y, z")
}

// T8: a faithful reduced SEVERITY_LADDER-style list-of-records — the
// real-world regression that motivated the fix. Prose fields carry commas,
// em-dashes and emoji; none of these may break record/field splitting.
func TestDefaultSeverityLadderListOfRecords(t *testing.T) {
	src := `---
params:
  - severity_ladder = list(tier = str, short = str, proves = str) := [{tier = "L1", short = "Theoretical", proves = "Reachability, in principle, not demonstrated"}, {tier = "L2", short = "Plausible — needs PoC", proves = "Control-flow reaches sink, no crash yet"}, {tier = "L3", short = "Confirmed ✅", proves = "Crash reproduced, sanitizer fired, input saved"}]

allow_unused: true
---
body`

	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource with SEVERITY_LADDER default failed: %v", err)
	}
	defer tmpl.Close()

	recs := asAnySlice(t, "severity_ladder", tmpl.Defaults()["severity_ladder"], 3)

	r0 := asAnyMap(t, "severity_ladder[0]", recs[0])
	wantString(t, "severity_ladder[0].tier", r0["tier"], "L1")
	wantString(t, "severity_ladder[0].short", r0["short"], "Theoretical")
	wantString(t, "severity_ladder[0].proves", r0["proves"], "Reachability, in principle, not demonstrated")

	r1 := asAnyMap(t, "severity_ladder[1]", recs[1])
	wantString(t, "severity_ladder[1].tier", r1["tier"], "L2")
	wantString(t, "severity_ladder[1].short", r1["short"], "Plausible — needs PoC")
	wantString(t, "severity_ladder[1].proves", r1["proves"], "Control-flow reaches sink, no crash yet")

	r2 := asAnyMap(t, "severity_ladder[2]", recs[2])
	wantString(t, "severity_ladder[2].tier", r2["tier"], "L3")
	wantString(t, "severity_ladder[2].short", r2["short"], "Confirmed ✅")
	wantString(t, "severity_ladder[2].proves", r2["proves"], "Crash reproduced, sanitizer fired, input saved")
}

// TestCodegenFromEmbeddedDelimiterDefaults proves the type-generation path also
// works on templates whose defaults carry embedded delimiters. Even though Go's
// codegen re-parses only TYPE specs, GenerateTypes first calls FromSource, so a
// core regression in default parsing would fail codegen too. Success shows the
// core parsed the defaults and produced the expected Go types.
func TestCodegenFromEmbeddedDelimiterDefaults(t *testing.T) {
	src := `---
params:
  - tags = list(str) := ["a, b", "c, d"]
  - config = struct(msg = str, n = int) := {msg = "x, y", n = 1}
  - rows = list(tier = str, proves = str) := [{tier = "L1", proves = "reaches, sink"}]

allow_unused: true
---
{{ tags }} {{ config.msg }} {{ rows }}`

	code, err := GenerateTypes(src, WithPackageName("gen"), WithRenderHelper(false))
	if err != nil {
		t.Fatalf("GenerateTypes on embedded-delimiter defaults: %v", err)
	}
	assertCompiles(t, code)

	for _, want := range []string{
		"Tags []string",
		"type Config struct",
		"type RowsItem struct",
		"Rows []RowsItem",
	} {
		if !containsNormalized(code, want) {
			t.Errorf("expected %q in generated code:\n%s", want, code)
		}
	}
}

// ---------------------------------------------------------------------------
// Extra edge cases (E1–E5) — currently correct behaviour, now locked in.
// ---------------------------------------------------------------------------

// parseSingleDefault compiles a one-param template and returns that param's
// parsed default. It keeps each edge case isolated so a failure points at the
// exact literal shape that regressed.
func parseSingleDefault(t *testing.T, decl string) any {
	t.Helper()
	src := "---\nparams:\n  - " + decl + "\n\nallow_unused: true\n---\nbody"
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource(%q) failed: %v", decl, err)
	}
	t.Cleanup(func() { tmpl.Close() })

	defaults := tmpl.Defaults()
	if defaults == nil {
		t.Fatalf("Defaults() returned nil for %q", decl)
	}
	v, ok := defaults["x"]
	if !ok {
		t.Fatalf("missing default for x in %q; got keys %v", decl, keysOf(defaults))
	}
	return v
}

// E1: an apostrophe inside a double-quoted item must not terminate the string,
// and the embedded comma stays literal.
func TestDefaultApostropheInDoubleQuotedItem(t *testing.T) {
	items := asAnySlice(t, "x", parseSingleDefault(t, `x = list(str) := ["it's, fine", "ok"]`), 2)
	wantString(t, "x[0]", items[0], "it's, fine")
	wantString(t, "x[1]", items[1], "ok")
}

// E2: an escaped/embedded double quote inside a single-quoted item is literal,
// and neither the inner quotes nor the comma split the element.
func TestDefaultDoubleQuoteInSingleQuotedItem(t *testing.T) {
	items := asAnySlice(t, "x", parseSingleDefault(t, `x = list(str) := ['say "hi", bye', "z"]`), 2)
	wantString(t, "x[0]", items[0], `say "hi", bye`)
	wantString(t, "x[1]", items[1], "z")
}

// E3: a nested list(list(str)) — commas inside inner-list strings and the
// nesting structure must both survive.
func TestDefaultNestedListOfLists(t *testing.T) {
	outer := asAnySlice(t, "x", parseSingleDefault(t, `x = list(list(str)) := [["a, b"], ["c, d", "e"]]`), 2)
	in0 := asAnySlice(t, "x[0]", outer[0], 1)
	wantString(t, "x[0][0]", in0[0], "a, b")
	in1 := asAnySlice(t, "x[1]", outer[1], 2)
	wantString(t, "x[1][0]", in1[0], "c, d")
	wantString(t, "x[1][1]", in1[1], "e")
}

// E4: leading/trailing spaces inside a quoted item are preserved verbatim.
func TestDefaultSpacesPreservedInQuotedItem(t *testing.T) {
	items := asAnySlice(t, "x", parseSingleDefault(t, `x = list(str) := [" a, b "]`), 1)
	wantString(t, "x[0]", items[0], " a, b ")
}

// E5a: a '#' with no leading space is an ordinary character and is kept.
func TestDefaultHashWithoutLeadingSpaceIsLiteral(t *testing.T) {
	wantString(t, "x", parseSingleDefault(t, `x = str := "a#b,c"`), "a#b,c")
}

// E5b: a ' #' (space-hash) starts a YAML inline comment (the core is not
// md-tmpl-string-aware), truncating the scalar to `x = str := "a` — an
// unterminated string literal that FromSource rejects as an invalid default.
// Wrap the whole declaration in outer YAML quotes to keep a literal ' #'
// (see TestDefaultOuterYamlQuotesProtectHash).
func TestDefaultSpaceHashStartsYamlComment(t *testing.T) {
	src := "---\nparams:\n  - x = str := \"a # b, c\"\n\nallow_unused: true\n---\nbody"
	tmpl, err := FromSource(src)
	if err == nil {
		tmpl.Close()
		t.Fatal("expected FromSource to reject a ` #`-truncated default, got success")
	}
	if !strings.Contains(err.Error(), "strings must be quoted") {
		t.Fatalf("expected 'strings must be quoted' error, got: %v", err)
	}
}

// E5c: outer YAML quotes around the whole declaration protect an inner ' #',
// recovering the full default value.
func TestDefaultOuterYamlQuotesProtectHash(t *testing.T) {
	wantString(t, "x", parseSingleDefault(t, `"x = str := \"a # b, c\""`), "a # b, c")
}

// E5d: a ' #' outside any string literal starts a YAML inline comment and is
// stripped, so the numeric default still parses.
func TestDefaultSpaceHashOutsideStringIsStripped(t *testing.T) {
	v := parseSingleDefault(t, `x = int := 3 # the retry count`)
	f, ok := v.(float64)
	if !ok || f != 3 {
		t.Fatalf("x: got %#v, want 3", v)
	}
}
