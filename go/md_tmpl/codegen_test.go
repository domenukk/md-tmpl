package md_tmpl

import (
	"go/parser"
	"go/token"
	"os"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// Helper: assert generated code compiles
// ---------------------------------------------------------------------------

func assertCompiles(t *testing.T, source string) {
	t.Helper()
	fset := token.NewFileSet()
	_, err := parser.ParseFile(fset, "generated.go", source, parser.AllErrors)
	if err != nil {
		t.Errorf("generated code does not compile:\n%s\n\nError: %v", source, err)
	}
}

// containsNormalized checks if haystack contains needle after collapsing
// whitespace runs to single spaces. This handles gofmt's tab alignment.
func containsNormalized(haystack, needle string) bool {
	return strings.Contains(normalizeWS(haystack), normalizeWS(needle))
}

func normalizeWS(s string) string {
	var b strings.Builder
	inSpace := false
	for _, r := range s {
		if r == ' ' || r == '\t' {
			if !inSpace {
				b.WriteRune(' ')
				inSpace = true
			}
		} else {
			b.WriteRune(r)
			inSpace = false
		}
	}
	return b.String()
}

// ---------------------------------------------------------------------------
// Basic primitive params
// ---------------------------------------------------------------------------

func TestGenerateBasicParams(t *testing.T) {
	source := `---
params:
  - name = str
  - count = int
  - score = float
  - active = bool

allow_unused: true
---
{{ name }} {{ count }} {{ score }} {{ active }}`

	code, err := GenerateTypes(source, WithPackageName("mypackage"))
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	// Verify package declaration.
	if !strings.Contains(code, "package mypackage") {
		t.Errorf("expected 'package mypackage', got:\n%s", code)
	}

	// Verify struct fields.
	for _, want := range []string{"Name string", "Count int64", "Score float64", "Active bool"} {
		if !containsNormalized(code, want) {
			t.Errorf("expected %q in generated code:\n%s", want, code)
		}
	}

	// Verify json tags.
	for _, want := range []string{`json:"name"`, `json:"count"`, `json:"score"`, `json:"active"`} {
		if !strings.Contains(code, want) {
			t.Errorf("expected json tag %q in generated code:\n%s", want, code)
		}
	}
}

// ---------------------------------------------------------------------------
// List params with fields
// ---------------------------------------------------------------------------

func TestGenerateListParams(t *testing.T) {
	source := `---
params:
  - findings = list(line = int, message = str)

allow_unused: true
---
> {% for f in findings %}

{{ f.line }}: {{ f.message }}

> {% /for %}`

	code, err := GenerateTypes(source)
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	// Verify item struct is generated.
	if !strings.Contains(code, "type FindingsItem struct") {
		t.Errorf("expected 'type FindingsItem struct' in generated code:\n%s", code)
	}

	// Verify slice type.
	if !strings.Contains(code, "Findings []FindingsItem") {
		t.Errorf("expected 'Findings []FindingsItem' in generated code:\n%s", code)
	}

	// Verify item fields (gofmt may add tab alignment).
	if !containsNormalized(code, "Line int64") {
		t.Errorf("expected 'Line int64' in FindingsItem:\n%s", code)
	}
	if !containsNormalized(code, "Message string") {
		t.Errorf("expected 'Message string' in FindingsItem:\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Struct params
// ---------------------------------------------------------------------------

func TestGenerateStructParams(t *testing.T) {
	source := `---
params:
  - config = struct(host = str, port = int)

allow_unused: true
---
{{ config.host }}:{{ config.port }}`

	code, err := GenerateTypes(source)
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	// Verify struct type.
	if !strings.Contains(code, "type Config struct") {
		t.Errorf("expected 'type Config struct' in generated code:\n%s", code)
	}

	// Verify field uses struct type directly.
	if !strings.Contains(code, "Config Config") {
		t.Errorf("expected 'Config Config' field in generated code:\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Enum params — unit variants
// ---------------------------------------------------------------------------

func TestGenerateEnumUnitVariants(t *testing.T) {
	source := `---
params:
  - severity = enum(Critical, High, Medium, Low)

allow_unused: true
---
{{ severity }}`

	code, err := GenerateTypes(source)
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	// Verify string type.
	if !strings.Contains(code, "type Severity string") {
		t.Errorf("expected 'type Severity string' in generated code:\n%s", code)
	}

	// Verify constants.
	for _, variant := range []string{"SeverityCritical", "SeverityHigh", "SeverityMedium", "SeverityLow"} {
		if !strings.Contains(code, variant) {
			t.Errorf("expected constant %q in generated code:\n%s", variant, code)
		}
	}

	// Verify constant values.
	if !strings.Contains(code, `"Critical"`) {
		t.Errorf("expected '\"Critical\"' string literal in generated code:\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Enum params — struct variants (mixed)
// ---------------------------------------------------------------------------

func TestGenerateEnumStructVariants(t *testing.T) {
	source := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)

allow_unused: true
---
> {% match outcome %}

> {% case Confirmed %}

{{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`

	code, err := GenerateTypes(source)
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	// Verify interface.
	if !strings.Contains(code, "type Outcome interface") {
		t.Errorf("expected 'type Outcome interface' in generated code:\n%s", code)
	}

	// Verify sealed method.
	if !strings.Contains(code, "isOutcome()") {
		t.Errorf("expected 'isOutcome()' method in generated code:\n%s", code)
	}

	// Verify variant types.
	if !strings.Contains(code, "type OutcomeConfirmed struct") {
		t.Errorf("expected 'type OutcomeConfirmed struct' in generated code:\n%s", code)
	}
	if !strings.Contains(code, "type OutcomeRejected struct") {
		t.Errorf("expected 'type OutcomeRejected struct' in generated code:\n%s", code)
	}

	// Verify struct variant field.
	if !containsNormalized(code, "Evidence string") {
		t.Errorf("expected 'Evidence string' in OutcomeConfirmed:\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Mixed complex template
// ---------------------------------------------------------------------------

func TestGenerateComplexMixed(t *testing.T) {
	source := `---
params:
  - file_path = str
  - severity = enum(Critical, High, Medium, Low)
  - findings = list(line = int, message = str)
  - config = struct(host = str, port = int)
  - verbose = bool

allow_unused: true
---
{{ file_path }} {{ severity }} {{ verbose }}

> {% for f in findings %}

{{ f.line }}

> {% /for %}

{{ config.host }}:{{ config.port }}`

	code, err := GenerateTypes(source, WithPackageName("analysis"), WithParamsName("ReviewParams"))
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	if !strings.Contains(code, "package analysis") {
		t.Errorf("expected 'package analysis':\n%s", code)
	}
	if !strings.Contains(code, "type ReviewParams struct") {
		t.Errorf("expected 'type ReviewParams struct':\n%s", code)
	}
	if !strings.Contains(code, "type Severity string") {
		t.Errorf("expected 'type Severity string':\n%s", code)
	}
	if !strings.Contains(code, "type FindingsItem struct") {
		t.Errorf("expected 'type FindingsItem struct':\n%s", code)
	}
	if !strings.Contains(code, "type Config struct") {
		t.Errorf("expected 'type Config struct':\n%s", code)
	}
	if !containsNormalized(code, "FilePath string") {
		t.Errorf("expected 'FilePath string':\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Render helper generation
// ---------------------------------------------------------------------------

func TestGenerateRenderHelper(t *testing.T) {
	source := `---
params:
  - name = str

allow_unused: true
---
{{ name }}`

	code, err := GenerateTypes(source, WithRenderHelper(true))
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	if !strings.Contains(code, "func (p Params) Render(") {
		t.Errorf("expected Render method in generated code:\n%s", code)
	}
	if !strings.Contains(code, "*md_tmpl.Template") {
		t.Errorf("expected md_tmpl.Template reference:\n%s", code)
	}
}

func TestGenerateWithoutRenderHelper(t *testing.T) {
	source := `---
params:
  - name = str

allow_unused: true
---
{{ name }}`

	code, err := GenerateTypes(source, WithRenderHelper(false))
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	if strings.Contains(code, "func (p Params) Render(") {
		t.Errorf("Render method should not be present when disabled:\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// GenerateTypesFromFile
// ---------------------------------------------------------------------------

func TestGenerateFromFile(t *testing.T) {
	dir := t.TempDir()
	path := dir + "/code_review.tmpl.md"
	content := `---
params:
  - file_path = str
  - severity = str
  - findings = list(line = int, message = str)
---

# Code Review: {{ file_path }}

Severity: {{ severity }}

## Findings

> {% for finding in findings %}

- Line {{ finding.line }}: {{ finding.message }}

  > {% /for %}
`
	if err := writeTestFile(path, content); err != nil {
		t.Fatalf("writing test file: %v", err)
	}

	code, err := GenerateTypesFromFile(path, WithPackageName("review"))
	if err != nil {
		t.Fatalf("GenerateTypesFromFile: %v", err)
	}

	assertCompiles(t, code)

	// Should derive name from filename: "code_review" → "CodeReviewParams"
	if !strings.Contains(code, "type CodeReviewParams struct") {
		t.Errorf("expected 'type CodeReviewParams struct' (derived from filename):\n%s", code)
	}

	if !strings.Contains(code, "type FindingsItem struct") {
		t.Errorf("expected 'type FindingsItem struct':\n%s", code)
	}
}

func TestGenerateFromFileOverrideName(t *testing.T) {
	dir := t.TempDir()
	path := dir + "/greeting.tmpl.md"
	content := `---
params: [name = str]
---
Hello {{ name }}!
`
	if err := writeTestFile(path, content); err != nil {
		t.Fatalf("writing test file: %v", err)
	}

	code, err := GenerateTypesFromFile(path, WithPackageName("main"), WithParamsName("MyParams"))
	if err != nil {
		t.Fatalf("GenerateTypesFromFile: %v", err)
	}

	assertCompiles(t, code)

	if !strings.Contains(code, "type MyParams struct") {
		t.Errorf("expected 'type MyParams struct' (overridden name):\n%s", code)
	}
}

func TestGenerateFromFileNotFound(t *testing.T) {
	_, err := GenerateTypesFromFile("/nonexistent/foo.tmpl.md")
	if err == nil {
		t.Fatal("expected error for missing file, got nil")
	}
}

// ---------------------------------------------------------------------------
// TypeSpec parser — edge cases
// ---------------------------------------------------------------------------

func TestParseTypeSpecPrimitives(t *testing.T) {
	for _, tc := range []struct {
		spec string
		kind typeKind
	}{
		{"str", kindStr},
		{"int", kindInt},
		{"float", kindFloat},
		{"bool", kindBool},
	} {
		node, err := parseTypeSpec(tc.spec)
		if err != nil {
			t.Errorf("parseTypeSpec(%q): %v", tc.spec, err)
			continue
		}
		if node.kind != tc.kind {
			t.Errorf("parseTypeSpec(%q) = kind %d, want %d", tc.spec, node.kind, tc.kind)
		}
	}
}

func TestParseTypeSpecList(t *testing.T) {
	node, err := parseTypeSpec("list(label = str, count = int)")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindList {
		t.Fatalf("expected kindList, got %d", node.kind)
	}
	if len(node.fields) != 2 {
		t.Fatalf("expected 2 fields, got %d", len(node.fields))
	}
	if node.fields[0].name != "label" || node.fields[0].typeNode.kind != kindStr {
		t.Errorf("field 0: got %v, want label=str", node.fields[0])
	}
	if node.fields[1].name != "count" || node.fields[1].typeNode.kind != kindInt {
		t.Errorf("field 1: got %v, want count=int", node.fields[1])
	}
}

func TestParseTypeSpecEnum(t *testing.T) {
	node, err := parseTypeSpec("enum(Confirmed(evidence = str), Rejected, NeedsWork)")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindEnum {
		t.Fatalf("expected kindEnum, got %d", node.kind)
	}
	if len(node.variants) != 3 {
		t.Fatalf("expected 3 variants, got %d", len(node.variants))
	}
	if node.variants[0].name != "Confirmed" || len(node.variants[0].fields) != 1 {
		t.Errorf("variant 0: got %v, want Confirmed(1 field)", node.variants[0])
	}
	if node.variants[1].name != "Rejected" || len(node.variants[1].fields) != 0 {
		t.Errorf("variant 1: got %v, want Rejected(0 fields)", node.variants[1])
	}
	if node.variants[2].name != "NeedsWork" || len(node.variants[2].fields) != 0 {
		t.Errorf("variant 2: got %v, want NeedsWork(0 fields)", node.variants[2])
	}
}

func TestParseTypeSpecStruct(t *testing.T) {
	node, err := parseTypeSpec("struct(host = str, port = int)")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindStruct {
		t.Fatalf("expected kindStruct, got %d", node.kind)
	}
	if len(node.fields) != 2 {
		t.Fatalf("expected 2 fields, got %d", len(node.fields))
	}
}

func TestParseTypeSpecNested(t *testing.T) {
	// A list with a nested enum field.
	node, err := parseTypeSpec("list(title = str, severity = enum(Critical, High, Low))")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindList {
		t.Fatalf("expected kindList, got %d", node.kind)
	}
	if len(node.fields) != 2 {
		t.Fatalf("expected 2 fields, got %d", len(node.fields))
	}
	// Second field should be an enum.
	if node.fields[1].typeNode.kind != kindEnum {
		t.Errorf("expected kindEnum for severity, got %d", node.fields[1].typeNode.kind)
	}
	if len(node.fields[1].typeNode.variants) != 3 {
		t.Errorf("expected 3 enum variants, got %d", len(node.fields[1].typeNode.variants))
	}
}

// ---------------------------------------------------------------------------
// PascalCase helper
// ---------------------------------------------------------------------------

func TestToPascalCase(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"file_path", "FilePath"},
		{"code_review", "CodeReview"},
		{"simple", "Simple"},
		{"a_b_c", "ABC"},
		{"kebab-case", "KebabCase"},
		{"already_Pascal", "AlreadyPascal"},
	}
	for _, tc := range tests {
		got := toPascalCase(tc.input)
		if got != tc.want {
			t.Errorf("toPascalCase(%q) = %q, want %q", tc.input, got, tc.want)
		}
	}
}

// ---------------------------------------------------------------------------
// Generated code header
// ---------------------------------------------------------------------------

func TestGeneratedCodeHasHeader(t *testing.T) {
	source := `---
params: [x = str]
allow_unused: true
---
{{ x }}`

	code, err := GenerateTypes(source)
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	if !strings.Contains(code, "Code generated by pt-gen-go; DO NOT EDIT.") {
		t.Errorf("expected generation header in output:\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Empty params
// ---------------------------------------------------------------------------

func TestGenerateEmptyParams(t *testing.T) {
	source := `---
params: []
---
Hello!`

	code, err := GenerateTypes(source, WithRenderHelper(false))
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	if !strings.Contains(code, "type Params struct") {
		t.Errorf("expected 'type Params struct':\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func writeTestFile(path, content string) error {
	return os.WriteFile(path, []byte(content), 0644)
}

// ---------------------------------------------------------------------------
// Option params — generates pointer types
// ---------------------------------------------------------------------------

func TestGenerateOptionParams(t *testing.T) {
	source := `---
params:
  - name = str
  - email = option(str)
  - age = option(int)
  - score = option(float)
  - active = option(bool)

allow_unused: true
---
{{ name }}`

	code, err := GenerateTypes(source, WithRenderHelper(false))
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	// Verify pointer types for option fields.
	if !containsNormalized(code, "Email *string") {
		t.Errorf("expected 'Email *string' for option(str):\n%s", code)
	}
	if !containsNormalized(code, "Age *int64") {
		t.Errorf("expected 'Age *int64' for option(int):\n%s", code)
	}
	if !containsNormalized(code, "Score *float64") {
		t.Errorf("expected 'Score *float64' for option(float):\n%s", code)
	}
	if !containsNormalized(code, "Active *bool") {
		t.Errorf("expected 'Active *bool' for option(bool):\n%s", code)
	}
	// Non-option field should not be a pointer.
	if !containsNormalized(code, "Name string") {
		t.Errorf("expected 'Name string' (not pointer):\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// Scalar list params — generates typed slices
// ---------------------------------------------------------------------------

func TestGenerateScalarListParams(t *testing.T) {
	source := `---
params:
  - tags = list(str)
  - scores = list(int)
  - weights = list(float)
  - flags = list(bool)

allow_unused: true
---
> {% for t in tags %}

{{ t }}

> {% /for %}
> {% for s in scores %}

{{ s }}

> {% /for %}
> {% for w in weights %}

{{ w }}

> {% /for %}
> {% for f in flags %}

{{ f }}

> {% /for %}`

	code, err := GenerateTypes(source, WithRenderHelper(false))
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	assertCompiles(t, code)

	// Verify typed slices.
	if !containsNormalized(code, "Tags []string") {
		t.Errorf("expected 'Tags []string' for scalar_list(str):\n%s", code)
	}
	if !containsNormalized(code, "Scores []int64") {
		t.Errorf("expected 'Scores []int64' for scalar_list(int):\n%s", code)
	}
	if !containsNormalized(code, "Weights []float64") {
		t.Errorf("expected 'Weights []float64' for scalar_list(float):\n%s", code)
	}
	if !containsNormalized(code, "Flags []bool") {
		t.Errorf("expected 'Flags []bool' for scalar_list(bool):\n%s", code)
	}
}

// ---------------------------------------------------------------------------
// TypeSpec parser — option and scalar_list
// ---------------------------------------------------------------------------

func TestParseTypeSpecOption(t *testing.T) {
	node, err := parseTypeSpec("option(str)")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindOption {
		t.Fatalf("expected kindOption, got %d", node.kind)
	}
	if node.innerType == nil {
		t.Fatal("expected innerType to be set")
	}
	if node.innerType.kind != kindStr {
		t.Errorf("expected inner type str, got %d", node.innerType.kind)
	}
}

func TestParseTypeSpecScalarList(t *testing.T) {
	node, err := parseTypeSpec("scalar_list(int)")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindScalarList {
		t.Fatalf("expected kindScalarList, got %d", node.kind)
	}
	if node.innerType == nil {
		t.Fatal("expected innerType to be set")
	}
	if node.innerType.kind != kindInt {
		t.Errorf("expected inner type int, got %d", node.innerType.kind)
	}
}

func TestParseTypeSpecListBareType(t *testing.T) {
	// list(str) (from Rust FFI for scalar typed lists) should parse as kindScalarList.
	node, err := parseTypeSpec("list(str)")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindScalarList {
		t.Fatalf("expected kindScalarList for list(str), got %d", node.kind)
	}
	if node.innerType == nil {
		t.Fatal("expected innerType to be set")
	}
	if node.innerType.kind != kindStr {
		t.Errorf("expected inner type str, got %d", node.innerType.kind)
	}
}

func TestParseTypeSpecOptionNested(t *testing.T) {
	// option wrapping a struct
	node, err := parseTypeSpec("option(struct(host = str, port = int))")
	if err != nil {
		t.Fatalf("parseTypeSpec: %v", err)
	}
	if node.kind != kindOption {
		t.Fatalf("expected kindOption, got %d", node.kind)
	}
	if node.innerType == nil || node.innerType.kind != kindStruct {
		t.Fatalf("expected inner type struct, got %v", node.innerType)
	}
	if len(node.innerType.fields) != 2 {
		t.Errorf("expected 2 fields in inner struct, got %d", len(node.innerType.fields))
	}
}
