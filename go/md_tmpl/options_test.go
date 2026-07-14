package md_tmpl

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// Functional options — FromSource
// ---------------------------------------------------------------------------

func TestFromSourceWithAllowUnusedOption(t *testing.T) {
	src := `---
params: [x = str, y = int]
---
{{ x }}`

	// Without the option the unused declaration is rejected.
	if _, err := FromSource(src); err == nil {
		t.Fatal("expected error for unused declaration without WithAllowUnused")
	}

	// With the option it compiles.
	tmpl, err := FromSource(src, WithAllowUnused())
	if err != nil {
		t.Fatalf("FromSource(WithAllowUnused): %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": "ok", "y": 1})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "ok" {
		t.Errorf("got %q, want %q", result, "ok")
	}
}

func TestFromSourceWithBaseDirOption(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "header.tmpl.md"),
		[]byte(`---
name: header
params: [title = str]
---
# {{ title }}`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	tmpl, err := FromSource(`---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body`, WithBaseDir(dir))
	if err != nil {
		t.Fatalf("FromSource(WithBaseDir): %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"title": "Hi"})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Hi") {
		t.Errorf("expected 'Hi' in output, got %q", result)
	}
}

func TestFromSourceWithEnvOption(t *testing.T) {
	tmpl, err := FromSource(`---
env: [MODEL = str]

params: []
---
Model: {{ MODEL }}`, WithEnv(map[string]any{"MODEL": "gpt-4"}))
	if err != nil {
		t.Fatalf("FromSource(WithEnv): %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderEmpty()
	if err != nil {
		t.Fatalf("RenderEmpty: %v", err)
	}
	if result != "Model: gpt-4" {
		t.Errorf("got %q, want %q", result, "Model: gpt-4")
	}
}

func TestFromSourceWithEnvNilUsesDefaults(t *testing.T) {
	// WithEnv(nil) is valid and must still satisfy env declarations with
	// defaults.
	tmpl, err := FromSource(`---
env:
  - MODEL = str := "gpt-3.5"

params: []
---
Model: {{ MODEL }}`, WithEnv(nil))
	if err != nil {
		t.Fatalf("FromSource(WithEnv(nil)): %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderEmpty()
	if err != nil {
		t.Fatalf("RenderEmpty: %v", err)
	}
	if result != "Model: gpt-3.5" {
		t.Errorf("got %q, want %q", result, "Model: gpt-3.5")
	}
}

func TestFromSourceCombinedOptions(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "part.tmpl.md"),
		[]byte(`---
name: part
params: [label = str]
---
{{ label }}`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// Combine all three options at once: a base dir for the include, an env
	// value, and an unused declared param (unused_extra).
	tmpl, err := FromSource(`---
env: [PREFIX = str]

params: [label = str, unused_extra = int]
---
{{ PREFIX }}:> {% include [part](./part.tmpl.md) with label=label %}`,
		WithBaseDir(dir),
		WithEnv(map[string]any{"PREFIX": "P"}),
		WithAllowUnused(),
	)
	if err != nil {
		t.Fatalf("FromSource(combined): %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"label": "hello", "unused_extra": 0})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "P:") || !strings.Contains(result, "hello") {
		t.Errorf("got %q, want it to contain %q and %q", result, "P:", "hello")
	}
}

// TestNamedConstructorsMatchOptions verifies the backward-compatible named
// constructors are exact shorthands for the equivalent functional-options form.
func TestNamedConstructorsMatchOptions(t *testing.T) {
	src := `---
params: [x = str, y = int]
---
{{ x }}`

	viaNamed, err := FromSourceAllowingUnused(src)
	if err != nil {
		t.Fatalf("FromSourceAllowingUnused: %v", err)
	}
	defer viaNamed.Close()

	viaOption, err := FromSource(src, WithAllowUnused())
	if err != nil {
		t.Fatalf("FromSource(WithAllowUnused): %v", err)
	}
	defer viaOption.Close()

	// Both should render identically.
	params := map[string]any{"x": "v", "y": 2}
	a, err := viaNamed.RenderMap(params)
	if err != nil {
		t.Fatalf("named render: %v", err)
	}
	b, err := viaOption.RenderMap(params)
	if err != nil {
		t.Fatalf("option render: %v", err)
	}
	if a != b {
		t.Errorf("named %q != option %q", a, b)
	}
}

// ---------------------------------------------------------------------------
// Functional options — FromFile
// ---------------------------------------------------------------------------

func TestFromFileWithOptions(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "footer.tmpl.md"),
		[]byte(`---
name: footer
params: [note = str]
---
-- {{ note }}`), 0o644); err != nil {
		t.Fatalf("WriteFile footer: %v", err)
	}

	mainPath := filepath.Join(dir, "main.tmpl.md")
	if err := os.WriteFile(mainPath,
		[]byte(`---
params: [note = str, unused = int]
---
> {% include [footer](./footer.tmpl.md) with note=note %}`), 0o644); err != nil {
		t.Fatalf("WriteFile main: %v", err)
	}

	// unused param requires WithAllowUnused; include requires the base dir,
	// which defaults to the file's own directory.
	tmpl, err := FromFile(mainPath, WithAllowUnused())
	if err != nil {
		t.Fatalf("FromFile(WithAllowUnused): %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"note": "done", "unused": 0})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "done") {
		t.Errorf("expected 'done' in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// RenderEmpty
// ---------------------------------------------------------------------------

func TestRenderEmptyNoParams(t *testing.T) {
	tmpl, err := FromSource(`---
params: []
---
static text`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderEmpty()
	if err != nil {
		t.Fatalf("RenderEmpty: %v", err)
	}
	if result != "static text" {
		t.Errorf("got %q, want %q", result, "static text")
	}
}

func TestRenderEmptyUsesDefaults(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str := "World"]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderEmpty()
	if err != nil {
		t.Fatalf("RenderEmpty: %v", err)
	}
	if result != "Hello World!" {
		t.Errorf("got %q, want %q", result, "Hello World!")
	}
}

func TestRenderEmptyMissingRequiredParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderEmpty()
	if err == nil {
		t.Fatal("expected missing-param error, got nil")
	}
	if !errors.Is(err, ErrMissingParams) {
		t.Errorf("expected ErrMissingParams, got %v (kind check)", err)
	}
}

func TestRenderEmptyClosedTemplate(t *testing.T) {
	tmpl, err := FromSource(`---
params: []
---
x`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	if _, err := tmpl.RenderEmpty(); !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// RenderUnchecked
// ---------------------------------------------------------------------------

func TestRenderUncheckedBasic(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "World"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.RenderUnchecked(ctx)
	if err != nil {
		t.Fatalf("RenderUnchecked: %v", err)
	}
	if result != "Hello World!" {
		t.Errorf("got %q, want %q", result, "Hello World!")
	}
}

func TestRenderUncheckedClosedTemplate(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if _, err := tmpl.RenderUnchecked(ctx); !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

func TestRenderUncheckedNilContext(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	if _, err := tmpl.RenderUnchecked(nil); !errors.Is(err, ErrNilContext) {
		t.Fatalf("expected ErrNilContext, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// RenderCached
// ---------------------------------------------------------------------------

func TestRenderCachedResolvesIncludes(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "header.tmpl.md"),
		[]byte(`---
name: header
params: [title = str]
---
# {{ title }}`), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	tmpl, err := FromSource(`---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body`, WithBaseDir(dir))
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("title", "Cached"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	// Render twice through the same cache; both must succeed and match.
	first, err := tmpl.RenderCached(ctx, cache)
	if err != nil {
		t.Fatalf("RenderCached (first): %v", err)
	}
	second, err := tmpl.RenderCached(ctx, cache)
	if err != nil {
		t.Fatalf("RenderCached (second): %v", err)
	}
	if first != second {
		t.Errorf("cached renders differ: %q vs %q", first, second)
	}
	if !strings.Contains(first, "Cached") {
		t.Errorf("expected 'Cached' in output, got %q", first)
	}
}

func TestRenderCachedClosedCache(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("x", "v"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	cache := NewCache()
	cache.Close()

	if _, err := tmpl.RenderCached(ctx, cache); !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed for closed cache, got %v", err)
	}
}

func TestRenderCachedNilContext(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	cache := NewCache()
	defer cache.Close()

	if _, err := tmpl.RenderCached(nil, cache); !errors.Is(err, ErrNilContext) {
		t.Fatalf("expected ErrNilContext, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// Name / Description accessors
// ---------------------------------------------------------------------------

func TestTemplateNameAndDescriptionPresent(t *testing.T) {
	tmpl, err := FromSource(`---
name: greeter
description: Says hello
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	name, ok := tmpl.Name()
	if !ok {
		t.Fatal("expected name to be present")
	}
	if name != "greeter" {
		t.Errorf("Name = %q, want %q", name, "greeter")
	}

	desc, ok := tmpl.Description()
	if !ok {
		t.Fatal("expected description to be present")
	}
	if desc != "Says hello" {
		t.Errorf("Description = %q, want %q", desc, "Says hello")
	}
}

func TestTemplateNameAndDescriptionAbsent(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	if name, ok := tmpl.Name(); ok {
		t.Errorf("expected no name, got %q", name)
	}
	if desc, ok := tmpl.Description(); ok {
		t.Errorf("expected no description, got %q", desc)
	}
}

func TestTemplateNameClosed(t *testing.T) {
	tmpl, err := FromSource(`---
name: x
params: []
---
y`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	if _, ok := tmpl.Name(); ok {
		t.Error("expected (\"\", false) for closed template Name()")
	}
	if _, ok := tmpl.Description(); ok {
		t.Error("expected (\"\", false) for closed template Description()")
	}
}

// ---------------------------------------------------------------------------
// Typed errors — TemplateError / ErrorKind
// ---------------------------------------------------------------------------

func TestParseErrorTyped(t *testing.T) {
	err := parseError("missing_params" + errKindSep + "missing required param: x")
	if err == nil {
		t.Fatal("parseError returned nil for non-empty input")
	}

	var te *TemplateError
	if !errors.As(err, &te) {
		t.Fatalf("expected *TemplateError, got %T", err)
	}
	if te.Kind != KindMissingParams {
		t.Errorf("Kind = %q, want %q", te.Kind, KindMissingParams)
	}
	if te.Message != "missing required param: x" {
		t.Errorf("Message = %q, want %q", te.Message, "missing required param: x")
	}
	if !errors.Is(err, ErrMissingParams) {
		t.Error("errors.Is(err, ErrMissingParams) should be true")
	}
	if errors.Is(err, ErrSyntax) {
		t.Error("errors.Is(err, ErrSyntax) should be false")
	}
}

func TestParseErrorUntyped(t *testing.T) {
	// A message with no kind separator is a plain error, not a *TemplateError.
	err := parseError("null template")
	if err == nil {
		t.Fatal("parseError returned nil for non-empty input")
	}
	var te *TemplateError
	if errors.As(err, &te) {
		t.Errorf("expected plain error, got *TemplateError: %v", te)
	}
	if err.Error() != "null template" {
		t.Errorf("Error() = %q, want %q", err.Error(), "null template")
	}
}

func TestParseErrorEmpty(t *testing.T) {
	if err := parseError(""); err != nil {
		t.Errorf("parseError(\"\") = %v, want nil", err)
	}
}

func TestTemplateErrorIsMatchesByKind(t *testing.T) {
	a := &TemplateError{Kind: KindTypeMismatch, Message: "one"}
	b := &TemplateError{Kind: KindTypeMismatch, Message: "two"}
	if !errors.Is(a, b) {
		t.Error("errors of the same kind should match regardless of message")
	}
	if errors.Is(a, ErrSyntax) {
		t.Error("differing kinds should not match")
	}
}

func TestTypedErrorFromRender(t *testing.T) {
	// A real render surfaces a typed missing_params error through the FFI.
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderEmpty()
	if err == nil {
		t.Fatal("expected error, got nil")
	}

	var te *TemplateError
	if !errors.As(err, &te) {
		t.Fatalf("expected *TemplateError, got %T: %v", err, err)
	}
	if te.Kind != KindMissingParams {
		t.Errorf("Kind = %q, want %q", te.Kind, KindMissingParams)
	}
	if te.Message == "" {
		t.Error("expected a non-empty error message")
	}
}
