/**
 * Shared test runner for cross-backend test fixtures (WASM backend).
 *
 * Runs the same shared TOML fixtures that are used by the Rust and
 * TypeScript backends, ensuring behavioral parity across all engines.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const TOML = require("smol-toml") as {
  parse: (s: string) => Record<string, unknown>;
};

import { Template } from "../pkg/md_tmpl_wasm.js";

// ---------------------------------------------------------------------------
// TOML option-convention helpers
// ---------------------------------------------------------------------------

function transformOptionValues(
  params: Record<string, unknown>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(params)) {
    result[key] = transformValue(value);
  }
  return result;
}

function transformValue(value: unknown): unknown {
  if (typeof value === "string") {
    if (value === "None") return null;
    if (value.startsWith("Some(") && value.endsWith(")")) {
      return value.slice(5, -1);
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map(transformValue);
  }
  if (value !== null && typeof value === "object") {
    return transformOptionValues(value as Record<string, unknown>);
  }
  return value;
}

// ---------------------------------------------------------------------------
// Fixture paths
// ---------------------------------------------------------------------------

import { fileURLToPath } from "node:url";
const __dirname = path.dirname(fileURLToPath(import.meta.url));

const FIXTURES_DIR = path.resolve(__dirname, "../../../tests/shared");

const INLINE_TMPL_SUITE = "inline_tmpl_tests.toml";
const INLINE_CONTROL_SUITE = "inline_control_tests";
const TMPL_PARAM_SUITE = "tmpl_param_tests.toml";
const FEATURE_E2E_SUITE = "feature_e2e_tests";
const ENV_SUITE = "env_tests.toml";

/** File suffix of shared fixture files. */
const FIXTURE_SUFFIX = ".toml";

/**
 * Resolve a shared suite (relative to `FIXTURES_DIR`) to its fixture files.
 * A `.toml` entry is a single file; any other entry is a directory of
 * per-topic fixture files, discovered in sorted order. Throws if empty.
 */
function suiteFiles(suite: string): string[] {
  const suitePath = path.join(FIXTURES_DIR, suite);
  if (suite.endsWith(FIXTURE_SUFFIX)) return [suitePath];
  const files = fs
    .readdirSync(suitePath)
    .filter((name) => name.endsWith(FIXTURE_SUFFIX))
    .sort()
    .map((name) => path.join(suitePath, name));
  if (files.length === 0) {
    throw new Error(`shared suite directory has no fixtures: ${suitePath}`);
  }
  return files;
}

/** Load and concatenate the `tests` arrays of every fixture in a suite. */
function loadSuiteCases<T>(suite: string): T[] {
  return suiteFiles(suite).flatMap((file) => {
    const { tests } = TOML.parse(fs.readFileSync(file, "utf-8")) as unknown as {
      tests: T[];
    };
    return tests;
  });
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface InlineTmplTestCase {
  name: string;
  description: string;
  template_lines?: string[];
  template?: string;
  params: Record<string, unknown>;
  expected_output?: string;
  expected_error?: string;
}

interface EnvTestCase {
  name: string;
  description: string;
  template_lines?: string[];
  template?: string;
  env?: Record<string, string>;
  params?: Record<string, unknown>;
  expected_output?: string;
  expected_error?: string;
}



function joinLines(lines: string[]): string {
  return lines.join("\n");
}

function getTemplateSrc(tc: InlineTmplTestCase): string {
  if (tc.template) {
    if (tc.template.endsWith(".tmpl.md")) {
      const fullPath = path.resolve(FIXTURES_DIR, tc.template);
      if (fs.existsSync(fullPath)) {
        return fs.readFileSync(fullPath, "utf-8");
      }
    }
    return tc.template;
  }
  return joinLines(tc.template_lines || []);
}

function matchesError(err: unknown, expected: string): boolean {
  // Both WASM and TS throw real Error objects; the string branch is kept as a
  // defensive fallback for any legacy path.
  const msg = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
  const name = err instanceof Error ? err.name : "";
  const msgLower = msg.toLowerCase();
  const nameLower = name.toLowerCase();
  // Support pipe-separated alternatives: "foo|bar" means the error must
  // contain at least one of the alternatives (matching Rust test runner).
  return expected.split("|").some((alt) => {
    const altLower = alt.toLowerCase();
    return msgLower.includes(altLower) || nameLower.includes(altLower);
  });
}

// ---------------------------------------------------------------------------
// Inline template tests
// ---------------------------------------------------------------------------

describe("WASM Shared: Inline template tests", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(INLINE_TMPL_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const src = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(src);
        const output = tmpl.render(transformOptionValues(tc.params || {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        assert.throws(
          () => {
            const tmpl = Template.fromSource(src);
            tmpl.render(transformOptionValues(tc.params || {}));
          },
          (err: unknown) => matchesError(err, tc.expected_error!),
          `${tc.description}: expected error containing '${tc.expected_error}'`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// File-based include tests — SKIPPED
//
// WASM runs in a sandboxed environment without filesystem access.
// Include/import tests require fromSourceWithBaseDir to resolve file paths,
// which fails with "operation not supported on this platform" in WASM.
// These tests are covered by the Rust and TypeScript backends.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Inline control flow tests
// ---------------------------------------------------------------------------

describe("WASM Shared: Inline control flow tests", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(INLINE_CONTROL_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(transformOptionValues(tc.params || {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        assert.throws(
          () => {
            const tmpl = Template.fromSource(templateSrc);
            tmpl.render(transformOptionValues(tc.params || {}));
          },
          (err: unknown) => matchesError(err, tc.expected_error!),
          `${tc.description}: expected error`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// tmpl() parameter tests
// ---------------------------------------------------------------------------

describe("WASM Shared: tmpl() parameter tests", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(TMPL_PARAM_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(transformOptionValues(tc.params || {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        assert.throws(
          () => {
            const tmpl = Template.fromSource(templateSrc);
            tmpl.render(transformOptionValues(tc.params || {}));
          },
          (err: unknown) => matchesError(err, tc.expected_error!),
          `${tc.description}: expected error`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Feature E2E tests
// ---------------------------------------------------------------------------

describe("WASM Shared: Feature E2E tests", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(FEATURE_E2E_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(transformOptionValues(tc.params || {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        assert.throws(
          () => {
            const tmpl = Template.fromSource(templateSrc);
            tmpl.render(transformOptionValues(tc.params || {}));
          },
          (err: unknown) => matchesError(err, tc.expected_error!),
          `${tc.description}: expected error`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Env tests
// ---------------------------------------------------------------------------

describe("WASM Shared: Env tests", () => {
  const tests = loadSuiteCases<EnvTestCase>(ENV_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const src = getTemplateSrc(tc as unknown as InlineTmplTestCase);

      if (tc.expected_output !== undefined) {
        // WASM's fromSourceWithEnv takes a flat env record, not {env: ...}
        const tmpl = Template.fromSourceWithEnv(src, tc.env ?? {});
        const output = tmpl.render(transformOptionValues(tc.params ?? {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        assert.throws(
          () => {
            const tmpl = Template.fromSourceWithEnv(src, tc.env ?? {});
            tmpl.render(transformOptionValues(tc.params ?? {}));
          },
          (err: unknown) => matchesError(err, tc.expected_error!),
          `${tc.description}: expected error containing '${tc.expected_error}'`,
        );
      }
    });
  }
});
