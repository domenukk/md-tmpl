/**
 * Comparative benchmarks: md-tmpl vs Handlebars vs Mustache.
 *
 * Uses the exact same 5 scenarios as the Rust Criterion suite
 * (`benchmarks/benches/comparison.rs`):
 * 1. `simple` — variable substitution
 * 2. `loop (3 items)` — iterating over a list of 3 structs
 * 3. `conditional` — if / elif / else branching
 * 4. `hero` — nested sections + entries + tags + conditionals + `| fixed(1)`
 * 5. `mega` — 3 teams × 5 members × skills + 4-way numeric rating + `| fixed(1)`
 *
 * Methodology notes (brutal honesty):
 * - Every `Template.fromSource()` call in Compile and Round-trip benchmarks
 *   performs a full cold parse, frontmatter validation, static type-checking,
 *   and AST compilation with zero cross-template caching.
 * - Every `tmpl.render()` call in `md-tmpl` performs full runtime schema
 *   validation (recursive type-checking + missing/extra parameter rejection)
 *   AND evaluates `| fixed(1)` float formatting and numeric comparisons at
 *   render time.
 * - Handlebars and Mustache perform zero schema validation, and because they
 *   lack `elif`, numeric comparisons, and `fixed(1)` filters, their input data
 *   includes pre-formatted `score_fmt` strings and pre-computed boolean flags
 *   (`isHigh`, `isMedium`, `has_positive_score`, `is_outstanding`, `is_good`,
 *   `is_average`).
 * - `Mustache.clearCache()` is called before each `Mustache.parse()` in Compile
 *   benchmarks and before each `Mustache.render()` in Round-trip benchmarks so
 *   Mustache actually parses the template instead of returning a cached token
 *   tree. In Render benchmarks, `Mustache.parse()` is called beforehand so
 *   Mustache uses its pre-parsed token cache.
 * - `Handlebars.compile(src)` returns a lazy wrapper closure that defers AST
 *   parsing and `new Function()` codegen until the first invocation. Therefore
 *   `Handlebars.compile()` in the Compile table only measures wrapper closure
 *   allocation, whereas Round-trip (`Handlebars.compile(src)(data)`) measures
 *   actual Handlebars compilation + first render.
 *
 * Usage:
 *   npm run build && node dist/benchmarks/comparison.js
 */

import assert from "node:assert/strict";
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
  for (let i = 0; i < Math.min(iterations, 500); i++) fn();

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

function normalize(s: string): string {
  return s
    .split("\n")
    .map((line) => line.trimEnd())
    .filter((line) => line.length > 0)
    .join("\n");
}

// ---------------------------------------------------------------------------
// 1. Simple: variable substitution
// ---------------------------------------------------------------------------

const PT_SIMPLE_SRC = `---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!`;
const HBS_SIMPLE_SRC = "Hello {{{name}}}, welcome to {{{place}}}!";
const MUS_SIMPLE_SRC = "Hello {{{name}}}, welcome to {{{place}}}!";

// ---------------------------------------------------------------------------
// 2. Loop: for/each with 3 items
// ---------------------------------------------------------------------------

const PT_LOOP_SRC = `---
params:
  - items = list(label = str, value = int)
---
> {% for item in items %}

- {{ item.label }}: {{ item.value }}

> {% /for %}`;

const HBS_LOOP_SRC = [
  "{{#each items}}",
  "- {{{this.label}}}: {{{this.value}}}",
  "{{/each}}",
].join("\n");

const MUS_LOOP_SRC = [
  "{{#items}}",
  "- {{{label}}}: {{{value}}}",
  "{{/items}}",
].join("\n");

// ---------------------------------------------------------------------------
// 3. Conditional: if/elif/else
// ---------------------------------------------------------------------------

const PT_COND_SRC = `---
params:
  - level = str
  - score = int
---
> {% if level == "high" %}

Rating: Excellent

> {% elif level == "medium" %}

Rating: Good (score {{ score }})

> {% else %}

Rating: Needs Improvement

> {% /if %}`;

