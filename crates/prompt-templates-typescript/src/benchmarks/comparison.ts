/**
 * Comparative benchmarks: prompt-templates vs Handlebars vs Mustache.
 *
 * Uses the same template logic for each engine so the numbers
 * are directly comparable.
 *
 * Usage:
 *   npm run build && node dist/benchmarks/comparison.js
 */

import { Template } from "../index.js";
import Handlebars from "handlebars";
import Mustache from "mustache";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

interface BenchResult {
  name: string;
  nsPerOp: number;
  opsPerSec: number;
}

const BENCH_REPEAT = 5; // best-of-N for stability (matches Python's timeit)

function bench(name: string, fn: () => void, iterations: number): BenchResult {
  // Warmup
  for (let i = 0; i < Math.min(iterations, 1000); i++) fn();

  // Run multiple rounds and take the best (minimum) to eliminate
  // GC pauses, JIT compilation jitter, and OS scheduling noise.
  let bestElapsed = Infinity;
  for (let round = 0; round < BENCH_REPEAT; round++) {
    const start = performance.now();
    for (let i = 0; i < iterations; i++) fn();
    const elapsed = performance.now() - start;
    if (elapsed < bestElapsed) bestElapsed = elapsed;
  }

  const nsPerOp = (bestElapsed * 1_000_000) / iterations;
  const opsPerSec = Math.round(1_000_000_000 / nsPerOp);
  return { name, nsPerOp, opsPerSec };
}

function printRow(
  scenario: string,
  pt: BenchResult,
  hbs: BenchResult,
  mus: BenchResult,
): void {
  const format = (r: BenchResult) => {
    const ns = Math.round(r.nsPerOp);
    return `${ns.toLocaleString().padStart(8)} ns`;
  };
  const winner = Math.min(pt.nsPerOp, hbs.nsPerOp, mus.nsPerOp);
  const tag = (r: BenchResult) => (r.nsPerOp === winner ? " 🏆" : "   ");

  console.log(
    `  ${scenario.padEnd(18)} ${format(pt)}${tag(pt)}  ${format(hbs)}${tag(hbs)}  ${format(mus)}${tag(mus)}`,
  );
}

// ---------------------------------------------------------------------------
// Simple: variable substitution
// ---------------------------------------------------------------------------

const PT_SIMPLE_SRC = `---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!`;
const HBS_SIMPLE_SRC = "Hello {{name}}, welcome to {{place}}!";
const MUS_SIMPLE_SRC = "Hello {{name}}, welcome to {{place}}!";

// ---------------------------------------------------------------------------
// Loop: for/each with 5 items
// ---------------------------------------------------------------------------

const PT_LOOP_SRC = `---
params:
  - title = str
  - items = list<label = str, value = str>
---
# {{ title | upper }}

> {% for item in items %}

- {{ item.label }}: {{ item.value }}

> {% /for %}`;

const HBS_LOOP_SRC = [
  "# {{upper title}}",
  "",
  "{{#each items}}",
  "- {{this.label}}: {{this.value}}",
  "{{/each}}",
].join("\n");

const MUS_LOOP_SRC = [
  "# {{title}}",
  "",
  "{{#items}}",
  "- {{label}}: {{value}}",
  "{{/items}}",
].join("\n");

// ---------------------------------------------------------------------------
// Conditional: if/else
// ---------------------------------------------------------------------------

