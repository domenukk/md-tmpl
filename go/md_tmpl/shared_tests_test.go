package md_tmpl

// Comprehensive shared conformance test runner covering all 569 cases across
// the 6 central cross-language test suites in tests/shared/*.toml.
//
// Language parity: Rust and TypeScript execute these exact same fixtures.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/BurntSushi/toml"
)

func sharedCorpusDir() string {
	return filepath.Join("..", "..", "tests", "shared")
}

var sharedCorpusFiles = []string{
	"inline_tmpl_tests.toml",
	"include_tests.toml",
	"inline_control_tests.toml",
	"tmpl_param_tests.toml",
	"feature_e2e_tests.toml",
	"env_tests.toml",
}

// transformOptionValues implements TOML option-value transformations:
// - string "None" -> nil
// - string "Some(x)" -> "x" (escape literal "None")
// - recursively processes slices and maps
func transformOptionValues(v any) any {
	switch x := v.(type) {
	case string:
		if x == "None" {
			return nil
		}
		if strings.HasPrefix(x, "Some(") && strings.HasSuffix(x, ")") {
			return x[5 : len(x)-1]
		}
		return x
	case []any:
		out := make([]any, len(x))
		for i, elem := range x {
			out[i] = transformOptionValues(elem)
		}
		return out
	case map[string]any:
		out := make(map[string]any, len(x))
		for k, elem := range x {
			out[k] = transformOptionValues(elem)
		}
		return out
	default:
		return v
	}
}

// matchesExpectedError checks if an error string matches a pipe-separated
// pattern (e.g. "reserved keyword|shadows built-in"). Case-insensitive.
func matchesExpectedError(errStr, pattern string) bool {
	lowerErr := strings.ToLower(errStr)
	alts := strings.Split(pattern, "|")
	for _, alt := range alts {
		if strings.Contains(lowerErr, strings.ToLower(strings.TrimSpace(alt))) {
			return true
		}
	}
	return false
}

func joinStringSlice(lines []any) string {
	var parts []string
	for _, l := range lines {
		if s, ok := l.(string); ok {
			parts = append(parts, s)
		}
	}
	return strings.Join(parts, "\n")
}

func resolveTemplateString(tc map[string]any, stringKey, linesKey string) string {
	if val, ok := tc[stringKey].(string); ok && val != "" {
		if strings.HasSuffix(val, ".tmpl.md") {
			fullPath := filepath.Join(sharedCorpusDir(), val)
			if data, err := os.ReadFile(fullPath); err == nil {
				return string(data)
			}
		}
		return val
	}
	if arr, ok := tc[linesKey].([]any); ok {
		return joinStringSlice(arr)
	}
	if arr, ok := tc[linesKey].([]string); ok {
		return strings.Join(arr, "\n")
	}
	return ""
}

func resolveFileContent(val any) string {
	switch x := val.(type) {
	case string:
		if strings.HasSuffix(x, ".tmpl.md") {
			fullPath := filepath.Join(sharedCorpusDir(), x)
			if data, err := os.ReadFile(fullPath); err == nil {
				return string(data)
			}
		}
		return x
	case []any:
		return joinStringSlice(x)
	case []string:
		return strings.Join(x, "\n")
	default:
		return ""
	}
}

func loadSharedCases(t *testing.T, file string) []map[string]any {
	t.Helper()
	path := filepath.Join(sharedCorpusDir(), file)
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read shared file %s: %v", path, err)
	}
	var root map[string]any
	if err := toml.Unmarshal(data, &root); err != nil {
		t.Fatalf("parse shared file %s: %v", path, err)
	}
	var cases []map[string]any
	switch arr := root["tests"].(type) {
	case []map[string]any:
		cases = arr
	case []any:
		for _, item := range arr {
			if m, ok := item.(map[string]any); ok {
				cases = append(cases, m)
			}
		}
	default:
		t.Fatalf("%s: [[tests]] key missing or invalid type %T", file, root["tests"])
	}
	return cases
}

// extractExpectedKey finds and removes key ("expected_output" or "expected_error")
// if it was placed inside nested tables due to TOML table header placement.
func extractExpectedKey(m map[string]any, key string) (string, bool) {
	if s, ok := m[key].(string); ok {
		delete(m, key)
		return s, true
	}
	for _, v := range m {
		if sub, ok := v.(map[string]any); ok {
			if s, found := extractExpectedKey(sub, key); found {
				return s, true
			}
		}
	}
	return "", false
}

