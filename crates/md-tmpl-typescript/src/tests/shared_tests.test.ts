/**
 * Shared test runner for cross-backend test fixtures.
 *
 * Runs the test cases defined in the `tests/shared/` suites (single-file
 * suites such as `inline_tmpl_tests.toml` and per-topic suite directories such
 * as `include_tests/`) against the TypeScript backend.
 *
 * These same fixtures are consumed by the Rust backend's shared test
 * runner, ensuring behavioral parity between implementations.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { parse as parseToml } from "smol-toml";

import { Template } from "../index.js";

/**
 * Check if an error message matches an expected_error pattern.
 * Supports `|` as OR separator: "reserved keyword|shadows built-in"
 * means the error must contain at least one of the alternatives.
 */
function matchesExpectedError(message: string, pattern: string): boolean {
  const alternatives = pattern.split("|");
  return alternatives.some((alt) =>
    message.toLowerCase().includes(alt.toLowerCase()),
  );
}

// ---------------------------------------------------------------------------
// TOML option-convention helpers
// ---------------------------------------------------------------------------

/**
 * Transform TOML option convention values:
 * - String "None" → null
 * - String "Some(x)" → "x" (escape for literal "None")
 * - Recurse into arrays and objects
 */
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

const FIXTURES_DIR = path.resolve(
  import.meta.dirname,
  "../../../../tests/shared",
);

const INLINE_TMPL_SUITE = "inline_tmpl_tests.toml";
const INCLUDE_SUITE = "include_tests";
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
    const { tests } = parseToml(fs.readFileSync(file, "utf-8")) as unknown as {
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
  params?: Record<string, unknown>;
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

interface IncludeTestCase {
  name: string;
  description: string;
  files?: Record<string, string[] | string>;
  parent_template_lines?: string[];
  parent_template?: string;
  params?: Record<string, unknown>;
  env?: Record<string, string>;
  expected_output?: string;
  expected_error?: string;
}

/** Join an array of lines with newlines. */
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
  return joinLines(tc.template_lines ?? []);
}

// ---------------------------------------------------------------------------
// Inline template tests (from shared fixtures)
// ---------------------------------------------------------------------------

