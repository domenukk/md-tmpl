# md-tmpl for Go

Strongly-typed prompt templates for LLMs.

## Why?

- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor or on GitHub.
- **Strict typing** — every parameter declares a type; mismatches are caught at render time with clear errors.
- **Agent-safe** — when an LLM edits prompts, the engine catches drift immediately.
- **Fast** — native Rust engine via CGo FFI, 3–6× faster than `text/template` on medium/large templates.

## Quick Example

The template — a plain `.tmpl.md` file:

<!-- prettier-ignore -->
```markdown
---
params:
  - model = str
  - steps = list(tool = str, status = enum(Search(reason = str), Done))
---

# Run: {{ model }}

> {% for step in steps %}

## Step {{ idx(step) }}: {{ step.tool }}

> {% match step.status %}
> {% when Search %}

Searching — {{ step.status.reason }}

> {% when Done %}

✅ Complete

> {% /match %}
> {% /for %}
```

Render it from Go:

```go
import pt "github.com/domenukk/md-tmpl/go/md_tmpl"

// Typed structs map directly to template parameters.
type AgentAction struct {
    pt.TaggedVariant
    Reason string `json:"reason"`
}

func Search(reason string) AgentAction {
    return AgentAction{TaggedVariant: pt.NewTaggedVariant("Search"), Reason: reason}
}

type Step struct {
    Tool   string `json:"tool"`
    Status any    `json:"status"` // Go lacks sum types — use any for enum slots
}

type RunParams struct {
    Model string `json:"model"`
    Steps []Step `json:"steps"`
}

tmpl, _ := pt.FromFile("prompts/run.tmpl.md")
defer tmpl.Close()

result, _ := tmpl.RenderStruct(RunParams{
    Model: "gemini-3.5-flash",
    Steps: []Step{
        {Tool: "web", Status: Search("latest Go release")},
        {Tool: "code", Status: pt.Variant{Kind: "Done"}},
    },
})
```

