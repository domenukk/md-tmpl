/**
 * Shared helpers for the md-tmpl TypeScript template test suites.
 */

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

/**
 * Assert that an indexed or optional lookup produced a value and return it.
 *
 * Replaces non-null assertions (`x!`) in tests: it narrows away `undefined`
 * at runtime via `assert`, yielding a clear failure message instead of a
 * downstream `TypeError`.
 */
export function defined<T>(
  value: T | undefined,
  message = "expected a defined value",
): T {
  assert.ok(value !== undefined, message);
  return value;
}

export function withTempFile(
  content: string,
  fn: (filepath: string) => void,
): void {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-test-"));
  const filepath = path.join(dir, "test.tmpl.md");
  fs.writeFileSync(filepath, content);
  try {
    fn(filepath);
  } finally {
    fs.rmSync(dir, { recursive: true });
  }
}
