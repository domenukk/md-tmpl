# prompt-templates

[![CI](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml/badge.svg)](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/prompt-templates.svg)](https://crates.io/crates/prompt-templates)
[![docs.rs](https://docs.rs/prompt-templates/badge.svg)](https://docs.rs/prompt-templates)

Strongly-typed prompt templates for LLMs.
Templates are markdown files with YAML frontmatter declaring typed
parameters — every variable, list shape, and enum variant is validated
at **compile time** via proc macros.

## Why?

LLM prompts grow complex — multi-shot examples, tool schemas, agentic
workflows — but most Rust projects still manage them as inline
`format!()` strings or untyped Tera/Handlebars templates.

**Inline strings** mix prose with code, making prompts unreadable and
hard to review. **Untyped template engines** push every error to
runtime: rename a variable, add a field, change a list shape — you
discover it when the prompt renders garbage in production.

`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, not `format!()` strings. They render as clean markdown in any editor or on GitHub — includes are clickable links, and control flow uses blockquote-prefixed lines so it stays visually separated from prose.
- **Compile-time safety** — proc macros validate syntax, types, and variable references at `cargo build`. No runtime surprises.
- **Agent-safe** — when an LLM writes or edits prompts, the compiler catches drift immediately instead of letting it propagate.

## Installation

```bash
cargo add prompt-templates
# optional: compile-time validation + code generation
cargo add prompt-templates-macros
```

## Quick Start

A `.tmpl.md` file is markdown with typed parameters:

```markdown
---
params:
  - name = str
---

Hello {{ name }}!
```

Validate at compile time and generate a typed struct:

```rust
use prompt_templates_macros::include_template;

// Generates: pub mod simple_greeting { pub struct Params { ... } }
include_template!("prompts/simple_greeting.tmpl.md");

let output = simple_greeting::Params { name: "world".into() }.render().unwrap();
assert_eq!(output, "\nHello world!\n");
```

Or parse at runtime with `ctx!`:

```rust
use prompt_templates::{ctx, Template};

let tmpl = Template::from_source("
---
params:
  - tasks = list<title = str, priority = str>
---

> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority }})

> {% /for %}"
).unwrap();

let output = tmpl.render(&ctx! {
    tasks: [
        { title: "Write documentation", priority: "High" },
        { title: "Add unit tests",      priority: "Medium" },
    ]
}).unwrap();

assert_eq!(output, "- **Write documentation** (High)\n- **Add unit tests** (Medium)\n");
```

Syntax errors, unknown variables, and type mismatches are caught at
`cargo build` — not at runtime.

## Compile-Time Safety

The companion crate `prompt-templates-macros` validates templates at
`cargo build` time. Syntax errors, unknown variables, and type mismatches
become compile errors.

### `include_template!`

Generates a module with a pre-compiled template and typed parameter struct:

```rust
use prompt_templates_macros::include_template;

// Emits: pub mod simple_greeting { pub fn template() -> ...; pub struct Params { ... } }
include_template!("prompts/simple_greeting.tmpl.md");

let output = simple_greeting::Params { name: "world".into() }.render().unwrap();
```

### `template!`

Like `include_template!`, but for inline template strings. The
`=> module_name` is required.

```rust
prompt_templates_macros::template!(r#"
---
params:
  - name = str
---
Hello {{ name }}!
"# => greeting);

let output = greeting::Params { name: "World".into() }
    .render()
    .unwrap();
assert_eq!(output, "Hello World!\n");
```

### Hot-Reload with Contract Validation

Load templates from disk at runtime, validate against the compiled struct:

```rust
# prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
let tmpl = prompt_templates::Template::from_file(
    std::path::Path::new("prompts/greeting.tmpl.md")
).unwrap();
greeting::Params::validate_template(&tmpl).unwrap();

let output = greeting::Params {
    name: "Bob".into(),
    count: 1,
    items: vec![],
}.render_with(&tmpl).unwrap();
```

### Runtime Loading

```rust
use prompt_templates::load_template;

let tmpl = load_template(std::path::Path::new("prompts"), "simple_greeting").unwrap();

let mut ctx = prompt_templates::Context::new();
ctx.set("name", "world");
let output = tmpl.render(&ctx).unwrap();
assert!(output.contains("Hello world!"));
```

## `ctx!` Macro

Ergonomic context construction with nested structs and lists:

```rust
use prompt_templates::{ctx, Template};

let tmpl = Template::from_source("
---
params:
  - tasks = list<title = str, priority = str>
---

> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority }})

