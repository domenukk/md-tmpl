package md_tmpl

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

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
