# prompt-templates-python

A **strongly-typed** template engine for LLM prompts, designed for
markdown. Close to Jinja2 in spirit, but purpose-built for prompts.
Written in Rust for maximum performance.

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
f-strings or untyped Jinja/Handlebars templates.

**Inline strings** mix prose with code, making prompts hard to read,
review, and iterate on. **Untyped template engines** push every error to
runtime: rename a variable, add a field, change a list shape — you won't
know it's broken until the prompt renders garbage in production.

`prompt-templates` gives you **separation of concerns** (prompts live in
`.tmpl.md` files that render as readable markdown in any editor or GitHub
preview) with **strict typing** (every parameter declares a type and
mismatches are caught at render time). Templates fail fast and
loud — which is especially valuable when an LLM agent is writing or
editing your prompts, because the engine catches drift immediately
instead of letting it propagate silently.

| Feature                      | Why it matters                                                                   |
| ---------------------------- | -------------------------------------------------------------------------------- |
| **Strongly typed**           | Every parameter declares a type (`str`, `int`, `list<…>`, `dict<…>`, `enum<…>`). |
| **Type aliases**             | `types:` block defines reusable named types; PascalCase Python classes.          |
| **Cross-template imports**   | `imports:` pulls types from other templates via dotted paths.                    |
| **Typed lists**              | `list<title = str, score = int>` — iterate with `{% for %}`, fields validated.   |
| **Enum dispatch**            | `match`/`case` on typed variants with exhaustiveness checking + field narrowing. |
| **Includes as links**        | `{% include [name](path.tmpl.md) with … %}` — clickable, type-checked.           |
| **Inline templates**         | `{% tmpl name %}` — reusable fragments without separate files.                   |
| **Generated Python types**   | `template()` auto-generates enum classes and item types from frontmatter.        |
| **Import hook**              | `import prompts.my_template` — use templates as regular Python modules.          |
| **Readable as raw markdown** | `> {% %}` blockquote prefix keeps control flow visually separated from prose.    |

> **Note:** The `> ` prefix is required only on the `{% %}` tag lines
> themselves — it is stripped before compilation. Content lines between tags
> (prose, `{{ }}` expressions) are **normal text** and should **not** use
> `> `. If a content line starts with `> `, it is kept verbatim as a literal
> markdown blockquote in the output.

## Installation

```bash
# Development install (requires Rust toolchain + maturin)
pip install maturin
cd crates/prompt-templates-python
maturin develop
```

## Quick Start

```python
from prompt_templates import Template

tmpl = Template.from_source("""
---
params:
  - name = str
---
Hello {{ name }}!
""")
print(tmpl.render(name="world"))  # → "Hello world!"
```

## Runtime Loading — `load_template` and `load_types`

For runtime loading from `.tmpl.md` files, use `load_template` and
`load_types`:

```python
from prompt_templates import load_template, load_types

tmpl = load_template("prompts/greeting.tmpl.md")
types = load_types("prompts/greeting.tmpl.md")

# types.Outcome, types.Priority, etc. are available as attributes
output = tmpl.render(name="world")
```

`load_types` returns a namespace with generated Python classes for all
type aliases and compound param types. Use `pick=` to load only specific
types:

```python
types = load_types("prompts/review.tmpl.md", pick=["Status"])
```

## Typed Lists with `{% for %}`

Declare list fields with types — the engine validates every item:

```python
from prompt_templates import Template

tmpl = Template.from_source("""
---
params:
  - tasks = list<title = str, priority = str>
---
> {% for task in tasks %}
- **{{ task.title }}** ({{ task.priority }})
> {% /for %}
""")

output = tmpl.render(tasks=[
    {"title": "Write documentation", "priority": "High"},
    {"title": "Add unit tests",      "priority": "Medium"},
])
```

## Enum Dispatch with `match`/`case`

Declare enum variants (with optional typed fields), dispatch with
`match`/`case`, and get **exhaustiveness checking** and **field
narrowing**:

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

Accessing `status.summary` outside `{% case Done %}` is a template
error — the type system narrows fields per variant.

## Generated Types with `template()`

The `template()` helper auto-generates Python enum classes and item
types from the frontmatter — no manual class definitions needed:

```python
from prompt_templates import template

review = template("prompts/code_review.tmpl.md")

output = review.render(
    reviewer="Alice",
    items=[
        review.Item(file="main.py", status=review.Status.Approved),
        review.Item(
            file="lib.py",
            status=review.Status.NeedsChanges(reason="missing tests"),
        ),
    ],
)
```

Unit variants are sentinels (no parens needed), struct variants are
callable constructors:

```python
review.Status.Approved           # unit variant — no parentheses
review.Status.NeedsChanges(reason="fix tests")  # struct variant
```

### PascalCase Naming

Generated Python classes use `PascalCase` for all type names:

- Param `bugs` → class `Bugs`
- Param `vuln_type` → class `VulnType`
- Param `code_review` → class `CodeReview`

Type aliases from the `types:` block use their declared name directly
as the Python class name (they should already be `PascalCase`).

## Type Aliases

Templates with a `types:` block generate corresponding Python types.
Type aliases are available as attributes on the template object:

```python
from prompt_templates import template

# Given a template with:
#   types:
#     - Priority = enum<High, Medium, Low>
#   params:
#     - tasks = list<title = str, priority = Priority>

tmpl = template("prompts/task_list.tmpl.md")

output = tmpl.render(
    tasks=[
        {"title": "Fix bug", "priority": tmpl.Priority.High},
        {"title": "Add tests", "priority": tmpl.Priority.Medium},
    ],
)
```

