# md-tmpl

Strongly-typed prompt templates for LLMs.

```python
from md_tmpl import template

# Load a template — enums and typed classes are generated automatically
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

# Pattern-match on the generated enum (Python 3.10+)
match review.Status.NeedsChanges(reason="missing tests"):
    case review.Status.Approved:
        print("Ship it!")
    case review.Status.NeedsChanges(reason=r):
        print(f"Please fix: {r}")
    case review.Status.Rejected:
        print("Back to the drawing board")
```

The template file is plain Markdown with YAML frontmatter:

<!-- prettier-ignore -->
```markdown
---
types:
  - Status = enum(Approved, NeedsChanges(reason = str), Rejected)

params:
  - reviewer = str
  - items = list(file = str, status = Status)
---

Review by {{ reviewer }}:

> {% for item in items %}

- **{{ item.file }}** — {% match item.status %}{% case Approved %}✅{% case NeedsChanges %}needs changes: {{ item.status.reason }}{% case Rejected %}❌{% /match %}

> {% /for %}
```

## Why?

- **Type-safe** — every parameter declares a type; mismatches are caught at render time with clear errors, not silently buried in LLM output.
- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor and on GitHub. No string soup.
- **Fast** — native-speed engine, 4–8× faster than Jinja2 for rendering, 75× faster to parse.

## Installation

Available on PyPI: <https://pypi.org/project/md-tmpl/>

```bash
pip install md-tmpl
```

<details>
<summary>Development installation (building from source)</summary>