const HBS_COND_SRC = [
  "{{#if isHigh}}",
  "Rating: Excellent",
  "{{else if isMedium}}",
  "Rating: Good (score {{{score}}})",
  "{{else}}",
  "Rating: Needs Improvement",
  "{{/if}}",
].join("\n");

const MUS_COND_SRC = [
  "{{#isHigh}}",
  "Rating: Excellent",
  "{{/isHigh}}",
  "{{#isMedium}}",
  "Rating: Good (score {{{score}}})",
  "{{/isMedium}}",
  "{{#isLow}}",
  "Rating: Needs Improvement",
  "{{/isLow}}",
].join("\n");

// ---------------------------------------------------------------------------
// 4. Hero: nested loops + conditionals + float formatting
// ---------------------------------------------------------------------------

const PT_HERO_SRC = `---
params:
  - title = str
  - sections = list(heading = str, entries = list(name = str, active = bool, score = float, tags = list(label = str)))
---
# {{ title }}

> {% for section in sections %}

## {{ section.heading }}

> {% for entry in section.entries %}

### {{ entry.name }}

> {% if entry.active %}

- Status: active
- Score: {{ entry.score | fixed(1) }}

> {% elif entry.score > 0 %}

- Status: inactive (score {{ entry.score | fixed(1) }})

> {% else %}

- Status: inactive

> {% /if %}
> {% for tag in entry.tags %}

  - tag: {{ tag.label }}

> {% /for %}
> {% /for %}
> {% /for %}`;

const HBS_HERO_SRC = [
  "# {{{title}}}",
  "",
  "{{#each sections}}## {{{this.heading}}}",
  "",
  "{{#each this.entries}}### {{{this.name}}}",
  "",
  "{{#if this.active}}- Status: active",
  "- Score: {{{this.score_fmt}}}",
  "{{else}}{{#if this.has_positive_score}}- Status: inactive (score {{{this.score_fmt}}})",
  "{{else}}- Status: inactive",
  "{{/if}}{{/if}}{{#each this.tags}}  - tag: {{{this.label}}}",
  "{{/each}}{{/each}}{{/each}}",
].join("\n");

const MUS_HERO_SRC = [
  "# {{{title}}}",
  "",
  "{{#sections}}## {{{heading}}}",
  "",
  "{{#entries}}### {{{name}}}",
  "",
  "{{#active}}- Status: active",
  "- Score: {{{score_fmt}}}",
  "{{/active}}{{^active}}{{#has_positive_score}}- Status: inactive (score {{{score_fmt}}})",
  "{{/has_positive_score}}{{^has_positive_score}}- Status: inactive",
  "{{/has_positive_score}}{{/active}}{{#tags}}  - tag: {{{label}}}",
  "{{/tags}}{{/entries}}{{/sections}}",
].join("\n");

// ---------------------------------------------------------------------------
// 5. Mega: 3 teams × 5 members × skills + 4-way rating + float formatting
// ---------------------------------------------------------------------------

const PT_MEGA_SRC = `---
params:
  - org = str
  - teams = list(name = str, lead = str, active = bool, idx = int, members = list(name = str, role = str, score = float, skills = list(name = str)))
---
# {{ org }} Organization Report

> {% for team in teams %}

## {{ team.idx }}. {{ team.name }}

Lead: {{ team.lead }}

> {% if team.active %}

Status: ACTIVE

> {% else %}

Status: INACTIVE

> {% /if %}

> {% for member in team.members %}

### {{ member.name }} ({{ member.role }})

Score: {{ member.score | fixed(1) }}

> {% if member.score > 90 %}

Rating: Outstanding

> {% elif member.score > 70 %}

Rating: Good

> {% elif member.score > 50 %}

Rating: Average

> {% else %}

Rating: Needs Improvement

> {% /if %}

Skills:

> {% for skill in member.skills %}

  - {{ skill.name }}

> {% /for %}
> {% /for %}

---
> {% /for %}`;

