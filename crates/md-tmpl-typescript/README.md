# md-tmpl

Strongly-typed prompt templates for LLMs.

## The Cool Part

Define a prompt as a `.tmpl.md` file — readable markdown with typed frontmatter:

```markdown
---
consts:
  - MAX_RETRIES = int := 3

types:
  - Priority = enum(High, Medium, Low)

params:
  - role = str
  - tasks = list(title = str, priority = Priority)
  - outcome = enum(Confirmed(evidence = str), Rejected)
---

You are {{ role }}. You have {{ len(tasks) }} tasks:

> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority | upper }})

> {% /for %}

> {% match outcome %}
> {% when Confirmed %}

✅ Confirmed — {{ outcome.evidence }}

> {% when Rejected %}

❌ Rejected. Retry up to {{ MAX_RETRIES }} times.

> {% /match %}
```

Generate TypeScript types from that file, then render with full type safety:

```ts
import type { Params } from "./generated/agent_task.js";
import { TypedTemplate, defineVariants, match } from "md-tmpl";

const tmpl = TypedTemplate.fromFile<Params>("prompts/agent_task.tmpl.md");

const Outcome = defineVariants({
  Confirmed: ["evidence"],
  Rejected: null,
});

const result = tmpl.render({
  role: "a code review agent",
  tasks: [
    { title: "Check error handling", priority: "High" },
    { title: "Verify test coverage", priority: "Medium" },
  ],
  outcome: Outcome.Confirmed({ evidence: "All edge cases covered" }),
});

// Pattern-match the outcome later
const summary = match(outcome, {
  Confirmed: (f) => `✅ ${f.evidence}`,
  Rejected: () => "❌ Needs revision",
});
```

The generated types look like this — interfaces, enums, constants, and defaults:

```ts
export interface TasksItem {
  readonly title: string;
  readonly priority: Priority;
}

export type Priority = "High" | "Medium" | "Low";

export interface Outcome_Confirmed {
  readonly __kind__: "Confirmed";
  readonly evidence: string;
}

export type Outcome = Outcome_Confirmed | "Rejected";

export interface Params {
  readonly role: string;
  readonly tasks: readonly TasksItem[];
  readonly outcome: Outcome;
}

export const CONSTANTS = {
  MAX_RETRIES: 3,
} as const;
```

## Why?

- **Markdown-native** — prompts live in `.tmpl.md` files, readable in any editor or on GitHub. No embedded strings, no escaping.
- **Strict typing** — every parameter declares a type; `generateTypes()` emits TypeScript interfaces from frontmatter. Catch errors before deployment, not at 3 AM in production.
- **Agent-safe** — when an LLM edits prompts, the engine catches drift immediately. Renamed a field? Changed a type? Broke the contract? You'll know before it ships.

## Installation

Available on npm: <https://www.npmjs.com/package/md-tmpl>

```bash
npm install md-tmpl
```

## Type Generation

`generateTypes()` turns frontmatter into full TypeScript — interfaces, enums, constants, and defaults. Run it as a build step:

```ts
import { generateTypesFromFile } from "md-tmpl";
import * as fs from "node:fs";

fs.writeFileSync(
  "src/generated/agent_task.ts",
  generateTypesFromFile("prompts/agent_task.tmpl.md"),
);
```

Or generate from a raw source string:

```ts
import { generateTypes } from "md-tmpl";

const code = generateTypes(`---
consts:
  - MAX_RETRIES = int := 3

params:
  - message = str
  - level = int := 1
  - tasks = list(title = str, priority = str)
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
{{ message }}`);
```

Output includes `Params`, item interfaces, enum unions, `CONSTANTS`, and `DEFAULTS` — ready to import.

## Enum Dispatch

`defineVariants` creates type-safe enum constructors. `match` gives you exhaustive pattern matching. `isVariant` is a type guard.

```ts
import { defineVariants, match, isVariant } from "md-tmpl";

const Status = defineVariants({
  Done: ["summary"],
  InProgress: null,
  Blocked: ["reason"],
});

// Construct
const status = Status.Done({ summary: "All tests pass" });

// Pattern match — exhaustive over all variants
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
  - tasks = list(title = str, priority = str)
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
{{ has(field) }}          → true if option(T) is present
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

tmpl.render(params: P)              // TS type-checked + runtime validated
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

Both `Template` and the WASM `Template` implement `ITemplate`. Write
backend-agnostic code:

```ts
import type { ITemplate } from "md-tmpl";

function renderGreeting(tmpl: ITemplate, name: string): string {
  return tmpl.render({ name });
}
```

## Performance

### Internal Benchmarks

Node.js 22, single-template timings (lower is better):

| Scenario                 |     render | renderUnchecked |
| ------------------------ | ---------: | --------------: |
| simple (1 str)           | **425 ns** |          449 ns |
| multi-param (4 types)    |   1,084 ns |      **931 ns** |
| list (2 items)           |   2,991 ns |    **1,757 ns** |
| list (20 items)          |  21,708 ns |             N/A |
| enum unit variant        |     535 ns |             N/A |
| enum struct variant      |   1,354 ns |             N/A |
| filters (idx+add, upper) |   5,537 ns |             N/A |
| if/elif/else             |   1,285 ns |    **1,065 ns** |

`renderUnchecked()` skips runtime type validation — use it when TypeScript's
static checks are sufficient.

### Comparison Benchmarks

Node.js 22, 50,000 iterations, best of 5 runs
([source](src/benchmarks/comparison.ts)).

**Render only** (pre-parsed template + data → output):

<!-- BENCHMARK:TS_COMPARISON_RENDER -->

| Scenario           |      render() | renderUnchecked() |      Handlebars |      Mustache |
| ------------------ | ------------: | ----------------: | --------------: | ------------: |
| **simple**         | **699 ns** 🏆 |            705 ns |          918 ns |        886 ns |
| **loop (5 items)** |      4,091 ns |          2,802 ns | **1,571 ns** 🏆 |      1,845 ns |
| **conditional**    |      1,371 ns |          1,288 ns |        1,424 ns | **444 ns** 🏆 |

<!-- /BENCHMARK:TS_COMPARISON_RENDER -->

**Round-trip** (parse + render):

<!-- BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

| Scenario           |   md-tmpl | Handlebars |        Mustache |
| ------------------ | --------: | ---------: | --------------: |
| **simple**         |  7,191 ns |  77,066 ns |   **895 ns** 🏆 |
| **loop (5 items)** | 16,263 ns | 108,863 ns | **1,811 ns** 🏆 |
| **conditional**    |       N/A |        N/A |             N/A |

<!-- /BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

> **Note:** Mustache wins raw render speed due to its minimal feature set — no
> type checking, no filters, no `elif`. md-tmpl wins `renderUnchecked()`
> for simple templates. The value proposition is **type safety and correctness**,
> not raw throughput.

A [WASM build](../md-tmpl-wasm/README.md) (~200 KB `.wasm`) is also
available for exact feature parity. Pure TypeScript is 2–6× faster for small
templates due to JS↔WASM serialization overhead; WASM closes the gap on complex
templates.

```bash
just bench-ts           # internal benchmarks
just bench-ts-compare   # vs Handlebars & Mustache
```

## Testing

```bash
just test-ts    # 798 tests
just lint-ts    # strict type-check with tsc
just fmt-ts     # format with prettier
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
