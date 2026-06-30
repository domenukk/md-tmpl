package md_tmpl

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// ---------------------------------------------------------------------------
// Template sources (matching the Rust benchmarks for comparable results)
// ---------------------------------------------------------------------------

const smallTemplate = `---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!`

const mediumTemplate = `---
params:
  - title = str
  - status = str
  - score = float
  - show_footer = bool
  - items = list(label = str, value = str)
---
# Report for {{ title | upper }}

Status: {{ status }}
Score: {{ score | fixed(2) }}

## Items

> {% for item in items %}

- {{ item.label }}: {{ item.value }}

> {% /for %}

> {% if show_footer %}

---
Generated for {{ title }}.

> {% /if %}`

const largeTemplate = `---
params:
  - title = str
  - sections = list(heading = str, entries = list(name = str, active = bool, score = float, tags = list(label = str)))
  - notes = str
---
# {{ title | upper }}

> {% for section in sections %}

## {{ section.heading }}

> {% for entry in section.entries %}

### {{ entry.name | trim }}

> {% if entry.active %}

- Status: **active**
- Score: {{ entry.score | fixed(1) }}

> {% elif entry.score > 0 %}

- Status: inactive (score {{ entry.score }})

> {% else %}

- Status: inactive

> {% /if %}

> {% for tag in entry.tags %}

  - tag: {{ tag.label | lower }}

> {% /for %}

> {% /for %}

> {% /for %}

> {% if notes %}

## Notes

{{ notes }}

> {% /if %}`

// ---------------------------------------------------------------------------
// Context builders
// ---------------------------------------------------------------------------

func smallContext(tb testing.TB) *Context {
	tb.Helper()
	ctx := NewContext()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		tb.Fatal(err)
	}
	if err := ctx.SetStr("place", "Wonderland"); err != nil {
		tb.Fatal(err)
	}
	return ctx
}

func mediumContext(tb testing.TB) *Context {
	tb.Helper()
	ctx := NewContext()
	if err := ctx.SetStr("title", "Monthly"); err != nil {
		tb.Fatal(err)
	}
	if err := ctx.SetStr("status", "complete"); err != nil {
		tb.Fatal(err)
	}
	if err := ctx.SetFloat("score", 87.456); err != nil {
		tb.Fatal(err)
	}
	if err := ctx.SetBool("show_footer", true); err != nil {
		tb.Fatal(err)
	}
	if err := ctx.SetJSON("items", `[
		{"label":"Alpha","value":"100"},
		{"label":"Beta","value":"200"},
		{"label":"Gamma","value":""},
		{"label":"Delta","value":"400"},
		{"label":"Epsilon","value":"500"}
	]`); err != nil {
		tb.Fatal(err)
	}
	return ctx
}

func largeContext(tb testing.TB) *Context {
	tb.Helper()
	ctx := NewContext()
	if err := ctx.SetStr("title", "Benchmark Report"); err != nil {
		tb.Fatal(err)
	}
	if err := ctx.SetStr("notes", "End of report."); err != nil {
		tb.Fatal(err)
	}

	// Build sections with multiple entries, each having tags.
	// We manually build JSON to ensure float values always include a decimal point,
	// which the Rust parser requires to distinguish int vs float.
	makeEntries := func(n int) string {
		entries := make([]string, n)
		for i := 0; i < n; i++ {
			score := float64(i) * 1.5
			active := "false"
			if i%3 == 0 {
				active = "true"
			}
			entries[i] = fmt.Sprintf(
				`{"name":"Entry-%d","active":%s,"score":%s,"tags":[{"label":"rust"},{"label":"bench"},{"label":"template"}]}`,
				i, active, formatFloat(score),
			)
		}
		return "[" + joinStrings(entries, ",") + "]"
	}

	sectionsJSON := fmt.Sprintf(
		`[{"heading":"Section A","entries":%s},{"heading":"Section B","entries":%s},{"heading":"Section C","entries":%s}]`,
		makeEntries(10), makeEntries(10), makeEntries(10),
	)
	if err := ctx.SetJSON("sections", sectionsJSON); err != nil {
		tb.Fatal(err)
	}
	return ctx
}

// formatFloat ensures a float64 always renders with a decimal point.
func formatFloat(f float64) string {
	s := fmt.Sprintf("%g", f)
	for _, c := range s {
		if c == '.' || c == 'e' || c == 'E' {
			return s
		}
	}
	return s + ".0"
}

// joinStrings joins string slices without importing strings for this file.
func joinStrings(parts []string, sep string) string {
	if len(parts) == 0 {
		return ""
	}
	result := parts[0]
	for _, p := range parts[1:] {
		result += sep + p
	}
	return result
}

// ---------------------------------------------------------------------------
// Compile benchmarks
// ---------------------------------------------------------------------------

func BenchmarkCompileSmall(b *testing.B) {
	b.ReportAllocs()
	for b.Loop() {
		tmpl, err := FromSource(smallTemplate)
		if err != nil {
			b.Fatal(err)
		}
		tmpl.Close()
	}
}