const HBS_MEGA_SRC = [
  "# {{{org}}} Organization Report",
  "",
  "{{#each teams}}## {{{this.idx}}}. {{{this.name}}}",
  "",
  "Lead: {{{this.lead}}}",
  "{{#if this.active}}Status: ACTIVE",
  "{{else}}Status: INACTIVE",
  "{{/if}}{{#each this.members}}### {{{this.name}}} ({{{this.role}}})",
  "",
  "Score: {{{this.score_fmt}}}",
  "{{#if this.is_outstanding}}Rating: Outstanding",
  "{{else}}{{#if this.is_good}}Rating: Good",
  "{{else}}{{#if this.is_average}}Rating: Average",
  "{{else}}Rating: Needs Improvement",
  "{{/if}}{{/if}}{{/if}}Skills:",
  "{{#each this.skills}}  - {{{this.name}}}",
  "{{/each}}{{/each}}---",
  "{{/each}}",
].join("\n");

const MUS_MEGA_SRC = [
  "# {{{org}}} Organization Report",
  "",
  "{{#teams}}## {{{idx}}}. {{{name}}}",
  "",
  "Lead: {{{lead}}}",
  "{{#active}}Status: ACTIVE",
  "{{/active}}{{^active}}Status: INACTIVE",
  "{{/active}}{{#members}}### {{{name}}} ({{{role}}})",
  "",
  "Score: {{{score_fmt}}}",
  "{{#is_outstanding}}Rating: Outstanding",
  "{{/is_outstanding}}{{#is_good}}Rating: Good",
  "{{/is_good}}{{#is_average}}Rating: Average",
  "{{/is_average}}{{#is_needs_improvement}}Rating: Needs Improvement",
  "{{/is_needs_improvement}}Skills:",
  "{{#skills}}  - {{{name}}}",
  "{{/skills}}{{/members}}---",
  "{{/teams}}",
].join("\n");

// ---------------------------------------------------------------------------
// Data (strict data for md-tmpl, pre-computed helper flags for HBS/Mustache)
// ---------------------------------------------------------------------------

const simpleData = { name: "Alice", place: "Wonderland" };

const loopData = {
  items: [
    { label: "Alpha", value: 10 },
    { label: "Beta", value: 20 },
    { label: "Gamma", value: 30 },
  ],
};

const condStrictData = {
  level: "medium",
  score: 75,
};

const condSharedData = {
  level: "medium",
  score: 75,
  isHigh: false,
  isMedium: true,
  isLow: false,
};

interface HeroStrictEntry {
  readonly name: string;
  readonly active: boolean;
  readonly score: number;
  readonly tags: readonly { readonly label: string }[];
}

interface HeroSharedEntry extends HeroStrictEntry {
  readonly score_fmt: string;
  readonly has_positive_score: boolean;
}

function makeHeroStrictEntry(
  name: string,
  active: boolean,
  score: number,
  tags: readonly string[],
): HeroStrictEntry {
  return {
    name,
    active,
    score,
    tags: tags.map((label) => ({ label })),
  };
}

function makeHeroSharedEntry(
  name: string,
  active: boolean,
  score: number,
  tags: readonly string[],
): HeroSharedEntry {
  return {
    name,
    active,
    score,
    score_fmt: score.toFixed(1),
    has_positive_score: score > 0,
    tags: tags.map((label) => ({ label })),
  };
}

const heroStrictData = {
  title: "System Report",
  sections: [
    {
      heading: "Overview",
      entries: [
        makeHeroStrictEntry("Service-A", true, 98.7, ["prod", "critical"]),
        makeHeroStrictEntry("Service-B", false, 45.2, ["staging"]),
        makeHeroStrictEntry("Service-C", false, 0.0, ["deprecated"]),
      ],
    },
    {
      heading: "Metrics",
      entries: [
        makeHeroStrictEntry("Latency", true, 12.3, ["p99"]),
        makeHeroStrictEntry("Throughput", false, 0.0, ["batch"]),
      ],
    },
  ],
};

