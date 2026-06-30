/**
 * Benchmarks: WASM vs pure-TypeScript md-tmpl performance.
 *
 * Proper microbenchmark with statistical rigor:
 *   - Auto-calibrated iteration count (runs until ≥500ms total)
 *   - Warmup phase (≥100 iterations or 50ms)
 *   - Per-sample timing with performance.now()
 *   - Reports: min, median, mean, p95, p99, stddev, ops/sec
 *
 * Usage:
 *   cd crates/md-tmpl-wasm
 *   wasm-pack build --target nodejs --out-dir pkg
 *   node benchmarks/bench.mjs            # table output
 *   node benchmarks/bench.mjs --json     # JSON to stdout
 */

import { Template as WasmTemplate } from "../pkg/md_tmpl_wasm.js";
import { Template as TsTemplate } from "../../md-tmpl-typescript/dist/index.js";

// ---------------------------------------------------------------------------
// CLI flags
// ---------------------------------------------------------------------------

const JSON_OUTPUT = process.argv.includes("--json");

// ---------------------------------------------------------------------------
// Statistical helpers
// ---------------------------------------------------------------------------

function percentile(sorted, p) {
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

function median(sorted) {
  const mid = sorted.length >> 1;
  if (sorted.length % 2 === 0) {
    return (sorted[mid - 1] + sorted[mid]) / 2;
  }
  return sorted[mid];
}

function mean(values) {
  let sum = 0;
  for (const v of values) sum += v;
  return sum / values.length;
}

function stddev(values, avg) {
  let sumSq = 0;
  for (const v of values) {
    const d = v - avg;
    sumSq += d * d;
  }
  return Math.sqrt(sumSq / values.length);
}

// ---------------------------------------------------------------------------
// Core benchmark function
// ---------------------------------------------------------------------------

/**
 * Run `fn` with auto-calibration and collect per-iteration timing samples.
 *
 * 1. Warmup: run for ≥`warmup` iterations or ≥50ms.
 * 2. Calibrate: determine how many iterations fit in ~10ms (a "batch").
 * 3. Collect: run batches until we have ≥`minSamples` samples AND
 *    ≥`minDuration` ms of total measurement time.
 * 4. Each "sample" is one batch's average ns/op.
 *
 * @returns {{ name, samples, min, median, mean, p95, p99, stddev, opsPerSec }}
 */
function benchmark(name, fn, options = {}) {
  const {
    minSamples = 50,
    minDuration = 500,
    warmupIters = 100,
    warmupMs = 50,
  } = options;

  // --- Warmup ---
  {
    const deadline = performance.now() + warmupMs;
    let i = 0;
    while (i < warmupIters || performance.now() < deadline) {
      fn();
      i++;
    }
  }

  // --- Calibrate batch size ---
  // Find how many iterations take ~10ms so each sample is meaningful
  // but we still collect many samples.
  let batchSize = 1;
  {
    while (true) {
      const t0 = performance.now();
      for (let i = 0; i < batchSize; i++) fn();
      const elapsed = performance.now() - t0;
      if (elapsed >= 10) break;
      batchSize = Math.min(batchSize * 2, 1_000_000);
    }
    // Refine: target ~10ms per batch
    const t0 = performance.now();
    for (let i = 0; i < batchSize; i++) fn();
    const elapsed = performance.now() - t0;
    const perIter = elapsed / batchSize;
    batchSize = Math.max(1, Math.round(10 / perIter));
  }

  // --- Collect samples ---
  const samples = []; // ns per operation
  let totalMs = 0;

  while (samples.length < minSamples || totalMs < minDuration) {
    const t0 = performance.now();
    for (let i = 0; i < batchSize; i++) fn();
    const elapsed = performance.now() - t0;
    totalMs += elapsed;
    // Each batch produces one sample: average ns/op for this batch
    samples.push((elapsed * 1e6) / batchSize); // ms → ns, per op
  }

  // --- Compute stats ---
  samples.sort((a, b) => a - b);
  const avg = mean(samples);

  return {
    name,
    sampleCount: samples.length,
    batchSize,
    totalMs,
    min: samples[0],
    median: median(samples),
    mean: avg,
    p95: percentile(samples, 95),
    p99: percentile(samples, 99),
    stddev: stddev(samples, avg),
    opsPerSec: 1e9 / avg,
  };
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

function fmtTime(ns) {
  if (ns >= 1e6) return (ns / 1e6).toFixed(1) + "ms";
  if (ns >= 1e3) return (ns / 1e3).toFixed(1) + "µs";
  return ns.toFixed(0) + "ns";
}

function fmtOps(ops) {
  return Math.round(ops).toLocaleString("en-US");
}

function pad(s, w) {
  return String(s).padStart(w);
}

// ---------------------------------------------------------------------------
// Template sources
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
  - tasks = list<title = str, priority = str>
---
> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority }})

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

