package md_tmpl

import (
	"errors"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// Defaults introspection
// ---------------------------------------------------------------------------

func TestDefaultsMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
  - count = int := 5
  - flag = bool
---
{{ name }} {{ count }} {{ flag }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	defaults := tmpl.Defaults()
	if defaults == nil {
		t.Fatal("Defaults() returned nil")
	}
	if v, ok := defaults["name"]; !ok {
		t.Error("missing name default")
	} else if v != "World" {
		t.Errorf("name default = %v, want \"World\"", v)
	}
	if v, ok := defaults["count"]; !ok {
		t.Error("missing count default")
	} else if v != float64(5) {
		t.Errorf("count default = %v, want 5", v)
	}
	if _, ok := defaults["flag"]; ok {
		t.Error("flag should not have a default")
	}
}

func TestDefaultsContext(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
  - greeting = str
---
{{ greeting }} {{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := tmpl.DefaultsContext()
	defer ctx.Close()
	if err := ctx.SetStr("greeting", "Hello"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	// name already has default "World"

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello World" {
		t.Errorf("got %q, want %q", result, "Hello World")
	}
}

func TestDefaultsContextOverride(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
---
{{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := tmpl.DefaultsContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Alice" {
		t.Errorf("got %q, want %q", result, "Alice")
	}
}

func TestDefaultsEmpty(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	defaults := tmpl.Defaults()
	if len(defaults) != 0 {
		t.Errorf("expected empty defaults, got %v", defaults)
	}
}

func TestDeclarationDefaultField(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
  - count = int
---
{{ name }} {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	decls := tmpl.Declarations()
	found := map[string]Declaration{}
	for _, d := range decls {
		found[d.Name] = d
	}

	if d, ok := found["name"]; !ok {
		t.Error("missing 'name' declaration")
	} else if d.Default != "World" {
		t.Errorf("name.Default = %v, want \"World\"", d.Default)
	}
	if d, ok := found["count"]; !ok {
		t.Error("missing 'count' declaration")
	} else if d.Default != nil {
		t.Errorf("count.Default = %v, want nil", d.Default)
	}
}

func TestRenderStructAllowExtra(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name  string `json:"name"`
		Extra string `json:"extra"`
	}

	result, err := tmpl.RenderStruct(Params{Name: "Carol", Extra: "ignored"}, AllowExtra())
	if err != nil {
		t.Fatalf("RenderStruct(AllowExtra): %v", err)
	}
	if result != "Carol" {
		t.Errorf("got %q, want %q", result, "Carol")
	}
}

func TestClosedTemplateRenderStructReturnsError(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	type P struct {
		X string `json:"x"`
	}
	_, err = tmpl.RenderStruct(P{X: "val"})
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

func TestSetTmplClosedContextUsesErrClosed(t *testing.T) {
	ctx := NewContext()
	ctx.Close()

	tmpl, err := FromSourceAllowingUnused(`---
name: dummy
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	err = ctx.SetTmpl("t", tmpl)
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed for closed context, got %v", err)
	}
}

func TestSetTmplNilTemplateUsesErrClosed(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()

	err := ctx.SetTmpl("t", nil)
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed for nil template, got %v", err)
	}
}

func TestRenderStructBareFieldNames(t *testing.T) {
	// Fields without json tags use the exact Go field name (matching encoding/json).
	tmpl, err := FromSource(`---
params: [Greeting = str]
---
{{ Greeting }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Greeting string // no json tag — uses exact field name "Greeting"
	}

	result, err := tmpl.RenderStruct(Params{Greeting: "hello"})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "hello" {
		t.Errorf("got %q, want %q", result, "hello")
	}
}

func TestFromSourceWithBaseDir(t *testing.T) {
	// Should parse successfully with a base dir (no includes needed for basic test).
	tmpl, err := FromSourceWithBaseDir(`---
params: [x = str]
---
{{ x }}`, "/tmp")
	if err != nil {
		t.Fatalf("FromSourceWithBaseDir: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": "works"})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "works" {
		t.Errorf("got %q, want %q", result, "works")
	}
}

func TestValidateDeclarationsMatch(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }}{{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Should succeed when declarations match.
	expected := []Declaration{
		{Name: "name", Type: "str"},
		{Name: "count", Type: "int"},
	}
	if err := tmpl.ValidateDeclarations(expected); err != nil {
		t.Fatalf("ValidateDeclarations should pass, got: %v", err)
	}
}

func TestValidateDeclarationsMismatch(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }}{{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Should fail when declarations don't match.
	wrong := []Declaration{
		{Name: "name", Type: "str"},
		{Name: "count", Type: "float"},
	}
	err = tmpl.ValidateDeclarations(wrong)
	if err == nil {
		t.Fatal("expected error for mismatched declarations")
	}
	if !strings.Contains(err.Error(), "retyped") {
		t.Errorf("expected 'retyped' in error, got: %v", err)
	}
}

func TestValidateDeclarationsAdded(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }}{{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Only expect "name" — "count" should show as added.
	partial := []Declaration{{Name: "name", Type: "str"}}
	err = tmpl.ValidateDeclarations(partial)
	if err == nil {
		t.Fatal("expected error for extra declarations")
	}
	if !strings.Contains(err.Error(), "added") {
		t.Errorf("expected 'added' in error, got: %v", err)
	}
}

func TestValidateDeclarationsClosedTemplate(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	err = tmpl.ValidateDeclarations([]Declaration{{Name: "x", Type: "str"}})
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

func TestImportedConstantsEmpty(t *testing.T) {
	// Simple template with no imports should return nil.
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ic := tmpl.ImportedConstants()
	if ic != nil {
		t.Errorf("expected nil imported constants, got %v", ic)
	}
}