const heroSharedData = {
  title: "System Report",
  sections: [
    {
      heading: "Overview",
      entries: [
        makeHeroSharedEntry("Service-A", true, 98.7, ["prod", "critical"]),
        makeHeroSharedEntry("Service-B", false, 45.2, ["staging"]),
        makeHeroSharedEntry("Service-C", false, 0.0, ["deprecated"]),
      ],
    },
    {
      heading: "Metrics",
      entries: [
        makeHeroSharedEntry("Latency", true, 12.3, ["p99"]),
        makeHeroSharedEntry("Throughput", false, 0.0, ["batch"]),
      ],
    },
  ],
};

interface MegaStrictMember {
  readonly name: string;
  readonly role: string;
  readonly score: number;
  readonly skills: readonly { readonly name: string }[];
}

interface MegaSharedMember extends MegaStrictMember {
  readonly score_fmt: string;
  readonly is_outstanding: boolean;
  readonly is_good: boolean;
  readonly is_average: boolean;
  readonly is_needs_improvement: boolean;
}

function makeMegaStrictMember(
  name: string,
  role: string,
  score: number,
  skills: readonly string[],
): MegaStrictMember {
  return {
    name,
    role,
    score,
    skills: skills.map((s) => ({ name: s })),
  };
}

function makeMegaSharedMember(
  name: string,
  role: string,
  score: number,
  skills: readonly string[],
): MegaSharedMember {
  const is_outstanding = score > 90;
  const is_good = !is_outstanding && score > 70;
  const is_average = !is_outstanding && !is_good && score > 50;
  const is_needs_improvement = !is_outstanding && !is_good && !is_average;
  return {
    name,
    role,
    score,
    score_fmt: score.toFixed(1),
    is_outstanding,
    is_good,
    is_average,
    is_needs_improvement,
    skills: skills.map((s) => ({ name: s })),
  };
}

const megaStrictData = {
  org: "Acme Corp",
  teams: [
    {
      name: "Backend",
      lead: "Alice",
      active: true,
      idx: 1,
      members: [
        makeMegaStrictMember("Bob", "Senior", 95.5, ["Rust", "Go", "SQL"]),
        makeMegaStrictMember("Carol", "Mid", 78.3, ["Python", "Docker"]),
        makeMegaStrictMember("Dave", "Junior", 62.1, ["JavaScript", "HTML"]),
        makeMegaStrictMember("Eve", "Senior", 91.0, ["Java", "Kotlin", "gRPC"]),
        makeMegaStrictMember("Frank", "Intern", 45.0, ["Python"]),
      ],
    },
    {
      name: "Frontend",
      lead: "Grace",
      active: true,
      idx: 2,
      members: [
        makeMegaStrictMember("Heidi", "Senior", 88.7, [
          "React",
          "TypeScript",
          "CSS",
        ]),
        makeMegaStrictMember("Ivan", "Mid", 71.2, ["Vue", "JavaScript"]),
        makeMegaStrictMember("Judy", "Junior", 55.0, ["HTML", "CSS"]),
        makeMegaStrictMember("Karl", "Senior", 92.4, [
          "Angular",
          "RxJS",
          "SCSS",
        ]),
        makeMegaStrictMember("Liam", "Mid", 68.9, ["Svelte"]),
      ],
    },
    {
      name: "SRE",
      lead: "Mallory",
      active: false,
      idx: 3,
      members: [
        makeMegaStrictMember("Nancy", "Senior", 97.1, [
          "Kubernetes",
          "Terraform",
          "Go",
        ]),
        makeMegaStrictMember("Oscar", "Mid", 73.5, ["Ansible", "Bash"]),
        makeMegaStrictMember("Peggy", "Junior", 51.2, ["Linux"]),
        makeMegaStrictMember("Quentin", "Senior", 89.3, [
          "Prometheus",
          "Grafana",
        ]),
        makeMegaStrictMember("Ruth", "Intern", 38.0, ["Python"]),
      ],
    },
  ],
};

