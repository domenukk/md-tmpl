# prompt-templates

[![CI](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml/badge.svg)](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/prompt-templates.svg)](https://crates.io/crates/prompt-templates)
[![docs.rs](https://docs.rs/prompt-templates/badge.svg)](https://docs.rs/prompt-templates)

A **strongly-typed** template engine for LLM prompts, designed for
markdown. Close to Jinja2 in spirit, but purpose-built for prompts.

<!-- prettier-ignore -->
```markdown
---
params:
  - reviewer = str
  - items = list<
      file = str,
      status = enum<
        Approved,
        NeedsChanges(reason = str),
        Skipped,
      >,
    >
---

# Code Review — {{ reviewer }}

> {% include [guidelines](review_guidelines.tmpl.md) %}

> {% for item in items %}

- `{{ item.file }}`
  > {% match item.status %}
  > {% case Approved %}
  ✅ Approved.
  > {% case NeedsChanges %}
  ⚠️ Needs changes: {{ item.status.reason }}
  > {% case Skipped %}
  ⏭️ Skipped.
  > {% /match %}
  > {% /for %}
```

## Why?

LLM prompts grow complex fast — multi-shot examples, tool descriptions,
agentic workflows — but most teams still manage them as inline
`format!()` strings or untyped Jinja/Handlebars templates.

**Inline strings** mix prose with code, making prompts hard to read,
review, and iterate on. **Untyped template engines** push every error to
runtime: rename a variable, add a field, change a list shape — you won't
know it's broken until the prompt renders garbage in production.

`prompt-templates` gives you **separation of concerns** (prompts live in
`.tmpl.md` files that render as readable markdown in any editor or GitHub
preview) with **strict typing** (every parameter declares a type and
mismatches are caught at compile time). Templates fail fast and
loud — which is especially valuable when an LLM agent is writing or
editing your prompts, because the compiler catches drift immediately
instead of letting it propagate silently.

| Feature                      | Why it matters                                                                     |
| ---------------------------- | ---------------------------------------------------------------------------------- |
| **Strongly typed**           | Every parameter declares a type (`str`, `int`, `list<…>`, `dict<…>`, `enum<…>`).   |
| **Typed lists**              | `list<title = str, score = int>` — iterate with `{% for %}`, fields are validated. |
| **Enum dispatch**            | `match`/`case` on typed variants with exhaustiveness checking and field narrowing. |
| **Includes as links**        | `{% include [name](path.tmpl.md) with … %}` — clickable, type-checked parameters.  |
| **Inline templates**         | `{% tmpl name %}` — reusable fragments without separate files.                     |
| **Compile-time safety**      | Proc macros validate syntax, types, and variable references at `cargo build`.      |
| **Zero-overhead rendering**  | `include_template!` pre-parses at compile time; `TemplateCache` deduplicates I/O.  |
| **Readable as raw markdown** | `> {% %}` blockquote prefix keeps control flow visually separated from prose.      |
| **Hot-reload safe**          | Reload templates at runtime; struct validation catches contract drift.             |

## Installation

```bash
cargo add prompt-templates
# optional: compile-time validation
cargo add prompt-templates-macros
```

## Quick Start

A `.tmpl.md` file is just markdown with YAML frontmatter declaring typed
parameters:

```markdown
---
params:
  - name = str
---

Hello {{ name }}!
```

Generate a typed Rust struct and pre-validate the template at compile time:

```rust
use prompt_templates_macros::{include_template, template_params_struct};

// Generates: struct GreetingParams { pub name: String }
template_params_struct!("prompts/simple_greeting.tmpl.md" => GreetingParams);

let tmpl = include_template!("prompts/simple_greeting.tmpl.md");
let output = GreetingParams { name: "world".into() }.render(&tmpl).unwrap();

assert_eq!(output, "\nHello world!\n");
```

Syntax errors, unknown variables, and type mismatches are all caught at
`cargo build` — not at runtime.

## Typed Lists with `{% for %}`

Declare list fields with types — the engine validates every item:

```markdown
---
params:
  - tasks = list<title = str, priority = str>
---

> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority }})
  > {% /for %}
```

```rust
use prompt_templates::{Context, Template};

let tmpl = Template::from_source("
---
params:
  - tasks = list<title = str, priority = str>
---
> {% for task in tasks %}
- **{{ task.title }}** ({{ task.priority }})
> {% /for %}").unwrap();

let mut ctx = Context::new();
ctx.set("tasks", vec![
    prompt_templates::Value::dict([
        ("title", "Write documentation"),
        ("priority", "High"),
    ]),
    prompt_templates::Value::dict([
        ("title", "Add unit tests"),
        ("priority", "Medium"),
    ]),
]);

assert_eq!(tmpl.render(&ctx).unwrap(),
    "- **Write documentation** (High)\n- **Add unit tests** (Medium)\n");
```

## Enum Dispatch with `match`/`case`

Declare enum variants (with optional typed fields), dispatch with
`match`/`case`, and get **compile-time exhaustiveness checking** and
**field narrowing**:

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

Accessing `status.summary` outside `{% case Done %}` is a
**compile error** — the type system narrows fields per variant.

## Includes and Inline Templates

Include other `.tmpl.md` files with explicit, type-checked parameter
passing. The path is a markdown link — click it in any editor to jump
to the file:

```markdown
> {% include [task_card](task_card.tmpl.md) with title=task.title %}
> {% include [row](row.tmpl.md) for item in items %}
```

Define reusable fragments inline without separate files using `{% tmpl %}`:

```markdown
## {% tmpl task_row %}

## params:

## - title = str

## - priority = str

- **{{ title }}** ({{ priority }})
  {% /tmpl %}

> {% for task in tasks %}
> {% include task_row with title=task.title, priority=task.priority %}
> {% /for %}
```

## The `ctx!` Macro

For ergonomic context construction with nested dicts and lists, use the
`ctx!` macro instead of manual `Context::set` calls:

```rust
use prompt_templates::{ctx, Template};

let tmpl = Template::from_source("
---
params:
  - tasks = list<title = str, priority = str>
---
> {% for task in tasks %}
- **{{ task.title }}** ({{ task.priority }})
> {% /for %}").unwrap();

let output = tmpl.render(&ctx! {
    tasks: [
        { title: "Write documentation", priority: "High" },
        { title: "Add unit tests",      priority: "Medium" },
    ]
}).unwrap();

assert_eq!(output, "- **Write documentation** (High)\n- **Add unit tests** (Medium)\n");
```

## Compile-Time Safety

The companion crate `prompt-templates-macros` validates templates at
`cargo build` time. Syntax errors, unknown variables, and type mismatches
become compile errors — not runtime surprises.

### `include_template!` — zero-cost pre-parsed templates

```rust
use prompt_templates_macros::include_template;

let tmpl = include_template!("prompts/simple_greeting.tmpl.md");

let mut ctx = prompt_templates::Context::new();
ctx.set("name", "world");
let output = tmpl.render(&ctx).unwrap();
```

### `template_params_struct!` — generated typed structs

Generates a Rust struct from the template's frontmatter. Fields, enums,
and nested structs are all derived from the `.tmpl.md` file:

```rust
prompt_templates_macros::template_params_struct!(
    "prompts/greeting.tmpl.md" => GreetingParams
);

// Generates:
//   struct GreetingParams { pub name: String, pub count: i64, pub items: Vec<…> }
//   impl GreetingParams { fn render(&self, tmpl: &Template) -> Result<String, …> }

let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
let output = GreetingParams {
    name: "Alice".into(),
    count: 42,
    items: vec![GreetingParamsItemsItem { label: "hello".into() }],
}.render(&tmpl).unwrap();
```

### Hot-reload with contract validation

Load templates from disk at runtime, but validate they haven't diverged
from the compiled struct:

```rust
# prompt_templates_macros::template_params_struct!("prompts/greeting.tmpl.md" => GreetingParams);
let tmpl = prompt_templates::Template::from_file(
    std::path::Path::new("prompts/greeting.tmpl.md")
).unwrap();
GreetingParams::validate_template(&tmpl).unwrap();

let output = GreetingParams {
    name: "Bob".into(),
    count: 1,
    items: vec![],
}.render(&tmpl).unwrap();
```

## Caching

`TemplateCache` hashes file contents — unchanged files return cached
compilations with zero re-parsing. `render_cached()` extends this to
included templates:

```rust
use prompt_templates::TemplateCache;

let dir = tempfile::tempdir().unwrap();
let path = dir.path().join("greeting.tmpl.md");
std::fs::write(&path, "---\nparams:\n  - name = str\n---\nHello {{ name }}!").unwrap();

let cache = TemplateCache::new();
let tmpl = cache.load(&path).unwrap();

let mut ctx = prompt_templates::Context::new();
ctx.set("name", "world");
let output = tmpl.render_cached(&ctx, &cache).unwrap();
assert_eq!(output, "Hello world!");
```

## Full Reference

For the complete syntax reference — all control-flow tags, filters,
built-in functions, whitespace control, error diagnostics, and more —
see **[SPEC.md](SPEC.md)**.
