/**
 * Correctness tests: WASM vs pure-TypeScript md-tmpl.
 *
 * For templates with only expression interpolation (no block control flow),
 * both engines produce identical output, so we assert exact equality.
 *
 * For templates with block control flow (for/if/match), the Rust engine
 * and the TS re-implementation differ in how `> ` prefix lines produce
 * surrounding newlines. For these, we verify each engine independently
 * against its own expected output, and we assert that the *content*
 * (after whitespace normalization) is the same.
 *
 * Cases live in topic modules (`*_correctness.ts`) that register and run on
 * import, in the order below; `harness_correctness.ts` holds the helpers.
 *
 * Usage:
 *   cd crates/md-tmpl-wasm
 *   wasm-pack build --target nodejs --out-dir pkg
 *   npm run build && node dist/correctness.js
 */

import { reportSummary } from "./harness_correctness.js";
import "./exact_correctness.js";
import "./control_flow_correctness.js";
import "./spec_features_correctness.js";
import "./spec_gaps_correctness.js";

reportSummary();