const megaSharedData = {
  org: "Acme Corp",
  teams: [
    {
      name: "Backend",
      lead: "Alice",
      active: true,
      idx: 1,
      members: [
        makeMegaSharedMember("Bob", "Senior", 95.5, ["Rust", "Go", "SQL"]),
        makeMegaSharedMember("Carol", "Mid", 78.3, ["Python", "Docker"]),
        makeMegaSharedMember("Dave", "Junior", 62.1, ["JavaScript", "HTML"]),
        makeMegaSharedMember("Eve", "Senior", 91.0, ["Java", "Kotlin", "gRPC"]),
        makeMegaSharedMember("Frank", "Intern", 45.0, ["Python"]),
      ],
    },
    {
      name: "Frontend",
      lead: "Grace",
      active: true,
      idx: 2,
      members: [
        makeMegaSharedMember("Heidi", "Senior", 88.7, [
          "React",
          "TypeScript",
          "CSS",
        ]),
        makeMegaSharedMember("Ivan", "Mid", 71.2, ["Vue", "JavaScript"]),
        makeMegaSharedMember("Judy", "Junior", 55.0, ["HTML", "CSS"]),
        makeMegaSharedMember("Karl", "Senior", 92.4, [
          "Angular",
          "RxJS",
          "SCSS",
        ]),
        makeMegaSharedMember("Liam", "Mid", 68.9, ["Svelte"]),
      ],
    },
    {
      name: "SRE",
      lead: "Mallory",
      active: false,
      idx: 3,
      members: [
        makeMegaSharedMember("Nancy", "Senior", 97.1, [
          "Kubernetes",
          "Terraform",
          "Go",
        ]),
        makeMegaSharedMember("Oscar", "Mid", 73.5, ["Ansible", "Bash"]),
        makeMegaSharedMember("Peggy", "Junior", 51.2, ["Linux"]),
        makeMegaSharedMember("Quentin", "Senior", 89.3, [
          "Prometheus",
          "Grafana",
        ]),
        makeMegaSharedMember("Ruth", "Intern", 38.0, ["Python"]),
      ],
    },
  ],
};

// ---------------------------------------------------------------------------
// Pre-compile and verify identical normalized outputs across all 3 engines
// ---------------------------------------------------------------------------

const ptSimple = Template.fromSource(PT_SIMPLE_SRC);
const hbsSimple = Handlebars.compile(HBS_SIMPLE_SRC);
assert.strictEqual(
  normalize(ptSimple.render(simpleData)),
  normalize(hbsSimple(simpleData)),
);
assert.strictEqual(
  normalize(ptSimple.render(simpleData)),
  normalize(Mustache.render(MUS_SIMPLE_SRC, simpleData)),
);

const ptLoop = Template.fromSource(PT_LOOP_SRC);
const hbsLoop = Handlebars.compile(HBS_LOOP_SRC);
assert.strictEqual(
  normalize(ptLoop.render(loopData)),
  normalize(hbsLoop(loopData)),
);
assert.strictEqual(
  normalize(ptLoop.render(loopData)),
  normalize(Mustache.render(MUS_LOOP_SRC, loopData)),
);

const ptCond = Template.fromSource(PT_COND_SRC);
const hbsCond = Handlebars.compile(HBS_COND_SRC);
assert.strictEqual(
  normalize(ptCond.render(condStrictData)),
  normalize(hbsCond(condSharedData)),
);
assert.strictEqual(
  normalize(ptCond.render(condStrictData)),
  normalize(Mustache.render(MUS_COND_SRC, condSharedData)),
);

