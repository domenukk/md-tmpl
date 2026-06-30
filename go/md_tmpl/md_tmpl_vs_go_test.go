package md_tmpl

import (
	"bytes"
	"fmt"
	"strings"
	"testing"
	"text/template"
)

// ---------------------------------------------------------------------------
// Comparison benchmarks: md-tmpl (Rust/CGo) vs Go text/template
//
// Each benchmark pair uses equivalent template logic and data so the
// numbers are directly comparable. The Go stdlib templates are the
// "Go_" prefixed benchmarks; md-tmpl benchmarks are already
// defined in md_tmpl_bench_test.go.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Small: simple variable substitution
// ---------------------------------------------------------------------------

const goSmallTemplate = `Hello {{.Name}}, welcome to {{.Place}}!`

type smallData struct {
	Name  string
	Place string
}

var goSmallData = smallData{Name: "Alice", Place: "Wonderland"}

func BenchmarkGo_CompileSmall(b *testing.B) {
	b.ReportAllocs()
	for b.Loop() {
		t, err := template.New("small").Parse(goSmallTemplate)
		if err != nil {
			b.Fatal(err)
		}
		_ = t
	}
}

func BenchmarkGo_RenderSmall(b *testing.B) {
	t := template.Must(template.New("small").Parse(goSmallTemplate))
	var buf bytes.Buffer

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		buf.Reset()
		if err := t.Execute(&buf, goSmallData); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkGo_RoundTripSmall(b *testing.B) {
	var buf bytes.Buffer
	b.ReportAllocs()
	for b.Loop() {
		t, err := template.New("small").Parse(goSmallTemplate)
		if err != nil {
			b.Fatal(err)
		}
		buf.Reset()
		if err := t.Execute(&buf, goSmallData); err != nil {
			b.Fatal(err)
		}
	}
}

// ---------------------------------------------------------------------------
// Medium: loop + conditional + function (upper)
// ---------------------------------------------------------------------------

const goMediumTemplate = `# Report for {{upper .Title}}

Status: {{.Status}}
Score: {{printf "%.2f" .Score}}

## Items

{{range .Items}}- {{.Label}}: {{.Value}}
{{end}}{{if .ShowFooter}}---
Generated for {{.Title}}.
{{end}}`

type mediumItem struct {
	Label string
	Value string
}

type mediumData struct {
	Title      string
	Status     string
	Score      float64
	ShowFooter bool
	Items      []mediumItem
}

var goMediumData = mediumData{
	Title:      "Monthly",
	Status:     "complete",
	Score:      87.456,
	ShowFooter: true,
	Items: []mediumItem{
		{Label: "Alpha", Value: "100"},
		{Label: "Beta", Value: "200"},
		{Label: "Gamma", Value: ""},
		{Label: "Delta", Value: "400"},
		{Label: "Epsilon", Value: "500"},
	},
}

var goMediumFuncs = template.FuncMap{
	"upper": strings.ToUpper,
}

func BenchmarkGo_CompileMedium(b *testing.B) {
	b.ReportAllocs()
	for b.Loop() {
		t, err := template.New("medium").Funcs(goMediumFuncs).Parse(goMediumTemplate)
		if err != nil {
			b.Fatal(err)
		}
		_ = t
	}
}

func BenchmarkGo_RenderMedium(b *testing.B) {
	t := template.Must(template.New("medium").Funcs(goMediumFuncs).Parse(goMediumTemplate))
	var buf bytes.Buffer

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		buf.Reset()
		if err := t.Execute(&buf, goMediumData); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkGo_RoundTripMedium(b *testing.B) {
	var buf bytes.Buffer
	b.ReportAllocs()
	for b.Loop() {
		t, err := template.New("medium").Funcs(goMediumFuncs).Parse(goMediumTemplate)
		if err != nil {
			b.Fatal(err)
		}
		buf.Reset()
		if err := t.Execute(&buf, goMediumData); err != nil {
			b.Fatal(err)
		}
	}
}

// ---------------------------------------------------------------------------
// Large: nested loops + conditionals + functions
// ---------------------------------------------------------------------------

const goLargeTemplate = `# {{upper .Title}}

{{range .Sections}}## {{.Heading}}

{{range .Entries}}### {{trim .Name}}

{{if .Active}}- Status: **active**
- Score: {{printf "%.1f" .Score}}
{{else if gt .Score 0.0}}- Status: inactive (score {{.Score}})
{{else}}- Status: inactive
{{end}}{{range .Tags}}  - tag: {{lower .Label}}
{{end}}{{end}}{{end}}{{if .Notes}}## Notes

{{.Notes}}
{{end}}`

type largeTag struct {
	Label string
}

type largeEntry struct {
	Name   string
	Active bool
	Score  float64
	Tags   []largeTag
}

type largeSection struct {
	Heading string
	Entries []largeEntry
}

type largeData struct {
	Title    string
	Sections []largeSection
	Notes    string
}

func makeLargeEntries(n int) []largeEntry {
	entries := make([]largeEntry, n)
	for i := range n {
		entries[i] = largeEntry{
			Name:   fmt.Sprintf("Entry-%d", i),
			Active: i%3 == 0,
			Score:  float64(i) * 1.5,
			Tags: []largeTag{
				{Label: "rust"},
				{Label: "bench"},
				{Label: "template"},
			},
		}
	}
	return entries
}

var goLargeData = largeData{
	Title: "Benchmark Report",
	Sections: []largeSection{
		{Heading: "Section A", Entries: makeLargeEntries(10)},
		{Heading: "Section B", Entries: makeLargeEntries(10)},
		{Heading: "Section C", Entries: makeLargeEntries(10)},
	},
	Notes: "End of report.",
}

var goLargeFuncs = template.FuncMap{
	"upper": strings.ToUpper,
	"lower": strings.ToLower,
	"trim":  strings.TrimSpace,
}

func BenchmarkGo_CompileLarge(b *testing.B) {
	b.ReportAllocs()
	for b.Loop() {
		t, err := template.New("large").Funcs(goLargeFuncs).Parse(goLargeTemplate)
		if err != nil {
			b.Fatal(err)
		}
		_ = t
	}
}

func BenchmarkGo_RenderLarge(b *testing.B) {
	t := template.Must(template.New("large").Funcs(goLargeFuncs).Parse(goLargeTemplate))
	var buf bytes.Buffer

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		buf.Reset()
		if err := t.Execute(&buf, goLargeData); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkGo_RoundTripLarge(b *testing.B) {
	var buf bytes.Buffer
	b.ReportAllocs()
	for b.Loop() {
		t, err := template.New("large").Funcs(goLargeFuncs).Parse(goLargeTemplate)
		if err != nil {
			b.Fatal(err)
		}
		buf.Reset()
		if err := t.Execute(&buf, goLargeData); err != nil {
			b.Fatal(err)
		}
	}
}

// ---------------------------------------------------------------------------
// Filter comparison: upper
// ---------------------------------------------------------------------------

func BenchmarkGo_FilterUpper(b *testing.B) {
	funcs := template.FuncMap{"upper": strings.ToUpper}
	t := template.Must(template.New("f").Funcs(funcs).Parse(`{{upper .Val}}`))
	data := struct{ Val string }{Val: "hello world benchmark string"}
	var buf bytes.Buffer

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		buf.Reset()
		if err := t.Execute(&buf, data); err != nil {
			b.Fatal(err)
		}
	}
}

// ---------------------------------------------------------------------------
// Filter comparison: chain (trim + upper)
// ---------------------------------------------------------------------------

func BenchmarkGo_FilterChain(b *testing.B) {
	funcs := template.FuncMap{
		"upper": strings.ToUpper,
		"trim":  strings.TrimSpace,
	}
	t := template.Must(template.New("f").Funcs(funcs).Parse(`{{.Val | trim | upper}}`))
	data := struct{ Val string }{Val: "  mixed Case Input  "}
	var buf bytes.Buffer

	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		buf.Reset()
		if err := t.Execute(&buf, data); err != nil {
			b.Fatal(err)
		}
	}
}
