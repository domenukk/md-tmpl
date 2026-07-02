/**
 * Shared test runner for cross-backend test fixtures.
 *
 * Runs test cases defined in `tests/shared/inline_tmpl_tests.toml`,
 * `tests/shared/include_tests.toml`, `tests/shared/inline_control_tests.toml`,
 * and `tests/shared/tmpl_param_tests.toml` against the TypeScript backend.
 *
 * These same fixtures are consumed by the Rust backend's shared test
 * runner, ensuring behavioral parity between implementations.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import TOML from "@iarna/toml";

import { Template } from "../index.js";

// ---------------------------------------------------------------------------
// Fixture paths
// ---------------------------------------------------------------------------

const FIXTURES_DIR = path.resolve(__dirname, "../../../../tests/shared");

const INLINE_TMPL_FIXTURES = path.join(FIXTURES_DIR, "inline_tmpl_tests.toml");
const INCLUDE_FIXTURES = path.join(FIXTURES_DIR, "include_tests.toml");
const INLINE_CONTROL_FIXTURES = path.join(
  FIXTURES_DIR,
  "inline_control_tests.toml",
);
const TMPL_PARAM_FIXTURES = path.join(FIXTURES_DIR, "tmpl_param_tests.toml");
const FEATURE_E2E_FIXTURES = path.join(FIXTURES_DIR, "feature_e2e_tests.toml");

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

interface IncludeTestCase {
  name: string;
  description: string;
  files: Record<string, string[] | string>;
  parent_template_lines?: string[];
  parent_template?: string;
  params: Record<string, unknown>;
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
  return joinLines(tc.template_lines || []);
}

// ---------------------------------------------------------------------------
// Inline template tests (from shared fixtures)
// ---------------------------------------------------------------------------

describe("Shared: Inline template tests", () => {
  const raw = fs.readFileSync(INLINE_TMPL_FIXTURES, "utf-8");
  const { tests } = TOML.parse(raw) as unknown as {
    tests: InlineTmplTestCase[];
  };

  for (const tc of tests) {
    it(tc.name, () => {
      const src = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(src);
        const output = tmpl.render(tc.params || {});
        assert.strictEqual(
          output,
          tc.expected_output,
          `${tc.description}: output mismatch`,
        );
      } else if (tc.expected_error !== undefined) {
        assert.throws(
          () => {
            const tmpl = Template.fromSource(src);
            tmpl.render(tc.params || {});
          },
          (err: unknown) => {
            if (!(err instanceof Error)) return false;
            return (
              err.message
                .toLowerCase()
                .includes(tc.expected_error!.toLowerCase()) ||
              err.constructor.name
                .toLowerCase()
                .includes(tc.expected_error!.toLowerCase())
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
  const raw = fs.readFileSync(INCLUDE_FIXTURES, "utf-8");
  const { tests } = TOML.parse(raw) as unknown as { tests: IncludeTestCase[] };

  for (const tc of tests) {
    it(tc.name, () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-shared-"));
      try {
        // Write all include files to the temp directory.
        for (const [filename, content] of Object.entries(tc.files || {})) {
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

        let parentSrc =
          tc.parent_template ||
          (tc.parent_template_lines ? joinLines(tc.parent_template_lines) : "");
        if (parentSrc.endsWith(".tmpl.md")) {
          const fullPath = path.resolve(FIXTURES_DIR, parentSrc);
          if (fs.existsSync(fullPath)) {
            parentSrc = fs.readFileSync(fullPath, "utf-8");
          }
        }

        if (tc.expected_output !== undefined) {
          const tmpl = Template.fromSourceWithBaseDir(parentSrc, dir);
          const output = tmpl.render(tc.params || {});
          assert.strictEqual(
            output,
            tc.expected_output,
            `${tc.description}: output mismatch`,
          );
        } else if (tc.expected_error !== undefined) {
          const expectedSubstring = tc.expected_error;
          assert.throws(
            () => {
              const tmpl = Template.fromSourceWithBaseDir(parentSrc, dir);
              tmpl.render(tc.params || {});
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
  const raw = fs.readFileSync(INLINE_CONTROL_FIXTURES, "utf-8");
  const { tests } = TOML.parse(raw) as unknown as {
    tests: InlineTmplTestCase[];
  };

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(tc.params || {});
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
            tmpl.render(tc.params || {});
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
    });
  }
});

// ---------------------------------------------------------------------------
// Shared tmpl() parameter tests
// ---------------------------------------------------------------------------

describe("Shared: tmpl() parameter tests", () => {
  const raw = fs.readFileSync(TMPL_PARAM_FIXTURES, "utf-8");
  const { tests } = TOML.parse(raw) as unknown as {
    tests: InlineTmplTestCase[];
  };

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(tc.params || {});
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
            tmpl.render(tc.params || {});
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
    });
  }
});

// ---------------------------------------------------------------------------
// Shared Feature E2E tests (Milestone E2E.2)
// ---------------------------------------------------------------------------

describe("Shared: Feature E2E tests (Milestone E2E.2)", () => {
  const raw = fs.readFileSync(FEATURE_E2E_FIXTURES, "utf-8");
  const { tests } = TOML.parse(raw) as unknown as {
    tests: InlineTmplTestCase[];
  };

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = getTemplateSrc(tc);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(tc.params || {});
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
            tmpl.render(tc.params || {});
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
    });
  }
});
