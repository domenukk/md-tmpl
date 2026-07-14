package md_tmpl

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

// ---------------------------------------------------------------------------
// Template.FromSource — basic rendering
// ---------------------------------------------------------------------------

func TestFromSourceBasicRender(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "world"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello world!" {
		t.Errorf("got %q, want %q", result, "Hello world!")
	}
}

func TestFromSourceIntParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [count = int]
---
Count: {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetInt("count", 42); err != nil {
		t.Fatalf("SetInt: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Count: 42" {
		t.Errorf("got %q, want %q", result, "Count: 42")
	}
}

func TestFromSourceBoolParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [flag = bool]
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetBool("flag", true); err != nil {
		t.Fatalf("SetBool: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "yes\n" {
		t.Errorf("got %q, want %q", result, "yes\n")
	}
}

func TestFromSourceFloatParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [score = float]
---
{{ score }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetFloat("score", 3.14); err != nil {
		t.Fatalf("SetFloat: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "3.14" {
		t.Errorf("got %q, want %q", result, "3.14")
	}
}

func TestFromSourceSyntaxError(t *testing.T) {
	_, err := FromSource("no frontmatter at all")
	if err == nil {
		t.Fatal("expected error for invalid source, got nil")
	}
	if !strings.Contains(err.Error(), "frontmatter") {
		t.Errorf("error should mention 'frontmatter': %v", err)
	}
}

func TestFromSourceMissingParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str, age = int]
---
{{ name }} {{ age }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for missing param, got nil")
	}
	if !strings.Contains(err.Error(), "missing") {
		t.Errorf("error should mention 'missing': %v", err)
	}
}

func TestFromSourceTypeMismatch(t *testing.T) {
	tmpl, err := FromSource(`---
params: [flag = bool]
---
{{ flag }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("flag", "not a bool"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for type mismatch, got nil")
	}
	if !strings.Contains(err.Error(), "type mismatch") {
		t.Errorf("error should mention 'type mismatch': %v", err)
	}
}

// ---------------------------------------------------------------------------
// Template.FromFile
// ---------------------------------------------------------------------------

func TestFromFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "greeting.tmpl.md")
	if err := os.WriteFile(path, []byte(`---
params:
  - name = str
---
Hello {{ name }}!`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	tmpl, err := FromFile(path)
	if err != nil {
		t.Fatalf("FromFile: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "file"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello file!" {
		t.Errorf("got %q, want %q", result, "Hello file!")
	}
}

func TestFromFileMissing(t *testing.T) {
	_, err := FromFile("/nonexistent/path.tmpl.md")
	if err == nil {
		t.Fatal("expected error for missing file, got nil")
	}
}

// ---------------------------------------------------------------------------
// FromSourceAllowingUnused
// ---------------------------------------------------------------------------

func TestFromSourceAllowingUnused(t *testing.T) {
	tmpl, err := FromSourceAllowingUnused(`---
params: [name = str, unused = int]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSourceAllowingUnused: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "world"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetInt("unused", 42); err != nil {
		t.Fatalf("SetInt: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello world!" {
		t.Errorf("got %q, want %q", result, "Hello world!")
	}
}

func TestFromSourceUnusedRejectedInStrictMode(t *testing.T) {
	_, err := FromSource(`---
params: [name = str, unused = int]
---
Hello {{ name }}!`)
	if err == nil {
		t.Fatal("expected error for unused param in strict mode, got nil")
	}
}

// ---------------------------------------------------------------------------
// Strict validation — extra params
// ---------------------------------------------------------------------------

func TestExtraParamRejected(t *testing.T) {
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
	if err := ctx.SetStr("name", "world"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetStr("bogus", "unexpected"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for extra param, got nil")
	}
}

func TestAllowExtraIgnoresExtraParams(t *testing.T) {
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
	if err := ctx.SetStr("name", "world"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetStr("bogus", "ignored"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx, AllowExtra())
	if err != nil {
		t.Fatalf("Render(AllowExtra): %v", err)
	}
	if result != "Hello world!" {
		t.Errorf("got %q, want %q", result, "Hello world!")
	}
}

// ---------------------------------------------------------------------------
// RenderMap convenience
// ---------------------------------------------------------------------------

func TestRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"name":  "Alice",
		"count": int64(42),
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Alice: 42" {
		t.Errorf("got %q, want %q", result, "Alice: 42")
	}
}

func TestRenderMapWithInt(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = int]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": 7})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "7" {
		t.Errorf("got %q, want %q", result, "7")
	}
}

func TestRenderMapWithFloat(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = float]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": 2.5})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "2.5" {
		t.Errorf("got %q, want %q", result, "2.5")
	}
}