> {% /for %}"
).unwrap();

let output = tmpl.render(&ctx! {
    tasks: [
        { title: "Write documentation", priority: "High" },
        { title: "Add unit tests",      priority: "Medium" },
    ]
}).unwrap();
```

## Caching

`TemplateCache` hashes file contents — unchanged files return cached
compilations. `render_cached()` extends this to included templates:

```rust
use prompt_templates::TemplateCache;

let dir = tempfile::tempdir().unwrap();
let path = dir.path().join("greeting.tmpl.md");
std::fs::write(&path, "\
---
params:
  - name = str
---
Hello {{ name }}!"
).unwrap();

let cache = TemplateCache::new();
let tmpl = cache.load(&path).unwrap();

let mut ctx = prompt_templates::Context::new();
ctx.set("name", "world");
let output = tmpl.render_cached(&ctx, &cache).unwrap();
assert_eq!(output, "Hello world!");
```

## `TypedBuilder` Integration

Enable `typed-builder` for compile-time-checked builder patterns:

```bash
cargo add prompt-templates --features typed-builder
cargo add prompt-templates-macros --features typed-builder
```

```rust
# prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
let params = greeting::Params::builder()
    .name("Alice")       // setter(into): accepts &str or String
    .count(42)
    .build();            // `items` defaults to vec![]
```

| Field type                     | Builder behaviour                                                     |
| ------------------------------ | --------------------------------------------------------------------- |
| `String`                       | `setter(into)` — accepts `&str`, `String`, or anything `Into<String>` |
| `Vec<…>`                       | `default` — omit the field to get an empty `Vec`                      |
| Scalars (`i64`, `f64`, `bool`) | Required                                                              |

Sub-structs also derive `TypedBuilder`:

```rust
# prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
let item = greeting::ParamsItemsItem::builder()
    .label("write docs")
    .build();

let params = greeting::Params::builder()
    .name("Alice")
    .count(1)
    .items(vec![item])
    .build();
```

## Extra Parameters and Defaults

### `render_allowing_extra()`

Extra context keys not declared in frontmatter are silently ignored:

```rust
use prompt_templates::{ctx, Template};

let tmpl = Template::from_source("\
---
params:
  - name = str
---
Hello {{ name }}!"
).unwrap();
let ctx = ctx! { name: "world", extra_key: "ignored" };
assert_eq!(tmpl.render_allowing_extra(&ctx).unwrap(), "Hello world!");
```

### `defaults_context()`

Returns a `Context` pre-filled with default values:

```rust
use prompt_templates::Template;

let tmpl = Template::from_source("\
---
params:
  - name = str
  - count = int := 5
---
{{ name }} ({{ count }})"
).unwrap();
let mut ctx = tmpl.defaults_context();
ctx.set("name", "Alice"); // count already has default 5
assert_eq!(tmpl.render(&ctx).unwrap(), "Alice (5)");
```

## `serde` Integration

Render directly from any `Serialize` struct:

```bash
cargo add prompt-templates --features serde
```

```rust
use prompt_templates::Template;
use serde::Serialize;

#[derive(Serialize)]
struct Data { name: String, count: i64 }

let tmpl = Template::from_source("\
---
params: [name = str, count = int]
---
{{ name }} has {{ count }} items"
).unwrap();
let output = tmpl.render_serde(&Data { name: "Alice".into(), count: 3 }).unwrap();
assert_eq!(output, "Alice has 3 items");
```

## Performance

Criterion benchmarks, render only (pre-compiled template + data → output).
([source](../../benchmarks/benches/comparison.rs))

| Scenario        | prompt-templates |     Tera | `MiniJinja` | Handlebars |
| --------------- | ---------------: | -------: | ----------: | ---------: |
| **simple**      |    **118 ns** 🏆 |   219 ns |      551 ns |     619 ns |
| **loop**        |    **433 ns** 🏆 |   817 ns |     1.91 µs |    2.84 µs |
| **conditional** |    **165 ns** 🏆 |   351 ns |      621 ns |    1.18 µs |
| **hero**        |   **1.99 µs** 🏆 |  2.33 µs |     7.83 µs |    21.1 µs |
| **mega**        |   **9.66 µs** 🏆 | 10.80 µs |     28.4 µs |    83.1 µs |

_Intel Xeon @ 2.60 GHz, 100 samples._

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax — control-flow
tags, filters, built-in functions, whitespace control, and error
diagnostics.

## License

Apache-2.0 OR MIT