describe("Shared: Inline template tests", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(INLINE_TMPL_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const src = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(src);
        const output = tmpl.render(transformOptionValues(tc.params ?? {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        const expectedError = tc.expected_error;
        assert.throws(
          () => {
            const tmpl = Template.fromSource(src);
            tmpl.render(transformOptionValues(tc.params ?? {}));
          },
          (err: unknown) => {
            if (!(err instanceof Error)) return false;
            return (
              matchesExpectedError(err.message, expectedError) ||
              matchesExpectedError(err.constructor.name, expectedError)
            );
          },
          `${tc.description}: expected error containing '${tc.expected_error}'`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// File-based include tests (from shared fixtures)
// ---------------------------------------------------------------------------

describe("Shared: File-based include tests", () => {
  const tests = loadSuiteCases<IncludeTestCase>(INCLUDE_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-shared-"));
      try {
        // Write all include files to the temp directory.
        for (const [filename, content] of Object.entries(tc.files ?? {})) {
          let contentStr =
            typeof content === "string" ? content : joinLines(content);
          if (contentStr.endsWith(".tmpl.md")) {
            const fullPath = path.resolve(FIXTURES_DIR, contentStr);
            if (fs.existsSync(fullPath)) {
              contentStr = fs.readFileSync(fullPath, "utf-8");
            }
          }
          const filePath = path.join(dir, filename);
          fs.mkdirSync(path.dirname(filePath), { recursive: true });
          fs.writeFileSync(filePath, contentStr);
        }

        let parentSrc = tc.parent_template ?? "";
        if (parentSrc === "" && tc.parent_template_lines) {
          parentSrc = joinLines(tc.parent_template_lines);
        }
        if (parentSrc.endsWith(".tmpl.md")) {
          const fullPath = path.resolve(FIXTURES_DIR, parentSrc);
          if (fs.existsSync(fullPath)) {
            parentSrc = fs.readFileSync(fullPath, "utf-8");
          }
        }

        if (tc.expected_output !== undefined) {
          const tmpl = tc.env
            ? Template.fromSourceWithOptions(parentSrc, {
                env: tc.env,
                baseDir: dir,
              })
            : Template.fromSourceWithBaseDir(parentSrc, dir);
          const output = tmpl.render(transformOptionValues(tc.params ?? {}));
          assert.strictEqual(
            output,
            tc.expected_output,
            `${tc.description}: output mismatch`,
          );
        } else if (tc.expected_error !== undefined) {
          const expectedSubstring = tc.expected_error;
          assert.throws(
            () => {
              const tmpl = tc.env
                ? Template.fromSourceWithOptions(parentSrc, {
                    env: tc.env,
                    baseDir: dir,
                  })
                : Template.fromSourceWithBaseDir(parentSrc, dir);
              tmpl.render(transformOptionValues(tc.params ?? {}));
            },
            (err: Error) => {
              assert.ok(
                err.message
                  .toLowerCase()
                  .includes(expectedSubstring.toLowerCase()),
                `expected error containing "${expectedSubstring}", got: "${err.message}"`,
              );
              return true;
            },
            `${tc.description}: expected error`,
          );
        }
      } finally {
        fs.rmSync(dir, { recursive: true });
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Inline control flow tests (from shared fixtures)
// ---------------------------------------------------------------------------

describe("Shared: Inline control flow tests", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(INLINE_CONTROL_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(transformOptionValues(tc.params ?? {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        const expectedSubstring = tc.expected_error;
        assert.throws(
          () => {
            const tmpl = Template.fromSource(templateSrc);
            tmpl.render(transformOptionValues(tc.params ?? {}));
          },
          (err: Error) => {
            assert.ok(
              matchesExpectedError(err.message, expectedSubstring),
              `expected error containing "${expectedSubstring}", got: "${err.message}"`,
            );
            return true;
          },
          `${tc.description}: expected error`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Shared tmpl() parameter tests
// ---------------------------------------------------------------------------

describe("Shared: tmpl() parameter tests", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(TMPL_PARAM_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(transformOptionValues(tc.params ?? {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        const expectedSubstring = tc.expected_error;
        assert.throws(
          () => {
            const tmpl = Template.fromSource(templateSrc);
            tmpl.render(transformOptionValues(tc.params ?? {}));
          },
          (err: Error) => {
            assert.ok(
              matchesExpectedError(err.message, expectedSubstring),
              `expected error containing "${expectedSubstring}", got: "${err.message}"`,
            );
            return true;
          },
          `${tc.description}: expected error`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Shared Feature E2E tests (Milestone E2E.2)
// ---------------------------------------------------------------------------

describe("Shared: Feature E2E tests (Milestone E2E.2)", () => {
  const tests = loadSuiteCases<InlineTmplTestCase>(FEATURE_E2E_SUITE);

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(transformOptionValues(tc.params ?? {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        const expectedSubstring = tc.expected_error;
        assert.throws(
          () => {
            const tmpl = Template.fromSource(templateSrc);
            tmpl.render(transformOptionValues(tc.params ?? {}));
          },
          (err: Error) => {
            assert.ok(
              matchesExpectedError(err.message, expectedSubstring),
              `expected error containing "${expectedSubstring}", got: "${err.message}"`,
            );
            return true;
          },
          `${tc.description}: expected error`,
        );
      }
    });
  }
});

// ---------------------------------------------------------------------------
// Shared env: tests (compile-time environment variables)
// ---------------------------------------------------------------------------

describe("Shared: Env tests", () => {
  const tests = loadSuiteCases<EnvTestCase>(ENV_SUITE);

  function getEnvTemplateSrc(tc: EnvTestCase): string {
    if (tc.template) {
      if (tc.template.endsWith(".tmpl.md")) {
        const fullPath = path.resolve(FIXTURES_DIR, tc.template);
        if (fs.existsSync(fullPath)) {
          return fs.readFileSync(fullPath, "utf-8");
        }
      }
      return tc.template;
    }
    return joinLines(tc.template_lines ?? []);
  }

  for (const tc of tests) {
    it(tc.name, () => {
      const src = getEnvTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSourceWithEnv(src, tc.env ?? {});
        const output = tmpl.render(transformOptionValues(tc.params ?? {}));
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        const expectedError = tc.expected_error;
        assert.throws(
          () => {
            const tmpl = Template.fromSourceWithEnv(src, tc.env ?? {});
            tmpl.render(transformOptionValues(tc.params ?? {}));
          },
          (err: unknown) => {
            if (!(err instanceof Error)) return false;
            return (
              matchesExpectedError(err.message, expectedError) ||
              matchesExpectedError(err.constructor.name, expectedError)
            );
          },
          `${tc.description}: expected error containing '${tc.expected_error}'`,
        );
      }
    });
  }
});
