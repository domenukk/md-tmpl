/**
 * Cross-language conformance harness (TypeScript side).
 *
 * Replays the shared JSON corpus in `<repo>/conformance` through the
 * TypeScript `md-tmpl` engine and asserts that every case matches the recorded
 * expectation. The exact same corpus is replayed by the Rust harness
 * (`crates/md-tmpl-core/tests/conformance.rs`); if both pass, the two backends
 * are behaviourally identical on the covered surface.
 *
 * The corpus expectations were originally derived by executing this reference
 * implementation, so the TypeScript side is expected to be an exact match on
 * every case (including phase for error cases).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

import { Template } from "../index.js";

// Compiled location is dist/tests/, so the repo root is four levels up.
const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS_DIR = resolve(HERE, "../../../../conformance");

// Every corpus file holds a flat list of cases whose `expect.kind` selects how
// the case is checked.
const CORPUS_FILES = [
  "render.json",
  "interpolation.json",
  "frontmatter.json",
  "errors.json",
  "escapes.json",
  "comments.json",
] as const;

interface Expect {
  kind: "render" | "default" | "error";
  output?: string;
  defaults?: Record<string, unknown>;
  phase?: "compile" | "render" | "any";
  error_contains?: string;
}

interface Case {
  name: string;
  source: string;
  params?: Record<string, unknown>;
  env?: Record<string, unknown>;
  expect: Expect;
}

function loadCases(file: string): Case[] {
  const text = readFileSync(resolve(CORPUS_DIR, file), "utf8");
  return JSON.parse(text) as Case[];
}

function compile(c: Case): Template {
  return c.env !== undefined
    ? Template.fromSourceWithEnv(c.source, c.env)
    : Template.fromSource(c.source);
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

// Compile a case, capturing any compile-time error as a string instead of
// throwing, so error-phase logic can inspect both phases uniformly.
function tryCompile(c: Case): { tmpl: Template | null; err: string | null } {
  try {
    return { tmpl: compile(c), err: null };
  } catch (e) {
    return { tmpl: null, err: messageOf(e) };
  }
}

function checkRender(c: Case): void {
  assert.ok(c.expect.output !== undefined, "render case needs expect.output");
  const out = compile(c).render(c.params ?? {});
  assert.strictEqual(out, c.expect.output);
}

function checkDefault(c: Case): void {
  assert.ok(
    c.expect.defaults !== undefined,
    "default case needs expect.defaults",
  );
  const defs = compile(c).defaults();
  assert.deepStrictEqual(defs, c.expect.defaults);
}

// Assert a rendered error message contains the recorded needle, when one is
// recorded. A missing needle means "any error is acceptable".
function assertNeedle(needle: string | undefined, haystack: string): void {
  if (needle !== undefined) {
    assert.ok(
      haystack.includes(needle),
      `error ${JSON.stringify(haystack)} lacks substring ${JSON.stringify(needle)}`,
    );
  }
}

function checkError(c: Case): void {
  const phase = c.expect.phase;
  assert.ok(phase !== undefined, "error case needs expect.phase");
  const needle = c.expect.error_contains;
  const { tmpl, err } = tryCompile(c);

  if (phase === "compile") {
    assert.ok(err !== null, "expected a COMPILE error but compile succeeded");
    assertNeedle(needle, err);
    return;
  }

  // Both "render" and "any" require a successful compile before rendering,
  // except "any" also accepts a compile-time failure (leak-safety may trip at
  // either phase; the phase is allowed to differ between backends).
  if (err !== null) {
    assert.strictEqual(
      phase,
      "any",
      `expected a RENDER error but failed at COMPILE: ${err}`,
    );
    assertNeedle(needle, err);
    return;
  }

  assert.ok(tmpl !== null, "compile reported success but produced no template");
  let renderErr: string | null = null;
  try {
    tmpl.render(c.params ?? {});
  } catch (e) {
    renderErr = messageOf(e);
  }
  assert.ok(renderErr !== null, "expected a RENDER error but render succeeded");
  assertNeedle(needle, renderErr);
}

function runCase(c: Case): void {
  switch (c.expect.kind) {
    case "render":
      checkRender(c);
      break;
    case "default":
      checkDefault(c);
      break;
    case "error":
      checkError(c);
      break;
    default:
      throw new Error(`unknown expect.kind for case ${c.name}`);
  }
}

for (const file of CORPUS_FILES) {
  describe(`conformance: ${file}`, () => {
    const cases = loadCases(file);
    assert.ok(cases.length > 0, `corpus file ${file} is empty`);
    for (const c of cases) {
      it(c.name, () => {
        runCase(c);
      });
    }
  });
}
