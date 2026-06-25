# prompt-templates for Python

Strongly-typed prompt templates for LLMs.

[![PyPI version](https://badge.fury.io/py/prompt-templates.svg)](https://badge.fury.io/py/prompt-templates)
Templates are markdown files with YAML frontmatter declaring typed
parameters — every variable, list shape, and enum variant is validated
at render time.

Built with PyO3/maturin on top of the Rust engine. **3–7× faster than
Jinja2** for rendering.

## Why?

LLM prompts grow complex — multi-shot examples, tool schemas, agentic
workflows — but most Python projects still manage them as inline
f-strings or untyped Jinja2/Mako templates.

**Inline f-strings** mix prose with code, making prompts unreadable and
hard to review. **Untyped template engines** push every error to
runtime: rename a variable, add a field, change a list shape — you
discover it when the prompt renders garbage in production.

`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, not f-strings. They render as clean markdown in any editor or on GitHub — includes are clickable links, and control flow uses blockquote-prefixed lines so it stays visually separated from prose.
- **Strict typing** — every parameter declares a type; mismatches are caught at render time with clear error messages.
- **Agent-safe** — when an LLM writes or edits prompts, the engine catches drift immediately instead of letting it propagate.

## Installation

```bash
pip install prompt-templates
```

For development (requires Rust toolchain):

```bash
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

## Runtime Loading

```python
from prompt_templates import load_template, load_types

tmpl = load_template("prompts/greeting.tmpl.md")
types = load_types("prompts/greeting.tmpl.md")

output = tmpl.render(name="world")
```

`load_types` returns a namespace with generated Python classes for all
type aliases and compound param types. Use `pick=` to load specific types:

```python
types = load_types("prompts/review.tmpl.md", pick=["Status"])
```

## Typed Lists

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

## Enum Dispatch

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

Accessing `status.summary` outside `{% case Done %}` is a template error.

## Generated Types with `template()`

Auto-generates Python enum classes and item types from frontmatter:

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

Unit variants are sentinels (no parens), struct variants are callable:

```python
review.Status.Approved           # unit variant
review.Status.NeedsChanges(reason="fix tests")  # struct variant
```

Generated Python classes use PascalCase: param `tasks` → class `Tasks`.

## Type Aliases

```python
from prompt_templates import template

# Given types: - Priority = enum<High, Medium, Low>
tmpl = template("prompts/task_list.tmpl.md")

output = tmpl.render(
    tasks=[
        {"title": "Fix bug", "priority": tmpl.Priority.High},
        {"title": "Add tests", "priority": tmpl.Priority.Medium},
    ],
)
```

## Import Hook

Use templates as regular Python modules:

```python
from prompt_templates import prompt_template_import_hook

prompt_template_import_hook()

from prompts.code_review import CodeReview, Status

output = CodeReview(
    reviewer="Alice",
    items=[...],
).render()
```

## Custom Enum Types

```python
from prompt_templates import variant, Variants

@variant
class NeedsChanges:
    reason: str

class Status(Variants):
    Approved = ()
    Rejected = ()
    NeedsChanges = {"reason": str}

Status.Approved              # unit sentinel
Status.NeedsChanges(reason="fix tests")  # struct constructor
```

## Pattern Matching (Python 3.10+)

```python
from prompt_templates import Variants

class Status(Variants):
    Approved = ()
    Rejected = ()
    NeedsChanges = {"reason": str}

def handle_review(status):
    match status:
        case Status.Approved:
            return "Ship it!"
        case Status.NeedsChanges(reason=r):
            return f"Please fix: {r}"
        case Status.Rejected:
            return "Back to the drawing board"
```

## Type Validation

Wrong types produce clear errors:

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
opt out.

## Caching

```python
from prompt_templates import TemplateCache

cache = TemplateCache()
tmpl = cache.load("prompts/greeting.tmpl.md")
output = tmpl.render(name="cached")

# Cache-aware rendering (resolves {% include %} through cache):
output = tmpl.render_cached(cache, name="world")

cache.template_count()  # 2
cache.clear()           # invalidate all entries
```

## Rendering from a Dict

```python
params = {"name": "world", "count": 3}
output = tmpl.render_dict(params)
output = tmpl.render_dict(params, allow_extra=True)
```

## Constants

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

## Hot-Reload Validation

```python
expected = tmpl.declarations()

new_tmpl = Template.from_file("prompts/greeting.tmpl.md")
new_tmpl.validate_declarations_against(expected)  # raises ValueError if changed
```

## Filters

| Filter      | Example                    |
| ----------- | -------------------------- |
| `upper`     | `{{ name \| upper }}`      |
| `lower`     | `{{ name \| lower }}`      |
| `trim`      | `{{ name \| trim }}`       |
| `fixed(N)`  | `{{ score \| fixed(2) }}`  |
| `join(sep)` | `{{ tags \| join(", ") }}` |
| `limit(N)`  | `{{ items \| limit(3) }}`  |
| `add(N)`    | `{{ count \| add(1) }}`    |
| `sub(N)`    | `{{ count \| sub(1) }}`    |

## Built-in Functions

| Function       | Example                                             |
| -------------- | --------------------------------------------------- |
| `idx(binding)` | `{{ idx(item) }}` — 0-based loop index              |
| `len(expr)`    | `{{ len(items) }}` — list or string length          |
| `kind(expr)`   | `{{ kind(status) }}` — enum variant name            |
| `has(expr)`    | `{{ has(field) }}` — true if `option<T>` is present |

## Errors

```python
from prompt_templates import (
    TemplateError,          # base class
    TemplateSyntaxError,    # invalid template syntax
    MissingParamsError,     # required parameters not provided
    TypeMismatchError,      # value type doesn't match declaration
    ExtraParamsError,       # undeclared parameters passed
)
```

## Performance

**3–7× faster than Jinja2** for rendering. PyO3/Rust FFI with direct
CPython dictionary iteration.

### Render Time (pre-compiled template + data)

10,000 iterations, best of 5 runs
([source](../../benchmarks/python/bench_templates.py)):

| Scenario        | prompt-templates |   Jinja2 |            Mako |       vs Jinja2 |
| --------------- | ---------------: | -------: | --------------: | --------------: |
| **simple**      |   **0.94 µs** 🏆 |  6.42 µs |         6.31 µs | **6.8× faster** |
| **loop**        |   **2.63 µs** 🏆 | 11.48 µs |         7.07 µs | **4.4× faster** |
| **conditional** |   **1.07 µs** 🏆 |  6.82 µs |         6.45 µs | **6.4× faster** |
| **hero**        |         23.95 µs | 74.92 µs | **20.46 µs** 🏆 | **3.1× faster** |

### Compile Time (source → compiled object)

| Scenario        | prompt-templates |    Jinja2 |      Mako |    Django |
| --------------- | ---------------: | --------: | --------: | --------: |
| **simple**      |          4.95 µs | 310.90 µs | 391.45 µs |  18.85 µs |
| **loop**        |          7.11 µs | 645.02 µs | 590.61 µs |  51.48 µs |
| **conditional** |  **10.04 µs** 🏆 |   1.13 ms | 711.58 µs | 159.81 µs |
| **hero**        |  **22.07 µs** 🏆 |   2.23 ms |   1.60 ms | 264.83 µs |

### End-to-End (compile + render)

| Scenario        | prompt-templates |    Jinja2 |      Mako |      Django |
| --------------- | ---------------: | --------: | --------: | ----------: |
| **simple**      |          5.36 µs | 332.53 µs | 416.56 µs |    40.32 µs |
| **loop**        |  **10.88 µs** 🏆 | 680.54 µs | 620.10 µs |   102.71 µs |
| **conditional** |  **11.48 µs** 🏆 |   1.15 ms | 757.49 µs |   198.62 µs |
| **hero**        |  **48.90 µs** 🏆 |   2.37 ms |   1.66 ms | 1,163.02 µs |

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