func BenchmarkCompileMedium(b *testing.B) {
	b.ReportAllocs()
	for b.Loop() {
		tmpl, err := FromSource(mediumTemplate)
		if err != nil {
			b.Fatal(err)
		}
		tmpl.Close()
	}
}

func BenchmarkCompileLarge(b *testing.B) {
	b.ReportAllocs()
	for b.Loop() {
		tmpl, err := FromSource(largeTemplate)
		if err != nil {
			b.Fatal(err)
		}
		tmpl.Close()
	}
}

// ---------------------------------------------------------------------------
// Render benchmarks
// ---------------------------------------------------------------------------

func BenchmarkRenderSmall(b *testing.B) {
	tmpl, err := FromSource(smallTemplate)
	if err != nil {
		b.Fatal(err)
	}
	defer tmpl.Close()
	ctx := smallContext(b)
	defer ctx.Close()

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		result, err := tmpl.Render(ctx)
		if err != nil {
			b.Fatal(err)
		}
		_ = result
	}
}

func BenchmarkRenderMedium(b *testing.B) {
	tmpl, err := FromSource(mediumTemplate)
	if err != nil {
		b.Fatal(err)
	}
	defer tmpl.Close()
	ctx := mediumContext(b)
	defer ctx.Close()

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		result, err := tmpl.Render(ctx)
		if err != nil {
			b.Fatal(err)
		}
		_ = result
	}
}

func BenchmarkRenderLarge(b *testing.B) {
	tmpl, err := FromSource(largeTemplate)
	if err != nil {
		b.Fatal(err)
	}
	defer tmpl.Close()
	ctx := largeContext(b)
	defer ctx.Close()

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		result, err := tmpl.Render(ctx)
		if err != nil {
			b.Fatal(err)
		}
		_ = result
	}
}

// ---------------------------------------------------------------------------
// Round-trip benchmarks (compile + render)
// ---------------------------------------------------------------------------

func BenchmarkRoundTripSmall(b *testing.B) {
	ctx := smallContext(b)
	defer ctx.Close()

	b.ReportAllocs()
	for b.Loop() {
		tmpl, err := FromSource(smallTemplate)
		if err != nil {
			b.Fatal(err)
		}
		result, err := tmpl.Render(ctx)
		if err != nil {
			b.Fatal(err)
		}
		_ = result
		tmpl.Close()
	}
}

func BenchmarkRoundTripMedium(b *testing.B) {
	ctx := mediumContext(b)
	defer ctx.Close()

	b.ReportAllocs()
	for b.Loop() {
		tmpl, err := FromSource(mediumTemplate)
		if err != nil {
			b.Fatal(err)
		}
		result, err := tmpl.Render(ctx)
		if err != nil {
			b.Fatal(err)
		}
		_ = result
		tmpl.Close()
	}
}

func BenchmarkRoundTripLarge(b *testing.B) {
	ctx := largeContext(b)
	defer ctx.Close()

	b.ReportAllocs()
	for b.Loop() {
		tmpl, err := FromSource(largeTemplate)
		if err != nil {
			b.Fatal(err)
		}
		result, err := tmpl.Render(ctx)
		if err != nil {
			b.Fatal(err)
		}
		_ = result
		tmpl.Close()
	}
}

// ---------------------------------------------------------------------------
// Filter benchmarks
// ---------------------------------------------------------------------------

func BenchmarkFilterUpper(b *testing.B) {
	tmpl, err := FromSource(`---
params: [val = str]
---
{{ val | upper }}`)
	if err != nil {
		b.Fatal(err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("val", "hello world benchmark string"); err != nil {
		b.Fatal(err)
	}

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		if _, err := tmpl.Render(ctx); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkFilterChain(b *testing.B) {
	tmpl, err := FromSource(`---
params: [val = str]
---
{{ val | trim | upper }}`)
	if err != nil {
		b.Fatal(err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("val", "  mixed Case Input  "); err != nil {
		b.Fatal(err)
	}

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		if _, err := tmpl.Render(ctx); err != nil {
			b.Fatal(err)
		}
	}
}

// ---------------------------------------------------------------------------
// Cache benchmark
// ---------------------------------------------------------------------------

func BenchmarkCacheLoad(b *testing.B) {
	dir := b.TempDir()
	path := filepath.Join(dir, "bench.tmpl.md")
	if err := os.WriteFile(path, []byte(smallTemplate), 0644); err != nil {
		b.Fatal(err)
	}

	cache := NewCache()
	defer cache.Close()

	// Prime the cache.
	tmpl, err := cache.Load(path)
	if err != nil {
		b.Fatal(err)
	}
	tmpl.Close()

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		t, err := cache.Load(path)
		if err != nil {
			b.Fatal(err)
		}
		t.Close()
	}
}

// ---------------------------------------------------------------------------
// RenderMap benchmark
// ---------------------------------------------------------------------------

func BenchmarkRenderMap(b *testing.B) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`)
	if err != nil {
		b.Fatal(err)
	}
	defer tmpl.Close()

	params := map[string]any{
		"name":  "Alice",
		"count": int64(42),
	}

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		if _, err := tmpl.RenderMap(params); err != nil {
			b.Fatal(err)
		}
	}
}