## Cross-Template Imports

Templates can import types from other templates via the `imports:` block.
Imported types are resolved automatically when loading the template:

```markdown
---
imports:
  - "[shared_types](shared_types.tmpl.md)"
params:
  - priority = shared_types.Priority
---
```

The import stem must match the filename without `.tmpl.md`.
See [SPEC.md](../../SPEC.md) for full details.

## Import Hook

Use templates as regular Python modules:

```python
from prompt_templates import prompt_template_import_hook

prompt_template_import_hook()

# Now import types directly from .tmpl.md files:
from prompts.code_review import CodeReviewParams, Status

output = CodeReviewParams(
    reviewer="Alice",
    items=[...],
).render()
```

## Custom Enum Types

Define your own variant types for use with templates — with
`@variant` for struct variants and `Variants` for full enum
definitions:

```python
from prompt_templates import variant, Variants

@variant
class NeedsChanges:
    reason: str

v = NeedsChanges(reason="fix tests")


class Status(Variants):
    Approved = ()
    Rejected = ()
    NeedsChanges = {"reason": str}

Status.Approved              # unit sentinel
Status.NeedsChanges(reason="fix tests")  # struct constructor
```

## Type Validation

All values are validated against the frontmatter declarations at render
time. Wrong types produce clear error messages:

```python
tmpl = Template.from_source("""
---
params:
  - count = int
---
Count: {{ count }}
""")

tmpl.render(count="not an int")
# ValueError: type mismatch for 'count': expected int, got str
```

Extra parameters are rejected by default — pass `allow_extra=True` to
opt out:

```python
tmpl.render(count=1, allow_extra=True)  # ignores extra kwargs
```

## Caching

`TemplateCache` hashes file contents — unchanged files return cached
compilations with zero re-parsing:

```python
from prompt_templates import TemplateCache

cache = TemplateCache()
tmpl = cache.load("prompts/greeting.tmpl.md")
output = tmpl.render(name="cached")
```

## Rendering from a Dict

Use `render_dict()` when your parameters come from a dictionary rather
than keyword arguments:

```python
params = {"name": "world", "count": 3}
output = tmpl.render_dict(params)
```

Pass `allow_extra=True` to ignore extra keys:

```python
output = tmpl.render_dict(params, allow_extra=True)
```

## Constants

Templates can define constants in the `consts:` block. Access them via
`consts()`:

```python
tmpl = Template.from_source("""
---
consts:
  - MAX_RETRIES = int := 3
  - MODEL = str := "gpt-4"
params:
  - query = str
---
{{ query }}
""")

constants = tmpl.consts()  # {"MAX_RETRIES": 3, "MODEL": "gpt-4"}
```

Use `imported_consts()` to access constants imported from other templates
(keyed by `stem.NAME`, e.g. `shared.MAX_RETRIES`).

## Hot-Reload Validation

After reloading a template from disk, validate that its declarations
haven't changed with `validate_declarations_against()`:

```python
expected = tmpl.declarations()  # save at startup

# ... later, after hot-reload ...
new_tmpl = Template.from_file("prompts/greeting.tmpl.md")
new_tmpl.validate_declarations_against(expected)  # raises ValueError if changed
```

## Template Body and Include Depth

`body()` returns the raw template text after frontmatter stripping:

```python
raw = tmpl.body()
```

`set_max_include_depth(depth)` controls how deeply `{% include %}`
directives can nest:

```python
tmpl.set_max_include_depth(5)
```

## Loading with a Base Directory

`from_source_with_base_dir()` parses a template string and resolves
`{% include %}` paths relative to `base_dir`:

```python
tmpl = Template.from_source_with_base_dir(source, "prompts/")
output = tmpl.render(name="world")
```

## Cache Management

`TemplateCache` also exposes management methods:

```python
cache = TemplateCache()
cache.load("prompts/greeting.tmpl.md")
cache.load("prompts/review.tmpl.md")

cache.template_count()  # 2
cache.include_count()   # number of cached includes

cache.clear()           # invalidate all entries
cache.template_count()  # 0
```

## Filters

Filters transform values inside expressions using the `|` pipe syntax:

| Filter      | Description                  | Example                    |
| ----------- | ---------------------------- | -------------------------- |
| `upper`     | Uppercase string             | `{{ name \| upper }}`      |
| `lower`     | Lowercase string             | `{{ name \| lower }}`      |
| `trim`      | Strip leading/trailing space | `{{ name \| trim }}`       |
| `fixed(N)`  | Format float to N decimals   | `{{ score \| fixed(2) }}`  |
| `join(sep)` | Join list items with sep     | `{{ tags \| join(", ") }}` |
| `limit(N)`  | Take first N list elements   | `{{ items \| limit(3) }}`  |
| `add(N)`    | Add N to numeric value       | `{{ count \| add(1) }}`    |
| `sub(N)`    | Subtract N from numeric      | `{{ count \| sub(1) }}`    |

## Built-in Functions

Functions can be used inside expressions:

| Function       | Description                                         | Example              |
| -------------- | --------------------------------------------------- | -------------------- |
| `idx(binding)` | Index of current item in `{% for %}` loop (0-based) | `{{ idx(item) }}`    |
| `len(expr)`    | Length of a list or string                          | `{{ len(items) }}`   |
| `kind(expr)`   | Active variant name of an enum value                | `{{ kind(status) }}` |

## Full Reference

For the complete syntax reference — all control-flow tags, filters,
built-in functions, whitespace control, and error diagnostics — see
**[SPEC.md](../../SPEC.md)**.