const ptHero = Template.fromSource(PT_HERO_SRC);
const hbsHero = Handlebars.compile(HBS_HERO_SRC);
assert.strictEqual(
  normalize(ptHero.render(heroStrictData)),
  normalize(hbsHero(heroSharedData)),
);
assert.strictEqual(
  normalize(ptHero.render(heroStrictData)),
  normalize(Mustache.render(MUS_HERO_SRC, heroSharedData)),
);

const ptMega = Template.fromSource(PT_MEGA_SRC);
const hbsMega = Handlebars.compile(HBS_MEGA_SRC);
assert.strictEqual(
  normalize(ptMega.render(megaStrictData)),
  normalize(hbsMega(megaSharedData)),
);
assert.strictEqual(
  normalize(ptMega.render(megaStrictData)),
  normalize(Mustache.render(MUS_MEGA_SRC, megaSharedData)),
);

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

const ITERATIONS = 50_000;
const COMPLEX_ITERATIONS = 10_000;
const RT_ITERATIONS = 3_000;
const RT_COMPLEX_ITERATIONS = 500;

console.log("md-tmpl TypeScript — Comparison Benchmarks");
console.log("=".repeat(72));
console.log(
  `${"  Scenario".padEnd(20)} ${"md-tmpl".padStart(14)}      ${"Handlebars".padStart(14)}      ${"Mustache".padStart(14)}`,
);
console.log("-".repeat(72));

// --- Compile benchmarks ---
console.log("\n📝 Compile (cold parse, no cache):");

const ptCompileSimple = bench(
  "pt-compile-simple",
  () => Template.fromSource(PT_SIMPLE_SRC),
  RT_ITERATIONS,
);
const hbsCompileSimple = bench(
  "hbs-compile-simple",
  () => Handlebars.compile(HBS_SIMPLE_SRC),
  RT_ITERATIONS,
);
const musCompileSimple = bench(
  "mus-compile-simple",
  () => {
    Mustache.clearCache();
    Mustache.parse(MUS_SIMPLE_SRC);
  },
  RT_ITERATIONS,
);
printRow("simple", ptCompileSimple, hbsCompileSimple, musCompileSimple);

const ptCompileLoop = bench(
  "pt-compile-loop",
  () => Template.fromSource(PT_LOOP_SRC),
  RT_ITERATIONS,
);
const hbsCompileLoop = bench(
  "hbs-compile-loop",
  () => Handlebars.compile(HBS_LOOP_SRC),
  RT_ITERATIONS,
);
const musCompileLoop = bench(
  "mus-compile-loop",
  () => {
    Mustache.clearCache();
    Mustache.parse(MUS_LOOP_SRC);
  },
  RT_ITERATIONS,
);
printRow("loop (3 items)", ptCompileLoop, hbsCompileLoop, musCompileLoop);

const ptCompileCond = bench(
  "pt-compile-cond",
  () => Template.fromSource(PT_COND_SRC),
  RT_ITERATIONS,
);
const hbsCompileCond = bench(
  "hbs-compile-cond",
  () => Handlebars.compile(HBS_COND_SRC),
  RT_ITERATIONS,
);
const musCompileCond = bench(
  "mus-compile-cond",
  () => {
    Mustache.clearCache();
    Mustache.parse(MUS_COND_SRC);
  },
  RT_ITERATIONS,
);
printRow("conditional", ptCompileCond, hbsCompileCond, musCompileCond);

const ptCompileHero = bench(
  "pt-compile-hero",
  () => Template.fromSource(PT_HERO_SRC),
  RT_COMPLEX_ITERATIONS,
);
const hbsCompileHero = bench(
  "hbs-compile-hero",
  () => Handlebars.compile(HBS_HERO_SRC),
  RT_COMPLEX_ITERATIONS,
);
const musCompileHero = bench(
  "mus-compile-hero",
  () => {
    Mustache.clearCache();
    Mustache.parse(MUS_HERO_SRC);
  },
  RT_COMPLEX_ITERATIONS,
);
printRow("hero", ptCompileHero, hbsCompileHero, musCompileHero);

