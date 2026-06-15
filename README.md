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

| Feature                      | Why it matters                                                                              |
| ---------------------------- | ------------------------------------------------------------------------------------------- |
| **Strongly typed**           | Every parameter declares a type (`str`, `int`, `list<…>`, `dict<…>`, `enum<…>`, `tmpl<…>`). |
| **Type aliases**             | `types:` block defines reusable named types; avoid repeating complex declarations.          |
| **Cross-template imports**   | `imports:` pulls types from other templates via dotted paths (`stem.TypeName`).             |
| **Typed lists**              | `list<title = str, score = int>` — iterate with `{% for %}`, fields are validated.          |
| **Enum dispatch**            | `match`/`case` on typed variants with exhaustiveness checking and field narrowing.          |
| **Includes as links**        | `{% include [name](path.tmpl.md) with … %}` — clickable, type-checked parameters.           |
| **Inline templates**         | `{% tmpl name %}` — reusable fragments without separate files.                              |
| **Compile-time safety**      | Proc macros validate syntax, types, and variable references at `cargo build`.               |
| **Zero-overhead rendering**  | `include_template!` pre-parses at compile time; `TemplateCache` deduplicates I/O.           |
| **Readable as raw markdown** | `> {% %}` blockquote prefix keeps control flow visually separated from prose.               |
| **Hot-reload safe**          | Reload templates at runtime; struct validation catches contract drift.                      |

> **Note:** The `> ` prefix is required only on the `{% %}` tag lines
> themselves — it is stripped before compilation. Content lines between tags
> (prose, `{{ }}` expressions) are **normal text** and should **not** use
> `> `. If a content line starts with `> `, it is kept verbatim as a literal
> markdown blockquote in the output.

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
use prompt_templates_macros::{include_template, include_types};

// Generates: mod simple_greeting { pub struct Params { pub name: String } }
include_types!("prompts/simple_greeting.tmpl.md");

let tmpl = include_template!("prompts/simple_greeting.tmpl.md");
let output = simple_greeting::Params { name: "world".into() }.render(&tmpl).unwrap();

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

## Type Aliases

Define reusable named types with the `types:` block to avoid repeating
complex type declarations:

<!-- prettier-ignore -->
```markdown
---
types:
  - Labelled = enum<Known(label = str), Unknown>
params:
  - bugs = list<title = str, vuln_type = Labelled>
  - components = list<name = str, category = Labelled>
---

> {% for bug in bugs %}
- **{{ bug.title }}**
  > {% match bug.vuln_type %}
  > {% case Known %}
  Known: {{ bug.vuln_type.label }}
  > {% case Unknown %}
  Unknown vulnerability.
  > {% /match %}
> {% /for %}
```

The `Labelled` type alias is defined once and used across both `bugs`
and `components` params. See [SPEC.md](SPEC.md) for full details.

## Cross-Template Imports

Import types from other templates with the `imports:` block:

```markdown
---
imports:
  - "[shared_types](shared_types.tmpl.md)"
params:
  - items = shared_types.items
  - priority = shared_types.Priority
---
```

The import stem must match the filename without `.tmpl.md`. Types are
referenced via dotted path: `stem.TypeName`. See [SPEC.md](SPEC.md).

## Constants

Declare file-scoped constants with `consts:` — available everywhere in
the template body without explicit passing:

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

Constants are inherited by inline `{% tmpl %}` blocks and accessible
from importing templates via `stem.CONST_NAME`. See [SPEC.md](SPEC.md).

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

### `validate_template!` — compile-time-only validation

Runs the same checks as `include_template!` but produces no runtime value.
Useful for static assertions in tests or build scripts:

```rust
// Fails to compile if the template has errors.
prompt_templates_macros::validate_template!("prompts/simple_greeting.tmpl.md");
```

### `template_params_struct!` — custom-named struct

Like `include_types!`, but emits the struct directly into the calling
scope with a caller-chosen name (no wrapping module):

```rust
prompt_templates_macros::template_params_struct!(
    "prompts/greeting.tmpl.md" => Greeting
);

let params = Greeting { name: "Alice".into(), count: 42, items: vec![] };
```

### `include_types!` — generated typed structs

Generates a Rust struct from the template's frontmatter. Fields, enums,
and nested structs are all derived from the `.tmpl.md` file:

```rust
prompt_templates_macros::include_types!(
    "prompts/greeting.tmpl.md"
);

// Generates:
//   mod greeting { pub struct Params { pub name: String, pub count: i64, pub items: Vec<…> } }
//   impl greeting::Params { fn render(&self, tmpl: &Template) -> Result<String, …> }

let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
let output = greeting::Params {
    name: "Alice".into(),
    count: 42,
    items: vec![greeting::ParamsItemsItem { label: "hello".into() }],
}.render(&tmpl).unwrap();
```

### Hot-reload with contract validation

Load templates from disk at runtime, but validate they haven't diverged
from the compiled struct:

```rust
# prompt_templates_macros::include_types!("prompts/greeting.tmpl.md");
let tmpl = prompt_templates::Template::from_file(
    std::path::Path::new("prompts/greeting.tmpl.md")
).unwrap();
greeting::Params::validate_template(&tmpl).unwrap();

let output = greeting::Params {
    name: "Bob".into(),
    count: 1,
    items: vec![],
}.render(&tmpl).unwrap();
```