Requires a Rust toolchain and [maturin](https://github.com/PyO3/maturin):

```bash
pip install maturin
cd crates/md-tmpl-python
maturin develop
```

</details>

## Generated Types

### `template()` — the recommended API

`template()` loads a `.tmpl.md` file and auto-generates Python classes for
every enum, struct, and list-item type declared in the frontmatter:

```python
from md_tmpl import template

tmpl = template("prompts/task_list.tmpl.md")

# Generated types are attributes on the returned object:
tmpl.Priority.High       # unit enum variant (sentinel, no parens)
tmpl.Priority.Medium
tmpl.Priority.Low

output = tmpl.render(
    tasks=[
        {"title": "Fix bug", "priority": tmpl.Priority.High},
        {"title": "Add tests", "priority": tmpl.Priority.Medium},
    ],
)
```

Generated class names use PascalCase: param `tasks` → class `Tasks`,
type alias `priority` → class `Priority`.

### Type Mapping

| Frontmatter Type            | Python Equivalent                             |
| :-------------------------- | :-------------------------------------------- |
| `str`                       | `str`                                         |
| `int`                       | `int`                                         |
| `float`                     | `float`                                       |
| `bool`                      | `bool`                                        |
| `list(field = type, ...)`   | `list[GeneratedClass]`                        |
| `list(type)`                | `list[PythonType]` (e.g. `list[str]`)         |
| `struct(field = type, ...)` | `GeneratedClass`                              |
| `enum(Variant, ...)`        | `GeneratedEnumClass`                          |
| `option(type)`              | `Optional[PythonType]` (`PythonType \| None`) |
| `tmpl(...)`                 | `Template` object or callable                 |

### `load_types()` — load types without a template object

```python
from md_tmpl import load_types

types = load_types("prompts/review.tmpl.md")
Status = types.Status

# Or pick specific types:
types = load_types("prompts/review.tmpl.md", pick=["Status"])
```

### `generate_types_source()` — static type stubs for mypy / pyright

Generate `.py` files with `@dataclass` classes so type checkers catch errors
at analysis time:

```python
from md_tmpl import generate_types_source

source = generate_types_source("prompts/greeting.tmpl.md")
with open("greeting_types.py", "w") as f:
    f.write(source)
```

Use the generated class like a normal dataclass:

```python
from greeting_types import Greeting

# mypy / pyright catch missing or mistyped fields here:
params = Greeting(name="world")
output = params.render()   # → "Hello world!"
```

## Import Hook

Use templates as regular Python modules:

```python
from md_tmpl import md_tmpl_import_hook

md_tmpl_import_hook()

from prompts.code_review import CodeReview, Status

output = CodeReview(
    reviewer="Alice",
    items=[...],
).render()
```

## Enum Dispatch

Define enums in frontmatter and branch on them with `{% match %}`:

<!-- prettier-ignore -->
```markdown
---
params:
  - status = enum(Done(summary = str), InProgress, Blocked)
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

### `Variants` — define enums in Python

```python
from md_tmpl import Variants

class Status(Variants):
    Approved = ()                    # unit variant (sentinel)
    Rejected = ()
    NeedsChanges = {"reason": str}   # struct variant (callable)

Status.Approved                          # no parens
Status.NeedsChanges(reason="fix tests")  # keyword constructor
```

### Pattern Matching (Python 3.10+)

```python
def handle_review(status):
    match status:
        case Status.Approved:
            return "Ship it!"
        case Status.NeedsChanges(reason=r):
            return f"Please fix: {r}"
        case Status.Rejected:
            return "Back to the drawing board"
```

### `@variant` decorator

Turn a class with annotations into a matchable variant:

```python
from md_tmpl import variant

@variant
class NeedsChanges:
    reason: str

v = NeedsChanges(reason="fix tests")
assert v.reason == "fix tests"
```

## Features

### Typed Lists

```python
from md_tmpl import Template

tmpl = Template.from_source("""\
---
params:
  - tasks = list(title = str, priority = str)
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

### Defaults

Parameters can declare defaults in the frontmatter. Call `render_empty()`
on templates where every parameter has a default.

### Filters

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

### Built-in Functions

| Function       | Example                                             |
| -------------- | --------------------------------------------------- |
| `idx(binding)` | `{{ idx(item) }}` — 0-based loop index              |
| `len(expr)`    | `{{ len(items) }}` — list or string length          |
| `kind(expr)`   | `{{ kind(status) }}` — enum variant name            |
| `kinds(type)`  | `{{ kinds(Status) }}` — list of enum variant names  |
| `has(expr)`    | `{{ has(field) }}` — true if `option(T)` is present |

### Includes

Templates can include other templates with `{% include "path.tmpl.md" %}`.

### Constants

```python
tmpl = Template.from_source("""\
---
consts:
  - MAX_RETRIES = int := 3
  - MODEL = str := "gemini-3.5-flash"

params:
  - query = str
---
{{ query }}""")

constants = tmpl.consts()  # {"MAX_RETRIES": 3, "MODEL": "gemini-3.5-flash"}
```

### Caching

```python
from md_tmpl import TemplateCache

cache = TemplateCache()
tmpl = cache.load("prompts/greeting.tmpl.md")
output = tmpl.render(name="cached")

# Cache-aware rendering (resolves {% include %} through cache):
output = tmpl.render_cached(cache, name="world")

cache.template_count()  # number of cached templates
cache.clear()           # invalidate all entries
```

## API Reference

### `Template`

| Method / Constructor                      | Description                                             |
| ----------------------------------------- | ------------------------------------------------------- |
| `Template.from_file(path)`                | Load and parse a `.tmpl.md` file                        |
| `Template.from_source(source)`            | Parse a template from an inline string                  |
| `tmpl.render(**kwargs)`                   | Render with keyword arguments (type-checked)            |
| `tmpl.render_dict(params, allow_extra=…)` | Render from a dict; `allow_extra=True` skips extras     |
| `tmpl.render_empty()`                     | Render a template with only defaults (no user args)     |
| `tmpl.declarations()`                     | Return `[(name, type_str), …]` for all params           |
| `tmpl.consts()`                           | Return `{name: value, …}` for constants                 |
| `tmpl.validate_declarations_against(…)`   | Raise `ValueError` if declarations changed (hot-reload) |

### Errors

```python
from md_tmpl import (
    TemplateError,          # base class
    TemplateSyntaxError,    # invalid template syntax
    MissingParamsError,     # required parameters not provided
    TypeMismatchError,      # value type doesn't match declaration
    ExtraParamsError,       # undeclared parameters passed
)
```

Wrong types produce clear errors:

```python
tmpl.render(count="not an int")
# ValueError: type mismatch for 'count': expected int, got str
```

Extra parameters are rejected by default — pass `allow_extra=True` to opt out.

## Performance

**4–8× faster than Jinja2** for rendering, backed by a native-speed engine.

### Render Time (pre-parsed template + data)

10,000 iterations, best of 5 runs
([source](../../benchmarks/python/bench_templates.py)):

| Scenario        |        md-tmpl |   Jinja2 |    Mako |  Chevron |    Django | string.Template |
| --------------- | -------------: | -------: | ------: | -------: | --------: | --------------: |
| **simple**      |        1.65 µs |  6.60 µs | 9.46 µs |  7.40 µs |   7.94 µs |  **1.63 µs** 🏆 |
| **loop**        | **1.96 µs** 🏆 | 10.32 µs | 6.64 µs | 20.86 µs |  46.52 µs |             N/A |
| **conditional** | **1.04 µs** 🏆 |  6.44 µs | 6.36 µs |      N/A |  14.62 µs |             N/A |
| **hero**        | **7.03 µs** 🏆 | 24.74 µs | 9.42 µs |      N/A | 227.48 µs |             N/A |

### Parse Time (source → template object)

| Scenario        |         md-tmpl |    Jinja2 |      Mako |        Chevron |    Django | string.Template |
| --------------- | --------------: | --------: | --------: | -------------: | --------: | --------------: |
| **simple**      |         4.37 µs | 313.39 µs | 394.31 µs | **0.13 µs** 🏆 |  19.16 µs |         0.22 µs |
| **loop**        |         6.72 µs | 551.93 µs | 500.29 µs | **0.13 µs** 🏆 |  42.96 µs |             N/A |
| **conditional** |  **8.37 µs** 🏆 | 663.49 µs | 554.38 µs |            N/A |  76.39 µs |             N/A |
| **hero**        | **27.97 µs** 🏆 |   2.24 ms |   1.40 ms |            N/A | 231.66 µs |             N/A |

### End-to-End (parse + render)

| Scenario        |         md-tmpl |    Jinja2 |      Mako |  Chevron |    Django |   str.Template |
| --------------- | --------------: | --------: | --------: | -------: | --------: | -------------: |
| **simple**      |         5.71 µs | 336.10 µs | 427.35 µs | 11.52 µs |  42.41 µs | **1.88 µs** 🏆 |
| **loop**        |  **9.65 µs** 🏆 | 639.43 µs | 579.59 µs | 22.56 µs | 105.59 µs |            N/A |
| **conditional** |  **9.90 µs** 🏆 | 704.27 µs | 595.91 µs |      N/A | 104.38 µs |            N/A |
| **hero**        | **38.39 µs** 🏆 |   2.63 ms |   1.52 ms |      N/A | 682.11 µs |            N/A |

```bash
just bench-python          # run comparison benchmarks
just bench-update-python   # run + update these tables
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
