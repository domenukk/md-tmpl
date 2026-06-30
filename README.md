# prompt-templates


**Strongly-typed prompt templates for LLM Agents**
Write prompts as markdown, catch errors faster. Vibe harder.

## The Template

<!-- prettier-ignore -->
```markdown
---
consts:
  - MODEL = str := "gemini-3.5-flash"
  - MAX_FINDINGS = int := 20

types:
  - Severity = enum(Critical, High, Medium, Low)
  - Verdict = enum(Approved, NeedsChanges(reason = str), Rejected(reason = str))

params:
  - reviewer = str
  - file_path = str
  - severity = Severity
  - findings = list(line = int, message = str, severity = Severity)
  - verdict = Verdict
---

# Code Review: {{ file_path }}

Reviewer: {{ reviewer }} · Model: {{ MODEL }}
Severity: {{ kind(severity) }} · Findings: {{ len(findings) }}/{{ MAX_FINDINGS }}

> {% for finding in findings %}

- **L{{ finding.line }}** ({{ kind(finding.severity) }}): {{ finding.message }}

> {% /for %}

> {% match verdict %}
> {% case Approved %}

✅ **Approved** — ship it.

> {% case NeedsChanges %}

🔄 **Needs changes:** {{ verdict.reason }}

> {% case Rejected %}

❌ **Rejected:** {{ verdict.reason }}

> {% /match %}
```

Types and fields are validated at build time — rendering happens at runtime:

```rust
use prompt_templates_macros::include_template;

include_template!("prompts/code_review.tmpl.md");

let output = code_review::Params {
    reviewer: "Alice".into(),
    file_path: "src/main.rs".into(),
    severity: code_review::Severity::High,
    findings: vec![
        code_review::FindingsItem {
            line: 42,
            message: "unused import".into(),
            severity: code_review::Severity::Low,
        },
    ],
    verdict: code_review::Verdict::NeedsChanges {
        reason: "remove dead imports".into(),
    },
}
.render()
.unwrap();
```

Wrong types, missing fields, non-exhaustive matches — all caught at **build time**.
Templates can also be loaded and validated at runtime for dynamic or hot-reload use cases.

## Why?

| Problem                       | Solution                                                                 |
| ----------------------------- | ------------------------------------------------------------------------ |
| Prompts buried in code        | **Markdown-native** — `.tmpl.md` files render in any editor or on GitHub |
| Errors found in production    | **Strict typing** — caught at build time (Rust) or render time           |
| LLMs drift templates silently | **Agent-safe** — the engine catches drift immediately                    |
| Syntax breaks in previews     | **Markdown-safe** — renders cleanly in any viewer, even unrendered       |

## Features

| Feature                    | Description                                                                              |
| -------------------------- | ---------------------------------------------------------------------------------------- |
| **Typed parameters**       | `str`, `int`, `float`, `bool`, `list(…)`, `struct(…)`, `enum(…)`, `option(…)`, `tmpl(…)` |
| **Type aliases**           | `types:` defines reusable named types                                                    |
| **Cross-template imports** | `imports:` pulls types via dotted paths (`stem.TypeName`)                                |
| **Typed lists**            | `list(title = str, score = int)` — iterate with `{% for %}`, fields validated            |
| **Enum dispatch**          | `match`/`case` with exhaustiveness checking and field narrowing                          |
| **Includes as links**      | `{% include [name](path.tmpl.md) with … %}` — clickable, type-checked                    |
| **Inline templates**       | `{% tmpl name %}` — reusable fragments without separate files                            |
| **Constants**              | `consts:` for file-scoped immutable values                                               |
| **Built-in functions**     | `idx(b)`, `len(x)`, `kind(x)`, `has(x)` + filters (`upper`, `lower`, `trim`, `join`, …)  |
| **Readable as markdown**   | `> {% %}` blockquote prefix keeps control flow visually separated from prose             |
| **Markdown-safe syntax**   | Valid YAML frontmatter, clean `()` type syntax — looks good even unrendered              |

> **Note:** The `> ` prefix is required only on `{% %}` tag lines — it is
> stripped before compilation. Content lines between tags are normal text
> and should not use `> `. A content line starting with `> ` is kept
> verbatim as a literal markdown blockquote in the output.

## Quick Start

<!-- prettier-ignore -->
````carousel
```rust
use prompt_templates_macros::include_template;

// Parsed + validated at build time — generates typed structs from frontmatter
include_template!("prompts/task_report.tmpl.md");

let output = task_report::Params {
    title: "Sprint 42".into(),
    priority: task_report::Priority::Critical,
    tasks: vec![
        task_report::TasksItem {
            name: "Fix auth bypass".into(),
            urgency: task_report::Priority::Critical,
        },
        task_report::TasksItem {
            name: "Update dependencies".into(),
            urgency: task_report::Priority::Low,
        },
    ],
}
.render()
.unwrap();
```
<!-- slide -->
```python
from prompt_templates import template

# Auto-generates Priority enum + TasksItem class from frontmatter
report = template("prompts/task_report.tmpl.md")

output = report.render(
    title="Sprint 42",
    priority=report.Priority.Critical,
    tasks=[
        {"name": "Fix auth bypass", "urgency": report.Priority.Critical},
        {"name": "Update dependencies", "urgency": report.Priority.Low},
    ],
)
```
<!-- slide -->
```go
import pt "github.com/domenukk/prompt-templates/go/prompt_templates"

tmpl, _ := pt.FromSource(`---
types:
  - Priority = enum(Critical, High, Medium, Low)

