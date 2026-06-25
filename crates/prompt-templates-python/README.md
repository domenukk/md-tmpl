# prompt-templates for Python

Strongly-typed prompt templates for LLMs.

[![PyPI version](https://badge.fury.io/py/prompt-templates.svg)](https://badge.fury.io/py/prompt-templates)
Templates are markdown files with YAML frontmatter declaring typed
parameters — every variable, list shape, and enum variant is validated
at render time.

Built with PyO3/maturin on top of the Rust engine. **3–8× faster than
Jinja2** for rendering.

## Why?

Inline f-strings are unreadable. Untyped Jinja2/Mako templates break at runtime.
`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor or on GitHub.
- **Strict typing** — every parameter declares a type; mismatches are caught at render time with clear errors.
- **Agent-safe** — when an LLM edits prompts, the engine catches drift immediately.

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

### Generated Types (recommended)

`template()` auto-generates Python enum classes and item types from
frontmatter — use them for type-safe rendering:

```python
from prompt_templates import template

# Loads the template and generates Status enum + Item class from frontmatter
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

### Inline Templates

For quick prototyping, parse templates inline:

```python
from prompt_templates import Template

tmpl = Template.from_source("""\
---
params:
  - name = str
---
Hello {{ name }}!""")
print(tmpl.render(name="world"))  # → "Hello world!"
```

## Import Hook

Use templates as regular Python modules — the most Pythonic approach:

```python
from prompt_templates import prompt_template_import_hook

prompt_template_import_hook()

from prompts.code_review import CodeReview, Status

output = CodeReview(
    reviewer="Alice",
    items=[...],
).render()
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

## Static Type Stubs (mypy / pyright)

Generate `.py` files with typed `@dataclass` classes so type checkers
catch errors at analysis time — no runtime needed:

```python
from prompt_templates import generate_types_source

source = generate_types_source("prompts/greeting.tmpl.md")
with open("greeting_types.py", "w") as f:
    f.write(source)
```

The generated file contains:

- `@dataclass` params class with typed fields
- A `render()` method that loads the template and returns `str`
- Nested `@dataclass` classes for struct/list params
- `Variants` subclasses for enum params
- Default values via `field(default=...)`
- `__all__` export list

Use the generated class like a normal dataclass:

```python
from greeting_types import Greeting

# mypy / pyright catch missing or mistyped fields here:
params = Greeting(name="world")
output = params.render()   # → "Hello world!"

# Pass an explicit template for hot-reload:
from prompt_templates import Template
tmpl = Template.from_file("prompts/greeting.tmpl.md")
output = params.render(template=tmpl)
```

## Typed Lists

```python
from prompt_templates import Template

tmpl = Template.from_source("""\
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
tmpl = Template.from_source("""\
---
params:
  - count = int
---
Count: {{ count }}""")

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
tmpl = Template.from_source("""\
---
consts:
  - MAX_RETRIES = int := 3
  - MODEL = str := "gpt-4"
params:
  - query = str
---
{{ query }}""")

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

**3–8× faster than Jinja2** for rendering. Backed by a native Rust
engine via PyO3 FFI — the speed advantage is partly due to compiled
code, not template design alone. On the hero render benchmark, Mako
(which generates Python bytecode) is slightly faster.

### Render Time (pre-compiled template + data)

10,000 iterations, best of 5 runs
([source](../../benchmarks/python/bench_templates.py)):

| Scenario        | prompt-templates |   Jinja2 |            Mako |       vs Jinja2 |
| --------------- | ---------------: | -------: | --------------: | --------------: |
| **simple**      |   **0.82 µs** 🏆 |  6.43 µs |         6.41 µs | **7.8× faster** |
| **loop**        |   **2.47 µs** 🏆 | 11.96 µs |         7.17 µs | **4.8× faster** |
| **conditional** |   **1.00 µs** 🏆 |  6.85 µs |         6.63 µs | **6.9× faster** |
| **hero**        |         24.20 µs | 76.91 µs | **21.50 µs** 🏆 | **3.2× faster** |

### Compile Time (source → compiled object)

| Scenario        | prompt-templates |    Jinja2 |      Mako |    Django |
| --------------- | ---------------: | --------: | --------: | --------: |
| **simple**      |   **4.19 µs** 🏆 | 316.41 µs | 405.50 µs |  18.73 µs |
| **loop**        |   **7.38 µs** 🏆 | 668.25 µs | 605.36 µs |  50.48 µs |
| **conditional** |   **9.98 µs** 🏆 |   1.15 ms | 731.26 µs | 161.04 µs |
| **hero**        |  **26.21 µs** 🏆 |   2.30 ms |   1.64 ms | 272.07 µs |

### End-to-End (compile + render)

| Scenario        | prompt-templates |    Jinja2 |      Mako |      Django |
| --------------- | ---------------: | --------: | --------: | ----------: |
| **simple**      |   **5.01 µs** 🏆 | 322.84 µs | 411.91 µs |    26.45 µs |
| **loop**        |   **9.85 µs** 🏆 | 680.21 µs | 612.53 µs |    86.89 µs |
| **conditional** |  **10.98 µs** 🏆 |   1.16 ms | 737.89 µs |   189.52 µs |
| **hero**        |  **50.41 µs** 🏆 |   2.38 ms |   1.66 ms | 1,113.65 µs |

```bash
just bench-python          # run comparison benchmarks
just bench-update-python   # run + update these tables
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
