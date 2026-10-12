/**
 * Benchmarks for md-tmpl TypeScript bindings.
 *
 * Measures cold parse + type-validated render throughput across micro and
 * realistic multi-section templates. Every `Template.fromSource()` call
 * parses and validates from scratch (no cross-template cache), and every
 * `tmpl.render()` call performs full runtime parameter validation.
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

const HERO_SRC = `---
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

const MEGA_SRC = `---
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

const heroData = {
  title: "System Report",
  sections: [
    {
      heading: "Overview",
      entries: [
        {
          name: "Service-A",
          active: true,
          score: 98.7,
          tags: [{ label: "prod" }, { label: "critical" }],
        },
        {
          name: "Service-B",
          active: false,
          score: 45.2,
          tags: [{ label: "staging" }],
        },
        {
          name: "Service-C",
          active: false,
          score: 0.0,
          tags: [{ label: "deprecated" }],
        },
      ],
    },
    {
      heading: "Metrics",
      entries: [
        {
          name: "Latency",
          active: true,
          score: 12.3,
          tags: [{ label: "p99" }],
        },
        {
          name: "Throughput",
          active: false,
          score: 0.0,
          tags: [{ label: "batch" }],
        },
      ],
    },
  ],
};

interface MegaMember {
  readonly name: string;
  readonly role: string;
  readonly score: number;
  readonly skills: readonly { readonly name: string }[];
}

function makeMegaMember(
  name: string,
  role: string,
  score: number,
  skills: readonly string[],
): MegaMember {
  return {
    name,
    role,
    score,
    skills: skills.map((s) => ({ name: s })),
  };
}

const megaData = {
  org: "Acme Corp",
  teams: [
    {
      name: "Backend",
      lead: "Alice",
      active: true,
      idx: 1,
      members: [
        makeMegaMember("Bob", "Senior", 95.5, ["Rust", "Go", "SQL"]),
        makeMegaMember("Carol", "Mid", 78.3, ["Python", "Docker"]),
        makeMegaMember("Dave", "Junior", 62.1, ["JavaScript", "HTML"]),
        makeMegaMember("Eve", "Senior", 91.0, ["Java", "Kotlin", "gRPC"]),
        makeMegaMember("Frank", "Intern", 45.0, ["Python"]),
      ],
    },
    {
      name: "Frontend",
      lead: "Grace",
      active: true,
      idx: 2,
      members: [
        makeMegaMember("Heidi", "Senior", 88.7, ["React", "TypeScript", "CSS"]),
        makeMegaMember("Ivan", "Mid", 71.2, ["Vue", "JavaScript"]),
        makeMegaMember("Judy", "Junior", 55.0, ["HTML", "CSS"]),
        makeMegaMember("Karl", "Senior", 92.4, ["Angular", "RxJS", "SCSS"]),
        makeMegaMember("Liam", "Mid", 68.9, ["Svelte"]),
      ],
    },
    {
      name: "SRE",
      lead: "Mallory",
      active: false,
      idx: 3,
      members: [
        makeMegaMember("Nancy", "Senior", 97.1, [
          "Kubernetes",
          "Terraform",
          "Go",
        ]),
        makeMegaMember("Oscar", "Mid", 73.5, ["Ansible", "Bash"]),
        makeMegaMember("Peggy", "Junior", 51.2, ["Linux"]),
        makeMegaMember("Quentin", "Senior", 89.3, ["Prometheus", "Grafana"]),
        makeMegaMember("Ruth", "Intern", 38.0, ["Python"]),
      ],
    },
  ],
};

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

parseResults.push(
  bench(
    "parse: hero (nested + fixed)",
    () => {
      Template.fromSource(HERO_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

parseResults.push(
  bench(
    "parse: mega (3 teams, 15 members)",
    () => {
      Template.fromSource(MEGA_SRC);
    },
    PARSE_ITERATIONS,
  ),
);

for (const r of parseResults) printResult(r);

// Render benchmarks (pre-parsed templates, full runtime validation)
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

const heroTmpl = Template.fromSource(HERO_SRC);
renderResults.push(
  bench(
    "render: hero (nested + fixed)",
    () => {
      heroTmpl.render(heroData);
    },
    ITERATIONS / 2,
  ),
);

const megaTmpl = Template.fromSource(MEGA_SRC);
renderResults.push(
  bench(
    "render: mega (3 teams, 15 members)",
    () => {
      megaTmpl.render(megaData);
    },
    ITERATIONS / 5,
  ),
);

for (const r of renderResults) printResult(r);

// Summary
console.log("\n" + "=".repeat(70));
const avgParseNs =
  parseResults.reduce((sum, r) => sum + r.nsPerOp, 0) / parseResults.length;
const avgRenderNs =
  renderResults.reduce((sum, r) => sum + r.nsPerOp, 0) / renderResults.length;
console.log(
  `Average parse:            ${String(Math.round(avgParseNs))} ns/op (${Math.round(1_000_000_000 / avgParseNs).toLocaleString()} ops/s)`,
);
console.log(
  `Average render:           ${String(Math.round(avgRenderNs))} ns/op (${Math.round(1_000_000_000 / avgRenderNs).toLocaleString()} ops/s)`,
);