func runSharedCase(t *testing.T, tc map[string]any) {
	t.Helper()
	name, _ := tc["name"].(string)

	var source string
	var baseDir string

	// An include test case is defined by the presence of files table or parent_template.
	_, hasFiles := tc["files"]
	_, hasParentTmpl := tc["parent_template"]
	_, hasParentLines := tc["parent_template_lines"]

	if hasFiles || hasParentTmpl || hasParentLines {
		tempDir, err := os.MkdirTemp("", "go-pt-shared-*")
		if err != nil {
			t.Fatalf("[%s] temp dir: %v", name, err)
		}
		defer os.RemoveAll(tempDir)

		if filesMap, ok := tc["files"].(map[string]any); ok {
			for relPath, rawContent := range filesMap {
				content := resolveFileContent(rawContent)
				fullFile := filepath.Join(tempDir, relPath)
				if err := os.MkdirAll(filepath.Dir(fullFile), 0755); err != nil {
					t.Fatalf("[%s] mkdir for file %s: %v", name, relPath, err)
				}
				if err := os.WriteFile(fullFile, []byte(content), 0644); err != nil {
					t.Fatalf("[%s] write file %s: %v", name, relPath, err)
				}
			}
		}
		baseDir = tempDir
		source = resolveTemplateString(tc, "parent_template", "parent_template_lines")
	} else {
		source = resolveTemplateString(tc, "template", "template_lines")
	}

	var opts []Option
	if baseDir != "" {
		opts = append(opts, WithBaseDir(baseDir))
	}
	if rawEnv, ok := tc["env"].(map[string]any); ok {
		transformedEnv := make(map[string]any, len(rawEnv))
		for k, v := range rawEnv {
			transformedEnv[k] = transformOptionValues(v)
		}
		opts = append(opts, WithEnv(transformedEnv))
	}

	wantOutput, hasOutput := extractExpectedKey(tc, "expected_output")
	wantError, hasError := extractExpectedKey(tc, "expected_error")

	var rawParams map[string]any
	if p, ok := tc["params"].(map[string]any); ok {
		rawParams = p
	}
	transformedParams := make(map[string]any)
	if rawParams != nil {
		if m, ok := transformOptionValues(rawParams).(map[string]any); ok {
			transformedParams = m
		}
	}

	tmpl, compileErr := FromSource(source, opts...)

	if hasOutput {
		if compileErr != nil {
			t.Fatalf("[%s] expected render output, but compile failed: %v", name, compileErr)
		}
		defer tmpl.Close()

		gotOutput, renderErr := tmpl.RenderMap(transformedParams)
		if renderErr != nil {
			t.Fatalf("[%s] expected render output, but render failed: %v", name, renderErr)
		}
		if gotOutput != wantOutput {
			t.Fatalf("[%s] output mismatch\n  want: %q\n  got:  %q", name, wantOutput, gotOutput)
		}
		return
	}

	if hasError {
		if compileErr != nil {
			if !matchesExpectedError(compileErr.Error(), wantError) {
				t.Fatalf("[%s] compile error %q did not match pattern %q", name, compileErr.Error(), wantError)
			}
			return
		}
		defer tmpl.Close()

		_, renderErr := tmpl.RenderMap(transformedParams)
		if renderErr == nil {
			t.Fatalf("[%s] expected error matching %q, but compile & render both succeeded", name, wantError)
		}
		if !matchesExpectedError(renderErr.Error(), wantError) {
			t.Fatalf("[%s] render error %q did not match pattern %q", name, renderErr.Error(), wantError)
		}
		return
	}

	t.Fatalf("[%s] test case has neither expected_output nor expected_error", name)
}

func TestSharedConformance(t *testing.T) {
	totalCases := 0
	for _, file := range sharedCorpusFiles {
		cases := loadSharedCases(t, file)
		if len(cases) == 0 {
			t.Fatalf("shared test suite %s is empty", file)
		}
		t.Run(file, func(t *testing.T) {
			for _, tc := range cases {
				name, _ := tc["name"].(string)
				t.Run(name, func(t *testing.T) {
					runSharedCase(t, tc)
				})
			}
		})
		totalCases += len(cases)
	}
	t.Logf("Executed all %d shared cross-language conformance test cases in Go", totalCases)
}
