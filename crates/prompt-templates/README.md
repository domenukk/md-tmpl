# prompt-templates

Strongly-typed prompt templates for LLMs — markdown files with YAML frontmatter,
validated at **build time** via proc macros, with a full runtime API for dynamic loading.

```rust
use prompt_templates_macros::include_template;

// Parses and validates the template at build time, generates typed structs + enums.
include_template!("prompts/task_report.tmpl.md");

// Generated types:
//   task_report::Params                  — typed struct
//   task_report::ParamsPriority          — enum(Critical, High, Medium, Low)
//   task_report::ParamsTasksItem         — struct { name, urgency }
//   task_report::ParamsTasksItemUrgency  — enum(Critical, High, Medium, Low)

let params = task_report::Params::builder()
    .title("Deploy v2.0")
    .priority(task_report::ParamsPriority::Critical)
    .tasks([
        task_report::ParamsTasksItem::builder()
            .name("run migrations")
            .urgency(task_report::ParamsTasksItemUrgency::High)
            .build(),
        task_report::ParamsTasksItem::builder()
            .name("update load balancer")
            .urgency(task_report::ParamsTasksItemUrgency::Medium)
            .build(),
    ])
    .build();

let output = params.render().unwrap();
assert!(output.contains("# Task Report: Deploy v2.0"));
assert!(output.contains("Priority: Critical"));
assert!(output.contains("run migrations (High)"));
```

The template behind it — a plain `.tmpl.md` markdown file:

```markdown
---
name: task_report
description: A task report template with types
types:
  - Priority = enum(Critical, High, Medium, Low)

params:
  - title = str
  - priority = Priority
  - tasks = list(name = str, urgency = Priority)
---

# Task Report: {{ title }}

Priority: {{ kind(priority) }}

> {% for task in tasks %}

- {{ task.name }} ({{ kind(task.urgency) }})

> {% /for %}
```

Rename a variant, add a field, remove a param — the compiler catches it
immediately. No runtime surprises.

## Why?

- **Build-time validation** — proc macros parse and validate syntax, types, and variable references at `cargo build`. Typos, missing fields, and type mismatches are build errors. Templates can also be loaded and validated at runtime.
- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor or on GitHub. Compound types use `()` (never `<>`), control-flow tags use `> {% %}` blockquote prefixes.
- **Agent-safe** — when an LLM edits prompts, the compiler catches drift immediately. `validate_template()` enables hot-reload with contract enforcement.

## Installation

```bash
cargo add prompt-templates
# build-time validation + code generation:
cargo add prompt-templates-macros
```

**MSRV:** 1.85 (Rust 2024 edition) · **`no_std`** compatible (disable default `std` feature)

## Build-Time Typed Structs

### `include_template!`

Reads a `.tmpl.md` file at build time, validates it, and generates a typed
module:

```rust
use prompt_templates_macros::include_template;

// Generates: pub mod simple_greeting { pub struct Params { pub name: String } }
include_template!("prompts/simple_greeting.tmpl.md");

let output = simple_greeting::Params { name: "world".into() }.render().unwrap();
assert_eq!(output, "\nHello world!\n");
```

### `template!`

Inline template strings — same validation, no file needed:

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

## `TypedBuilder` Integration

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

## serde Integration

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
  - findings = list(line = int, message = str)
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

For dynamic or scripting use cases, parse templates at runtime.

### `ctx!` Macro

Ergonomic context construction with nested structs and lists:

```rust
use prompt_templates::{ctx, Template};

let tmpl = Template::from_source("
---
params:
  - tasks = list(title = str, priority = str)
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

## Hot-Reload

Load templates from disk at runtime while keeping type safety —
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

## Defaults & Extra Params

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

### Internal Benchmarks (Criterion)

| Operation |   Small |  Medium |   Large |
| --------- | ------: | ------: | ------: |
| render    |  196 ns | 1.11 µs | 22.1 µs |
| parse     | 3.65 µs | 15.3 µs | 30.8 µs |

### vs Competitors

Criterion benchmarks, render only (pre-parsed template + data → output).
([source](../../benchmarks/benches/comparison.rs))

| Scenario        | prompt-templates |    Tera | `MiniJinja` | Handlebars |
| --------------- | ---------------: | ------: | ----------: | ---------: |
| **simple**      |    **130 ns** 🏆 |  213 ns |      558 ns |     632 ns |
| **loop**        |    **445 ns** 🏆 |  618 ns |     2.00 µs |    2.85 µs |
| **conditional** |    **173 ns** 🏆 |  348 ns |      625 ns |    1.16 µs |
| **hero**        |   **2.07 µs** 🏆 | 2.09 µs |     7.62 µs |    21.4 µs |
| **mega**        |   **10.1 µs** 🏆 | 11.1 µs |     30.1 µs |    84.7 µs |

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
