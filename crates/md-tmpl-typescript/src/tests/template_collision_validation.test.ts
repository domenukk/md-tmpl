/**
 * Tests for the md-tmpl TypeScript bindings: statement whitespace trimming, tmpl() parsing and collision validation.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template } from "../index.js";
import { toPascalCase } from "../validation.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Statement-level whitespace control on standalone blockquote tags.
// Trim modifiers ({%-, -%}) are REDUNDANT on standalone blockquote tags —
// blockquote preprocessing already manages all surrounding whitespace.
// These tests verify that trim-modified versions produce identical output
// to non-trim versions, matching Rust's behaviour.
// ---------------------------------------------------------------------------
describe("Statement whitespace trimming (standalone blockquote — redundant)", () => {
  it("{%- for %} is redundant — same output as {% for %}", () => {
    const baseSrc = [
      `---`,
      "params:",
      "  - items = list(name = str)",
      `---`,
      "start   ",
      "",
      "> {% for item in items %}",
      "",
      "{{ item.name }}",
      "",
      "> {% /for %}",
    ].join("\n");
    const trimSrc = baseSrc.replace("> {% for", "> {%- for");
    const baseResult = Template.fromSource(baseSrc).render({
      items: [{ name: "a" }],
    });
    const trimResult = Template.fromSource(trimSrc).render({
      items: [{ name: "a" }],
    });
    assert.strictEqual(
      trimResult,
      baseResult,
      `{%- on standalone blockquote should be redundant`,
    );
  });

  it("{% for -%} is redundant — same output as {% for %}", () => {
    const baseSrc = [
      `---`,
      "params:",
      "  - items = list(name = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "   {{ item.name }}",
      "",
      "> {% /for %}",
    ].join("\n");
    const trimSrc = baseSrc.replace("items %}", "items -%}");
    const baseResult = Template.fromSource(baseSrc).renderUnchecked({
      items: [{ name: "hello" }],
    });
    const trimResult = Template.fromSource(trimSrc).renderUnchecked({
      items: [{ name: "hello" }],
    });
    assert.strictEqual(
      trimResult,
      baseResult,
      `-%} on standalone blockquote should be redundant`,
    );
  });

  it("{%- /for %} is redundant — same output as {% /for %}", () => {
    const baseSrc = [
      `---`,
      "params:",
      "  - items = list(name = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "{{ item.name }}   ",
      "",
      "> {% /for %}",
    ].join("\n");
    const trimSrc = baseSrc.replace("> {% /for", "> {%- /for");
    const baseResult = Template.fromSource(baseSrc).render({
      items: [{ name: "x" }],
    });
    const trimResult = Template.fromSource(trimSrc).render({
      items: [{ name: "x" }],
    });
    assert.strictEqual(
      trimResult,
      baseResult,
      `{%- on standalone closing tag should be redundant`,
    );
  });

  it("{%- if %} is redundant — same output as {% if %}", () => {
    const baseSrc = [
      `---`,
      "params:",
      "  - flag = bool",
      `---`,
      "before   ",
      "",
      "> {% if flag %}",
      "",
      "yes",
      "",
      "> {% /if %}",
    ].join("\n");
    const trimSrc = baseSrc.replace("> {% if", "> {%- if");
    const baseResult = Template.fromSource(baseSrc).render({ flag: true });
    const trimResult = Template.fromSource(trimSrc).render({ flag: true });
    assert.strictEqual(
      trimResult,
      baseResult,
      `{%- on standalone blockquote if should be redundant`,
    );
  });

  it("{% /if -%} is redundant — same output as {% /if %}", () => {
    const baseSrc = [
      `---`,
      "params:",
      "  - flag = bool",
      `---`,
      "> {% if flag %}",
      "",
      "yes",
      "",
      "> {% /if %}",
      "",
      "   after",
    ].join("\n");
    const trimSrc = baseSrc.replace("/if %}", "/if -%}");
    const baseResult = Template.fromSource(baseSrc).render({ flag: true });
    const trimResult = Template.fromSource(trimSrc).render({ flag: true });
    assert.strictEqual(
      trimResult,
      baseResult,
      `-%} on standalone closing tag should be redundant`,
    );
  });
});

// ---------------------------------------------------------------------------
// tmpl() type parsing
// ---------------------------------------------------------------------------
describe("tmpl() type parsing", () => {
  it("tmpl(field = type) is parsed as struct-like type", () => {
    const src = [
      `---`,
      "params:",
      "  - sub = tmpl(title = str, count = int)",
      `---`,
      "{{ sub.title }}: {{ sub.count }}",
    ].join("\n");
    const result = Template.fromSource(src).render({
      sub: { title: "hello", count: 42 },
    });
    assert.strictEqual(result, "hello: 42");
  });

  it("tmpl() declarations are accessible", () => {
    const src = [
      `---`,
      "params:",
      "  - sub = tmpl(title = str)",
      `---`,
      "{{ sub.title }}",
    ].join("\n");
    const decls = Template.fromSource(src).declarations();
    assert.strictEqual(decls.length, 1);
    assert.strictEqual(defined(decls[0])[0], "sub");
  });
});

// ---------------------------------------------------------------------------
// Collision validation
// ---------------------------------------------------------------------------

describe("Collision validation", () => {
  // ── Rule 1: Reserved keywords ───────────────────────────────────────

  describe("Reserved keywords", () => {
    const reserved = [
      "str",
      "bool",
      "int",
      "float",
      "list",
      "struct",
      "enum",
      "tmpl",
      "params",
    ];

    for (const kw of reserved) {
      it(`rejects reserved keyword '${kw}' as param name`, () => {
        const src = `---
params:
  - ${kw} = str
---
{{ ${kw} }}`;
        assert.throws(
          () => Template.fromSource(src),
          (err: Error) => err.message.includes("reserved"),
        );
      });
    }

    it("rejects reserved keyword as const name", () => {
      const src = `---
consts:
  - str = int := 42
---
Hello`;
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("reserved"),
      );
    });

    it("allows non-reserved param names", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - my_str = str
---
{{ my_str }}`,
      );
      assert.strictEqual(tmpl.render({ my_str: "hello" }), "hello");
    });
  });

  // ── Rule 2: Duplicate names ─────────────────────────────────────────

  describe("Duplicate names", () => {
    it("rejects duplicate param names", () => {
      const src = `---
params:
  - name = str
  - name = int
---
{{ name }}`;
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("duplicate"),
      );
    });

    it("rejects duplicate const names", () => {
      const src = `---
consts:
  - VER = str := "1"
  - VER = int := 2
---
Hello`;
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("duplicate"),
      );
    });

    it("allows different param names", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - name = str
  - age = int
---
{{ name }} {{ age }}`,
      );
      assert.strictEqual(tmpl.render({ name: "Alice", age: 30 }), "Alice 30");
    });
  });

  // ── Rule 3: Param ↔ const conflict ──────────────────────────────────

  describe("Param vs const conflict", () => {
    it("rejects param and const with same name", () => {
      const src = `---
params:
  - level = str

consts:
  - level = int := 5
---
{{ level }}`;
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) =>
          err.message.includes("both a param and a constant") ||
          err.message.includes("conflicts with constant"),
      );
    });

    it("allows param and const with different names", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - name = str

consts:
  - VERSION = str := "1.0"
---
{{ name }} v{{ VERSION }}`,
      );
      assert.strictEqual(tmpl.render({ name: "test" }), "test v1.0");
    });
  });

  // ── Rule 4: Type alias vs PascalCase param collision ────────────────

  describe("Type/param PascalCase conflict", () => {
    it("rejects param whose PascalCase matches a type alias with different type", () => {
      const src = [
        `---`,
        "types:",
        "  - CodeReview = list(title = str)",
        "",
        "params:",
        "  - code_review = str",
        `---`,
        "{{ code_review }}",
      ].join("\n");
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("conflicts with type alias"),
      );
    });

    it("allows param whose PascalCase matches type alias when types match", () => {
      const src = [
        `---`,
        "types:",
        "  - CodeReview = list(title = str)",
        "",
        "params:",
        "  - code_review = list(title = str)",
        `---`,
        "> {% for item in code_review %}",
        "",
        "{{ item.title }}",

        "",
        "> {% /for %}",
      ].join("\n");
      // Should NOT throw — the exception applies
      const tmpl = Template.fromSource(src);
      const output = tmpl.render({ code_review: [{ title: "review1" }] });
      assert.ok(output.includes("review1"));
    });

    it("rejects const whose PascalCase matches a type alias with different type", () => {
      const src = [
        `---`,
        "types:",
        "  - CodeReview = list(x = int)",
        "",
        "consts:",
        '  - code_review = str := "hello"',
        `---`,
        "{{ code_review }}",
      ].join("\n");
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("conflicts with type alias"),
      );
    });
  });

  // ── Rule 5: Type alias ↔ import stem ────────────────────────────────

  describe("Type alias vs import stem", () => {
    it("rejects type alias with same name as import stem", () => {
      const src = [
        `---`,
        "imports:",
        '  - "[Utils](./Utils.tmpl.md)"',
        "",
        "types:",
        "  - Utils = list(x = str)",
        "",
        "params:",
        "  - data = Utils",
        `---`,
        "> {% for item in data %}",
        "",
        "{{ item.x }}",

        "",
        "> {% /for %}",
      ].join("\n");
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("shadows"),
      );
    });
  });

  // ── Rule 6: Param PascalCase ↔ import stem ──────────────────────────

  describe("Param PascalCase vs import stem", () => {
    it("rejects param whose PascalCase matches import stem", () => {
      const src = [
        `---`,
        "imports:",
        '  - "[CodeReview](./CodeReview.tmpl.md)"',
        "",
        "params:",
        "  - code_review = str",
        `---`,
        "{{ code_review }}",
      ].join("\n");
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("shadows import"),
      );
    });

    it("rejects const whose PascalCase matches import stem", () => {
      const src = [
        `---`,
        "imports:",
        '  - "[MyConst](./MyConst.tmpl.md)"',
        "",
        "consts:",
        '  - my_const = str := "val"',
        `---`,
        "{{ my_const }}",
      ].join("\n");
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("shadows import"),
      );
    });

    it("allows param whose PascalCase does not match any import stem", () => {
      const src = [
        `---`,
        "imports:",
        '  - "[Utils](./Utils.tmpl.md)"',
        "",
        "params:",
        "  - user_name = str",
        `---`,
        "{{ user_name }}",
      ].join("\n");
      const tmpl = Template.fromSource(src);
      assert.strictEqual(tmpl.render({ user_name: "Alice" }), "Alice");
    });
  });

  // ── Rule 7: Built-in shadowing ──────────────────────────────────────

  describe("Built-in shadowing", () => {
    const builtins = [
      "str",
      "bool",
      "int",
      "float",
      "list",
      "struct",
      "enum",
      "tmpl",
    ];

    for (const name of builtins) {
      it(`rejects type alias that shadows built-in '${name}'`, () => {
        const src = [
          `---`,
          "types:",
          `  - ${name} = list(x = str)`,
          "",
          "params:",
          `  - data = ${name}`,
          `---`,
          "{{ data }}",
        ].join("\n");
        assert.throws(
          () => Template.fromSource(src),
          (err: Error) =>
            err.message.includes("shadow") ||
            err.message.includes("reserved") ||
            err.message.includes("built-in"),
        );
      });
    }

    it("allows type alias that does not shadow a built-in", () => {
      const src = [
        `---`,
        "types:",
        "  - Priority = list(label = str)",
        "",
        "params:",
        "  - items = Priority",
        `---`,
        "> {% for item in items %}",
        "",
        "{{ item.label }}",

        "",
        "> {% /for %}",
      ].join("\n");
      const tmpl = Template.fromSource(src);
      const output = tmpl.render({ items: [{ label: "high" }] });
      assert.ok(output.includes("high"));
    });
  });

  // ── Rule 8: Unused type aliases ─────────────────────────────────────

  describe("Unused type aliases", () => {
    it("rejects unused type alias when params exist", () => {
      const src = [
        `---`,
        "types:",
        "  - Unused = list(x = str)",
        "",
        "params:",
        "  - name = str",
        `---`,
        "{{ name }}",
      ].join("\n");
      assert.throws(
        () => Template.fromSource(src),
        (err: Error) => err.message.includes("unused type alias"),
      );
    });

    it("allows unused type alias with allow_unused: true", () => {
      const src = [
        `---`,
        "allow_unused: true",
        "types:",
        "  - Unused = list(x = str)",
        "",
        "params:",
        "  - name = str",
        `---`,
        "{{ name }}",
      ].join("\n");
      const tmpl = Template.fromSource(src);
      assert.strictEqual(tmpl.render({ name: "hello" }), "hello");
    });

    it("allows used type alias", () => {
      const src = [
        `---`,
        "types:",
        "  - Items = list(title = str)",
        "",
        "params:",
        "  - items = Items",
        `---`,
        "> {% for item in items %}",
        "",
        "{{ item.title }}",

        "",
        "> {% /for %}",
      ].join("\n");
      const tmpl = Template.fromSource(src);
      const output = tmpl.render({ items: [{ title: "test" }] });
      assert.ok(output.includes("test"));
    });

    it("skips unused check when no params and no consts (type-library)", () => {
      const src = [
        `---`,
        "types:",
        "  - SomeType = list(x = str)",
        `---`,
        "Static content",
      ].join("\n");
      // No params/consts → R4 should NOT fire
      const tmpl = Template.fromSource(src);
      assert.ok(tmpl.body().includes("Static content"));
    });
  });

  // ── Validation not in lenient mode ──────────────────────────────────

  describe("Validation scope", () => {
    it("fromSourceAllowingUnused does NOT run collision validation", () => {
      // This has a param↔const conflict which would fail in strict mode
      const src = `---
params:
  - level = str

consts:
  - level = int := 5
---
{{ level }}`;
      // Should NOT throw in lenient mode
      const tmpl = Template.fromSourceAllowingUnused(src);
      assert.ok(tmpl.declarations().length > 0);
    });

    it("renderUnchecked does NOT re-run validation", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - name = str
---
{{ name }}`,
      );
      // Should succeed without re-validation
      assert.strictEqual(tmpl.renderUnchecked({ name: "test" }), "test");
    });
  });

  // ── toPascalCase helper ─────────────────────────────────────────────

  describe("toPascalCase", () => {
    it("converts snake_case", () => {
      assert.strictEqual(toPascalCase("code_review"), "CodeReview");
      assert.strictEqual(toPascalCase("simple_greeting"), "SimpleGreeting");
    });

    it("converts kebab-case", () => {
      assert.strictEqual(toPascalCase("task-report"), "TaskReport");
    });

    it("converts single word", () => {
      assert.strictEqual(toPascalCase("single"), "Single");
    });

    it("handles empty string", () => {
      assert.strictEqual(toPascalCase(""), "");
    });

    it("handles leading/trailing separators", () => {
      assert.strictEqual(toPascalCase("_leading"), "Leading");
      assert.strictEqual(toPascalCase("trailing_"), "Trailing");
      assert.strictEqual(toPascalCase("__double__"), "Double");
    });

    it("preserves existing caps in segments", () => {
      assert.strictEqual(
        toPascalCase("already_PascalCase"),
        "AlreadyPascalCase",
      );
    });
  });
});
