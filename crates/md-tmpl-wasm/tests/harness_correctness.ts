/**
 * Shared harness for the WASM vs TypeScript correctness runner
 * (`correctness.ts`): assertion helpers, pass/fail counters, and summary.
 */

import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { Template: WasmTemplate } = require("../pkg/md_tmpl_wasm.js");
import { Template as TsTemplate } from "../../md-tmpl-typescript/dist/index.js";

// ---------------------------------------------------------------------------
// Test framework
// ---------------------------------------------------------------------------

let passed = 0;
let failed = 0;

/** Exact match: both engines must produce the same output. */
export function testExact(name, source, params) {
  try {
    const wasmResult = WasmTemplate.fromSource(source).render(params);
    const tsResult = TsTemplate.fromSource(source).render(params);

    if (wasmResult === tsResult) {
      console.log(`  ✅ ${name}`);
      passed++;
    } else {
      console.log(`  ❌ ${name}`);
      console.log(`     WASM: ${JSON.stringify(wasmResult)}`);
      console.log(`     TS:   ${JSON.stringify(tsResult)}`);
      failed++;
    }
  } catch (e) {
    console.log(`  ❌ ${name} — ERROR: ${e}`);
    failed++;
  }
}

/** Content match: same content after collapsing whitespace runs. */
export function testContent(name, source, params) {
  try {
    const wasmResult = WasmTemplate.fromSource(source).render(params);
    const tsResult = TsTemplate.fromSource(source).render(params);

    const normalize = (s) => s.replace(/\n+/g, "\n").trim();
    const wasmNorm = normalize(wasmResult);
    const tsNorm = normalize(tsResult);

    if (wasmNorm === tsNorm) {
      console.log(`  ✅ ${name}`);
      passed++;
    } else {
      console.log(`  ❌ ${name}`);
      console.log(`     WASM (normalized): ${JSON.stringify(wasmNorm)}`);
      console.log(`     TS   (normalized): ${JSON.stringify(tsNorm)}`);
      failed++;
    }
  } catch (e) {
    console.log(`  ❌ ${name} — ERROR: ${e}`);
    failed++;
  }
}

/** WASM-only: verify WASM output against an expected string. */
export function testWasm(name, source, params, expected) {
  try {
    const wasmResult = WasmTemplate.fromSource(source).render(params);

    if (wasmResult === expected) {
      console.log(`  ✅ ${name}`);
      passed++;
    } else {
      console.log(`  ❌ ${name}`);
      console.log(`     Got:      ${JSON.stringify(wasmResult)}`);
      console.log(`     Expected: ${JSON.stringify(expected)}`);
      failed++;
    }
  } catch (e) {
    console.log(`  ❌ ${name} — ERROR: ${e}`);
    failed++;
  }
}

// Printed when the harness is first imported, before any case module runs.
console.log("md-tmpl — Correctness Tests (WASM vs TS)");
console.log("=".repeat(60));
console.log("");

/** Print the pass/fail summary and exit non-zero if any case failed. */
export function reportSummary(): void {
  console.log("");
  console.log("=".repeat(60));
  console.log(`Results: ${passed} passed, ${failed} failed, ${passed + failed} total`);

  if (failed > 0) {
    process.exit(1);
  } else {
    console.log("All tests passed! ✅");
    process.exit(0);
  }
}
