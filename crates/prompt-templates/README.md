# prompt-templates

[![CI](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml/badge.svg)](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/prompt-templates.svg)](https://crates.io/crates/prompt-templates)
[![docs.rs](https://docs.rs/prompt-templates/badge.svg)](https://docs.rs/prompt-templates)

Strongly-typed prompt templates for LLMs.
Templates are markdown files with YAML frontmatter declaring typed
parameters — every variable, list shape, and enum variant is validated
at **compile time** via proc macros.

**MSRV:** 1.85 (Rust 2024 edition) · **`no_std`** compatible (disable default `std` feature)

## Why?

Inline `format!()` strings are unreadable. Untyped Tera/Handlebars templates break at runtime.
`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor or on GitHub.
- **Compile-time safety** — proc macros validate syntax, types, and variable references at `cargo build`.
- **Agent-safe** — when an LLM edits prompts, the compiler catches drift immediately.

## Installation

```bash
cargo add prompt-templates
# compile-time validation + code generation:
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

### Compile-Time Typed Structs

`include_template!` validates the template at `cargo build` and generates
a typed struct — wrong types, missing fields, and unknown variables are
compile errors:

```rust
use prompt_templates_macros::include_template;

// Generates: pub mod simple_greeting { pub struct Params { pub name: String } }
include_template!("prompts/simple_greeting.tmpl.md");

let output = simple_greeting::Params { name: "world".into() }.render().unwrap();
assert_eq!(output, "\nHello world!\n");
```

Use `template!` for inline template strings:

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

### `TypedBuilder` Integration

Enable `typed-builder` for ergonomic builder patterns:

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

let output = params.render().unwrap();
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

### `serde` Integration

Render directly from any `Serialize` struct:

```bash
cargo add prompt-templates --features serde
```

```rust
use prompt_templates::Template;
use serde::Serialize;

#[derive(Serialize)]
struct ReviewParams {
    file_path: String,
    severity: String,
    findings: Vec<Finding>,
}

#[derive(Serialize)]
struct Finding { line: i64, message: String }

let tmpl = Template::from_source("\
---
params:
  - file_path = str
  - severity = str
  - findings = list<line = int, message = str>
---

# Code Review: {{ file_path }}

Severity: {{ severity }}

> {% for finding in findings %}

- Line {{ finding.line }}: {{ finding.message }}

  > {% /for %}"
).unwrap();

let output = tmpl.render(&ReviewParams {
    file_path: "main.rs".into(),
    severity: "high".into(),
    findings: vec![
        Finding { line: 42, message: "unused variable".into() },
    ],
}).unwrap();
```

## Runtime API

For dynamic or scripting use cases, parse templates at runtime:

### `ctx!` Macro

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

let output = tmpl.render_ctx(&ctx! {
    tasks: [
        { title: "Write documentation", priority: "High" },
        { title: "Add unit tests",      priority: "Medium" },
    ]
}).unwrap();

assert_eq!(output, "- **Write documentation** (High)\n- **Add unit tests** (Medium)\n");
```

### Runtime Loading

```rust
use prompt_templates::load_template;

let tmpl = load_template(std::path::Path::new("prompts"), "simple_greeting").unwrap();

let mut ctx = prompt_templates::Context::new();
ctx.set("name", "world");
let output = tmpl.render_ctx(&ctx).unwrap();
assert!(output.contains("Hello world!"));
```

## Hot-Reload with Contract Validation

Load templates from disk at runtime while keeping compile-time type safety —
iterate on prompt wording without recompiling:

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
}.render_reloaded(&tmpl).unwrap();
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
let output = tmpl.render_ctx_cached(&ctx, &cache).unwrap();
assert_eq!(output, "Hello world!");
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
assert_eq!(tmpl.render_ctx_allowing_extra(&ctx).unwrap(), "Hello world!");
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
assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Alice (5)");
```

## Performance

Criterion benchmarks, render only (pre-compiled template + data → output).
([source](../../benchmarks/benches/comparison.rs))

| Scenario        | prompt-templates |     Tera | `MiniJinja` | Handlebars |
| --------------- | ---------------: | -------: | ----------: | ---------: |
| **simple**      |    **149 ns** 🏆 |   279 ns |      676 ns |     815 ns |
| **loop**        |    **519 ns** 🏆 |   671 ns |     2.41 µs |    2.88 µs |
| **conditional** |    **208 ns** 🏆 |   421 ns |      704 ns |    1.42 µs |
| **hero**        |       ~2.4 µs    | ~2.7 µs  |     9.16 µs |    26.8 µs |
| **mega**        |       ~11 µs     | ~13 µs   |     34.9 µs |    99.6 µs |

_Intel Xeon @ 2.60 GHz, 3 runs × 100 Criterion samples.
Hero/mega margins are small — treat as comparable to Tera._

```bash
just bench-rust          # run Criterion benchmarks
just bench-update-rust   # run + update this table
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax — control-flow
tags, filters, built-in functions, whitespace control, and error
diagnostics.

## License

Apache-2.0 OR MIT
