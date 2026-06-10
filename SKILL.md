---
name: prompt-templates
description: >
  Write and maintain strongly-typed LLM prompt templates (.tmpl.md files)
  using the prompt-templates Rust crate. Covers template syntax,
  frontmatter types, control flow, includes, and the Rust rendering API.
---

# prompt-templates Skill

Use this skill when creating, editing, or debugging `.tmpl.md` prompt
template files or the Rust code that renders them.

## File Format

Template files use the `.tmpl.md` extension. They are valid markdown with
a YAML frontmatter block:

```markdown
---
params:
  - name = str
  - count = int
---

Hello {{ name }}, you have {{ count }} items.
```

**Every parameter must declare a type.** Bare `- name` without a type is
a hard error.

## Frontmatter Types

| Type annotation             | Rust type (generated)     | Example                                      |
| --------------------------- | ------------------------- | -------------------------------------------- |
| `str`                       | `String`                  | `- name = str`                               |
| `int`                       | `i64`                     | `- count = int`                              |
| `float`                     | `f64`                     | `- score = float`                            |
| `bool`                      | `bool`                    | `- active = bool`                            |
| `list<field = type, …>`     | `Vec<GeneratedStruct>`    | `- bugs = list<title = str, severity = str>` |
| `dict<field = type, …>`     | Generated struct          | `- config = dict<timeout = int>`             |
| `enum<Variant1, Variant2>`  | Generated enum            | `- status = enum<Active, Paused>`            |
| `enum<V(field = type), V2>` | Enum with struct variants | `- outcome = enum<Ok(msg = str), Err>`       |

### Default values

Append `:= <literal>` after the type:

```yaml
params:
  - name = str := "World"
  - count = int := 42
  - verbose = bool := false
```

## Variable Substitution

Use `{{ expr }}` for output:

```markdown
{{ name }}
{{ bug.title }}
{{ name | upper }}
{{ score | fixed(2) }}
```

### Available filters

`upper`, `lower`, `trim`, `fixed(N)`, `length`, `default("val")`,
`join("sep")`, `limit(N)`, `gt(N)`.

### Built-in functions

`idx(binding)` (0-based loop index), `len(expr)`, `str(expr)`.

## Control Flow

**Control-flow tags on their own line must start with `>`** (blockquote
prefix). Content lines do not — only `{% %}` lines need it:

<!-- prettier-ignore -->
```markdown
> {% if severity == "critical" %}
🔴 Immediate action required.
> {% elif severity == "high" %}
🟠 High priority.
> {% else %}
🟢 Normal.
> {% /if %}
```

### For loops

```markdown
> {% for bug in bugs %}

- **{{ bug.title }}** ({{ bug.severity }})
  > {% /for %}
```

The loop variable must be a `list` type — enforced at compile time.

### Match / case (enums)

<!-- prettier-ignore -->
```markdown
> {% match outcome %}
> {% case Confirmed %}
✅ Evidence: {{ outcome.evidence }}
> {% case Rejected %}
❌ Not confirmed.
> {% /match %}
```

- All variants must be covered (exhaustiveness checking).
- Fields are narrowed per arm — accessing `outcome.evidence` outside
  `{% case Confirmed %}` is a compile error if `evidence` is not on
  all variants.

## Reuse — Keep Prompts DRY

Avoid duplicating content across templates. Use **file includes** for
shared fragments across multiple templates, and **inline `{% tmpl %}`
definitions** for repeated blocks within a single file.

### File includes

Include other `.tmpl.md` files with explicit parameter passing:

```markdown
> {% include [bug_card](bug_card.tmpl.md) with title=bug.title %}
```

The `[name](path)` syntax is a markdown link — clickable in editors.

**Iterated includes** unroll a list:

```markdown
> {% include [row](row.tmpl.md) for item in items %}
```

Parameters are type-checked against the included template's frontmatter.
No implicit scope leaking — you must pass everything explicitly via
`with`.

### Inline templates

For repeated blocks within a single file, define reusable fragments
inline without separate files:

```markdown
## {% tmpl bug_row %}

## params:

## - title = str

## - severity = str

- **{{ title }}** ({{ severity }})
  {% /tmpl %}

> {% for bug in bugs %}
> {% include bug_row with title=bug.title, severity=bug.severity %}
> {% /for %}
```

## Whitespace Control

Add `-` inside delimiters to strip adjacent whitespace:

- `{%-` strips whitespace before the tag
- `-%}` strips whitespace after the tag
- `{{-` / `-}}` same for expressions

## Raw Blocks

Output literal template syntax without processing:

```markdown
> {% raw %}
> {{ not_processed }}
> {% /raw %}
```

## Comments

```markdown
{# This won't appear in output #}
```

Parameters referenced inside comments count as "used" for the
unused-parameter check.

## Rust API — Rendering Templates

### Runtime (dynamic context)

```rust
use prompt_templates::{ctx, Template};

let tmpl = Template::from_source("---
params:
  - name = str
---
Hello {{ name }}!").unwrap();

let output = tmpl.render(&ctx! { name: "world" }).unwrap();
```

### Compile-time (typed structs)

```rust
use prompt_templates_macros::{include_template, template_params_struct};

// Generates a typed struct from the template's frontmatter
template_params_struct!("prompts/greeting.tmpl.md" => GreetingParams);

let tmpl = include_template!("prompts/greeting.tmpl.md");
let output = GreetingParams {
    name: "Alice".into(),
    count: 42,
    items: vec![GreetingParamsItemsItem { label: "hello".into() }],
}.render(&tmpl).unwrap();
```

### Hot-reload with validation

```rust
let tmpl = prompt_templates::Template::from_file(
    std::path::Path::new("prompts/greeting.tmpl.md")
).unwrap();
GreetingParams::validate_template(&tmpl).unwrap();
```

## Common Mistakes to Avoid

1. **Missing `>` prefix on `{% %}` lines.** Lines that start with a
   `{% %}` directive MUST have the `>` blockquote prefix. Content lines (text, `{{ }}`) do
   not need it — only `{% %}` directives do.

2. **Forgetting to type parameters.** Every param needs `= type`.
   `- name` alone is a hard error; write `- name = str`.

3. **Accessing enum fields outside the matching arm.** The type system
   narrows fields per `{% case %}` — only access variant-specific
   fields inside the correct arm.

4. **Implicit scope in includes.** Unlike Jinja, includes do NOT inherit
   the parent scope. Pass all needed values explicitly with `with`.

5. **Unused parameters.** Declared params not referenced in the body are
   a hard error by default. Use `allow_unused: true` in frontmatter to
   suppress, or reference them in a comment: `{# {{ unused_var }} #}`.

6. **Undeclared variables.** Variables used in the body but not declared
   in `params:` are always rejected, even with `allow_unused: true`.

## Full Reference

See [SPEC.md](SPEC.md) for the complete syntax reference including all
filters, functions, whitespace control, and error diagnostics.
