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
> {% case Confirmed %}

✅ Confirmed — {{ outcome.evidence }}

> {% case Rejected %}

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

### Type Mapping

| Frontmatter Type            | TypeScript Type                                 |
| :-------------------------- | :---------------------------------------------- |
| `str`                       | `string`                                        |
| `int`                       | `number`                                        |
| `float`                     | `number`                                        |
| `bool`                      | `boolean`                                       |
| `list(field = type, ...)`   | `readonly GeneratedItem[]`                      |
| `list(type)`                | `readonly T[]` (e.g. `readonly string[]`)       |
| `struct(field = type, ...)` | `GeneratedInterface`                            |
| `enum(Variant, ...)`        | Union type (`Variant1 \| Variant2_Struct`)      |
| `option(type)`              | `T \| null` (`T \| undefined` is also accepted) |
| `tmpl(...)`                 | `ITemplate` (or callable template reference)    |

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

### Environment Variables

Inject values at compile time from the build environment using `env:`
declarations. Env vars are resolved once when the template is compiled and
behave like constants at render time.

```ts
const tmpl = Template.fromSourceWithEnv(
  `---
params:
  - name = str
env:
  - MODEL = str
  - MAX_TOKENS = int := 4096
---
Hello {{ name }}! Using {{ MODEL }} (max {{ MAX_TOKENS }} tokens).`,
  { env: { MODEL: "gemini-2.0-flash" } },
);

tmpl.render({ name: "Alice" });
// → "Hello Alice! Using gemini-2.0-flash (max 4096 tokens)."
```

Or load from a file with env:

```ts
const tmpl = Template.fromFileWithEnv("prompts/agent.tmpl.md", {
  env: { MODEL: "gemini-2.0-flash", MAX_TOKENS: "8192" },
});
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

````
{{ name | upper }}            → ALICE
{{ name | lower }}            → alice
{{ name | trim }}             → (strips whitespace)
{{ score | fixed(2) }}        → 3.14
{{ items | join(", ") }}      → a, b, c
{{ items | limit(2) }}        → first 2 elements
{{ count | add(1) }}          → 43
{{ count | sub(1) }}          → 41
{{ name | trim | upper }}     → chains work
{{ val | escape_xml }}        → &lt;tag&gt; (or | xml)
{{ val | escape_json }}       → safe\/json (or | json)
{{ text | sanitize_tokens }}  → neutralizes LLM control tokens (or sanitize_tokens("tag"))
{{ code | fence("ts") }}      → ```ts\ncode\n``` (adaptive)
{{ input | quarantine }}      → <untrusted_content>...</untrusted_content> (or quarantine("tag"))
````

### Built-in Functions

```
{{ len(items) }}          → 3 (list length)
{{ len(name) }}           → 5 (string length)
{{ idx(item) }}           → 0, 1, 2, … (loop index)
{{ kind(status) }}        → "Done" (variant name)
{{ kinds(Status) }}       → ["Done", "InProgress", "Blocked"] (all variant names)
{{ has(field) }}          → true if option(T) is present
```

## Browser

The core engine reads files synchronously (via `node:fs`), which browsers can't
do. The `md-tmpl/browser` entry point bridges this with a lazy, fetch-on-demand
loader — **nothing is preloaded**; only the files a given render actually
reaches are fetched over the network, then cached.

```ts
import { loadTemplate } from "md-tmpl/browser";

// Fetches the entry (and its `imports:`) up front.
const tmpl = await loadTemplate("/prompts/agent.tmpl.md");

// renderAsync fetches any `{% include %}` this render reaches — including
// dynamic, param-dependent include paths — on demand, then caches them.
const out = await tmpl.renderAsync({ role: "reviewer" });

// Once the cache is warm, the same shapes can render synchronously.
const out2 = tmpl.render({ role: "planner" });
```

`loadTemplate(url, options?)` options: `env` (values for `env:` frontmatter),
`fetch` (custom transport for auth/testing), `baseUrl` (for relative URLs;
defaults to the document location), and `maxRounds` (defensive fetch cap).

For custom runtimes, compose the primitives directly:
`setFileSystemProvider(fs, path)` with the exported `MemoryFs` and `posixPath`.

## API Reference

### Template

```ts
// Constructors
Template.fromSource(source: string): Template
Template.fromSourceWithBaseDir(source, dir): Template
Template.fromSourceWithEnv(source, env: Record<string, unknown>): Template
Template.fromSourceWithOptions(source, options: CompileOptions): Template
Template.fromFile(path: string): Template
Template.fromFileWithEnv(path, options?: CompileOptions): Template

