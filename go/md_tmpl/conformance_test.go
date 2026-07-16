package md_tmpl

// Cross-language conformance harness (Go side).
//
// Replays the shared TOML corpus in <repo>/tests/conformance through the Go
// md_tmpl engine and asserts that every case matches the recorded expectation.
// The exact same corpus is replayed by the Rust, TypeScript, and Python
// harnesses; if all pass, the four backends are behaviourally identical on the
// covered surface.
//
// TOML has no null, so option-None is encoded in the corpus as the sentinel
// inline table `{ __none__ = true }` and decoded back to Go nil on load.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/BurntSushi/toml"
)

func corpusDir() string {
	// go/md_tmpl -> go -> <repo root>.
	return filepath.Join("..", "..", "tests", "conformance")
}

var corpusFiles = []string{
	"render.toml",
	"interpolation.toml",
	"frontmatter.toml",
	"errors.toml",
	"escapes.toml",
	"comments.toml",
	"literals.toml",
}

// denull decodes the corpus's `{ __none__ = true }` option-None sentinel back
// into a Go nil (TOML has no null of its own).
func denull(v any) any {
	switch x := v.(type) {
	case []map[string]any:
		for i := range x {
			if m, ok := denull(x[i]).(map[string]any); ok {
				x[i] = m
			}
		}
		return x
	case []any:
		for i := range x {
			x[i] = denull(x[i])
		}
		return x
	case map[string]any:
		if len(x) == 1 {
			if b, ok := x["__none__"].(bool); ok && b {
				return nil
			}
		}
		for k := range x {
			x[k] = denull(x[k])
		}
		return x
	default:
		return v
	}
}

func loadCases(t *testing.T, file string) []map[string]any {
	t.Helper()
	path := filepath.Join(corpusDir(), file)
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read corpus file %s: %v", path, err)
	}
	var root map[string]any
	if err := toml.Unmarshal(data, &root); err != nil {
		t.Fatalf("parse corpus file %s: %v", path, err)
	}

	var cases []map[string]any
	switch arr := root["cases"].(type) {
	case []map[string]any:
		cases = arr
	case []any:
		for _, item := range arr {
			m, ok := item.(map[string]any)
			if !ok {
				t.Fatalf("%s: a case is not a table (%T)", file, item)
			}
			cases = append(cases, m)
		}
	default:
		t.Fatalf("%s: [[cases]] has unexpected type %T", file, root["cases"])
	}

	for i := range cases {
		if m, ok := denull(cases[i]).(map[string]any); ok {
			cases[i] = m
		}
	}
	return cases
}

// jsonNorm projects a value through JSON so the Go engine's numeric types
// (int64/int/float64) and the TOML-decoded types compare structurally.
func jsonNorm(t *testing.T, v any) any {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("normalize marshal: %v", err)
	}
	var out any
	if err := json.Unmarshal(b, &out); err != nil {
		t.Fatalf("normalize unmarshal: %v", err)
	}
	return out
}

func compileCase(c map[string]any) (*Template, error) {
	source, _ := c["source"].(string)
	if env, ok := c["env"].(map[string]any); ok {
		return FromSourceWithEnv(source, env)
	}
	return FromSource(source)
}

func caseParams(c map[string]any) map[string]any {
	if params, ok := c["params"].(map[string]any); ok {
		return params
	}
	return map[string]any{}
}

func assertNeedle(t *testing.T, needle, haystack string) {
	t.Helper()
	if needle != "" && !strings.Contains(haystack, needle) {
		t.Fatalf("error %q lacks substring %q", haystack, needle)
	}
}

func checkRender(t *testing.T, c map[string]any, expect map[string]any) {
	t.Helper()
	tmpl, err := compileCase(c)
	if err != nil {
		t.Fatalf("compile error: %v", err)
	}
	got, err := tmpl.RenderMap(caseParams(c))
	if err != nil {
		t.Fatalf("render error: %v", err)
	}
	want, _ := expect["output"].(string)
	if got != want {
		t.Fatalf("render mismatch\n  want: %q\n  got:  %q", want, got)
	}
}

func checkDefault(t *testing.T, c map[string]any, expect map[string]any) {
	t.Helper()
	tmpl, err := compileCase(c)
	if err != nil {
		t.Fatalf("compile error: %v", err)
	}
	want, _ := expect["defaults"].(map[string]any)
	if want == nil {
		want = map[string]any{}
	}
	got := tmpl.Defaults()
	if !reflect.DeepEqual(jsonNorm(t, got), jsonNorm(t, want)) {
		t.Fatalf("defaults mismatch\n  want: %v\n  got:  %v", want, got)
	}
}

func checkError(t *testing.T, c map[string]any, expect map[string]any) {
	t.Helper()
	phase, _ := expect["phase"].(string)
	needle, _ := expect["error_contains"].(string)
	tmpl, compileErr := compileCase(c)

	switch phase {
	case "compile":
		if compileErr == nil {
			t.Fatalf("expected a COMPILE error but compile succeeded")
		}
		assertNeedle(t, needle, compileErr.Error())
	case "render", "any":
		// Both require a successful compile before rendering, except "any" also
		// accepts a compile-time failure (leak-safety may trip at either phase;
		// the phase is allowed to differ between backends).
		if compileErr != nil {
			if phase != "any" {
				t.Fatalf("expected a RENDER error but failed at COMPILE: %v", compileErr)
			}
			assertNeedle(t, needle, compileErr.Error())
			return
		}
		if _, err := tmpl.RenderMap(caseParams(c)); err != nil {
			assertNeedle(t, needle, err.Error())
		} else {
			t.Fatalf("expected a RENDER error but render succeeded")
		}
	default:
		t.Fatalf("bad phase %q", phase)
	}
}

func TestConformance(t *testing.T) {
	for _, file := range corpusFiles {
		cases := loadCases(t, file)
		if len(cases) == 0 {
			t.Fatalf("corpus file %s is empty", file)
		}
		for _, c := range cases {
			name, _ := c["name"].(string)
			expect, ok := c["expect"].(map[string]any)
			if !ok {
				t.Fatalf("%s/%s: missing expect table", file, name)
			}
			t.Run(file+"/"+name, func(t *testing.T) {
				switch expect["kind"] {
				case "render":
					checkRender(t, c, expect)
				case "default":
					checkDefault(t, c, expect)
				case "error":
					checkError(t, c, expect)
				default:
					t.Fatalf("unknown expect.kind %v", expect["kind"])
				}
			})
		}
	}
}
