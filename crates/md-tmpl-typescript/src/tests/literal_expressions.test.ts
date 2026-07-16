/**
 * Regression tests locking literal-expression support across ALL expression
 * positions (design-doc "option B": literals valid everywhere).
 *
 * Bare int / float / bool literals resolve to values in output, filter-input,
 * panic-arg, and string-interpolation positions — exactly like the like-typed
 * variable would — reusing the same numeric parser the condition evaluator
 * uses (`numericLiteralToValue` in evaluator.ts is the single source of truth).
 *
 * Outputs here are asserted byte-for-byte to match the Rust core:
 *   {{ 42 }}->42  {{ -7 }}->-7  {{ 0 }}->0
 *   {{ 3.14 }}->3.14  {{ 3.0 }}->3  {{ -0.0 }}->0
 *   {{ true }}->true  {{ false }}->false
 *   {{ "hi" }}->hi  {{ 'hi' }}->hi  {{ "a\"b" }}->a"b
 *
 * Non-goals that MUST stay unchanged (also locked below):
 *   - `{{ 42 | upper }}` still errors: `upper` requires a string VALUE.
 *   - Undefined variables still error (a bare literal must not mask them).
 *   - Function builtins (len/has/kind/kinds/idx) stay path/binding-only.
 *
 * @module
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  Template,
  TemplateSyntaxError,
  TemplatePanicError,
  UndefinedVariableError,
} from "../index.js";

/** Render `body` (with optional params/ctx) and return the trimmed output. */
function render(
  body: string,
  fmLines: readonly string[] = ["params: []"],
  ctx: Record<string, unknown> = {},
): string {
  const src = `---\n${fmLines.join("\n")}\n---\n${body}`;
  return Template.fromSource(src).render(ctx).trim();
}

describe("Literal expressions — option B (literals valid in all positions)", () => {
  describe("P1 output — bare literals render like their variable", () => {
    it("renders integer literals (positive, negative, zero)", () => {
      assert.strictEqual(render("{{ 42 }}"), "42");
      assert.strictEqual(render("{{ -7 }}"), "-7");
      assert.strictEqual(render("{{ 0 }}"), "0");
      assert.strictEqual(render("{{ 100 }}"), "100");
    });

    it("renders float literals, dropping trailing .0 and normalizing -0.0", () => {
      assert.strictEqual(render("{{ 3.14 }}"), "3.14");
      assert.strictEqual(render("{{ 3.0 }}"), "3");
      assert.strictEqual(render("{{ -0.0 }}"), "0");
      assert.strictEqual(render("{{ -2.5 }}"), "-2.5");
    });

    it("renders bool literals", () => {
      assert.strictEqual(render("{{ true }}"), "true");
      assert.strictEqual(render("{{ false }}"), "false");
    });

    it("renders string literals (double and single quotes, escapes)", () => {
      assert.strictEqual(render(`{{ "hi" }}`), "hi");
      assert.strictEqual(render(`{{ 'hi' }}`), "hi");
      assert.strictEqual(render(`{{ "a\\"b" }}`), 'a"b');
    });
  });

  describe("P2 filter-input — literals flow into filters", () => {
    it("applies string filters to string literals", () => {
      assert.strictEqual(render(`{{ "hi" | upper }}`), "HI");
    });

    it("applies numeric filters to numeric literals", () => {
      assert.strictEqual(render("{{ 3.0 | fixed(2) }}"), "3.00");
      assert.strictEqual(render("{{ 42 | fixed(0) }}"), "42");
    });

    it("still rejects a string filter applied to a numeric literal VALUE", () => {
      // Option B makes `42` a valid int expression, but `upper` operates on
      // strings — the value-type check must still fire (was previously masked
      // by an 'undefined variable' error when `42` was treated as a path).
      assert.throws(
        () => render("{{ 42 | upper }}"),
        (err: Error) =>
          err instanceof TemplateSyntaxError &&
          err.message.includes("'upper' requires a string"),
      );
    });
  });

  describe("P7 panic-arg — literals usable as panic messages", () => {
    it("accepts a bare string/number/bool literal as the panic message", () => {
      assert.throws(
        () => render(`z {% panic("boom") %}`),
        (err: Error) =>
          err instanceof TemplatePanicError && err.message.includes("boom"),
      );
      assert.throws(
        () => render("z {% panic(42) %}"),
        (err: Error) =>
          err instanceof TemplatePanicError && err.message.includes("42"),
      );
      assert.throws(
        () => render("z {% panic(true) %}"),
        (err: Error) =>
          err instanceof TemplatePanicError && err.message.includes("true"),
      );
    });

    it("interpolates a literal inside a panic-string message", () => {
      assert.throws(
        () => render(`z {% panic("boom {{ 42 }}") %}`),
        (err: Error) =>
          err instanceof TemplatePanicError && err.message.includes("boom 42"),
      );
      assert.throws(
        () => render(`z {% panic("f={{ 3.14 }}") %}`),
        (err: Error) =>
          err instanceof TemplatePanicError && err.message.includes("f=3.14"),
      );
    });
  });

  describe("P8 string-interpolation — literals interpolate inside strings", () => {
    it("interpolates literals inside a string operand of a comparison", () => {
      assert.strictEqual(
        render(
          `z {% if x == "v{{ 42 }}" %}Y{% else %}N{% /if %}`,
          ["params:", "  - x = str"],
          { x: "v42" },
        ),
        "z Y",
      );
    });
  });

  describe("consistency with conditions (already literal-aware)", () => {
    it("keeps condition truthiness/comparison literal behavior", () => {
      assert.strictEqual(render("z {% if 42 %}Y{% /if %}"), "z Y");
      assert.strictEqual(render("z {% if 0 %}Y{% /if %}"), "z");
      assert.strictEqual(render("z {% if true %}Y{% /if %}"), "z Y");
      assert.strictEqual(render("z {% if false %}Y{% /if %}"), "z");
    });
  });

  describe("safety — literals must not mask real errors", () => {
    it("still reports undefined variables (bare identifier)", () => {
      assert.throws(
        () => render("{{ foo }}"),
        (err: Error) =>
          err instanceof TemplateSyntaxError &&
          err.message.includes("undeclared variable"),
      );
    });

    it("does not mis-parse a malformed multi-dot number as a value", () => {
      // `3.1.4` has no valid numeric shape — it must fall through to path
      // resolution and error, NEVER render as NaN.
      assert.throws(
        () => render("{{ 3.1.4 }}"),
        (err: Error) => err instanceof UndefinedVariableError,
      );
    });
  });
});
