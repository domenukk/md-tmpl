/**
 * Shared test runner for cross-backend test fixtures.
 *
 * Runs test cases defined in `tests/shared/inline_tmpl_tests.json` and
 * `tests/shared/include_tests.json` against the TypeScript backend.
 *
 * These same fixtures are consumed by the Rust backend's shared test
 * runner, ensuring behavioral parity between implementations.
 *
 * Templates use `template_lines` / `parent_template_lines` (arrays of
 * strings joined with `\n`) so the JSON stays readable.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

import { Template } from "../index.js";

// ---------------------------------------------------------------------------
// Fixture paths
// ---------------------------------------------------------------------------

const FIXTURES_DIR = path.resolve(__dirname, "../../../../tests/shared");

const INLINE_TMPL_FIXTURES = path.join(FIXTURES_DIR, "inline_tmpl_tests.json");
const INCLUDE_FIXTURES = path.join(FIXTURES_DIR, "include_tests.json");
const INLINE_CONTROL_FIXTURES = path.join(
  FIXTURES_DIR,
  "inline_control_tests.json",
);
const TMPL_PARAM_FIXTURES = path.join(FIXTURES_DIR, "tmpl_param_tests.json");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface InlineTmplTestCase {
  name: string;
  description: string;
  template_lines: string[];
  params: Record<string, unknown>;
  expected_output?: string;
  expected_error?: string;
}

interface IncludeTestCase {
  name: string;
  description: string;
  files: Record<string, string[]>;
  parent_template_lines: string[];
  params: Record<string, unknown>;
  expected_output?: string;
  expected_error?: string;
}

/** Join an array of lines with newlines. */
function joinLines(lines: string[]): string {
  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// Inline template tests (from shared fixtures)
// ---------------------------------------------------------------------------

describe("Shared: Inline template tests", () => {
  const raw = fs.readFileSync(INLINE_TMPL_FIXTURES, "utf-8");
  const { tests } = JSON.parse(raw) as { tests: InlineTmplTestCase[] };

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = joinLines(tc.template_lines);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(tc.params);
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
            tmpl.render(tc.params);
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
// File-based include tests (from shared fixtures)
// ---------------------------------------------------------------------------

describe("Shared: File-based include tests", () => {
  const raw = fs.readFileSync(INCLUDE_FIXTURES, "utf-8");
  const { tests } = JSON.parse(raw) as { tests: IncludeTestCase[] };

  for (const tc of tests) {
    it(tc.name, () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-shared-"));
      try {
        // Write all include files to the temp directory.
        for (const [filename, contentLines] of Object.entries(tc.files)) {
          fs.writeFileSync(path.join(dir, filename), joinLines(contentLines));
        }

        const parentSrc = joinLines(tc.parent_template_lines);

        if (tc.expected_output !== undefined) {
          const tmpl = Template.fromSourceWithBaseDir(parentSrc, dir);
          const output = tmpl.render(tc.params);
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
              tmpl.render(tc.params);
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
  const { tests } = JSON.parse(raw) as { tests: InlineTmplTestCase[] };

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = joinLines(tc.template_lines);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(tc.params);
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
            tmpl.render(tc.params);
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
  const { tests } = JSON.parse(raw) as { tests: InlineTmplTestCase[] };

  for (const tc of tests) {
    it(tc.name, () => {
      const templateSrc = joinLines(tc.template_lines);

      if (tc.expected_output !== undefined) {
        const tmpl = Template.fromSource(templateSrc);
        const output = tmpl.render(tc.params);
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
            tmpl.render(tc.params);
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
