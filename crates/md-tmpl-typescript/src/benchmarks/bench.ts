/**
 * Benchmarks for md-tmpl TypeScript bindings.
 *
 * Measures parse + render throughput for common template patterns.
 *
 * Usage:
 *   npm run build && node dist/benchmarks/bench.js
 */

import { Template } from "../index.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

interface BenchResult {
  name: string;
  ops: number;
  nsPerOp: number;
  msTotal: number;
}

const BENCH_REPEAT = 5; // best-of-N for stability

/**
 * Run a function `iterations` times, repeat `BENCH_REPEAT` times, and return
 * timing statistics from the best (fastest) run. Using min eliminates GC
 * pauses, JIT compilation jitter, and OS scheduling noise.
 */
function bench(name: string, fn: () => void, iterations: number): BenchResult {
  // Warmup
  for (let i = 0; i < Math.min(iterations, 1000); i++) fn();

  let bestElapsed = Infinity;
  for (let round = 0; round < BENCH_REPEAT; round++) {
    const start = performance.now();
    for (let i = 0; i < iterations; i++) fn();
    const elapsed = performance.now() - start;
    if (elapsed < bestElapsed) bestElapsed = elapsed;
  }

  const nsPerOp = (bestElapsed * 1_000_000) / iterations;
  return { name, ops: iterations, nsPerOp, msTotal: bestElapsed };
}

/**
 * Return the element at `index`, throwing if it is missing. Used instead of
 * non-null assertions so that out-of-range access surfaces a clear error.
 */
function nth<T>(arr: readonly T[], index: number): T {
  const value = arr[index];
  if (value === undefined) {
    throw new Error(`missing benchmark result at index ${String(index)}`);
  }
  return value;
}

