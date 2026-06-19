# prompt-templates for Go

Strongly-typed prompt templates for LLMs.
Templates are markdown files with YAML frontmatter declaring typed
parameters — every variable, list shape, and enum variant is validated
at render time.

Up to **5.8× faster** than Go's `text/template` with **258× fewer
allocations**.

## Why?

LLM prompts grow complex — multi-shot examples, tool schemas, agentic
workflows — but most Go projects still manage them as inline
`fmt.Sprintf` strings or `text/template` files with no type safety.

**Inline strings** mix prose with code, making prompts unreadable and
hard to review. **`text/template`** provides no type checking: rename a
variable, add a field, change a list shape — you discover it when the
template panics or renders garbage.

`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, not `fmt.Sprintf` strings. They render as clean markdown in any editor or on GitHub — includes are clickable links, and control flow uses blockquote-prefixed lines so it stays visually separated from prose.
- **Strict typing** — every parameter declares a type; mismatches are caught at render time with clear error messages.
- **Agent-safe** — when an LLM writes or edits prompts, the engine catches drift immediately instead of letting it propagate.

## Installation

```bash
go get github.com/domenukk/prompt-templates/go/prompt_templates
```

Build the native library first:

```bash
just build-go-ffi
```

## Quick Start

```go
tmpl, err := pt.FromSource(`---
params:
  - name = str
  - role = str
---
You are {{ role }}. Hello {{ name }}!`)
if err != nil {
    log.Fatal(err)
}
defer tmpl.Close()

// Option 1: Map
result, err := tmpl.RenderMap(map[string]any{
    "name": "Alice",
    "role": "an AI assistant",
})

// Option 2: Struct
type Params struct {
    Name string `json:"name"`
    Role string `json:"role"`
}
result, err = tmpl.RenderStruct(Params{Name: "Alice", Role: "an AI assistant"})

// Option 3: Context (fine-grained control)
ctx := pt.NewContext()
defer ctx.Close()
ctx.SetStr("name", "Alice")
ctx.SetStr("role", "an AI assistant")
result, err = tmpl.Render(ctx)
```

## Typed Lists

```go
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

result, err := tmpl.RenderMap(map[string]any{
    "tasks": []map[string]string{
        {"title": "Write documentation", "priority": "High"},
        {"title": "Add unit tests",      "priority": "Medium"},
    },
})
```

## Enum Dispatch

Typed variants with exhaustiveness checking and field narrowing:

```go
// Option 1: Static typing with TaggedVariant (zero allocations)
type DoneVariant struct {
    pt.TaggedVariant
    Summary string `json:"summary"`
}

result, err := tmpl.RenderStruct(map[string]any{
    "status": DoneVariant{
        TaggedVariant: pt.NewTaggedVariant("Done"),
        Summary:       "All tests pass",
    },
})

// Option 2: Dynamic Variant
result, err = tmpl.RenderMap(map[string]any{
    "status": pt.Variant{
        Kind:   "Done",
        Fields: map[string]any{"summary": "All tests pass"},
    },
})

// Unit variants
ctx.Set("status", pt.Variant{Kind: "Blocked"})
```

### TaggedVariant (static typing)

Embed `TaggedVariant` in Go structs for compile-time typed enum variants:

```go
type Confirmed struct {
    prompt_templates.TaggedVariant
    Evidence string `json:"evidence"`
}

func NewConfirmed(evidence string) Confirmed {
    return Confirmed{
        TaggedVariant: prompt_templates.NewTaggedVariant("Confirmed"),
        Evidence:      evidence,
    }
}
```

### Codegen

`GenerateTypes` produces sealed interfaces and concrete structs with
direct FFI setters for scalar fields — bypassing reflection and
FlexBuffers serialization.

## RenderStruct & MergeStruct

Fields are mapped by `json` tags, falling back to lowercased field name.
Uses FlexBuffers under the hood:

```go
type Params struct {
    Name  string `json:"name"`
    Count int64  `json:"count"`
    Tag   string `json:"tag,omitempty"` // skipped when zero
}

result, err := tmpl.RenderStruct(Params{Name: "Alice", Count: 42})

// Or merge into an existing context:
ctx := tmpl.DefaultsContext()
defer ctx.Close()
ctx.MergeStruct(Params{Name: "Alice", Count: 42})
result, err = tmpl.Render(ctx)
```

## Default Values

```go
tmpl, err := pt.FromSource(`---
params:
  - name = str
  - greeting = str := "Hello"
---
{{ greeting }}, {{ name }}!`)

result, err := tmpl.RenderMap(map[string]any{"name": "Alice"})
// Hello, Alice!

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
{{ idx(item) }}               → 0, 1, 2, … (loop index)
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

| Scenario   | prompt-templates | Go `text/template` |  speedup |
| ---------- | ---------------: | -----------------: | -------: |
| **small**  |           536 ns |             532 ns |    ~1.0× |
| **medium** |  **1,718 ns** 🏆 |           5,919 ns |     3.4× |
| **large**  | **24,238 ns** 🏆 |         141,069 ns | **5.8×** |

**Round-trip** (compile + render):

| Scenario   | prompt-templates | Go `text/template` | speedup |
| ---------- | ---------------: | -----------------: | ------: |
| **small**  |  **5,515 ns** 🏆 |           5,643 ns |   ~1.0× |
| **medium** | **19,436 ns** 🏆 |          21,798 ns |    1.1× |
| **large**  | **57,015 ns** 🏆 |         167,417 ns |    2.9× |

**Filters:**

| Filter         | prompt-templates | Go `text/template` | speedup |
| -------------- | ---------------: | -----------------: | ------: |
| **upper**      |    **489 ns** 🏆 |             959 ns |    2.0× |
| **trim+upper** |    **505 ns** 🏆 |           1,325 ns |    2.6× |

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