const ENUM_SRC = `---
params:
  - outcome = enum<Confirmed(evidence = str), Rejected, NeedsWork>
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`;

const DEFAULTS_SRC = `---
params:
  - greeting = str := "Hello"
  - name = str
  - suffix = str := "!"
  - lang = str := "en"
---
{{ greeting }}, {{ name }}{{ suffix }} [{{ lang }}]`;

const COMPLEX_SRC = `---
params:
  - config = struct<db = struct<host = str, port = int>, retries = int>
  - items = list<label = str>
  - name = str
---
DB: {{ config.db.host }}:{{ config.db.port }} (retries={{ config.retries }})

> {% for item in items %}

- {{ item.label | upper }}

> {% /for %}

By: {{ name | trim }}`;

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

const simpleParams = { name: "world" };
const multiParams = { name: "Alice", count: 42, score: 9.5, enabled: true };
const list2Params = {
  tasks: [
    { title: "Task A", priority: "High" },
    { title: "Task B", priority: "Low" },
  ],
};
const list20Params = {
  tasks: Array.from({ length: 20 }, (_, i) => ({
    title: `Task ${String.fromCharCode(65 + (i % 26))}${i}`,
    priority: i % 3 === 0 ? "High" : i % 3 === 1 ? "Medium" : "Low",
  })),
};
const condParams = { level: 2, name: "Alice" };
const enumParams = { outcome: { __kind__: "Confirmed", evidence: "proof found" } };
const defaultsParams = { name: "World" }; // greeting, suffix, lang use defaults
const complexParams = {
  config: { db: { host: "localhost", port: 5432 }, retries: 3 },
  items: [{ label: "alpha" }, { label: "beta" }, { label: "gamma" }],
  name: "  Admin  ",
};

// ---------------------------------------------------------------------------
// Scenario definitions
// ---------------------------------------------------------------------------

const scenarios = [
  // Parse-only
  {
    name: "parse simple",
    wasm: () => WasmTemplate.fromSource(SIMPLE_SRC),
    ts: () => TsTemplate.fromSource(SIMPLE_SRC),
  },
  // Render benchmarks (pre-compiled template)
  {
    name: "render simple (1 param)",
    setup: (T) => T.fromSource(SIMPLE_SRC),
    params: simpleParams,
  },
  {
    name: "render multi-param (4 params)",
    setup: (T) => T.fromSource(MULTI_PARAM_SRC),
    params: multiParams,
  },
  {
    name: "render list/for (2 items)",
    setup: (T) => T.fromSource(LIST_SRC),
    params: list2Params,
  },
  {
    name: "render list/for (20 items)",
    setup: (T) => T.fromSource(LIST_SRC),
    params: list20Params,
  },
  {
    name: "render conditional (if/elif)",
    setup: (T) => T.fromSource(CONDITIONAL_SRC),
    params: condParams,
  },
  {
    name: "render enum dispatch",
    setup: (T) => T.fromSource(ENUM_SRC),
    params: enumParams,
  },
  {
    name: "render with defaults",
    setup: (T) => T.fromSource(DEFAULTS_SRC),
    params: defaultsParams,
  },
  {
    name: "render complex (nested+list+filter)",
    setup: (T) => T.fromSource(COMPLEX_SRC),
    params: complexParams,
  },
  // Metadata access
  {
    name: "sourceHash()",
    setup: (T) => T.fromSource(COMPLEX_SRC),
    methodName: "sourceHash",
  },
  {
    name: "declarations()",
    setup: (T) => T.fromSource(COMPLEX_SRC),
    methodName: "declarations",
  },
  {
    name: "consts()",
    setup: (T) =>
      T.fromSource(`---
consts:
  - MAX = int := 100
  - PREFIX = str := ">> "
params: []
---
Max={{ MAX }}, prefix={{ PREFIX }}`),
    methodName: "consts",
  },
  // JSON bulk serialization render benchmarks (WASM-only optimization)
  {
    name: "renderJson simple (1 param)",
    setup: (T) => T.fromSource(SIMPLE_SRC),
    jsonParams: JSON.stringify(simpleParams),
    params: simpleParams,
  },
  {
    name: "renderJson multi-param (4 params)",
    setup: (T) => T.fromSource(MULTI_PARAM_SRC),
    jsonParams: JSON.stringify(multiParams),
    params: multiParams,
  },
  {
    name: "renderJson complex (nested+list+filter)",
    setup: (T) => T.fromSource(COMPLEX_SRC),
    jsonParams: JSON.stringify(complexParams),
    params: complexParams,
  },
];