const ptCompileMega = bench(
  "pt-compile-mega",
  () => Template.fromSource(PT_MEGA_SRC),
  RT_COMPLEX_ITERATIONS,
);
const hbsCompileMega = bench(
  "hbs-compile-mega",
  () => Handlebars.compile(HBS_MEGA_SRC),
  RT_COMPLEX_ITERATIONS,
);
const musCompileMega = bench(
  "mus-compile-mega",
  () => {
    Mustache.clearCache();
    Mustache.parse(MUS_MEGA_SRC);
  },
  RT_COMPLEX_ITERATIONS,
);
printRow("mega", ptCompileMega, hbsCompileMega, musCompileMega);

// --- Render benchmarks (pre-compiled) ---
console.log("\n🚀 Render (pre-compiled):");

Mustache.clearCache();
Mustache.parse(MUS_SIMPLE_SRC);
Mustache.parse(MUS_LOOP_SRC);
Mustache.parse(MUS_COND_SRC);
Mustache.parse(MUS_HERO_SRC);
Mustache.parse(MUS_MEGA_SRC);

const ptRenderSimple = bench(
  "pt-render-simple",
  () => ptSimple.render(simpleData),
  ITERATIONS,
);
const hbsRenderSimple = bench(
  "hbs-render-simple",
  () => hbsSimple(simpleData),
  ITERATIONS,
);
const musRenderSimple = bench(
  "mus-render-simple",
  () => Mustache.render(MUS_SIMPLE_SRC, simpleData),
  ITERATIONS,
);
printRow("simple", ptRenderSimple, hbsRenderSimple, musRenderSimple);

const ptRenderLoop = bench(
  "pt-render-loop",
  () => ptLoop.render(loopData),
  ITERATIONS,
);
const hbsRenderLoop = bench(
  "hbs-render-loop",
  () => hbsLoop(loopData),
  ITERATIONS,
);
const musRenderLoop = bench(
  "mus-render-loop",
  () => Mustache.render(MUS_LOOP_SRC, loopData),
  ITERATIONS,
);
printRow("loop (3 items)", ptRenderLoop, hbsRenderLoop, musRenderLoop);

const ptRenderCond = bench(
  "pt-render-cond",
  () => ptCond.render(condStrictData),
  ITERATIONS,
);
const hbsRenderCond = bench(
  "hbs-render-cond",
  () => hbsCond(condSharedData),
  ITERATIONS,
);
const musRenderCond = bench(
  "mus-render-cond",
  () => Mustache.render(MUS_COND_SRC, condSharedData),
  ITERATIONS,
);
printRow("conditional", ptRenderCond, hbsRenderCond, musRenderCond);

const ptRenderHero = bench(
  "pt-render-hero",
  () => ptHero.render(heroStrictData),
  COMPLEX_ITERATIONS,
);
const hbsRenderHero = bench(
  "hbs-render-hero",
  () => hbsHero(heroSharedData),
  COMPLEX_ITERATIONS,
);
const musRenderHero = bench(
  "mus-render-hero",
  () => Mustache.render(MUS_HERO_SRC, heroSharedData),
  COMPLEX_ITERATIONS,
);
printRow("hero", ptRenderHero, hbsRenderHero, musRenderHero);

const ptRenderMega = bench(
  "pt-render-mega",
  () => ptMega.render(megaStrictData),
  COMPLEX_ITERATIONS,
);
const hbsRenderMega = bench(
  "hbs-render-mega",
  () => hbsMega(megaSharedData),
  COMPLEX_ITERATIONS,
);
const musRenderMega = bench(
  "mus-render-mega",
  () => Mustache.render(MUS_MEGA_SRC, megaSharedData),
  COMPLEX_ITERATIONS,
);
printRow("mega", ptRenderMega, hbsRenderMega, musRenderMega);

