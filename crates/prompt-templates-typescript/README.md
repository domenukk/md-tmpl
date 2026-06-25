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

Inline template literals are unreadable. Untyped Handlebars/Mustache templates break at runtime.
`prompt-templates` gives you:

- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor or on GitHub.
- **Strict typing** — every parameter declares a type; `generateTypes()` emits TypeScript interfaces from frontmatter.
- **Agent-safe** — when an LLM edits prompts, the engine catches drift immediately.

## Installation

```bash
npm install prompt-templates
```

## Quick Start

### Type-Safe Templates (recommended)

Generate TypeScript interfaces from frontmatter, then use
`TypedTemplate<P>` for compile-time checked rendering:

**1. Generate types** (build script or CLI):

```ts
import { generateTypesFromFile } from "prompt-templates";
import * as fs from "node:fs";

fs.writeFileSync(
  "src/generated/greeting.ts",
  generateTypesFromFile("prompts/greeting.tmpl.md"),
);
```

This produces interfaces like:

```ts
export interface Params {
  readonly name: string;
  readonly count: number;
}
```

**2. Use with `TypedTemplate<P>`**:

```ts
import type { Params } from "./generated/greeting.js";
import { TypedTemplate } from "prompt-templates";

const tmpl = TypedTemplate.fromFile<Params>("prompts/greeting.tmpl.md");

tmpl.render({ name: "Alice", count: 42 }); // ✅ type-checked
// tmpl.render({ name: "Alice" });           // ❌ TS error: missing 'count'

tmpl.renderTrusted(params); // validate first call, skip rest — fast path
```

### Inline Templates

For quick prototyping, parse templates inline:

```ts
import { Template } from "prompt-templates";

const tmpl = Template.fromSource(`---
params:
  - name = str
  - role = str
---
You are {{ role }}. Hello {{ name }}!`);

console.log(tmpl.render({ name: "Alice", role: "an AI assistant" }));
// → "You are an AI assistant. Hello Alice!"
```

## Type Generation

`generateTypes()` turns frontmatter into full TypeScript — interfaces,
enums, constants, and defaults:

```ts
import { generateTypes } from "prompt-templates";

const code = generateTypes(`---
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

## Enum Dispatch

`defineVariants` creates type-safe enum constructors with pattern matching:

```ts
import { defineVariants, match, isVariant, Template } from "prompt-templates";

const Status = defineVariants({
  Done: ["summary"],
  InProgress: null,
  Blocked: ["reason"],
});

tmpl.render({ status: Status.Done({ summary: "All tests pass" }) });

// Pattern matching — exhaustive over all variants
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
{{ idx(item) }}           → 0, 1, 2, … (loop index)
{{ kind(status) }}        → "Done" (variant name)
{{ has(field) }}          → true if option<T> is present
```

## API Reference

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

```ts
TypedTemplate.fromSource<P>(source): TypedTemplate<P>
TypedTemplate.fromFile<P>(path): TypedTemplate<P>

tmpl.render(params: P)              // TS compile-time + runtime checked
tmpl.renderUnchecked(params: P)     // skip runtime validation, trust TS types
tmpl.renderTrusted(params: P)       // validate first call, skip rest
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

## Performance

Node.js 22, 50,000 iterations, best of 5 runs
([source](src/benchmarks/comparison.ts)).

**Render only** (pre-compiled template + data → output):

<!-- BENCHMARK:TS_COMPARISON_RENDER -->

| Scenario           | render() | renderUnchecked() |      Handlebars |      Mustache |
| ------------------ | -------: | ----------------: | --------------: | ------------: |
| **simple**         |   752 ns |     **723 ns** 🏆 |          942 ns |        732 ns |
| **loop (5 items)** | 6,750 ns |          4,413 ns | **2,195 ns** 🏆 |      3,389 ns |
| **conditional**    | 1,280 ns |          1,205 ns |        1,201 ns | **897 ns** 🏆 |

<!-- /BENCHMARK:TS_COMPARISON_RENDER -->

**Round-trip** (compile + render):

<!-- BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

| Scenario           | prompt-templates | Handlebars |        Mustache |
| ------------------ | ---------------: | ---------: | --------------: |
| **simple**         |         6,828 ns |  78,160 ns |   **920 ns** 🏆 |
| **loop (5 items)** |        18,862 ns | 156,362 ns | **3,500 ns** 🏆 |
| **conditional**    |              N/A |        N/A |             N/A |

<!-- /BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

`renderUnchecked()` skips runtime type validation for ~723 ns simple render.

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