// ---------------------------------------------------------------------------
// Run all benchmarks
// ---------------------------------------------------------------------------

const COL = {
  name: 40,
  engine: 6,
  median: 10,
  mean: 10,
  p95: 10,
  stddev: 10,
  ops: 12,
};

function printHeader() {
  const hdr =
    "Scenario".padEnd(COL.name) +
    " | " + "Engine".padEnd(COL.engine) +
    " | " + pad("Median", COL.median) +
    " | " + pad("Mean", COL.mean) +
    " | " + pad("p95", COL.p95) +
    " | " + pad("Stddev", COL.stddev) +
    " | " + pad("Ops/sec", COL.ops);
  console.log(hdr);
  console.log("-".repeat(hdr.length));
}

function printRow(name, engine, stats) {
  console.log(
    name.padEnd(COL.name) +
    " | " + engine.padEnd(COL.engine) +
    " | " + pad(fmtTime(stats.median), COL.median) +
    " | " + pad(fmtTime(stats.mean), COL.mean) +
    " | " + pad(fmtTime(stats.p95), COL.p95) +
    " | " + pad(fmtTime(stats.stddev), COL.stddev) +
    " | " + pad(fmtOps(stats.opsPerSec), COL.ops)
  );
}

function printDelta(wasmStats, tsStats) {
  const ratio = tsStats.median / wasmStats.median;
  if (ratio >= 1.0) {
    console.log(`  → WASM is ${ratio.toFixed(2)}× faster (median)`);
  } else {
    console.log(`  → TS is ${(1 / ratio).toFixed(2)}× faster (median)`);
  }
}

console.log("");
console.log("╔══════════════════════════════════════════════════════════════════╗");
console.log("║     md-tmpl — WASM vs TypeScript Microbenchmarks       ║");
console.log("╚══════════════════════════════════════════════════════════════════╝");
console.log("");
console.log(`Platform: ${process.platform} ${process.arch}`);
console.log(`Node.js:  ${process.version}`);
console.log(`Date:     ${new Date().toISOString()}`);
console.log("");

printHeader();

const allResults = [];