// Rendering
tmpl.render(params, options?)        // type-validated
tmpl.renderEmpty()                   // render with defaults only (no params)
tmpl.renderDict(params, options?)    // from Map or Record
// options: { allowExtra?: boolean, allowUnused?: boolean }

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

Node.js 22, single-template timings with full runtime schema validation and cold
parsing (`Template.fromSource` with zero cross-template caching, lower is
better):

| Scenario                   |    render | cold parse |
| -------------------------- | --------: | ---------: |
| simple (1 str)             |     66 ns |   3,455 ns |
| multi-param (4 types)      |    229 ns |   7,624 ns |
| list (2 items)             |    378 ns |   9,441 ns |
| list (20 items)            |  2,759 ns |   9,441 ns |
| enum unit variant          |    255 ns |  16,530 ns |
| enum struct variant        |    299 ns |  16,530 ns |
| filters (idx+add, upper)   |    875 ns |  11,495 ns |
| if/elif/else               |    163 ns |  14,235 ns |
| hero (nested + fixed)      |  3,115 ns |  44,372 ns |
| mega (3 teams, 15 members) | 12,848 ns |  58,065 ns |

### Comparison Benchmarks

Node.js 22, best of 5 runs ([source](src/benchmarks/comparison.ts)). Every
`md-tmpl` `render()` call performs full runtime schema validation (recursive
type checking and missing/extra parameter rejection) plus runtime `| fixed(1)`
float formatting and numeric comparisons; Handlebars and Mustache perform no
schema validation and receive pre-formatted strings and boolean helper flags for
features they lack (`elif`, numeric comparisons, `fixed(1)`).

**Render only** (pre-parsed template + data → output):

<!-- BENCHMARK:TS_COMPARISON_RENDER -->

| Scenario           |          md-tmpl | Handlebars |  Mustache |
| ------------------ | ---------------: | ---------: | --------: |
| **simple**         |    **121 ns** 🏆 |     968 ns |    472 ns |
| **loop (3 items)** |    **625 ns** 🏆 |   1,590 ns |  1,086 ns |
| **conditional**    |    **177 ns** 🏆 |   1,336 ns |    888 ns |
| **hero**           |  **3,030 ns** 🏆 |   7,673 ns |  4,670 ns |
| **mega**           | **11,664 ns** 🏆 |  25,029 ns | 17,684 ns |

<!-- /BENCHMARK:TS_COMPARISON_RENDER -->

**Round-trip** (parse + render — `md-tmpl` performs full YAML frontmatter
parsing, static type/displayability/collision analysis, AST compilation, and
runtime schema validation on every call; Mustache only tokenizes tags with zero
type checking):

<!-- BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

| Scenario           |          md-tmpl | Handlebars |         Mustache |
| ------------------ | ---------------: | ---------: | ---------------: |
| **simple**         |  **5,589 ns** 🏆 |  83,198 ns |         6,319 ns |
| **loop (3 items)** |        10,516 ns | 122,275 ns |  **7,307 ns** 🏆 |
| **conditional**    | **13,649 ns** 🏆 | 185,236 ns |        13,783 ns |
| **hero**           |        45,370 ns | 506,590 ns | **27,943 ns** 🏆 |
| **mega**           |        65,636 ns | 740,898 ns | **50,189 ns** 🏆 |

<!-- /BENCHMARK:TS_COMPARISON_ROUNDTRIP -->

A [WASM build](../md-tmpl-wasm/README.md) (~200 KB `.wasm`) is also
available for exact Rust-engine feature parity (such as `renderFlexbuffers`).
Pure TypeScript is 5–35× faster across rendering scenarios (and 2.6× faster on
cold parse) because it avoids JS↔WASM boundary serialization overhead.

```bash
just bench-ts           # internal benchmarks
just bench-ts-compare   # vs Handlebars & Mustache
```

## Testing

```bash
just test-ts    # 1,935 tests
just lint-ts    # strict type-check with tsc
just fmt-ts     # format with prettier
```

## Full Reference

See **[SPEC.md](../../SPEC.md)** for the complete syntax reference.

## License

Apache-2.0 OR MIT