// --- Round-trip benchmarks (cold compile + render) ---
console.log("\n🔄 Round-trip (cold compile + render):");

const ptRTSimple = bench(
  "pt-rt-simple",
  () => Template.fromSource(PT_SIMPLE_SRC).render(simpleData),
  RT_ITERATIONS,
);
const hbsRTSimple = bench(
  "hbs-rt-simple",
  () => Handlebars.compile(HBS_SIMPLE_SRC)(simpleData),
  RT_ITERATIONS,
);
const musRTSimple = bench(
  "mus-rt-simple",
  () => {
    Mustache.clearCache();
    return Mustache.render(MUS_SIMPLE_SRC, simpleData);
  },
  RT_ITERATIONS,
);
printRow("simple", ptRTSimple, hbsRTSimple, musRTSimple);

const ptRTLoop = bench(
  "pt-rt-loop",
  () => Template.fromSource(PT_LOOP_SRC).render(loopData),
  RT_ITERATIONS,
);
const hbsRTLoop = bench(
  "hbs-rt-loop",
  () => Handlebars.compile(HBS_LOOP_SRC)(loopData),
  RT_ITERATIONS,
);
const musRTLoop = bench(
  "mus-rt-loop",
  () => {
    Mustache.clearCache();
    return Mustache.render(MUS_LOOP_SRC, loopData);
  },
  RT_ITERATIONS,
);
printRow("loop (3 items)", ptRTLoop, hbsRTLoop, musRTLoop);

const ptRTCond = bench(
  "pt-rt-cond",
  () => Template.fromSource(PT_COND_SRC).render(condStrictData),
  RT_ITERATIONS,
);
const hbsRTCond = bench(
  "hbs-rt-cond",
  () => Handlebars.compile(HBS_COND_SRC)(condSharedData),
  RT_ITERATIONS,
);
const musRTCond = bench(
  "mus-rt-cond",
  () => {
    Mustache.clearCache();
    return Mustache.render(MUS_COND_SRC, condSharedData);
  },
  RT_ITERATIONS,
);
printRow("conditional", ptRTCond, hbsRTCond, musRTCond);

const ptRTHero = bench(
  "pt-rt-hero",
  () => Template.fromSource(PT_HERO_SRC).render(heroStrictData),
  RT_COMPLEX_ITERATIONS,
);
const hbsRTHero = bench(
  "hbs-rt-hero",
  () => Handlebars.compile(HBS_HERO_SRC)(heroSharedData),
  RT_COMPLEX_ITERATIONS,
);
const musRTHero = bench(
  "mus-rt-hero",
  () => {
    Mustache.clearCache();
    return Mustache.render(MUS_HERO_SRC, heroSharedData);
  },
  RT_COMPLEX_ITERATIONS,
);
printRow("hero", ptRTHero, hbsRTHero, musRTHero);

const ptRTMega = bench(
  "pt-rt-mega",
  () => Template.fromSource(PT_MEGA_SRC).render(megaStrictData),
  RT_COMPLEX_ITERATIONS,
);
const hbsRTMega = bench(
  "hbs-rt-mega",
  () => Handlebars.compile(HBS_MEGA_SRC)(megaSharedData),
  RT_COMPLEX_ITERATIONS,
);
const musRTMega = bench(
  "mus-rt-mega",
  () => {
    Mustache.clearCache();
    return Mustache.render(MUS_MEGA_SRC, megaSharedData);
  },
  RT_COMPLEX_ITERATIONS,
);
printRow("mega", ptRTMega, hbsRTMega, musRTMega);

console.log("\n" + "=".repeat(72));
console.log(
  "Note: md-tmpl render() includes full runtime schema validation and filter evaluation.\n",
);