params:
  - title = str
  - priority = Priority
  - tasks = list(name = str, urgency = Priority)
---

# Task Report: {{ title }}

Priority: {{ priority }}

> {% for task in tasks %}

- {{ task.name }} ({{ task.urgency }})

> {% /for %}`)
defer tmpl.Close()

result, _ := tmpl.RenderMap(map[string]any{
    "title":    "Sprint 42",
    "priority": pt.Variant{Kind: "Critical"},
    "tasks": []map[string]any{
        {"name": "Fix auth bypass", "urgency": pt.Variant{Kind: "Critical"}},
        {"name": "Update dependencies", "urgency": pt.Variant{Kind: "Low"}},
    },
})
```
<!-- slide -->
```typescript
import { Template, defineVariants } from "prompt-templates";

const Priority = defineVariants({
  Critical: null, High: null, Medium: null, Low: null,
});

const tmpl = Template.fromSource(`---
types:
  - Priority = enum(Critical, High, Medium, Low)

params:
  - title = str
  - priority = Priority
  - tasks = list(name = str, urgency = Priority)
---

# Task Report: {{ title }}

Priority: {{ priority }}

> {% for task in tasks %}

- {{ task.name }} ({{ task.urgency }})

> {% /for %}`);

console.log(tmpl.render({
  title: "Sprint 42",
  priority: Priority.Critical(),
  tasks: [
    { name: "Fix auth bypass", urgency: Priority.Critical() },
    { name: "Update dependencies", urgency: Priority.Low() },
  ],
}));
```
````

## Performance

Built for speed — zero-allocation rendering in Rust, native FFI in all bindings.

### Rust (render-only, pre-parsed)

| Scenario        | prompt-templates |    Tera | MiniJinja | Handlebars |
| --------------- | ---------------: | ------: | --------: | ---------: |
| **simple**      |    **130 ns** 🏆 |  213 ns |    558 ns |     632 ns |
| **loop**        |    **445 ns** 🏆 |  618 ns |   2.00 µs |    2.85 µs |
| **conditional** |    **173 ns** 🏆 |  348 ns |    625 ns |    1.16 µs |
| **hero**        |   **2.07 µs** 🏆 | 2.09 µs |   7.62 µs |    21.4 µs |
| **mega**        |   **10.1 µs** 🏆 | 11.1 µs |   30.1 µs |    84.7 µs |

### Cross-Language (render-only)

| Language       |    simple |       loop | conditional |      large |
| -------------- | --------: | ---------: | ----------: | ---------: |
| **Rust**       | 196 ns 🏆 | 1.11 µs 🏆 |         N/A | 22.1 µs 🏆 |
| **Go**         |    525 ns |   1,716 ns |         N/A |  24,808 ns |
| Go `text/tmpl` |    569 ns |   6,251 ns |         N/A | 140,495 ns |
| **TypeScript** |    610 ns |   4,135 ns |    2,011 ns |        N/A |
| **Python**     |   0.84 µs |    1.74 µs |     0.87 µs |    6.62 µs |

See the language-specific READMEs or the [benchmarks suite](benchmarks/README.md) for full details and methodology.

## Language Bindings

### Rust ([README](crates/prompt-templates/README.md))


Build-time validation via proc macros, plus a full runtime API for dynamic loading. Pre-parsed templates with zero-overhead rendering. Includes `ctx!` macro, `TypedBuilder`, `serde`, hot-reload, caching, and benchmarks.

### Python ([README](crates/prompt-templates-python/README.md))

Includes generated types, import hooks, pattern matching, enum construction, caching, and benchmarks.

### Go ([README](go/prompt_templates/README.md))

Includes `RenderStruct`, `TaggedVariant`, codegen, caching, and benchmarks.

### TypeScript ([README](crates/prompt-templates-typescript/README.md))

Includes `TypedTemplate(P)`, `defineVariants`, codegen, WASM backend, and benchmarks.

### WASM ([README](crates/prompt-templates-wasm/README.md))

WebAssembly bindings wrapping the full Rust engine via `wasm-bindgen`. Same `ITemplate` interface as the pure-TypeScript package.

## Full Reference

See **[SPEC.md](SPEC.md)** for the complete syntax — all control-flow
tags, filters, built-in functions, whitespace control, and error
diagnostics.

## License

Apache-2.0 OR MIT
