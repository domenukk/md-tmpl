package md_tmpl

import (
	"os"
	"path/filepath"
	"strings"
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

func TestFromSourceUndeclaredVariableInBodyRejected(t *testing.T) {
	// Referencing a variable declared nowhere is a compile-time error,
	// mirroring the Rust core's check_undeclared_variables.
	_, err := FromSource(`---
params:
  - x = str
---
{{ x }} {{ y }}`)
	if err == nil {
		t.Fatal("expected error for undeclared variable, got nil")
	}
	if !strings.Contains(err.Error(), "undeclared variable") {
		t.Errorf("error should mention 'undeclared variable': %v", err)
	}
}

func TestFromSourceUndeclaredVariableInConditionRejected(t *testing.T) {
	// Undeclared variables inside an if condition are also rejected at
	// construction time.
	_, err := FromSource(`---
params:
  - x = int
---
> {% if y > 0 %}{{ x }}{% /if %}`)
	if err == nil {
		t.Fatal("expected error for undeclared variable in condition, got nil")
	}
	if !strings.Contains(err.Error(), "undeclared variable") {
		t.Errorf("error should mention 'undeclared variable': %v", err)
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
// Declaration.String()
// ---------------------------------------------------------------------------

func TestDeclarationString(t *testing.T) {
	d := Declaration{Name: "tasks", Type: "list(title = str)"}
	if s := d.String(); s != "tasks = list(title = str)" {
		t.Errorf("got %q, want %q", s, "tasks = list(title = str)")
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
