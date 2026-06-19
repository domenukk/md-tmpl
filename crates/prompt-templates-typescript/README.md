# prompt-templates for TypeScript

[![CI](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml/badge.svg)](https://github.com/domenukk/prompt-templates/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/prompt-templates.svg)](https://www.npmjs.com/package/prompt-templates)

Strongly-typed prompt templates for LLMs.
Templates are markdown files with YAML frontmatter declaring typed
parameters — every variable, list shape, and enum variant is validated
before rendering.

A [WASM backend](../prompt-templates-wasm/README.md) wrapping the full
Rust engine is also available for exact feature parity.

## Why?

LLM prompts grow complex — multi-shot examples, tool schemas, agentic
workflows — but most TypeScript projects still manage them as inline
template literals or untyped Handlebars/Mustache templates.

**Inline template literals** mix prose with code, making prompts
unreadable and hard to review. **Untyped template engines** push every
error to runtime: rename a variable, add a field, change a list
shape — you discover it when the prompt renders garbage in production.

`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, not template literals. They render as clean markdown in any editor or on GitHub — includes are clickable links, and control flow uses blockquote-prefixed lines so it stays visually separated from prose.
- **Strict typing** — every parameter declares a type; mismatches are caught before rendering. `generateTypes()` emits TypeScript interfaces from frontmatter.
- **Agent-safe** — when an LLM writes or edits prompts, the engine catches drift immediately instead of letting it propagate.

## Installation

```bash
npm install prompt-templates
```

## Quick Start

```ts
import { Template } from "prompt-templates";

const tmpl = Template.fromSource(`
---
params:
  - name = str
  - role = str
---
You are {{ role }}. Hello {{ name }}!`);

console.log(tmpl.render({ name: "Alice", role: "an AI assistant" }));
// → "You are an AI assistant. Hello Alice!"
```

## API

### Template

```ts
// Constructors
Template.fromSource(source: string): Template
Template.fromSourceAllowingUnused(source): Template
Template.fromSourceWithBaseDir(source, dir): Template
Template.fromFile(path: string): Template

// Rendering
tmpl.render(params, options?)        // type-validated
tmpl.renderUnchecked(params)         // skip validation (fastest)
tmpl.renderDict(params, options?)    // from Map or Record
// options: { allowExtra?: boolean }

// Metadata
tmpl.declarations()                  // → [["name", "str"], ["count", "int"]]
tmpl.sourceHash()                    // content hash
tmpl.body()                          // template body after frontmatter
tmpl.defaults()                      // → { count: 5 }
tmpl.consts()                        // → { MAX_RETRIES: 3 }
tmpl.frontmatter                     // parsed frontmatter
tmpl.setMaxIncludeDepth(depth)
tmpl.validateDeclarationsAgainst(expected)
```

### TypedTemplate\<P\>

Compile-time type-checked wrapper — use with generated types:

```ts
import type { Params } from "./my_template.js";
import { TypedTemplate } from "prompt-templates";

const tmpl = TypedTemplate.fromSource<Params>(`---
params:
  - name = str
  - count = int
---
{{ name }} ({{ count }})`);

tmpl.render({ name: "Alice", count: 42 }); // ✅ type-checked
// tmpl.render({ name: "Alice" });           // ❌ TS error: missing 'count'

tmpl.renderUnchecked(params); // skip validation, trust TS types
tmpl.renderTrusted(params); // validate first call, skip rest
```

### TemplateCache

```ts
const cache = new TemplateCache();
const tmpl = cache.load("prompts/greeting.tmpl.md");
cache.templateCount();
cache.clear();
```

### ITemplate Interface

Both the pure-TypeScript `Template` and the WASM `Template` implement
`ITemplate`. Write backend-agnostic code:

```ts
import type { ITemplate } from "prompt-templates";

function renderGreeting(tmpl: ITemplate, name: string): string {
  return tmpl.render({ name });
}
```

## Type Generation

Generate TypeScript interfaces from frontmatter:

```ts
import {
  generateTypes,
  generateTypesFromFile,
  inferTypes,
} from "prompt-templates";

const code = generateTypes(`
---
consts:
  - MAX_RETRIES = int := 3
params:
  - message = str
  - level = int := 1
  - tasks = list<title = str, priority = str>
  - outcome = enum<Confirmed(evidence = str), Rejected>
---
{{ message }}`);
```

Output:

```ts
export interface TasksItem {
  readonly title: string;
  readonly priority: string;
}

export interface Outcome_Confirmed {
  readonly __kind__: "Confirmed";
  readonly evidence: string;
}

export type Outcome = Outcome_Confirmed | "Rejected";

export interface Params {
  readonly message: string;
  readonly level?: number;
  readonly tasks: readonly TasksItem[];
  readonly outcome: Outcome;
}

export const CONSTANTS = {
  MAX_RETRIES: 3,
} as const;

export const DEFAULTS: Partial<Params> = {
  level: 1,
};
```

Build script:

```ts
import { generateTypesFromFile } from "prompt-templates";
import * as fs from "node:fs";

fs.writeFileSync(
  "src/greeting.ts",
  generateTypesFromFile("prompts/greeting.tmpl.md"),
);
```

## Features

### Typed Lists

```ts
const tmpl = Template.fromSource(`---
params:
  - tasks = list<title = str, priority = str>
---
> {% for task in tasks %}

- {{ task.title }}: {{ task.priority }}
> {% /for %}`);

tmpl.render({
  tasks: [
    { title: "Write documentation", priority: "High" },
    { title: "Add unit tests", priority: "Medium" },
  ],
});
```

### Default Values

```ts
const tmpl = Template.fromSource(`---
params:
  - greeting = str := "Hello"
  - name = str
---
{{ greeting }}, {{ name }}!`);

tmpl.render({ name: "Alice" }); // → "Hello, Alice!"
tmpl.defaults(); // → { greeting: "Hello" }
```

### Enum Dispatch

```ts
import { defineVariants, match, isVariant, Template } from "prompt-templates";

const Status = defineVariants({
  Done: ["summary"],
  InProgress: null,
  Blocked: ["reason"],
});

tmpl.render({ status: Status.Done({ summary: "All tests pass" }) });

// Pattern matching
const msg = match(status, {
  Done: (f) => `✅ ${f.summary}`,
  InProgress: () => "🔄 Working...",
  Blocked: (f) => `❌ ${f.reason}`,
});

// Wildcard fallback
const simple = match(status, {
  Done: (f) => f.summary as string,
  _: () => "pending",
});

// Type guard
if (isVariant(status, "Done")) {
  console.log("Completed!");
}
```

### Filters

```
{{ name | upper }}        → ALICE
{{ name | lower }}        → alice
{{ name | trim }}         → (strips whitespace)
{{ score | fixed(2) }}    → 3.14
{{ items | join(", ") }}  → a, b, c
{{ items | limit(2) }}    → first 2 elements
{{ count | add(1) }}      → 43
{{ count | sub(1) }}      → 41
{{ name | trim | upper }} → chains work
```

### Built-in Functions

```
{{ len(items) }}          → 3 (list length)
{{ len(name) }}           → 5 (string length)
{{ idx(item) }}               → 0, 1, 2, … (loop index)
{{ kind(status) }}        → "Done" (variant name)
{{ has(field) }}          → true if option<T> is present
```

### If/Elif/Else

```markdown
> {% if level == 1 %}

Beginner: {{ name }}

> {% elif level == 2 %}

Intermediate: {{ name }}

> {% else %}

Expert: {{ name }}

> {% /if %}
```

## Performance

Node.js 22, 50,000 iterations, best of 5 runs
([source](src/benchmarks/comparison.ts)).

**Render only** (pre-compiled template + data → output):

<!-- BENCHMARK:TS_COMPARISON_RENDER -->

| Scenario           | prompt-templates |      Handlebars |      Mustache |
| ------------------ | ---------------: | --------------: | ------------: |
| **simple**         |    **690 ns** 🏆 |        1,057 ns |        767 ns |
| **loop (5 items)** |         6,301 ns | **2,386 ns** 🏆 |      3,462 ns |
| **conditional**    |         1,219 ns |        1,418 ns | **906 ns** 🏆 |

<!-- /BENCHMARK:TS_COMPARISON_RENDER -->

**Round-trip** (compile + render):

<!-- BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

| Scenario           | prompt-templates | Handlebars |        Mustache |
| ------------------ | ---------------: | ---------: | --------------: |
| **simple**         |         6,150 ns |  81,294 ns |   **843 ns** 🏆 |
| **loop (5 items)** |        16,399 ns | 156,028 ns | **3,477 ns** 🏆 |

<!-- /BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

`renderUnchecked()` skips runtime type validation for ~746 ns simple render.

A [WASM build](../prompt-templates-wasm/README.md) wrapping the Rust
engine is also available (~200 KB `.wasm`). Pure TypeScript is 2–6×
faster for small templates due to JS↔WASM serialization overhead; WASM
closes the gap on complex templates and provides exact Rust parity.

```bash
just bench-ts           # internal benchmarks
just bench-ts-compare   # vs Handlebars & Mustache
```

## Testing

```bash
just test-ts    # 486 tests
just lint-ts    # strict type-check with tsc
just fmt-ts     # format with prettier
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