> **On `Status any`:** Go has no sum types or tagged unions, so enum-typed
> fields use `any`. The template engine validates the variant kind and fields
> at render time — you still get clear errors for mismatches. For static
> typing within Go, embed [`TaggedVariant`](#taggedvariant-static-typing) in
> custom structs.

## Prerequisites

The Go binding calls a native library via CGo. Build it first:

```bash
# Option A: using just (recommended)
just build-go-ffi

# Option B: manual
cargo build -p md-tmpl-ffi --release
```

You need a working [Rust toolchain](https://rustup.rs/) for the build step.

## Installation

```bash
go get github.com/domenukk/md-tmpl/go/md_tmpl
```

## Struct-Based Rendering

Define Go structs that match your template parameters. Fields are mapped
by `json` tag, falling back to lowercased field name:

```go
type ReviewParams struct {
    Reviewer string `json:"reviewer"`
    FilePath string `json:"file_path"`
    Severity string `json:"severity"`
}

tmpl, err := pt.FromFile("prompts/code_review.tmpl.md")
if err != nil {
    log.Fatal(err)
}
defer tmpl.Close()

result, err := tmpl.RenderStruct(ReviewParams{
    Reviewer: "Alice",
    FilePath: "main.go",
    Severity: "high",
})
```

## Map-Based Rendering

For dynamic use cases, pass a `map[string]any`:

```go
result, err := tmpl.RenderMap(map[string]any{
    "reviewer":  "Alice",
    "file_path": "main.go",
    "severity":  "high",
})
```

## Enum Dispatch

Typed variants with exhaustiveness checking and field narrowing.

### TaggedVariant (static typing)

Embed `TaggedVariant` in Go structs for statically typed variants:

```go
type DoneVariant struct {
    pt.TaggedVariant
    Summary string `json:"summary"`
}

func NewDone(summary string) DoneVariant {
    return DoneVariant{
        TaggedVariant: pt.NewTaggedVariant("Done"),
        Summary:       summary,
    }
}

type StatusParams struct {
    Status any `json:"status"`
}

result, err := tmpl.RenderStruct(StatusParams{
    Status: NewDone("All tests pass"),
})
```

### Dynamic Variant

When the variant is determined at runtime:

```go
result, err := tmpl.RenderMap(map[string]any{
    "status": pt.Variant{
        Kind:   "Done",
        Fields: map[string]any{"summary": "All tests pass"},
    },
})

// Unit variants (no fields):
result, err = tmpl.RenderMap(map[string]any{
    "status": pt.Variant{Kind: "Blocked"},
})
```

## Features

### Typed Lists

Template:

<!-- prettier-ignore -->
```markdown
---
params:
  - tasks = list(title = str, priority = str)
---

> {% for task in tasks %}

- {{ task.title }}: {{ task.priority }}

> {% /for %}
```

Go:

```go
type Task struct {
    Title    string `json:"title"`
    Priority string `json:"priority"`
}

tmpl, _ := pt.FromFile("prompts/task_list.tmpl.md")
defer tmpl.Close()

result, _ := tmpl.RenderStruct(struct {
    Tasks []Task `json:"tasks"`
}{
    Tasks: []Task{
        {Title: "Write docs", Priority: "High"},
        {Title: "Add tests", Priority: "Medium"},
    },
})
```

### Default Values

```markdown
---
params:
  - name = str
  - greeting = str := "Hello"
---

{{ greeting }}, {{ name }}!
```

```go
tmpl, _ := pt.FromFile("prompts/greeting.tmpl.md")

result, _ := tmpl.RenderMap(map[string]any{"name": "Alice"})
// → "Hello, Alice!"
```

### Filters

```
{{ name | upper }}        → ALICE
{{ name | lower }}        → alice
{{ name | trim }}         → (strips whitespace)
{{ score | fixed(2) }}    → 3.14
{{ items | join(", ") }}  → a, b, c
{{ items | limit(2) }}    → first 2 elements
{{ count | add(1) }}      → 43
{{ count | sub(1) }}      → 41
{{ name | trim | upper }} → chains work
```

### Built-in Functions

```
{{ idx(item) }}           → 0, 1, 2, … (loop index)
{{ len(items) }}          → 3 (list length)
{{ len(name) }}           → 5 (string length)
{{ kind(status) }}        → "Done" (variant name)
{{ has(field) }}          → true if option(T) is present
```

`idx(binding)` tracks each loop variable independently in nested loops.

### Includes

```markdown
> {% include [header](header.tmpl.md) with title=title %}
```

### Constants

```markdown
---
consts:
  - MAX_RETRIES = int := 3

params: []
---

Max retries: {{ MAX_RETRIES }}
```

### Caching

```go
cache := pt.NewCache()
defer cache.Close()

tmpl, err := cache.Load("prompts/greeting.tmpl.md")
defer tmpl.Close()
cache.TemplateCount()
cache.Clear()
```

## API Reference

### Template

```go
// Constructors
pt.FromSource(source string) (*Template, error)
pt.FromSourceAllowingUnused(source string) (*Template, error)
pt.FromSourceWithBaseDir(source, baseDir string) (*Template, error)
pt.FromSourceWithFrontmatter(source string) (*Template, *Frontmatter, error)
pt.FromFile(path string) (*Template, error)

// Rendering — each has an AllowingExtra variant
tmpl.Render(ctx *Context) (string, error)
tmpl.RenderMap(params map[string]any) (string, error)
tmpl.RenderStruct(v any) (string, error)
tmpl.RenderJSON(jsonStr string) (string, error)

// Metadata
tmpl.Declarations() []Declaration
tmpl.Defaults() map[string]any
tmpl.Constants() map[string]any
tmpl.ImportedConstants() map[string]any
tmpl.DefaultsContext() *Context
tmpl.SourceHash() uint64
tmpl.Body() string
tmpl.ValidateDeclarations([]Declaration) error
tmpl.SetMaxIncludeDepth(depth int)
tmpl.Close()
```

### Context

```go
ctx := pt.NewContext()
defer ctx.Close()

ctx.SetStr("name", "Alice")
ctx.SetInt("count", 42)
ctx.SetFloat("score", 9.5)
ctx.SetBool("enabled", true)
ctx.SetJSON("items", `[{"label":"alpha"}]`)
ctx.SetTmpl("card", cardTemplate)
ctx.Set("key", value) // auto-detect type
ctx.MergeStruct(myStruct) // merge struct fields into context
```

### Errors

```go
var pt.ErrClosed     // operating on a closed resource
var pt.ErrNilContext // rendering with nil context
```

## Performance

vs Go's `text/template`, median of 3 runs
([source](md_tmpl_vs_go_test.go)).

**Render** (pre-parsed template + data → output):

| Scenario |          md-tmpl | Go `text/template` | Speedup |
| -------- | ---------------: | -----------------: | ------: |
| small    |    **525 ns** 🏆 |             569 ns |    1.1× |
| medium   |  **1,716 ns** 🏆 |           6,251 ns |    3.6× |
| large    | **24,808 ns** 🏆 |         140,495 ns |    5.7× |

**Round-trip** (parse + render):

| Scenario |          md-tmpl | Go `text/template` | Speedup |
| -------- | ---------------: | -----------------: | ------: |
| small    |         5,330 ns |  4,735 ns + 569 ns |   ~1.0× |
| medium   | **19,438 ns** 🏆 |          20,509 ns |    1.1× |
| large    | **61,531 ns** 🏆 |         165,633 ns |    2.7× |

**Filters:**

| Filter     |       md-tmpl | Go `text/template` | Speedup |
| ---------- | ------------: | -----------------: | ------: |
| upper      | **489 ns** 🏆 |             907 ns |    1.9× |
| trim+upper | **441 ns** 🏆 |           1,312 ns |    3.0× |

Allocations: 2 per render (small/medium/large) vs 3–517 for `text/template`.

```bash
just test-go     # 144 tests
just bench-go    # 24 benchmarks
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