const PT_COND_SRC = `---
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

const HBS_COND_SRC = [
  "{{#if isBeginner}}",
  "Beginner: {{name}}",
  "{{else if isIntermediate}}",
  "Intermediate: {{name}}",
  "{{else}}",
  "Expert: {{name}}",
  "{{/if}}",
].join("\n");

// Mustache has no else-if, so we use a simpler conditional
const MUS_COND_SRC = [
  "{{#isBeginner}}",
  "Beginner: {{name}}",
  "{{/isBeginner}}",
  "{{^isBeginner}}",
  "Expert: {{name}}",
  "{{/isBeginner}}",
].join("\n");

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

const simpleData = { name: "Alice", place: "Wonderland" };
const loopItems = [
  { label: "Alpha", value: "100" },
  { label: "Beta", value: "200" },
  { label: "Gamma", value: "300" },
  { label: "Delta", value: "400" },
  { label: "Epsilon", value: "500" },
];
const loopData = { title: "Report", items: loopItems };
const condData = {
  level: 2,
  name: "Bob",
  isBeginner: false,
  isIntermediate: true,
};

// ---------------------------------------------------------------------------
// Register Handlebars helpers
// ---------------------------------------------------------------------------

Handlebars.registerHelper("upper", (s: string) =>
  typeof s === "string" ? s.toUpperCase() : s,
);

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

const ITERATIONS = 50_000;

console.log("prompt-templates TypeScript — Comparison Benchmarks");
console.log("=".repeat(72));
console.log(
  `${"  Scenario".padEnd(20)} ${"prompt-templates".padStart(14)}      ${"Handlebars".padStart(14)}      ${"Mustache".padStart(14)}`,
);
console.log("-".repeat(72));

// --- Compile benchmarks ---
console.log("\n📝 Compile (parse):");

const ptCompileSimple = bench(
  "pt-compile-simple",
  () => Template.fromSource(PT_SIMPLE_SRC),
  ITERATIONS,
);
const hbsCompileSimple = bench(
  "hbs-compile-simple",
  () => Handlebars.compile(HBS_SIMPLE_SRC),
  ITERATIONS,
);
const musCompileSimple = bench(
  "mus-compile-simple",
  () => {
    Mustache.parse(MUS_SIMPLE_SRC);
  },
  ITERATIONS,
);
printRow("simple", ptCompileSimple, hbsCompileSimple, musCompileSimple);

const ptCompileLoop = bench(
  "pt-compile-loop",
  () => Template.fromSource(PT_LOOP_SRC),
  ITERATIONS,
);
const hbsCompileLoop = bench(
  "hbs-compile-loop",
  () => Handlebars.compile(HBS_LOOP_SRC),
  ITERATIONS,
);
const musCompileLoop = bench(
  "mus-compile-loop",
  () => {
    Mustache.parse(MUS_LOOP_SRC);
  },
  ITERATIONS,
);
printRow("loop (5 items)", ptCompileLoop, hbsCompileLoop, musCompileLoop);

// --- Render benchmarks ---
console.log("\n🚀 Render (pre-compiled):");

const ptSimple = Template.fromSource(PT_SIMPLE_SRC);
const ptRenderSimple = bench(
  "pt-render-simple",
  () => ptSimple.render(simpleData),
  ITERATIONS,
);
const hbsSimple = Handlebars.compile(HBS_SIMPLE_SRC);
const hbsRenderSimple = bench(
  "hbs-render-simple",
  () => hbsSimple(simpleData),
  ITERATIONS,
);
Mustache.parse(MUS_SIMPLE_SRC); // cache parse
const musRenderSimple = bench(
  "mus-render-simple",
  () => Mustache.render(MUS_SIMPLE_SRC, simpleData),
  ITERATIONS,
);
printRow("simple", ptRenderSimple, hbsRenderSimple, musRenderSimple);

const ptLoop = Template.fromSource(PT_LOOP_SRC);
const ptRenderLoop = bench(
  "pt-render-loop",
  () => ptLoop.render(loopData),
  ITERATIONS,
);
const hbsLoop = Handlebars.compile(HBS_LOOP_SRC);
const hbsRenderLoop = bench(
  "hbs-render-loop",
  () => hbsLoop(loopData),
  ITERATIONS,
);
Mustache.parse(MUS_LOOP_SRC);
const musRenderLoop = bench(
  "mus-render-loop",
  () => Mustache.render(MUS_LOOP_SRC, loopData),
  ITERATIONS,
);
printRow("loop (5 items)", ptRenderLoop, hbsRenderLoop, musRenderLoop);

const ptCond = Template.fromSource(PT_COND_SRC);
const ptRenderCond = bench(
  "pt-render-cond",
  () => ptCond.render(condData, { allowExtra: true }),
  ITERATIONS,
);
const hbsCond = Handlebars.compile(HBS_COND_SRC);
const hbsRenderCond = bench(
  "hbs-render-cond",
  () => hbsCond(condData),
  ITERATIONS,
);
Mustache.parse(MUS_COND_SRC);
const musRenderCond = bench(
  "mus-render-cond",
  () => Mustache.render(MUS_COND_SRC, condData),
  ITERATIONS,
);
printRow("conditional", ptRenderCond, hbsRenderCond, musRenderCond);

// --- Round-trip benchmarks ---
console.log("\n🔄 Round-trip (compile + render):");

const ptRTSimple = bench(
  "pt-rt-simple",
  () => Template.fromSource(PT_SIMPLE_SRC).render(simpleData),
  ITERATIONS,
);
const hbsRTSimple = bench(
  "hbs-rt-simple",
  () => Handlebars.compile(HBS_SIMPLE_SRC)(simpleData),
  ITERATIONS,
);
const musRTSimple = bench(
  "mus-rt-simple",
  () => Mustache.render(MUS_SIMPLE_SRC, simpleData),
  ITERATIONS,
);
printRow("simple", ptRTSimple, hbsRTSimple, musRTSimple);

const ptRTLoop = bench(
  "pt-rt-loop",
  () => Template.fromSource(PT_LOOP_SRC).render(loopData),
  ITERATIONS,
);
const hbsRTLoop = bench(
  "hbs-rt-loop",
  () => Handlebars.compile(HBS_LOOP_SRC)(loopData),
  ITERATIONS,
);
const musRTLoop = bench(
  "mus-rt-loop",
  () => Mustache.render(MUS_LOOP_SRC, loopData),
  ITERATIONS,
);
printRow("loop (5 items)", ptRTLoop, hbsRTLoop, musRTLoop);

// --- Render unchecked benchmarks ---
console.log("\n⚡ Render unchecked (no type validation, pre-compiled):");

const ptUncheckedSimple = bench(
  "pt-unchecked-simple",
  () => ptSimple.renderUnchecked(simpleData),
  ITERATIONS,
);
printRow("simple", ptUncheckedSimple, hbsRenderSimple, musRenderSimple);

const ptUncheckedLoop = bench(
  "pt-unchecked-loop",
  () => ptLoop.renderUnchecked(loopData),
  ITERATIONS,
);
printRow("loop (5 items)", ptUncheckedLoop, hbsRenderLoop, musRenderLoop);

const ptUncheckedCond = bench(
  "pt-unchecked-cond",
  () => ptCond.renderUnchecked(condData),
  ITERATIONS,
);
printRow("conditional", ptUncheckedCond, hbsRenderCond, musRenderCond);

console.log("\n" + "=".repeat(72));
console.log(
  "Note: render() includes type validation; renderUnchecked() skips it.\n",
);