### Runtime loading — `load_template`

Load templates by name from a directory at runtime:

```rust
use prompt_templates::load_template;

let tmpl = load_template(std::path::Path::new("prompts"), "simple_greeting").unwrap();
// Looks for prompts/simple_greeting.tmpl.md

let mut ctx = prompt_templates::Context::new();
ctx.set("name", "world");
let output = tmpl.render(&ctx).unwrap();
assert!(output.contains("Hello world!"));
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

## `TypedBuilder` Integration

Enable the optional `typed-builder` feature to derive
[`TypedBuilder`](https://docs.rs/typed-builder) on every generated
parameter struct. This gives you a compile-time-checked builder pattern
instead of manual field construction.

### Enable the feature

```bash
cargo add prompt-templates --features typed-builder
cargo add prompt-templates-macros --features typed-builder
```

Or in `Cargo.toml`:

```toml
[dependencies]
prompt-templates = { version = "0.1", features = ["typed-builder"] }
prompt-templates-macros = { version = "0.1", features = ["typed-builder"] }
```

### Before & after

**Without `typed-builder`** — every field must be set manually:

```rust
# prompt_templates_macros::include_types!("prompts/greeting.tmpl.md");
let params = greeting::Params {
    name: "Alice".into(),
    count: 42,
    items: vec![],  // must always specify, even when empty
};
```

**With `typed-builder`** — use the builder pattern with ergonomic
setters:

```rust
# prompt_templates_macros::include_types!("prompts/greeting.tmpl.md");
let params = greeting::Params::builder()
    .name("Alice")       // setter(into): accepts &str or String
    .count(42)
    .build();            // `items` defaults to vec![]
```

### Field-level behaviour

| Field type                     | Builder behaviour                                                     |
| ------------------------------ | --------------------------------------------------------------------- |
| `String`                       | `setter(into)` — accepts `&str`, `String`, or anything `Into<String>` |
| `Vec<…>`                       | `default` — omit the field to get an empty `Vec`                      |
| Scalars (`i64`, `f64`, `bool`) | Required — must be set explicitly                                     |

### Sub-struct builders

Generated sub-structs (for `list<…>` items and `dict<…>` fields) also
derive `TypedBuilder`:

```rust
# prompt_templates_macros::include_types!("prompts/greeting.tmpl.md");
let item = greeting::ParamsItemsItem::builder()
    .label("write docs")
    .build();

let params = greeting::Params::builder()
    .name("Alice")
    .count(1)
    .items(vec![item])
    .build();
```

## `dict!` Macro

Construct a `Value::Dict` with JSON-like syntax — useful alongside
`ctx!` for programmatic value construction:

```rust
use prompt_templates::{Value, dict};

let item = dict! { label: "alpha", score: 42_i64 };
assert_eq!(item.get_field("label").unwrap().to_string(), "alpha");
```

## Extra Parameters and Default Contexts

### `render_allowing_extra()`

Like `render()`, but extra context keys that aren't declared in
frontmatter are silently ignored instead of producing an error. Useful
when forwarding a shared context to multiple templates:

```rust
use prompt_templates::{ctx, Template};

let tmpl = Template::from_source("---\nparams:\n  - name = str\n---\nHello {{ name }}!").unwrap();
let ctx = ctx! { name: "world", extra_key: "ignored" };
assert_eq!(tmpl.render_allowing_extra(&ctx).unwrap(), "Hello world!");
```

### `defaults_context()`

Returns a `Context` pre-filled with all default values — set only the
params you need:

```rust
use prompt_templates::Template;

let tmpl = Template::from_source(
    "---\nparams:\n  - name = str\n  - count = int := 5\n---\n{{ name }} ({{ count }})",
)
.unwrap();
let mut ctx = tmpl.defaults_context();
ctx.set("name", "Alice"); // count already has default 5
assert_eq!(tmpl.render(&ctx).unwrap(), "Alice (5)");
```

## `serde` Integration

Enable the optional `serde` feature to render directly from any
`Serialize` struct — no manual `Context` construction needed:

```bash
cargo add prompt-templates --features serde
```

```rust
use prompt_templates::Template;
use serde::Serialize;

#[derive(Serialize)]
struct Data { name: String, count: i64 }

let tmpl = Template::from_source(
    "---\nparams: [name = str, count = int]\n---\n{{ name }} has {{ count }} items",
).unwrap();
let output = tmpl.render_serde(&Data { name: "Alice".into(), count: 3 }).unwrap();
assert_eq!(output, "Alice has 3 items");
```

## Python Bindings

The `prompt-templates-python` package provides full Python bindings via
`PyO3`. Install with `pip install prompt-templates` (or `maturin develop`
for development):

```python
from prompt_templates import Template

tmpl = Template.from_source("---\nparams:\n  - name = str\n---\nHello {{ name }}!")
output = tmpl.render(name="world")
assert output == "Hello world!"
```

Features include dynamic type generation from frontmatter, a PEP 302
import hook for `.tmpl.md` files, `@variant` / `Variants` helpers for
enum construction, and `TemplateCache` for content-hashed caching. See
the [Python README](crates/prompt-templates-python/README.md) for full
documentation.

## Full Reference

For the complete syntax reference — all control-flow tags, filters,
built-in functions, whitespace control, error diagnostics, and more —
see **[SPEC.md](SPEC.md)**.