for (const scenario of scenarios) {
  let wasmFn, tsFn;

  if (scenario.wasm && scenario.ts) {
    // Custom fn (e.g., parse-only)
    wasmFn = scenario.wasm;
    tsFn = scenario.ts;
  } else if (scenario.setup && scenario.jsonParams) {
    // JSON render benchmark: WASM uses renderJson, TS uses render
    const wasmTmpl = scenario.setup(WasmTemplate);
    const tsTmpl = scenario.setup(TsTemplate);
    const jsonStr = scenario.jsonParams;
    const params = scenario.params;
    wasmFn = () => wasmTmpl.renderJson(jsonStr);
    tsFn = () => tsTmpl.render(params);
  } else if (scenario.setup && scenario.params) {
    // Render benchmark
    const wasmTmpl = scenario.setup(WasmTemplate);
    const tsTmpl = scenario.setup(TsTemplate);
    const params = scenario.params;
    wasmFn = () => wasmTmpl.render(params);
    tsFn = () => tsTmpl.render(params);
  } else if (scenario.setup && scenario.methodName) {
    // Metadata access
    const wasmTmpl = scenario.setup(WasmTemplate);
    const tsTmpl = scenario.setup(TsTemplate);
    const method = scenario.methodName;
    wasmFn = () => wasmTmpl[method]();
    tsFn = () => tsTmpl[method]();
  } else {
    console.error(`Invalid scenario config: ${scenario.name}`);
    process.exit(1);
  }

  const wasmStats = benchmark(`WASM: ${scenario.name}`, wasmFn);
  const tsStats = benchmark(`TS: ${scenario.name}`, tsFn);

  printRow(scenario.name, "WASM", wasmStats);
  printRow(scenario.name, "TS", tsStats);
  printDelta(wasmStats, tsStats);
  console.log("");

  allResults.push({
    scenario: scenario.name,
    wasm: wasmStats,
    ts: tsStats,
    speedup: tsStats.median / wasmStats.median,
  });
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

console.log("=".repeat(96));
console.log("");
console.log("Summary");
console.log(`-------`);

const wasmWins = allResults.filter((r) => r.speedup >= 1.0);
const tsWins = allResults.filter((r) => r.speedup < 1.0);
const avgSpeedup = mean(allResults.map((r) => r.speedup));
const geoMean = Math.exp(
  allResults.reduce((s, r) => s + Math.log(r.speedup), 0) / allResults.length
);

console.log("");
console.log(`  Scenarios run:        ${allResults.length}`);
console.log(`  WASM faster in:       ${wasmWins.length} scenario(s)`);
console.log(`  TS faster in:         ${tsWins.length} scenario(s)`);
console.log(`  Avg speedup (WASM):   ${avgSpeedup.toFixed(2)}×`);
console.log(`  Geo-mean speedup:     ${geoMean.toFixed(2)}×`);

if (wasmWins.length > 0) {
  console.log("");
  console.log("  WASM wins:");
  for (const r of wasmWins.sort((a, b) => b.speedup - a.speedup)) {
    console.log(`    • ${r.scenario}: ${r.speedup.toFixed(2)}× faster`);
  }
}

if (tsWins.length > 0) {
  console.log("");
  console.log("  TS wins:");
  for (const r of tsWins.sort((a, b) => a.speedup - b.speedup)) {
    console.log(`    • ${r.scenario}: ${(1 / r.speedup).toFixed(2)}× faster`);
  }
}

console.log("");
if (geoMean >= 1.2) {
  console.log("  Recommendation: WASM provides a meaningful performance advantage.");
} else if (geoMean <= 0.8) {
  console.log("  Recommendation: Pure-TS is faster overall; WASM overhead dominates.");
} else {
  console.log("  Recommendation: Performance is comparable; choose based on bundle-size/portability tradeoffs.");
}
console.log("");

// ---------------------------------------------------------------------------
// JSON output (--json)
// ---------------------------------------------------------------------------

if (JSON_OUTPUT) {
  const jsonResults = allResults.map((r) => ({
    scenario: r.scenario,
    wasm: {
      median_ns: r.wasm.median,
      mean_ns: r.wasm.mean,
      min_ns: r.wasm.min,
      p95_ns: r.wasm.p95,
      p99_ns: r.wasm.p99,
      stddev_ns: r.wasm.stddev,
      ops_per_sec: r.wasm.opsPerSec,
      samples: r.wasm.sampleCount,
    },
    ts: {
      median_ns: r.ts.median,
      mean_ns: r.ts.mean,
      min_ns: r.ts.min,
      p95_ns: r.ts.p95,
      p99_ns: r.ts.p99,
      stddev_ns: r.ts.stddev,
      ops_per_sec: r.ts.opsPerSec,
      samples: r.ts.sampleCount,
    },
    speedup: r.speedup,
    faster: r.speedup >= 1.0 ? "wasm" : "ts",
  }));

  console.log(`--- JSON BEGIN ---`);
  console.log(JSON.stringify({
    platform: `${process.platform} ${process.arch}`,
    node: process.version,
    timestamp: new Date().toISOString(),
    results: jsonResults,
    summary: {
      scenarios: allResults.length,
      wasm_wins: wasmWins.length,
      ts_wins: tsWins.length,
      avg_speedup: avgSpeedup,
      geo_mean_speedup: geoMean,
    },
  }, null, 2));
  console.log(`--- JSON END ---`);
}
