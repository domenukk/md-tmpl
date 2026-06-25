# prompt-templates for Go

Strongly-typed prompt templates for LLMs.
Templates are markdown files with YAML frontmatter declaring typed
parameters — every variable, list shape, and enum variant is validated
at render time.

Up to **5.8× faster** than Go's `text/template` with **258× fewer
allocations**.

## Why?

Inline `fmt.Sprintf` strings are unreadable. `text/template` has no type checking — rename a variable and you find out when it panics.
`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor or on GitHub.
- **Strict typing** — every parameter declares a type; mismatches are caught at render time with clear errors.
- **Agent-safe** — when an LLM edits prompts, the engine catches drift immediately.

## Prerequisites

The Go binding uses CGo to call the Rust-based engine. You need:

1. **Rust toolchain** — install via [rustup.rs](https://rustup.rs/)
2. **Build the native library** before `go build`:

   ```bash
   # Option A: using just (recommended)
   just build-go-ffi

   # Option B: manual
   cargo build -p prompt-templates-ffi --release
   ```

## Installation

```bash
go get github.com/domenukk/prompt-templates/go/prompt_templates
```

## Quick Start

### Struct-Based Rendering (recommended)

Define Go structs that match your template parameters — fields are
mapped by `json` tags, falling back to lowercased field name:

```go
import pt "github.com/domenukk/prompt-templates/go/prompt_templates"

type ReviewParams struct {
    Reviewer string `json:"reviewer"`
    FilePath string `json:"file_path"`
    Severity string `json:"severity"`
}

tmpl, err := pt.FromSource(`---
params:
  - reviewer = str
  - file_path = str
  - severity = str
---
# Code Review by {{ reviewer }}

File: {{ file_path }}
Severity: {{ severity }}`)
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

### Map-Based Rendering

For dynamic use cases, pass a `map[string]any`:

```go
result, err := tmpl.RenderMap(map[string]any{
    "reviewer":  "Alice",
    "file_path": "main.go",
    "severity":  "high",
})
```

### Context API (fine-grained control)

```go
ctx := pt.NewContext()
defer ctx.Close()
ctx.SetStr("reviewer", "Alice")
ctx.SetStr("file_path", "main.go")
ctx.SetStr("severity", "high")
result, err := tmpl.Render(ctx)
```

## Typed Lists

```go
type TaskParams struct {
    Tasks []Task `json:"tasks"`
}

type Task struct {
    Title    string `json:"title"`
    Priority string `json:"priority"`
}

tmpl, err := pt.FromSource(`---
params:
  - tasks = list<title = str, priority = str>
---
> {% for task in tasks %}

- {{ task.title }}: {{ task.priority }}
> {% /for %}`)
if err != nil {
    log.Fatal(err)
}
defer tmpl.Close()

result, err := tmpl.RenderStruct(TaskParams{
    Tasks: []Task{
        {Title: "Write documentation", Priority: "High"},
        {Title: "Add unit tests",      Priority: "Medium"},
    },
})
```

## Enum Dispatch

Typed variants with exhaustiveness checking and field narrowing.

### TaggedVariant (static typing, recommended)

Embed `TaggedVariant` in Go structs for compile-time typed enum variants
with zero allocations:

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

For cases where the variant is determined at runtime:

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

### Codegen

`GenerateTypes` produces sealed interfaces and concrete structs with
direct FFI setters for scalar fields — bypassing reflection and
FlexBuffers serialization.

## Default Values

```go
tmpl, err := pt.FromSource(`---
params:
  - name = str
  - greeting = str := "Hello"
---
{{ greeting }}, {{ name }}!`)

result, err := tmpl.RenderMap(map[string]any{"name": "Alice"})
// → "Hello, Alice!"

// Pre-fill with defaults:
ctx := tmpl.DefaultsContext()
defer ctx.Close()
ctx.SetStr("name", "Alice")
result, err = tmpl.Render(ctx)
```

## Filters

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

## Built-in Functions

```
{{ idx(item) }}           → 0, 1, 2, … (loop index)
{{ len(items) }}          → 3 (list length)
{{ len(name) }}           → 5 (string length)
{{ kind(status) }}        → "Done" (variant name)
{{ has(field) }}          → true if option<T> is present
```

`idx(binding)` tracks each loop variable independently in nested loops.

## Includes

```markdown
> {% include [header](header.tmpl.md) with title=title %}
```

## Constants

```markdown
---
consts:
  - MAX_RETRIES = int := 3
params: []
---

Max retries: {{ MAX_RETRIES }}
```

## Caching

```go
cache := pt.NewCache()
defer cache.Close()

tmpl, err := cache.Load("prompts/greeting.tmpl.md")
defer tmpl.Close()
cache.TemplateCount()
cache.Clear()
```

## Declaration Validation

Detect template contract changes at load time:

```go
expected := tmpl.Declarations()

reloaded, _ := pt.FromFile("prompt.tmpl.md")
defer reloaded.Close()
if err := reloaded.ValidateDeclarations(expected); err != nil {
    log.Fatalf("template contract changed: %v", err)
}
```

## Extra Parameters

`AllowingExtra` variants silently ignore undeclared parameters:

```go
result, err := tmpl.RenderMapAllowingExtra(sharedParams)
result, err = tmpl.RenderStructAllowingExtra(sharedStruct)
result, err = tmpl.RenderAllowingExtra(sharedCtx)
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

vs Go's `text/template`, Intel Xeon @ 2.60 GHz, median of 3 runs
([source](prompt_templates_vs_go_test.go)).

**Render only** (pre-compiled template + data → output):

| Scenario   | prompt-templates | Go `text/template` | speedup |
| ---------- | ---------------: | -----------------: | ------: |
| **small**  |    **506 ns** 🏆 |           1,035 ns |    2.0× |
| **medium** |  **1,523 ns** 🏆 |           6,349 ns |    4.2× |
| **large**  | **24,257 ns** 🏆 |         137,271 ns |    5.7× |

**Round-trip** (compile + render):

| Scenario   | prompt-templates | Go `text/template` | speedup |
| ---------- | ---------------: | -----------------: | ------: |
| **small**  |         5,603 ns |           5,802 ns |   ~1.0× |
| **medium** | **19,313 ns** 🏆 |          21,452 ns |   1.11× |
| **large**  | **63,594 ns** 🏆 |         168,819 ns |    2.7× |

**Filters:**

| Filter         | prompt-templates | Go `text/template` | speedup |
| -------------- | ---------------: | -----------------: | ------: |
| **upper**      |    **435 ns** 🏆 |             938 ns |    2.2× |
| **trim+upper** |    **450 ns** 🏆 |           1,297 ns |    2.9× |

```bash
just test-go     # 144 tests
just bench-go    # 24 benchmarks
just lint-go     # go vet
just fmt-go      # gofmt
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
