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

## Full Reference

For the complete syntax reference — all control-flow tags, filters,
built-in functions, whitespace control, and error diagnostics — see
**[SPEC.md](../../SPEC.md)**.
