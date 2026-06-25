# prompt-templates

[![CI](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml/badge.svg)](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml)

**Strongly-typed prompt templates for LLMs** — write prompts as markdown,
validate parameters at compile time.

Templates are `.tmpl.md` files with YAML frontmatter declaring typed
parameters. Every variable, list shape, and enum variant is validated
before the prompt is rendered — at **compile time** in Rust, or at
render time in Python, Go, and TypeScript.

## Quick Start

A prompt template is just markdown with a typed header:

```markdown
---
params:
  - name = str
---

Hello {{ name }}!
```

Render it from any supported language:

<!-- prettier-ignore -->
````carousel
```rust
use prompt_templates_macros::include_template;

include_template!("prompts/simple_greeting.tmpl.md");

let output = simple_greeting::Params { name: "world".into() }
    .render().unwrap();
// → "Hello world!"
```
<!-- slide -->
```python
from prompt_templates import Template

tmpl = Template.from_source("""---
params:
  - name = str
---
Hello {{ name }}!""")
print(tmpl.render(name="world"))  # → "Hello world!"
```
<!-- slide -->
```go
tmpl, _ := pt.FromSource(`---
params:
  - name = str
---
Hello {{ name }}!`)
defer tmpl.Close()
result, _ := tmpl.RenderMap(map[string]any{"name": "world"})
```
<!-- slide -->
```typescript
import { Template } from "prompt-templates";

const tmpl = Template.fromSource(`---
params:
  - name = str
---
Hello {{ name }}!`);
console.log(tmpl.render({ name: "world" }));
```
````

## Why?

Inline format strings are unreadable; untyped templates break at runtime.

| Problem                       | Solution                                                                    |
| ----------------------------- | --------------------------------------------------------------------------- |
| Prompts buried in code        | **Markdown-native** — `.tmpl.md` files render in any editor or on GitHub    |
| Errors found in production    | **Strict typing** — mismatches caught at compile time (Rust) or render time |
| LLMs drift templates silently | **Agent-safe** — the engine catches drift immediately                       |

## Features

| Feature                    | Description                                                                              |
| -------------------------- | ---------------------------------------------------------------------------------------- |
| **Typed parameters**       | `str`, `int`, `float`, `bool`, `list<…>`, `struct<…>`, `enum<…>`, `option<…>`, `tmpl<…>` |
| **Type aliases**           | `types:` defines reusable named types                                                    |
| **Cross-template imports** | `imports:` pulls types via dotted paths (`stem.TypeName`)                                |
| **Typed lists**            | `list<title = str, score = int>` — iterate with `{% for %}`, fields validated            |
| **Enum dispatch**          | `match`/`case` with exhaustiveness checking and field narrowing                          |
| **Includes as links**      | `{% include [name](path.tmpl.md) with … %}` — clickable, type-checked                    |
| **Inline templates**       | `{% tmpl name %}` — reusable fragments without separate files                            |
| **Constants**              | `consts:` for file-scoped immutable values                                               |
| **Built-in functions**     | `idx(b)`, `len(x)`, `kind(x)`, `has(x)` + filters (`upper`, `lower`, `trim`, `join`, …)  |
| **Readable as markdown**   | `> {% %}` blockquote prefix keeps control flow visually separated from prose             |

> **Note:** The `> ` prefix is required only on `{% %}` tag lines — it is
> stripped before compilation. Content lines between tags are normal text
> and should not use `> `. A content line starting with `> ` is kept
> verbatim as a literal markdown blockquote in the output.

## Examples

### Typed Lists

```markdown
---
params:
  - tasks = list<title = str, priority = str>
---

> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority }})

  > {% /for %}
```

### Enum Dispatch

Declare enum variants with optional typed fields, then dispatch with
`match`/`case` — the engine enforces exhaustiveness and narrows fields
per variant:

<!-- prettier-ignore -->
```markdown
---
params:
  - status = enum<Done(summary = str), InProgress, Blocked>
---

> {% match status %}
> {% case Done %}

✅ Completed: {{ status.summary }}

> {% case InProgress %}

🔄 Still in progress.

> {% case Blocked %}

❌ Blocked.

> {% /match %}
```

Accessing `status.summary` outside `{% case Done %}` is a **compile
error** — the type system narrows fields per variant.

### Includes and Inline Templates

```markdown
> {% include [task_card](task_card.tmpl.md) with title=task.title %}
> {% include [row](row.tmpl.md) for item in items %}
```

Define reusable fragments inline with `{% tmpl %}`:

```markdown
> {% tmpl task_row %}

---

params:

- title = str
- priority = str

---

- **{{ title }}** ({{ priority }})

  > {% /tmpl %}

> {% for task in tasks %}
> {% include task_row with title=task.title, priority=task.priority %}
> {% /for %}
```

### Type Aliases

```markdown
---
types:
  - Category = enum<Labelled(label = str), Unlabelled>
params:
  - tasks = list<title = str, category = Category>
  - components = list<name = str, category = Category>
---
```

### Cross-Template Imports

```markdown
---
imports:
  - "[shared_types](shared_types.tmpl.md)"
params:
  - items = shared_types.items
  - priority = shared_types.Priority
---
```

### Constants

```markdown
---
consts:
  - NOTEBOOK_FILENAME = str := "thought_process.md"
  - MAX_RETRIES = int := 3
params:
  - name = str
---

{{ name }}'s notebook: {{ NOTEBOOK_FILENAME }} (max {{ MAX_RETRIES }} retries)
```

## Performance

Built for speed with zero-allocation rendering (in Rust). Approximate benchmark results (render-only):

| Language                                | simple         | loop           | conditional    | hero            |
| --------------------------------------- | -------------- | -------------- | -------------- | --------------- |
| **Rust** (vs Tera/MiniJinja/Handlebars) | **119 ns** 🏆  | **402 ns** 🏆  | **196 ns** 🏆  | **2.08 µs** 🏆  |
| **Python** (vs Jinja2/Mako)             | **0.94 µs** 🏆 | **2.63 µs** 🏆 | **1.07 µs** 🏆 | 23.95 µs        |
| **Go** (vs `text/template`)             | 536 ns         | **1.71 µs** 🏆 | N/A            | **24.23 µs** 🏆 |
| **TypeScript** (vs Handlebars/Mustache) | **690 ns** 🏆  | 6.30 µs        | 1.21 µs        | N/A             |

See the language-specific READMEs or the [benchmarks suite](benchmarks/README.md) for full details, compile-time overhead, and methodology.

## Language Bindings

### Rust ([README](crates/prompt-templates/README.md))

[![crates.io](https://img.shields.io/crates/v/prompt-templates.svg)](https://crates.io/crates/prompt-templates)
[![docs.rs](https://docs.rs/prompt-templates/badge.svg)](https://docs.rs/prompt-templates)

Compile-time validation via proc macros. Pre-parsed templates with zero-overhead rendering. Full API includes `ctx!` macro, `TypedBuilder`, `serde`, hot-reload, caching, and benchmarks.

### Python ([README](crates/prompt-templates-python/README.md))

Includes generated types, import hooks, pattern matching, enum construction, caching, and benchmarks.

### Go ([README](go/prompt_templates/README.md))

Includes `RenderStruct`, `TaggedVariant`, codegen, caching, and benchmarks.

### TypeScript ([README](crates/prompt-templates-typescript/README.md))

Includes `TypedTemplate<P>`, `defineVariants`, codegen, WASM backend, and benchmarks.

### WASM ([README](crates/prompt-templates-wasm/README.md))

WebAssembly bindings wrapping the full Rust engine via `wasm-bindgen`. Same `ITemplate` interface as the pure-TypeScript package.

## Full Reference

See **[SPEC.md](SPEC.md)** for the complete syntax — all control-flow
tags, filters, built-in functions, whitespace control, and error
diagnostics.

## License

Apache-2.0 OR MIT
