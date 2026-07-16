/**
 * Cross-language conformance harness (TypeScript side).
 *
 * Replays the shared TOML corpus in `<repo>/tests/conformance` through the
 * TypeScript `md-tmpl` engine and asserts that every case matches the recorded
 * expectation. The exact same corpus is replayed by the Rust, Go, and Python
 * harnesses; if all pass, the four backends are behaviourally identical on the
 * covered surface.
 *
 * TOML has no `null`, so option-`None` is encoded in the corpus as the sentinel
 * inline table `{ __none__ = true }` and decoded back to `null` on load.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { parse as parseToml } from "smol-toml";

import { Template } from "../index.js";

// Compiled location is dist/tests/, so the repo root is four levels up.
const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS_DIR = resolve(HERE, "../../../../tests/conformance");

// Every corpus file holds a flat list of cases whose `expect.kind` selects how
// the case is checked.
const CORPUS_FILES = [
  "render.toml",
  "interpolation.toml",
  "frontmatter.toml",
  "errors.toml",
  "escapes.toml",
  "comments.toml",
  "literals.toml",
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

// Recursively decode the corpus's `{ __none__ = true }` option-None sentinel
// back into `null` (TOML has no null of its own).
function denull(x: unknown): unknown {
  if (Array.isArray(x)) {
    return x.map(denull);
  }
  if (x !== null && typeof x === "object") {
    const obj = x as Record<string, unknown>;
    const keys = Object.keys(obj);
    if (keys.length === 1 && obj.__none__ === true) {
      return null;
    }
    const out: Record<string, unknown> = {};
    for (const k of keys) {
      out[k] = denull(obj[k]);
    }
    return out;
  }
  return x;
}

function loadCases(file: string): Case[] {
  const text = readFileSync(resolve(CORPUS_DIR, file), "utf8");
  const root = parseToml(text) as { cases: unknown[] };
  return denull(root.cases) as Case[];
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