function printResult(r: BenchResult): void {
  const opsPerSec = Math.round(1_000_000_000 / r.nsPerOp);
  console.log(
    `  ${r.name.padEnd(40)} ${Math.round(r.nsPerOp).toString().padStart(8)} ns/op  (${opsPerSec.toLocaleString()} ops/s)`,
  );
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

const SIMPLE_SRC = `---
params:
  - name = str
---
Hello {{ name }}!`;

const MULTI_PARAM_SRC = `---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`;

const LIST_SRC = `---
params:
  - tasks = list(title = str, priority = str)
---
> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority }})

> {% /for %}`;

const ENUM_SRC = `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}

> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`;

const FILTER_SRC = `---
params:
  - items = list(label = str)
---
> {% for item in items %}

{{ idx(item) | add(1) }}. {{ item.label | upper }}

> {% /for %}`;

const CONDITIONAL_SRC = `---
params:
  - level = int
  - name = str
---
> {% if level == 1 %}

Beginner: {{ name }}

> {% elif level == 2 %}

Intermediate: {{ name }}

> {% else %}

Expert: {{ name }}

> {% /if %}`;

// ---------------------------------------------------------------------------
// Run benchmarks
// ---------------------------------------------------------------------------

const ITERATIONS = 50_000;
const PARSE_ITERATIONS = 10_000;

console.log("md-tmpl TypeScript — Benchmarks");
console.log("=".repeat(70));

// Parse benchmarks
console.log("\n📝 Parse benchmarks:");
const parseResults: BenchResult[] = [];

parseResults.push(
  bench(
    "parse: simple (1 param)",
    () => {
      Template.fromSource(SIMPLE_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

parseResults.push(
  bench(
    "parse: multi-param (4 params)",
    () => {
      Template.fromSource(MULTI_PARAM_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

parseResults.push(
  bench(
    "parse: list + for loop",
    () => {
      Template.fromSource(LIST_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

parseResults.push(
  bench(
    "parse: enum + match",
    () => {
      Template.fromSource(ENUM_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

parseResults.push(
  bench(
    "parse: filters + functions",
    () => {
      Template.fromSource(FILTER_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

parseResults.push(
  bench(
    "parse: if/elif/else",
    () => {
      Template.fromSource(CONDITIONAL_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

for (const r of parseResults) printResult(r);

// Render benchmarks (pre-parsed templates)
console.log("\n🚀 Render benchmarks:");
const renderResults: BenchResult[] = [];

const simpleTmpl = Template.fromSource(SIMPLE_SRC);
renderResults.push(
  bench(
    "render: simple (1 str)",
    () => {
      simpleTmpl.render({ name: "world" });
    },
    ITERATIONS,
  ),
);

const multiTmpl = Template.fromSource(MULTI_PARAM_SRC);
renderResults.push(
  bench(
    "render: multi-param (4 types)",
    () => {
      multiTmpl.render({ name: "Alice", count: 42, score: 9.5, enabled: true });
    },
    ITERATIONS,
  ),
);

const listTmpl = Template.fromSource(LIST_SRC);
const smallList = [
  { title: "Task A", priority: "High" },
  { title: "Task B", priority: "Low" },
];
renderResults.push(
  bench(
    "render: list (2 items)",
    () => {
      listTmpl.render({ tasks: smallList });
    },
    ITERATIONS,
  ),
);

const bigList = Array.from({ length: 20 }, (_, i) => ({
  title: `Task ${String(i)}`,
  priority: i % 2 === 0 ? "High" : "Low",
}));
renderResults.push(
  bench(
    "render: list (20 items)",
    () => {
      listTmpl.render({ tasks: bigList });
    },
    ITERATIONS / 5,
  ),
);

const enumTmpl = Template.fromSource(ENUM_SRC);
renderResults.push(
  bench(
    "render: enum unit variant",
    () => {
      enumTmpl.render({ outcome: "Rejected" });
    },
    ITERATIONS,
  ),
);

renderResults.push(
  bench(
    "render: enum struct variant",
    () => {
      enumTmpl.render({
        outcome: { __kind__: "Confirmed", evidence: "proof" },
      });
    },
    ITERATIONS,
  ),
);

const filterTmpl = Template.fromSource(FILTER_SRC);
const filterItems = [
  { label: "first" },
  { label: "second" },
  { label: "third" },
];
renderResults.push(
  bench(
    "render: filters (idx+add, upper)",
    () => {
      filterTmpl.render({ items: filterItems });
    },
    ITERATIONS,
  ),
);

const condTmpl = Template.fromSource(CONDITIONAL_SRC);
renderResults.push(
  bench(
    "render: if/elif/else",
    () => {
      condTmpl.render({ level: 2, name: "Alice" });
    },
    ITERATIONS,
  ),
);

for (const r of renderResults) printResult(r);

// Unchecked render benchmarks (no type validation)
console.log("\n⚡ Render unchecked (no type validation):");
const uncheckedResults: BenchResult[] = [];

uncheckedResults.push(
  bench(
    "renderUnchecked: simple (1 str)",
    () => {
      simpleTmpl.renderUnchecked({ name: "world" });
    },
    ITERATIONS,
  ),
);

uncheckedResults.push(
  bench(
    "renderUnchecked: multi-param (4 types)",
    () => {
      multiTmpl.renderUnchecked({
        name: "Alice",
        count: 42,
        score: 9.5,
        enabled: true,
      });
    },
    ITERATIONS,
  ),
);

uncheckedResults.push(
  bench(
    "renderUnchecked: list (2 items)",
    () => {
      listTmpl.renderUnchecked({ tasks: smallList });
    },
    ITERATIONS,
  ),
);

uncheckedResults.push(
  bench(
    "renderUnchecked: if/elif/else",
    () => {
      condTmpl.renderUnchecked({ level: 2, name: "Alice" });
    },
    ITERATIONS,
  ),
);

for (const r of uncheckedResults) printResult(r);

// Speedup comparison
console.log("\n📊 Speedup (render vs renderUnchecked):");
const pairs = [
  ["simple", nth(renderResults, 0), nth(uncheckedResults, 0)],
  ["multi-param", nth(renderResults, 1), nth(uncheckedResults, 1)],
  ["list (2 items)", nth(renderResults, 2), nth(uncheckedResults, 2)],
  ["conditional", nth(renderResults, 7), nth(uncheckedResults, 3)],
] as const;
for (const [label, checked, unchecked] of pairs) {
  const speedup = checked.nsPerOp / unchecked.nsPerOp;
  console.log(
    `  ${label.padEnd(20)} ${Math.round(checked.nsPerOp).toString().padStart(6)} → ${Math.round(unchecked.nsPerOp).toString().padStart(6)} ns  (${speedup.toFixed(1)}× faster)`,
  );
}

// Summary
console.log("\n" + "=".repeat(70));
const avgRenderNs =
  renderResults.reduce((sum, r) => sum + r.nsPerOp, 0) / renderResults.length;
const avgUncheckedNs =
  uncheckedResults.reduce((sum, r) => sum + r.nsPerOp, 0) /
  uncheckedResults.length;
console.log(
  `Average render:           ${String(Math.round(avgRenderNs))} ns/op (${Math.round(1_000_000_000 / avgRenderNs).toLocaleString()} ops/s)`,
);
console.log(
  `Average renderUnchecked:  ${String(Math.round(avgUncheckedNs))} ns/op (${Math.round(1_000_000_000 / avgUncheckedNs).toLocaleString()} ops/s)`,
);