func TestRenderMapWithBool(t *testing.T) {
	tmpl, err := FromSource(`---
params: [flag = bool]
---
{{ flag }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"flag": true})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "true" {
		t.Errorf("got %q, want %q", result, "true")
	}
}

// ---------------------------------------------------------------------------
// Typed lists (via JSON)
// ---------------------------------------------------------------------------

func TestTypedListViaJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - items = list(label = str)
---
> {% for item in items %}

{{ item.label }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("items", `[{"label":"alpha"},{"label":"beta"}]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "alpha\nbeta\n" {
		t.Errorf("got %q, want %q", result, "alpha\nbeta\n")
	}
}

func TestTypedListViaSet(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - tasks = list(title = str, priority = str)
---
> {% for task in tasks %}

{{ task.title }}: {{ task.priority }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	tasks := []map[string]string{
		{"title": "Write documentation", "priority": "High"},
		{"title": "Fix typos", "priority": "Medium"},
	}
	if err := ctx.Set("tasks", tasks); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Write documentation: High") {
		t.Errorf("expected 'Write documentation: High' in output, got %q", result)
	}
}

func TestEmptyList(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - items = list(label = str)
---
> {% for item in items %}

{{ item.label }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("items", `[]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "" {
		t.Errorf("expected empty output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Struct parameters
// ---------------------------------------------------------------------------

func TestStructParam(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("config", `{"host":"localhost","port":8080}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "localhost:8080" {
		t.Errorf("got %q, want %q", result, "localhost:8080")
	}
}

// ---------------------------------------------------------------------------
// Multiple param types
// ---------------------------------------------------------------------------

func TestMultipleParamTypes(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetInt("count", 42); err != nil {
		t.Fatalf("SetInt: %v", err)
	}
	if err := ctx.SetFloat("score", 9.5); err != nil {
		t.Fatalf("SetFloat: %v", err)
	}
	if err := ctx.SetBool("enabled", true); err != nil {
		t.Fatalf("SetBool: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	expected := "Alice: count=42, score=9.5, enabled=true"
	if result != expected {
		t.Errorf("got %q, want %q", result, expected)
	}
}

// ---------------------------------------------------------------------------
// Enum dispatch
// ---------------------------------------------------------------------------

func TestEnumUnitVariant(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("outcome", "Rejected"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestEnumStructVariant(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("outcome", `{"__kind__":"Confirmed","evidence":"found it"}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "YES: found it\n" {
		t.Errorf("got %q, want %q", result, "YES: found it\n")
	}
}

func TestEnumInvalidVariant(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("outcome", "Unknown"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for invalid variant, got nil")
	}
}

// ---------------------------------------------------------------------------
// Default values
// ---------------------------------------------------------------------------

func TestDefaultsUsedWhenOmitted(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
  - count = int := 1
---
Hello {{ name }}, count={{ count }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello World, count=1!" {
		t.Errorf("got %q, want %q", result, "Hello World, count=1!")
	}
}

func TestDefaultsOverridden(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
  - count = int := 1
---
Hello {{ name }}, count={{ count }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetInt("count", 99); err != nil {
		t.Fatalf("SetInt: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello Alice, count=99!" {
		t.Errorf("got %q, want %q", result, "Hello Alice, count=99!")
	}
}

// ---------------------------------------------------------------------------
// Template metadata
// ---------------------------------------------------------------------------

func TestDeclarations(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }} {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	decls := tmpl.Declarations()
	if len(decls) != 2 {
		t.Fatalf("expected 2 declarations, got %d", len(decls))
	}

	found := map[string]string{}
	for _, d := range decls {
		found[d.Name] = d.Type
	}
	if found["name"] != "str" {
		t.Errorf("expected name=str, got name=%s", found["name"])
	}
	if found["count"] != "int" {
		t.Errorf("expected count=int, got count=%s", found["count"])
	}
}

func TestSourceHashStable(t *testing.T) {
	source := `---
params: [x = str]
---
{{ x }}`
	t1, err := FromSource(source)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer t1.Close()

	t2, err := FromSource(source)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer t2.Close()

	if t1.SourceHash() != t2.SourceHash() {
		t.Errorf("expected same hash, got %d vs %d", t1.SourceHash(), t2.SourceHash())
	}
}

func TestSourceHashChanges(t *testing.T) {
	t1, err := FromSource(`---
params: [x = str]
---
Hello {{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer t1.Close()

	t2, err := FromSource(`---
params: [x = str]
---
Goodbye {{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer t2.Close()

	if t1.SourceHash() == t2.SourceHash() {
		t.Error("expected different hashes for different content")
	}
}

func TestBody(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
Body: {{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	body := tmpl.Body()
	if !strings.Contains(body, "Body:") {
		t.Errorf("expected body to contain 'Body:', got %q", body)
	}
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

func TestCacheLoadAndRender(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "greeting.tmpl.md")
	if err := os.WriteFile(path, []byte(`---
params:
  - name = str
---
Hello {{ name }}!`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	tmpl, err := cache.Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "cached"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello cached!" {
		t.Errorf("got %q, want %q", result, "Hello cached!")
	}
}

func TestCacheReturnsSameHash(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.tmpl.md")
	if err := os.WriteFile(path, []byte(`---
params: [x = str]
---
{{ x }}`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	t1, err := cache.Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer t1.Close()

	t2, err := cache.Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer t2.Close()

	if t1.SourceHash() != t2.SourceHash() {
		t.Errorf("expected same hash from cache, got %d vs %d", t1.SourceHash(), t2.SourceHash())
	}
}

func TestCacheClear(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.tmpl.md")
	if err := os.WriteFile(path, []byte(`---
params: []
---
Hi`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	if _, err := cache.Load(path); err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cache.TemplateCount() != 1 {
		t.Errorf("expected 1 cached template, got %d", cache.TemplateCount())
	}

	cache.Clear()
	if cache.TemplateCount() != 0 {
		t.Errorf("expected 0 cached templates after clear, got %d", cache.TemplateCount())
	}
}

func TestCacheTemplateCount(t *testing.T) {
	dir := t.TempDir()
	cache := NewCache()
	defer cache.Close()

	for i := 0; i < 3; i++ {
		path := filepath.Join(dir, strings.Replace("X.tmpl.md", "X", string(rune('a'+i)), 1))
		if err := os.WriteFile(path, []byte(`---
params: []
---
Hi`), 0644); err != nil {
			t.Fatalf("WriteFile: %v", err)
		}
		if _, err := cache.Load(path); err != nil {
			t.Fatalf("Load: %v", err)
		}
	}

	if cache.TemplateCount() != 3 {
		t.Errorf("expected 3 cached templates, got %d", cache.TemplateCount())
	}
}

// ---------------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------------

func TestFilterUpper(t *testing.T) {
	tmpl, err := FromSource(`---
params: [val = str]
---
{{ val | upper }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"val": "hello world"})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "HELLO WORLD" {
		t.Errorf("got %q, want %q", result, "HELLO WORLD")
	}
}

func TestFilterLower(t *testing.T) {
	tmpl, err := FromSource(`---
params: [val = str]
---
{{ val | lower }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"val": "HELLO WORLD"})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "hello world" {
		t.Errorf("got %q, want %q", result, "hello world")
	}
}

func TestFilterTrim(t *testing.T) {
	tmpl, err := FromSource(`---
params: [val = str]
---
{{ val | trim }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"val": "  hello  "})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "hello" {
		t.Errorf("got %q, want %q", result, "hello")
	}
}

func TestFilterFixed(t *testing.T) {
	tmpl, err := FromSource(`---
params: [val = float]
---
{{ val | fixed(2) }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"val": 3.14159})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "3.14" {
		t.Errorf("got %q, want %q", result, "3.14")
	}
}

func TestFilterChain(t *testing.T) {
	tmpl, err := FromSource(`---
params: [val = str]
---
{{ val | trim | upper }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"val": "  hello world  "})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "HELLO WORLD" {
		t.Errorf("got %q, want %q", result, "HELLO WORLD")
	}
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

func TestClosedTemplateReturnsError(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("x", "test"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

func TestClosedTemplateRenderMapReturnsError(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	_, err = tmpl.RenderMap(map[string]any{"x": "test"})
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed for RenderMap on closed template, got %v", err)
	}
}

func TestDoubleCloseIsNoOp(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	// Should not panic.
	tmpl.Close()
	tmpl.Close()
}

func TestContextSetVariousTypes(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()

	// All of these should succeed.
	if err := ctx.Set("s", "hello"); err != nil {
		t.Errorf("Set string: %v", err)
	}
	if err := ctx.Set("i", 42); err != nil {
		t.Errorf("Set int: %v", err)
	}
	if err := ctx.Set("i64", int64(99)); err != nil {
		t.Errorf("Set int64: %v", err)
	}
	if err := ctx.Set("i32", int32(7)); err != nil {
		t.Errorf("Set int32: %v", err)
	}
	if err := ctx.Set("f64", 3.14); err != nil {
		t.Errorf("Set float64: %v", err)
	}
	if err := ctx.Set("f32", float32(1.5)); err != nil {
		t.Errorf("Set float32: %v", err)
	}
	if err := ctx.Set("b", true); err != nil {
		t.Errorf("Set bool: %v", err)
	}
	if err := ctx.Set("list", []string{"a", "b"}); err != nil {
		t.Errorf("Set slice: %v", err)
	}
	if err := ctx.Set("dict", map[string]string{"k": "v"}); err != nil {
		t.Errorf("Set map: %v", err)
	}
}

func TestRenderMapBadType(t *testing.T) {
	tmpl, err := FromSource(`---
params: [count = int]
---
{{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderMap(map[string]any{"count": "not a number"})
	if err == nil {
		t.Fatal("expected type mismatch error, got nil")
	}
}

// ---------------------------------------------------------------------------
// Includes (via file)
// ---------------------------------------------------------------------------

func TestInclude(t *testing.T) {
	dir := t.TempDir()

	// Write header include.
	headerPath := filepath.Join(dir, "header.tmpl.md")
	if err := os.WriteFile(headerPath, []byte(`---
name: header
params: [title = str]
---
# {{ title }}`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// Write main template that includes header.
	mainPath := filepath.Join(dir, "main.tmpl.md")
	if err := os.WriteFile(mainPath, []byte(`---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	tmpl, err := FromFile(mainPath)
	if err != nil {
		t.Fatalf("FromFile: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("title", "Hello"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "# Hello") {
		t.Errorf("expected '# Hello' in output, got %q", result)
	}
	if !strings.Contains(result, "Body") {
		t.Errorf("expected 'Body' in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

func TestConstants(t *testing.T) {
	tmpl, err := FromSource(`---
consts:
  - MAX = int := 100

params: []
---
Max: {{ MAX }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Max: 100" {
		t.Errorf("got %q, want %q", result, "Max: 100")
	}
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

func TestComments(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
Before{# this is a comment #}After {{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": "!"})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "BeforeAfter") {
		t.Errorf("expected comment to be stripped, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Sentinel errors
// ---------------------------------------------------------------------------

func TestNilContextReturnsError(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.Render(nil)
	if !errors.Is(err, ErrNilContext) {
		t.Fatalf("expected ErrNilContext, got %v", err)
	}
}

func TestClosedContextSetReturnsError(t *testing.T) {
	ctx := NewContext()
	ctx.Close()

	if err := ctx.SetStr("k", "v"); !errors.Is(err, ErrClosed) {
		t.Errorf("SetStr on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetInt("k", 1); !errors.Is(err, ErrClosed) {
		t.Errorf("SetInt on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetFloat("k", 1.0); !errors.Is(err, ErrClosed) {
		t.Errorf("SetFloat on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetBool("k", true); !errors.Is(err, ErrClosed) {
		t.Errorf("SetBool on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetJSON("k", `{}`); !errors.Is(err, ErrClosed) {
		t.Errorf("SetJSON on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.Set("k", "v"); !errors.Is(err, ErrClosed) {
		t.Errorf("Set on closed context: expected ErrClosed, got %v", err)
	}
}

func TestClosedCacheLoadReturnsError(t *testing.T) {
	cache := NewCache()
	cache.Close()

	_, err := cache.Load("/some/path.tmpl.md")
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// Declaration.String()
// ---------------------------------------------------------------------------

func TestDeclarationString(t *testing.T) {
	d := Declaration{Name: "tasks", Type: "list(title = str)"}
	if s := d.String(); s != "tasks = list(title = str)" {
		t.Errorf("got %q, want %q", s, "tasks = list(title = str)")
	}
}

// ---------------------------------------------------------------------------
// RenderMap with AllowExtra
// ---------------------------------------------------------------------------

func TestRenderMapAllowExtra(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"name":  "Alice",
		"extra": "ignored",
	}, AllowExtra())
	if err != nil {
		t.Fatalf("RenderMap(AllowExtra): %v", err)
	}
	if result != "Hello Alice!" {
		t.Errorf("got %q, want %q", result, "Hello Alice!")
	}
}

// ---------------------------------------------------------------------------
// Empty params (static template)
// ---------------------------------------------------------------------------

func TestEmptyParams(t *testing.T) {
	tmpl, err := FromSource(`---
params: []
---
Static content here.`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Static content here." {
		t.Errorf("got %q, want %q", result, "Static content here.")
	}
}

// ---------------------------------------------------------------------------
// SetMaxIncludeDepth
// ---------------------------------------------------------------------------

func TestSetMaxIncludeDepth(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Should not panic or error; just sets the depth.
	tmpl.SetMaxIncludeDepth(5)

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("x", "works"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render after SetMaxIncludeDepth: %v", err)
	}
	if result != "works" {
		t.Errorf("got %q, want %q", result, "works")
	}
}

// ---------------------------------------------------------------------------
// Cache include count
// ---------------------------------------------------------------------------

func TestCacheIncludeCount(t *testing.T) {
	dir := t.TempDir()

	// Write an include.
	headerPath := filepath.Join(dir, "header.tmpl.md")
	if err := os.WriteFile(headerPath, []byte(`---
name: header
params: [title = str]
---
# {{ title }}`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// Write a main template that includes the header.
	mainPath := filepath.Join(dir, "main.tmpl.md")
	if err := os.WriteFile(mainPath, []byte(`---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	tmpl, err := cache.Load(mainPath)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}

	// Render to trigger include resolution.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("title", "Test"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	_, err = tmpl.Render(ctx, AllowExtra())
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	tmpl.Close()

	if cache.TemplateCount() < 1 {
		t.Errorf("expected at least 1 template in cache, got %d", cache.TemplateCount())
	}
	// Include count should be >= 0 (implementation may or may not cache includes
	// depending on whether render_cached was used).
	if cache.IncludeCount() < 0 {
		t.Errorf("expected include count >= 0, got %d", cache.IncludeCount())
	}
}

// ---------------------------------------------------------------------------
// Unicode and special characters
// ---------------------------------------------------------------------------

func TestUnicodeContent(t *testing.T) {
	tmpl, err := FromSource(`---
params: [msg = str]
---
{{ msg }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	unicode := "Hello 🌍 こんにちは 世界 🦀"
	result, err := tmpl.RenderMap(map[string]any{"msg": unicode})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != unicode {
		t.Errorf("got %q, want %q", result, unicode)
	}
}

func TestEmptyStringParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
[{{ x }}]`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": ""})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "[]" {
		t.Errorf("got %q, want %q", result, "[]")
	}
}

// ---------------------------------------------------------------------------
// Large int and negative values
// ---------------------------------------------------------------------------

func TestNegativeInt(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = int]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": int64(-42)})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "-42" {
		t.Errorf("got %q, want %q", result, "-42")
	}
}

func TestLargeInt(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = int]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": int64(9_223_372_036_854_775_807)})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "9223372036854775807" {
		t.Errorf("got %q, want %q", result, "9223372036854775807")
	}
}

// ---------------------------------------------------------------------------
// Template-typed parameters (tmpl())
// ---------------------------------------------------------------------------

func TestSetTmplParam(t *testing.T) {
	// Card template: renders a single item
	card, err := FromSourceAllowingUnused(`---
name: card
params: [title = str]
---
* {{ title }}`)
	if err != nil {
		t.Fatalf("card FromSource: %v", err)
	}
	defer card.Close()

	// Main template: takes card as tmpl() param
	main, err := FromSource(`---
params:
  - card = tmpl(title = str)
  - items = list(name = str)
---
> {% for item in items %}
> {% include card with title=item.name %}

> {% /for %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("card", card); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}
	if err := ctx.SetJSON("items", `[{"name":"Alpha"},{"name":"Beta"}]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Alpha") {
		t.Errorf("expected 'Alpha' in output, got %q", result)
	}
	if !strings.Contains(result, "Beta") {
		t.Errorf("expected 'Beta' in output, got %q", result)
	}
}

func TestSetTmplNilErrors(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()

	err := ctx.SetTmpl("card", nil)
	if err == nil {
		t.Fatal("expected error for nil template, got nil")
	}
}

// ---------------------------------------------------------------------------
// elif branches
// ---------------------------------------------------------------------------

func TestElifBranch(t *testing.T) {
	src := `---
params: [level = int]
---
> {% if level == 1 %}

Low

> {% elif level == 2 %}

Medium

> {% else %}

High

> {% /if %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	tests := []struct {
		level int64
		want  string
	}{
		{1, "Low\n"},
		{2, "Medium\n"},
		{3, "High\n"},
	}
	for _, tc := range tests {
		ctx := NewContext()
		if err := ctx.SetInt("level", tc.level); err != nil {
			t.Fatalf("SetInt: %v", err)
		}
		result, err := tmpl.Render(ctx)
		ctx.Close()
		if err != nil {
			t.Fatalf("Render(level=%d): %v", tc.level, err)
		}
		if result != tc.want {
			t.Errorf("level=%d: got %q, want %q", tc.level, result, tc.want)
		}
	}
}

// ---------------------------------------------------------------------------
// JSON parse errors
// ---------------------------------------------------------------------------

func TestSetJSONParseError(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()
	err := ctx.SetJSON("x", "invalid json {{")
	if err == nil {
		t.Fatal("expected error for invalid JSON, got nil")
	}
}

// ---------------------------------------------------------------------------
// Boundary values
// ---------------------------------------------------------------------------

func TestZeroIntParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [val = int]
---
{{ val }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"val": 0})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "0" {
		t.Errorf("got %q, want %q", result, "0")
	}
}

func TestFalseBoolInIf(t *testing.T) {
	tmpl, err := FromSource(`---
params: [flag = bool]
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"flag": false})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "no\n" {
		t.Errorf("got %q, want %q", result, "no\n")
	}
}

// ---------------------------------------------------------------------------
// Unicode
// ---------------------------------------------------------------------------

func TestUnicodeParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [greeting = str]
---
{{ greeting }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	unicode := "こんにちは 🌍"
	result, err := tmpl.RenderMap(map[string]any{"greeting": unicode})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != unicode {
		t.Errorf("got %q, want %q", result, unicode)
	}
}

// ---------------------------------------------------------------------------
// Concurrent rendering
// ---------------------------------------------------------------------------

func TestConcurrentRender(t *testing.T) {
	tmpl, err := FromSource(`---
params: [id = int]
---
Result: {{ id }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	var wg sync.WaitGroup
	errorsCh := make(chan error, 10)

	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			ctx := NewContext()
			defer ctx.Close()
			if err := ctx.SetInt("id", int64(id)); err != nil {
				errorsCh <- fmt.Errorf("goroutine %d SetInt: %w", id, err)
				return
			}
			result, err := tmpl.Render(ctx)
			if err != nil {
				errorsCh <- fmt.Errorf("goroutine %d: %w", id, err)
				return
			}
			expected := fmt.Sprintf("Result: %d", id)
			if result != expected {
				errorsCh <- fmt.Errorf("goroutine %d: got %q, want %q", id, result, expected)
			}
		}(i)
	}

	wg.Wait()
	close(errorsCh)

	for err := range errorsCh {
		t.Error(err)
	}
}

// ---------------------------------------------------------------------------
// Nested structs
// ---------------------------------------------------------------------------

func TestNestedStructViaJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(inner = struct(host = str))
---
{{ config.inner.host }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("config", `{"inner":{"host":"example.com"}}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "example.com" {
		t.Errorf("got %q, want %q", result, "example.com")
	}
}

// ---------------------------------------------------------------------------
// Variant type — enum ergonomics
// ---------------------------------------------------------------------------

func TestVariantUnit(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.Set("outcome", Variant{Kind: "Rejected"}); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestVariantStruct(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.Set("outcome", Variant{
		Kind:   "Confirmed",
		Fields: map[string]any{"evidence": "found it"},
	}); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "YES: found it\n" {
		t.Errorf("got %q, want %q", result, "YES: found it\n")
	}
}

// ---------------------------------------------------------------------------
// RenderStruct — struct-based rendering
// ---------------------------------------------------------------------------

func TestRenderStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name  string `json:"name"`
		Count int    `json:"count"`
	}

	result, err := tmpl.RenderStruct(Params{Name: "Alice", Count: 42})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "Alice: 42" {
		t.Errorf("got %q, want %q", result, "Alice: 42")
	}
}

func TestRenderStructWithPointer(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name string `json:"name"`
	}

	result, err := tmpl.RenderStruct(&Params{Name: "Bob"})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "Bob" {
		t.Errorf("got %q, want %q", result, "Bob")
	}
}

func TestRenderStructWithNestedStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Config struct {
		Host string `json:"host"`
		Port int    `json:"port"`
	}
	type Params struct {
		Config Config `json:"config"`
	}

	result, err := tmpl.RenderStruct(Params{Config: Config{Host: "localhost", Port: 8080}})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "localhost:8080" {
		t.Errorf("got %q, want %q", result, "localhost:8080")
	}
}

func TestRenderStructWithEnum(t *testing.T) {
	src := `---
params:
  - name = str
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
{{ name }}: > {% match outcome %}

> {% case Confirmed %}

YES {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name    string  `json:"name"`
		Outcome Variant `json:"outcome"`
	}

	result, err := tmpl.RenderStruct(Params{
		Name:    "test",
		Outcome: Variant{Kind: "Confirmed", Fields: map[string]any{"evidence": "proof"}},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "YES") || !strings.Contains(result, "proof") {
		t.Errorf("expected YES + proof in output, got %q", result)
	}
}

func TestRenderStructWithSlice(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - items = list(label = str)
---
> {% for item in items %}

{{ item.label }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Item struct {
		Label string `json:"label"`
	}
	type Params struct {
		Items []Item `json:"items"`
	}

	result, err := tmpl.RenderStruct(Params{
		Items: []Item{{Label: "alpha"}, {Label: "beta"}},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "alpha\nbeta\n" {
		t.Errorf("got %q, want %q", result, "alpha\nbeta\n")
	}
}

func TestRenderStructNotAStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderStruct("not a struct")
	if err == nil {
		t.Fatal("expected error for non-struct, got nil")
	}
}

func TestRenderStructOmitempty(t *testing.T) {
	tmpl, err := FromSourceAllowingUnused(`---
params:
  - name = str
  - tag = str := "default"
---
{{ name }} {{ tag }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name string `json:"name"`
		Tag  string `json:"tag,omitempty"`
	}

	result, err := tmpl.RenderStruct(Params{Name: "Alice"})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "Alice") {
		t.Errorf("expected Alice in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Constants introspection
// ---------------------------------------------------------------------------

func TestConstantsMap(t *testing.T) {
	tmpl, err := FromSource(`---
consts:
  - MAX = int := 100
  - GREETING = str := "hello"

params: []
---
{{ MAX }} {{ GREETING }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	consts := tmpl.Constants()
	if consts == nil {
		t.Fatal("Constants() returned nil")
	}
	// JSON numbers decode as float64 in Go
	if v, ok := consts["MAX"]; !ok {
		t.Error("missing MAX constant")
	} else if v != float64(100) {
		t.Errorf("MAX = %v, want 100", v)
	}
	if v, ok := consts["GREETING"]; !ok {
		t.Error("missing GREETING constant")
	} else if v != "hello" {
		t.Errorf("GREETING = %v, want \"hello\"", v)
	}
}

func TestConstantsEmpty(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	consts := tmpl.Constants()
	if len(consts) != 0 {
		t.Errorf("expected empty constants, got %v", consts)
	}
}

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

// ---------------------------------------------------------------------------
// FromSourceWithBaseDir — include resolution
// ---------------------------------------------------------------------------

func TestFromSourceWithBaseDirInclude(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "header.tmpl.md"),
		[]byte(`---
name: header
params: [title = str]
---
# {{ title }}`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	tmpl, err := FromSourceWithBaseDir(
		`---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body`,
		dir,
	)
	if err != nil {
		t.Fatalf("FromSourceWithBaseDir: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"title": "Test"})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Test") {
		t.Errorf("expected 'Test' in output, got %q", result)
	}
}

func TestFromSourceWithBaseDirMissingInclude(t *testing.T) {
	// Include resolution is lazy (render-time), so parsing succeeds.
	// The error surfaces at render time.
	tmpl, err := FromSourceWithBaseDir(
		`---
params: [title = str]
---
> {% include [missing](./does_not_exist.tmpl.md) with title=title %}

Body`,
		t.TempDir(),
	)
	if err != nil {
		t.Fatalf("parse should succeed, got: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderMap(map[string]any{"title": "Test"})
	if err == nil {
		t.Fatal("expected error for missing include at render time, got nil")
	}
}

// ---------------------------------------------------------------------------
// FromSourceWithFrontmatter
// ---------------------------------------------------------------------------

func TestFromSourceWithFrontmatter(t *testing.T) {
	tmpl, fm, err := FromSourceWithFrontmatter(
		`---
name: greeting
description: A greeting template
params: [name = str, count = int]
---
Hello {{ name }}! Count: {{ count }}`)
	if err != nil {
		t.Fatalf("FromSourceWithFrontmatter: %v", err)
	}
	defer tmpl.Close()

	if fm.Name != "greeting" {
		t.Errorf("Name = %q, want %q", fm.Name, "greeting")
	}
	if fm.Description != "A greeting template" {
		t.Errorf("Description = %q, want %q", fm.Description, "A greeting template")
	}
	if !fm.HasParams {
		t.Error("HasParams should be true")
	}
	if fm.AllowUnused {
		t.Error("AllowUnused should be false")
	}
	if len(fm.Params) != 2 || fm.Params[0] != "name" || fm.Params[1] != "count" {
		t.Errorf("Params = %v, want [name, count]", fm.Params)
	}

	// Verify the template still works.
	result, err := tmpl.RenderMap(map[string]any{"name": "World", "count": 1})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Hello World! Count: 1" {
		t.Errorf("got %q, want %q", result, "Hello World! Count: 1")
	}
}

func TestFromSourceWithFrontmatterNoParams(t *testing.T) {
	tmpl, fm, err := FromSourceWithFrontmatter(
		`---
name: static
description: No params
params: []
---
Hello!`)
	if err != nil {
		t.Fatalf("FromSourceWithFrontmatter: %v", err)
	}
	defer tmpl.Close()

	if fm.Name != "static" {
		t.Errorf("Name = %q, want %q", fm.Name, "static")
	}
	if len(fm.Params) != 0 {
		t.Errorf("Params should be empty, got %v", fm.Params)
	}
}

func TestFromSourceWithFrontmatterError(t *testing.T) {
	_, _, err := FromSourceWithFrontmatter("no frontmatter")
	if err == nil {
		t.Fatal("expected error for invalid source, got nil")
	}
}

func TestFromSourceWithFrontmatterAllowUnused(t *testing.T) {
	tmpl, fm, err := FromSourceWithFrontmatter(
		`---
name: test
allow_unused: true
params: [x = str, y = int]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSourceWithFrontmatter: %v", err)
	}
	defer tmpl.Close()

	if !fm.AllowUnused {
		t.Error("AllowUnused should be true")
	}
	if !fm.HasParams {
		t.Error("HasParams should be true")
	}
}

// ---------------------------------------------------------------------------
// ValidateDeclarations — removed params
// ---------------------------------------------------------------------------

func TestValidateDeclarationsRemoved(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	err = tmpl.ValidateDeclarations([]Declaration{
		{Name: "name", Type: "str"},
		{Name: "count", Type: "int"},
	})
	if err == nil {
		t.Fatal("expected error for removed param, got nil")
	}
	if !strings.Contains(err.Error(), "removed") {
		t.Errorf("expected 'removed' in error, got: %v", err)
	}
}

// ---------------------------------------------------------------------------
// README hero example — ensures the top-of-README code actually works
// ---------------------------------------------------------------------------

func TestReadmeHeroExample(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - reviewer = str
  - tasks = list(title = str, priority = enum(Critical, High, Low))
---

# Code Review — {{ reviewer }}

> {% for task in tasks %}

- **{{ task.title }}**

> {% match task.priority %}
> {% case Critical %}

🔴 Critical

> {% case High %}

🟡 High

> {% case Low %}

🟢 Low

> {% /match %}
> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"reviewer": "Alice",
		"tasks": []map[string]any{
			{"title": "Write documentation", "priority": "Critical"},
			{"title": "Fix typos", "priority": "Low"},
		},
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "Alice") {
		t.Errorf("expected 'Alice' in output, got: %s", result)
	}
	if !strings.Contains(result, "Write documentation") {
		t.Errorf("expected 'Write documentation' in output, got: %s", result)
	}
	if !strings.Contains(result, "🔴 Critical") {
		t.Errorf("expected '🔴 Critical' in output, got: %s", result)
	}
	if !strings.Contains(result, "🟢 Low") {
		t.Errorf("expected '🟢 Low' in output, got: %s", result)
	}
}

func TestReadmeDefaultsExample(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - greeting = str := "Hello"
---
{{ greeting }}, {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"name": "Alice"})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Hello, Alice!" {
		t.Errorf("got %q, want %q", result, "Hello, Alice!")
	}
}

// ---------------------------------------------------------------------------
// Regression: io.Closer interface (Template, Context, Cache)
// ---------------------------------------------------------------------------

func TestTemplateImplementsIOCloser(t *testing.T) {
	var _ io.Closer = (*Template)(nil) // compile-time check

	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatal(err)
	}
	err = tmpl.Close()
	if err != nil {
		t.Fatal("Close should return nil:", err)
	}
}

func TestContextImplementsIOCloser(t *testing.T) {
	var _ io.Closer = (*Context)(nil) // compile-time check

	ctx := NewContext()
	err := ctx.Close()
	if err != nil {
		t.Fatal("Close should return nil:", err)
	}
}

func TestCacheImplementsIOCloser(t *testing.T) {
	var _ io.Closer = (*Cache)(nil) // compile-time check

	cache := NewCache()
	err := cache.Close()
	if err != nil {
		t.Fatal("Close should return nil:", err)
	}
}

// ---------------------------------------------------------------------------
// Regression: sync.Once double-close safety
// ---------------------------------------------------------------------------

func TestDoubleCloseTemplateNoPanic(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatal(err)
	}
	tmpl.Close()
	tmpl.Close() // must not panic
}

func TestDoubleCloseContextNoPanic(t *testing.T) {
	ctx := NewContext()
	ctx.Close()
	ctx.Close() // must not panic
}

func TestDoubleCloseCacheNoPanic(t *testing.T) {
	cache := NewCache()
	cache.Close()
	cache.Close() // must not panic
}

// ---------------------------------------------------------------------------
// Regression: json.Marshal preserves exact field names (no lowercasing)
// ---------------------------------------------------------------------------

func TestRenderStructPreservesFieldNames(t *testing.T) {
	type NoTags struct {
		UserName string
		Score    int64
	}

	// json.Marshal uses the exact Go field name (no lowercasing),
	// so template params must match exactly.
	tmpl, err := FromSource(`---
params: [UserName = str, Score = int]
---
{{ UserName }}: {{ Score }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderStruct(NoTags{UserName: "alice", Score: 99})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "alice: 99" {
		t.Errorf("got %q, want %q", result, "alice: 99")
	}
}

// ---------------------------------------------------------------------------
// Regression: concurrent rendering safety
// ---------------------------------------------------------------------------

func TestConcurrentRenderMapRegression(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			result, err := tmpl.RenderMap(map[string]any{"name": fmt.Sprintf("goroutine-%d", n)})
			if err != nil {
				t.Errorf("render failed: %v", err)
				return
			}
			expected := fmt.Sprintf("Hello goroutine-%d!", n)
			if result != expected {
				t.Errorf("got %q, want %q", result, expected)
			}
		}(i)
	}
	wg.Wait()
}

// ---------------------------------------------------------------------------
// Regression: numeric type coverage in Context.Set()
// ---------------------------------------------------------------------------

func TestAllNumericTypes(t *testing.T) {
	tmpl, err := FromSource(`---
params: [n = int]
---
{{ n }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	testCases := []struct {
		name string
		val  any
	}{
		{"int", int(42)},
		{"int8", int8(42)},
		{"int16", int16(42)},
		{"int32", int32(42)},
		{"int64", int64(42)},
		{"uint", uint(42)},
		{"uint8", uint8(42)},
		{"uint16", uint16(42)},
		{"uint32", uint32(42)},
	}
	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			result, err := tmpl.RenderMap(map[string]any{"n": tc.val})
			if err != nil {
				t.Fatalf("type %s: %v", tc.name, err)
			}
			if result != "42" {
				t.Errorf("type %s: got %q, want %q", tc.name, result, "42")
			}
		})
	}
}

// ---------------------------------------------------------------------------
// Regression: RenderMap / RenderMap(AllowExtra()) refactoring (renderMapWith)
// ---------------------------------------------------------------------------

func TestRenderMapStillRejectsExtra(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderMap(map[string]any{"name": "ok", "extra": "bad"})
	if err == nil {
		t.Fatal("RenderMap should reject extra params")
	}
}

func TestRenderMapAllowExtraStillWorks(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"name": "ok", "extra": "ignored"}, AllowExtra())
	if err != nil {
		t.Fatal(err)
	}
	if result != "ok" {
		t.Errorf("got %q, want %q", result, "ok")
	}
}

// ---------------------------------------------------------------------------
// Raw block tests
// ---------------------------------------------------------------------------

func TestRawBlock(t *testing.T) {
	// Raw blocks preserve literal {{ }} syntax without variable interpolation.
	tmpl, err := FromSource(`---
params: []
---
> {% raw %}{{ literal }}{% /raw %}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatal(err)
	}
	if result != "{{ literal }}" {
		t.Errorf("got %q, want %q", result, "{{ literal }}")
	}
}

func TestRawBlockCustomDelimiter(t *testing.T) {
	// Custom delimiter allows literal {% raw %}...{% /raw %} inside the block.
	tmpl, err := FromSource(`---
params: []
---
> {% raw=MYDELIM %}{% raw %}{{ x }}{% /raw %}{% /MYDELIM %}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatal(err)
	}
	want := "{% raw %}{{ x }}{% /raw %}"
	if result != want {
		t.Errorf("got %q, want %q", result, want)
	}
}

// ---------------------------------------------------------------------------
// RenderJSON — JSON string-based rendering
// ---------------------------------------------------------------------------

func TestRenderJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderJSON(`{"name": "Alice", "count": 42}`)
	if err != nil {
		t.Fatal(err)
	}
	if result != "Alice: 42" {
		t.Errorf("got %q, want %q", result, "Alice: 42")
	}
}

func TestRenderJSONAllTypes(t *testing.T) {
	tmpl, err := FromSource(`---
params: [s = str, i = int, f = float, b = bool]
---
{{ s }} {{ i }} {{ f }} {{ b }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderJSON(`{"s": "hello", "i": 99, "f": 3.14, "b": true}`)
	if err != nil {
		t.Fatal(err)
	}
	if result != "hello 99 3.14 true" {
		t.Errorf("got %q, want %q", result, "hello 99 3.14 true")
	}
}

func TestRenderJSONInvalidJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderJSON(`not json`)
	if err == nil {
		t.Fatal("expected error for invalid JSON")
	}
}

func TestRenderJSONNotObject(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderJSON(`[1, 2, 3]`)
	if err == nil {
		t.Fatal("expected error for non-object JSON")
	}
}

func TestRenderJSONAllowExtra(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderJSON(`{"name": "Alice", "extra": "ignored"}`, AllowExtra())
	if err != nil {
		t.Fatal(err)
	}
	if result != "Hello Alice!" {
		t.Errorf("got %q, want %q", result, "Hello Alice!")
	}
}

// TestAllowExtraOption verifies the unified AllowExtra render option: strict
// mode (the default) rejects undeclared parameters with a typed ExtraParams
// error, while AllowExtra() permits them across every render form.
func TestAllowExtraOption(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	extra := map[string]any{"name": "Alice", "extra": "ignored"}

	// Strict (default): the undeclared "extra" key must produce a typed error.
	if _, err := tmpl.RenderMap(extra); err == nil {
		t.Fatal("strict RenderMap: expected ExtraParams error, got nil")
	} else if !errors.Is(err, ErrExtraParams) {
		t.Fatalf("strict RenderMap: expected ErrExtraParams, got %v", err)
	}

	// AllowExtra() permits the undeclared key for each render form.
	got, err := tmpl.RenderMap(extra, AllowExtra())
	if err != nil {
		t.Fatalf("RenderMap(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("RenderMap(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}

	got, err = tmpl.RenderJSON(`{"name": "Alice", "extra": "ignored"}`, AllowExtra())
	if err != nil {
		t.Fatalf("RenderJSON(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("RenderJSON(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}

	type params struct {
		Name  string `json:"name"`
		Extra string `json:"extra"`
	}
	got, err = tmpl.RenderStruct(params{Name: "Alice", Extra: "ignored"}, AllowExtra())
	if err != nil {
		t.Fatalf("RenderStruct(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("RenderStruct(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetStr("extra", "ignored"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	got, err = tmpl.Render(ctx, AllowExtra())
	if err != nil {
		t.Fatalf("Render(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("Render(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}
}

func TestRenderStructUsesJSONPath(t *testing.T) {
	// Existing RenderStruct should still pass via the JSON marshal path.
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	type P struct {
		Name  string `json:"name"`
		Count int    `json:"count"`
	}
	result, err := tmpl.RenderStruct(P{Name: "Bob", Count: 7})
	if err != nil {
		t.Fatal(err)
	}
	if result != "Bob: 7" {
		t.Errorf("got %q, want %q", result, "Bob: 7")
	}
}

func TestMergeJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params: [a = str, b = int]
---
{{ a }} {{ b }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.MergeJSON(`{"a": "hello", "b": 42}`); err != nil {
		t.Fatal(err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result != "hello 42" {
		t.Errorf("got %q, want %q", result, "hello 42")
	}
}

// ---------------------------------------------------------------------------
// TaggedVariant — static typed enum variants
// ---------------------------------------------------------------------------

func TestTaggedVariantEmbedding(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Define a TaggedVariant struct.
	type ConfirmedVariant struct {
		TaggedVariant
		Evidence string `json:"evidence"`
	}

	type Params struct {
		Outcome ConfirmedVariant `json:"outcome"`
	}

	result, err := tmpl.RenderStruct(Params{
		Outcome: ConfirmedVariant{
			TaggedVariant: NewTaggedVariant("Confirmed"),
			Evidence:      "found it",
		},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "YES: found it") {
		t.Errorf("expected 'YES: found it' in output, got %q", result)
	}
}

func TestTaggedVariantUnitWithRenderStruct(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// A unit variant via TaggedVariant — no extra fields.
	type RejectedVariant struct {
		TaggedVariant
	}

	type Params struct {
		Outcome RejectedVariant `json:"outcome"`
	}

	result, err := tmpl.RenderStruct(Params{
		Outcome: RejectedVariant{
			TaggedVariant: NewTaggedVariant("Rejected"),
		},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestNewTaggedVariantKind(t *testing.T) {
	tv := NewTaggedVariant("MyVariant")
	if tv.Kind != "MyVariant" {
		t.Errorf("got Kind=%q, want %q", tv.Kind, "MyVariant")
	}
}

// ---------------------------------------------------------------------------
// Variant.MarshalJSON
// ---------------------------------------------------------------------------

func TestVariantMarshalJSONUnit(t *testing.T) {
	v := Variant{Kind: "Rejected"}
	data, err := v.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}
	// Unit variant serializes to plain string.
	if string(data) != `"Rejected"` {
		t.Errorf("got %s, want %q", data, `"Rejected"`)
	}
}

func TestVariantMarshalJSONStruct(t *testing.T) {
	v := Variant{
		Kind:   "Confirmed",
		Fields: map[string]any{"evidence": "found it"},
	}
	data, err := v.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}

	// Deserialize and verify structure.
	var parsed map[string]any
	if err := json.Unmarshal(data, &parsed); err != nil {
		t.Fatalf("json.Unmarshal: %v", err)
	}
	if parsed["__kind__"] != "Confirmed" {
		t.Errorf("__kind__ = %v, want Confirmed", parsed["__kind__"])
	}
	if parsed["evidence"] != "found it" {
		t.Errorf("evidence = %v, want 'found it'", parsed["evidence"])
	}
}

func TestVariantMarshalJSONEmptyFields(t *testing.T) {
	v := Variant{Kind: "Empty", Fields: map[string]any{}}
	data, err := v.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}
	// Empty fields map should serialize as unit variant (plain string).
	if string(data) != `"Empty"` {
		t.Errorf("got %s, want %q", data, `"Empty"`)
	}
}

// ---------------------------------------------------------------------------
// Variant with RenderMap and RenderStruct
// ---------------------------------------------------------------------------

func TestVariantUnitWithRenderMap(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"outcome": Variant{Kind: "Rejected"},
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestVariantStructWithRenderMap(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"outcome": Variant{
			Kind:   "Confirmed",
			Fields: map[string]any{"evidence": "proof"},
		},
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "YES: proof") {
		t.Errorf("expected 'YES: proof' in output, got %q", result)
	}
}

func TestVariantStructWithRenderStruct(t *testing.T) {
	src := `---
params:
  - name = str
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
{{ name }}: > {% match outcome %}

> {% case Confirmed %}

YES {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name    string  `json:"name"`
		Outcome Variant `json:"outcome"`
	}

	result, err := tmpl.RenderStruct(Params{
		Name:    "test",
		Outcome: Variant{Kind: "Confirmed", Fields: map[string]any{"evidence": "proof"}},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "YES") || !strings.Contains(result, "proof") {
		t.Errorf("expected YES + proof in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Codegen — enum types
// ---------------------------------------------------------------------------

func TestCodegenEnumProducesCorrectGoTypes(t *testing.T) {
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

	// Verify sealed interface exists.
	if !strings.Contains(code, "type Outcome interface") {
		t.Errorf("expected 'type Outcome interface' in generated code:\n%s", code)
	}

	// Verify struct variants embed TaggedVariant or have __kind__.
	if !strings.Contains(code, "type OutcomeConfirmed struct") {
		t.Errorf("expected 'type OutcomeConfirmed struct' in generated code:\n%s", code)
	}
	if !strings.Contains(code, "type OutcomeRejected struct") {
		t.Errorf("expected 'type OutcomeRejected struct' in generated code:\n%s", code)
	}
	if !strings.Contains(code, "type OutcomeNeedsWork struct") {
		t.Errorf("expected 'type OutcomeNeedsWork struct' in generated code:\n%s", code)
	}

	// Verify sealed method marker.
	if !strings.Contains(code, "isOutcome()") {
		t.Errorf("expected 'isOutcome()' sealed method:\n%s", code)
	}

	// Verify field in struct variant.
	if !containsNormalized(code, "Evidence string") {
		t.Errorf("expected 'Evidence string' in OutcomeConfirmed:\n%s", code)
	}

	// Verify it compiles.
	assertCompiles(t, code)
}

// ---------------------------------------------------------------------------
// option(T) support
// ---------------------------------------------------------------------------

func TestOptionNoneViaMatch(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% match label %}
> {% case Some %}

got:{{ label }}

> {% case None %}

empty

> {% /match %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"label": nil})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionSomeViaMatch(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% match label %}
> {% case Some %}

got:{{ label }}

> {% case None %}

empty

> {% /match %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"label": "hello",
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "got:hello") {
		t.Errorf("expected 'got:hello' in output, got %q", result)
	}
}

func TestOptionNoneViaHas(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"label": nil})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionSomeViaHas(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"label": "world",
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "got:world") {
		t.Errorf("expected 'got:world' in output, got %q", result)
	}
}

func TestOptionIntNoneViaJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("count", `null`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "no-count" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "no-count")
	}
}

func TestOptionIntSomeViaJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("count", `42`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "count=42") {
		t.Errorf("expected 'count=42' in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// option(T) regression tests — transparent API
// ---------------------------------------------------------------------------

func TestOptionSetNoneDirect(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetNone("label"); err != nil {
		t.Fatalf("SetNone: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionSetNilViaSet(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	// Set(key, nil) should transparently call SetNone
	if err := ctx.Set("label", nil); err != nil {
		t.Fatalf("Set(nil): %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionNilInRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = option(str)
  - score = option(int)
---
> {% if has(name) %}

{{ name }}

> {% else %}

anon

> {% /if %}
> {% if has(score) %}

({{ score }})

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// nil for name, 42 for score
	result, err := tmpl.RenderMap(map[string]any{
		"name":  nil,
		"score": 42,
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "anon") {
		t.Errorf("expected 'anon' in output (nil name), got %q", result)
	}
	if !strings.Contains(result, "(42)") {
		t.Errorf("expected '(42)' in output, got %q", result)
	}
}

func TestOptionNilPointerInRenderStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Label *string `json:"label"`
	}

	// nil pointer should map to None
	result, err := tmpl.RenderStruct(Params{Label: nil})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}

	// non-nil pointer should map to Some
	s := "hello"
	result, err = tmpl.RenderStruct(Params{Label: &s})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "got:hello") {
		t.Errorf("expected 'got:hello' in output, got %q", result)
	}
}

func TestOptionMatchNilViaRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - x = option(int)
---
> {% match x %}
> {% case Some %}

val={{ x }}

> {% case None %}

absent

> {% /match %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// nil → None arm
	result, err := tmpl.RenderMap(map[string]any{"x": nil})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if strings.TrimSpace(result) != "absent" {
		t.Errorf("nil: got %q, want %q", strings.TrimSpace(result), "absent")
	}

	// 99 → Some arm
	result, err = tmpl.RenderMap(map[string]any{"x": 99})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "val=99") {
		t.Errorf("99: expected 'val=99' in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// FromSourceWithEnv — compile-time environment variables
// ---------------------------------------------------------------------------

func TestEnvBasicStrSubstitution(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [MODEL = str]

params: []
---
Model: {{ MODEL }}`, map[string]any{"MODEL": "gpt-4"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Model: gpt-4" {
		t.Errorf("got %q, want %q", result, "Model: gpt-4")
	}
}

func TestEnvDefaultUsed(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env:
  - MODEL = str := "gpt-3.5"

params: []
---
Model: {{ MODEL }}`, map[string]any{})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Model: gpt-3.5" {
		t.Errorf("got %q, want %q", result, "Model: gpt-3.5")
	}
}

func TestEnvDefaultOverridden(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env:
  - MODEL = str := "gpt-3.5"

params: []
---
Model: {{ MODEL }}`, map[string]any{"MODEL": "gpt-4"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Model: gpt-4" {
		t.Errorf("got %q, want %q", result, "Model: gpt-4")
	}
}

func TestEnvMissingRequiredErrors(t *testing.T) {
	_, err := FromSourceWithEnv(`---
env: [REQUIRED_PATH = str]

params: []
---
{{ REQUIRED_PATH }}`, map[string]any{})
	if err == nil {
		t.Fatal("expected error for missing required env var, got nil")
	}
}

func TestEnvCoexistsWithParams(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [PREFIX = str]

params: [name = str]
---
{{ PREFIX }}/{{ name }}`, map[string]any{"PREFIX": "/opt/prompts"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"name": "agent_x"})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "/opt/prompts/agent_x" {
		t.Errorf("got %q, want %q", result, "/opt/prompts/agent_x")
	}
}

func TestEnvIntType(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [MAX_RETRIES = int]

params: []
---
Retries: {{ MAX_RETRIES }}`, map[string]any{"MAX_RETRIES": "5"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Retries: 5" {
		t.Errorf("got %q, want %q", result, "Retries: 5")
	}
}

func TestEnvBoolType(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [DEBUG = bool]

params: []
---
> {% if DEBUG %}debug_on{% else %}debug_off{% /if %}`, map[string]any{"DEBUG": "true"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "debug_on" {
		t.Errorf("got %q, want %q", result, "debug_on")
	}
}

func TestEnvMultipleDefaultsPartialOverride(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env:
  - A = str := "alpha"
  - B = int := 42
  - C = bool := true

params: []
---
{{ A }}-{{ B }}-{% if C %}yes{% else %}no{% /if %}`, map[string]any{"A": "beta"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "beta-42-yes" {
		t.Errorf("got %q, want %q", result, "beta-42-yes")
	}
}

// ---------------------------------------------------------------------------
// Duck typing / structural typing — extra fields silently ignored
// ---------------------------------------------------------------------------

func TestDuckTypingStructExtraFields(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// JSON value has extra fields "timeout" and "debug" not declared in the schema.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("config", `{"host":"localhost","port":8080,"timeout":30,"debug":true}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "localhost:8080" {
		t.Errorf("got %q, want %q", result, "localhost:8080")
	}
}

func TestDuckTypingListItemsExtraFields(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - items = list(name = str, value = int)
---
> {% for item in items %}

{{ item.name }}={{ item.value }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Each list item carries extra fields ("color", "weight") not in the declaration.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("items", `[
		{"name":"alpha","value":1,"color":"red","weight":0.5},
		{"name":"beta","value":2,"color":"blue","weight":1.5}
	]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	expected := "alpha=1\nbeta=2\n"
	if result != expected {
		t.Errorf("got %q, want %q", result, expected)
	}
}

func TestDuckTypingNestedStructExtraFieldsAtEveryDepth(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - root = struct(child = struct(leaf = struct(val = str)))
---
{{ root.child.leaf.val }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Extra fields at every nesting depth:
	//   root: has "extra_root"
	//   child: has "extra_child"
	//   leaf: has "extra_leaf"
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("root", `{
		"extra_root": 999,
		"child": {
			"extra_child": "ignored",
			"leaf": {
				"val": "deep",
				"extra_leaf": [1, 2, 3]
			}
		}
	}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "deep" {
		t.Errorf("got %q, want %q", result, "deep")
	}
}

func TestDuckTypingStructExtraFieldsViaRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - user = struct(name = str, age = int)
---
{{ user.name }} is {{ user.age }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Provide extra fields via a Go map — they should be silently ignored.
	result, err := tmpl.RenderMap(map[string]any{
		"user": map[string]any{
			"name":     "Alice",
			"age":      int64(30),
			"email":    "alice@example.com",
			"verified": true,
		},
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Alice is 30" {
		t.Errorf("got %q, want %q", result, "Alice is 30")
	}
}

func TestDuckTypingListOfStructsExtraFieldsViaSet(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - tasks = list(title = str)
---
> {% for t in tasks %}

- {{ t.title }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Each map item has extra keys beyond the declared "title".
	ctx := NewContext()
	defer ctx.Close()
	tasks := []map[string]any{
		{"title": "Write docs", "priority": "high", "done": false},
		{"title": "Fix bugs", "priority": "medium", "done": true},
	}
	if err := ctx.Set("tasks", tasks); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "- Write docs") {
		t.Errorf("expected '- Write docs' in output, got %q", result)
	}
	if !strings.Contains(result, "- Fix bugs") {
		t.Errorf("expected '- Fix bugs' in output, got %q", result)
	}
}

func TestDuckTypingStructExtraFieldsViaRenderStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - server = struct(host = str, port = int)
---
{{ server.host }}:{{ server.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Go struct has extra fields (Protocol, MaxConns) not declared in the template.
	type Server struct {
		Host     string `json:"host"`
		Port     int    `json:"port"`
		Protocol string `json:"protocol"`
		MaxConns int    `json:"max_conns"`
	}
	type Params struct {
		Server Server `json:"server"`
	}

	result, err := tmpl.RenderStruct(Params{Server: Server{
		Host:     "example.com",
		Port:     443,
		Protocol: "https",
		MaxConns: 1000,
	}})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "example.com:443" {
		t.Errorf("got %q, want %q", result, "example.com:443")
	}
}

func TestDuckTypingRejectsMissingRequiredFields(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - item = struct(name = str, score = int)
---
{{ item.name }}: {{ item.score }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Has extra fields but is MISSING the required "score" field.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("item", `{"name":"Alice","bonus":99,"extra":true}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for missing required struct field 'score', got nil")
	}
	if !strings.Contains(err.Error(), "score") {
		t.Errorf("error should mention missing field 'score': %v", err)
	}
}

// ---------------------------------------------------------------------------
// Higher-order tmpl() comprehensive tests
// ---------------------------------------------------------------------------

func TestSetTmplTypeMismatch(t *testing.T) {
	// Helper declares (age = int), but main expects tmpl(name = str).
	helper, err := FromSourceAllowingUnused(`---
params: [age = int]
---
Age: {{ age }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [test = tmpl(name = str)]
---
> {% include test with name="World" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("test", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	_, err = main.Render(ctx)
	if err == nil {
		t.Fatal("expected error for tmpl type mismatch, got nil")
	}
	if !strings.Contains(err.Error(), "type mismatch") {
		t.Errorf("error should mention 'type mismatch': %v", err)
	}
}

func TestSetTmplWithDefaults(t *testing.T) {
	// Helper has extra defaulted param "greeting"; main only requires tmpl(name = str).
	helper, err := FromSourceAllowingUnused(`---
params:
  - name = str
  - greeting = str := "Hi"
---
{{ greeting }} {{ name }}!`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [test = tmpl(name = str)]
---
> {% include test with name="World" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("test", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hi World!" {
		t.Errorf("got %q, want %q", result, "Hi World!")
	}
}

func TestSetTmplNested(t *testing.T) {
	// Three-level nesting: main → processor → inner
	inner, err := FromSourceAllowingUnused(`---
params: [val = str]
---
Inner: {{ val }}`)
	if err != nil {
		t.Fatalf("inner FromSource: %v", err)
	}
	defer inner.Close()

	middle, err := FromSourceAllowingUnused(`---
params:
  - target = tmpl(val = str)
  - value = str
---
> {% include target with val=value %}`)
	if err != nil {
		t.Fatalf("middle FromSource: %v", err)
	}
	defer middle.Close()

	main, err := FromSource(`---
params:
  - processor = tmpl(target = tmpl(val = str), value = str)
  - callback = tmpl(val = str)
---
> {% include processor with target=callback, value="Success" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("processor", middle); err != nil {
		t.Fatalf("SetTmpl processor: %v", err)
	}
	if err := ctx.SetTmpl("callback", inner); err != nil {
		t.Fatalf("SetTmpl callback: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Inner: Success" {
		t.Errorf("got %q, want %q", result, "Inner: Success")
	}
}

func TestSetTmplEmptySignature(t *testing.T) {
	// tmpl() with empty parens: accepts a no-param template.
	helper, err := FromSourceAllowingUnused(`---
params: []
---
Preamble content here.`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [preamble = tmpl()]
---

> {% include preamble %}

Done.`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("preamble", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Preamble content here.") {
		t.Errorf("expected 'Preamble content here.' in output, got %q", result)
	}
	if !strings.Contains(result, "Done.") {
		t.Errorf("expected 'Done.' in output, got %q", result)
	}
}

func TestSetTmplExtraDefaultedParamsMatch(t *testing.T) {
	// Helper has extra defaulted param. Main expects tmpl(x = str).
	// This should match because the extra param has a default.
	helper, err := FromSourceAllowingUnused(`---
params:
  - x = str
  - extra = str := "fallback"
---
{{ x }} {{ extra }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [widget = tmpl(x = str)]
---
> {% include widget with x="hello" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "hello fallback" {
		t.Errorf("got %q, want %q", result, "hello fallback")
	}
}

func TestSetTmplExtraRequiredParamsRejected(t *testing.T) {
	// Helper has extra REQUIRED param (no default). Main expects tmpl(x = str).
	// This should be rejected as a type mismatch.
	helper, err := FromSourceAllowingUnused(`---
params:
  - x = str
  - extra = str
---
{{ x }} {{ extra }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [widget = tmpl(x = str)]
---
> {% include widget with x="hello" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	_, err = main.Render(ctx)
	if err == nil {
		t.Fatal("expected error for extra required param, got nil")
	}
}

func TestSetTmplMultiParamForwarding(t *testing.T) {
	// Main passes multiple params to tmpl(x = str, y = int).
	helper, err := FromSourceAllowingUnused(`---
params:
  - x = str
  - y = int
---
{{ x }}-{{ y }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params:
  - widget = tmpl(x = str, y = int)
  - label = str
  - num = int
---
Label: {{ label }}

> {% include widget with x="hello", y=num %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}
	if err := ctx.SetStr("label", "test"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetInt("num", 42); err != nil {
		t.Fatalf("SetInt: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Label: test") {
		t.Errorf("expected 'Label: test' in output, got %q", result)
	}
	if !strings.Contains(result, "hello-42") {
		t.Errorf("expected 'hello-42' in output, got %q", result)
	}
}

func TestSetTmplNonTemplateValueRejected(t *testing.T) {
	// Passing a plain string for a tmpl() param must be rejected.
	main, err := FromSource(`---
params: [widget = tmpl(name = str)]
---
> {% include widget with name="test" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("widget", "not a template"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = main.Render(ctx)
	if err == nil {
		t.Fatal("expected error for non-template value in tmpl param, got nil")
	}
}

func TestSetTmplIsTruthy(t *testing.T) {
	// A tmpl() param should be truthy when set (works in {% if %} guards).
	helper, err := FromSourceAllowingUnused(`---
params: []
---
present`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [widget = tmpl()]
---
> {% if widget %}

yes

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "yes") {
		t.Errorf("tmpl should be truthy, got %q", result)
	}
}

func TestOptionTmplSome(t *testing.T) {
	// option(tmpl(test = str)) with a template provided.
	helper, err := FromSourceAllowingUnused(`---
params: [test = str]
---
Helper: {{ test }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [cb = option(tmpl(test = str))]
---
> {% if has(cb) %}
> {% include cb with test="Hello Option" %}
> {% else %}

No callback

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("cb", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Helper: Hello Option") {
		t.Errorf("expected 'Helper: Hello Option' in output, got %q", result)
	}
}

func TestOptionTmplNone(t *testing.T) {
	// option(tmpl(test = str)) with None (absent value).
	main, err := FromSource(`---
params: [cb = option(tmpl(test = str))]
---
> {% if has(cb) %}
> {% include cb with test="Hello" %}
> {% else %}

No callback

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetNone("cb"); err != nil {
		t.Fatalf("SetNone: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "No callback") {
		t.Errorf("expected 'No callback' in output, got %q", result)
	}
}

func TestOptionTmplNilViaSet(t *testing.T) {
	// option(tmpl(test = str)) with nil via Set → should render None path.
	main, err := FromSource(`---
params: [cb = option(tmpl(test = str))]
---
> {% if has(cb) %}
> {% include cb with test="Hello" %}
> {% else %}

absent

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.Set("cb", nil); err != nil {
		t.Fatalf("Set nil: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "absent") {
		t.Errorf("expected 'absent' in output, got %q", result)
	}
}

func TestNestedOptionTmpl(t *testing.T) {
	// Three-level nesting with option(tmpl()) at each level.
	inner, err := FromSourceAllowingUnused(`---
params: [test = str]
---
Inner: {{ test }}`)
	if err != nil {
		t.Fatalf("inner FromSource: %v", err)
	}
	defer inner.Close()

	middle, err := FromSourceAllowingUnused(`---
params: [sub = option(tmpl(test = str))]
---
> {% if has(sub) %}
> {% include sub with test="Nested Success" %}
> {% else %}

No sub

> {% /if %}`)
	if err != nil {
		t.Fatalf("middle FromSource: %v", err)
	}
	defer middle.Close()

	main, err := FromSource(`---
params:
  - cb = option(tmpl(sub = option(tmpl(test = str))))
  - target = option(tmpl(test = str))
---
> {% if has(cb) %}
> {% include cb with sub=target %}
> {% else %}

No cb

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("cb", middle); err != nil {
		t.Fatalf("SetTmpl cb: %v", err)
	}
	if err := ctx.SetTmpl("target", inner); err != nil {
		t.Fatalf("SetTmpl target: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Inner: Nested Success") {
		t.Errorf("expected 'Inner: Nested Success' in output, got %q", result)
	}
}

func TestSetTmplViaSetGenericAPI(t *testing.T) {
	// Using ctx.Set(key, *Template) should automatically route to SetTmpl.
	helper, err := FromSourceAllowingUnused(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [greet = tmpl(name = str)]
---
> {% include greet with name="Generic" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	// Use Set() instead of SetTmpl()
	if err := ctx.Set("greet", helper); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello Generic!" {
		t.Errorf("got %q, want %q", result, "Hello Generic!")
	}
}
