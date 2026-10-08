/**
 * Tests for the md-tmpl TypeScript bindings: transparent option(T) and flow-sensitive narrowing.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template, TemplateSyntaxError, TypeMismatchError } from "../index.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Transparent option(T) comprehensive tests
// ---------------------------------------------------------------------------

describe("Transparent option(T)", () => {
  it("null renders as empty string for option(str)", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
[{% if has(x) %}{{ x }}{% /if %}]`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "[]");
  });

  it("undefined renders as empty string for option(str)", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
[{% if has(x) %}{{ x }}{% /if %}]`);
    assert.strictEqual(tmpl.render({ x: undefined }).trim(), "[]");
  });

  it("value renders directly for option(str)", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
[{% if has(x) %}{{ x }}{% /if %}]`);
    assert.strictEqual(tmpl.render({ x: "hello" }).trim(), "[hello]");
  });

  it("has() returns false for null option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

yes

> {% else %}

no

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "no");
  });

  it("has() returns true for present option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

yes

> {% else %}

no

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: "hello" }).trim(), "yes");
  });

  it("has() returns true for option value that is the string 'None'", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

{{ x }}

> {% else %}

absent

> {% /if %}`);
    // The string "None" is a valid string value, not the None variant
    const output = tmpl.render({ x: "None" }).trim();
    assert.strictEqual(output, "None");
  });

  it("kind() returns None for null option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "None");
  });

  it("kind() returns Some for present option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: 42 }).trim(), "Some");
  });

  it("match works with option None", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% match x %}

> {% case Some %}

value: {{ x }}

> {% case None %}

absent

> {% /match %}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "absent");
  });

  it("match works with option Some", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% match x %}

> {% case Some %}

value: {{ x }}

> {% case None %}

absent

> {% /match %}`);
    assert.strictEqual(tmpl.render({ x: 42 }).trim(), "value: 42");
  });

  it("option(int) accepts number values", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: 7 }).trim(), "7");
  });

  it("option(int) accepts null", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
[{% if has(x) %}{{ x }}{% /if %}]`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "[]");
  });

  it("option(float) accepts number values", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(float)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: 3.14 }).trim(), "3.14");
  });

  it("option(bool) accepts boolean values", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(bool)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: true }).trim(), "true");
  });

  it("option(bool) null renders empty", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(bool)
---
[{% if has(x) %}{{ x }}{% /if %}]`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "[]");
  });

  it("option in struct field — null", () => {
    const tmpl = Template.fromSource(`---
params:
  - person = struct(name = str, email = option(str))
---
> {% if has(person.email) %}

email: {{ person.email }}

> {% else %}

no email

> {% /if %}`);
    assert.strictEqual(
      tmpl.render({ person: { name: "Alice", email: null } }).trim(),
      "no email",
    );
  });

  it("option in struct field — present", () => {
    const tmpl = Template.fromSource(`---
params:
  - person = struct(name = str, email = option(str))
---
> {% if has(person.email) %}

email: {{ person.email }}

> {% else %}

no email

> {% /if %}`);
    assert.strictEqual(
      tmpl.render({ person: { name: "Alice", email: "a@b.com" } }).trim(),
      "email: a@b.com",
    );
  });

  it("option in list items", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(name = str, note = option(str))
---
> {% for item in items %}
> {% if has(item.note) %}

{{ item.name }}: {{ item.note }}

> {% else %}

{{ item.name }}: (no note)

> {% /if %}
> {% /for %}`);
    const output = tmpl
      .render({
        items: [
          { name: "a", note: "hello" },
          { name: "b", note: null },
        ],
      })
      .trim();
    assert.ok(output.includes("a: hello"));
    assert.ok(output.includes("b: (no note)"));
  });

  it("throws on undefined variable in expression", () => {
    // Undeclared root variables are rejected at compile time (mirrors the Rust
    // core's `check_undeclared_variables`), so the error surfaces at
    // `fromSource` before `render` is ever reached.
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - x = str
---
{{ x }} {{ y }}`),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(err.message.includes("undeclared variable"));
        return true;
      },
    );
  });

  it("default None creates null in defaults()", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str) := None
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const defs = tmpl.defaults();
    assert.strictEqual(defs.x, null);
  });

  it("default value creates the value in defaults()", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str) := "hello"
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const defs = tmpl.defaults();
    assert.strictEqual(defs.x, "hello");
  });

  it("option(int) default renders correctly", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := 42
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render().trim(), "42");
  });

  it("option(int) default None renders empty", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := None
---
[{% if has(x) %}{{ x }}{% /if %}]`);
    assert.strictEqual(tmpl.render().trim(), "[]");
  });

  it("type checking rejects wrong type for option inner", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.throws(
      () => tmpl.render({ x: "not a number" }),
      (err: Error) => {
        assert.ok(err instanceof TypeMismatchError);
        return true;
      },
    );
  });

  it("type checking accepts null for option(int)", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.doesNotThrow(() => tmpl.render({ x: null }));
  });

  it("rejects option in bare condition at compile time", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - x = option(str)
---
> {% if x %}

yes

> {% else %}

no

> {% /if %}`),
      /cannot evaluate truthiness of option/,
    );
  });

  it("option roundtrip: declarations show option(T)", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const decls = tmpl.declarations();
    assert.strictEqual(decls.length, 1);
    assert.strictEqual(defined(decls[0])[0], "x");
    assert.strictEqual(defined(decls[0])[1], "option(str)");
  });

  it("inline match with option Some", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% match x %}

> {% case Some %}

got: {{ x }}

> {% case None %}

> {% /match %}`);
    assert.strictEqual(tmpl.render({ x: 42 }).trim(), "got: 42");
    assert.strictEqual(tmpl.render({ x: null }).trim(), "");
  });

  it("match with else arm", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% match x %}

> {% case Some %}

{{ x }}

> {% else %}

default

> {% /match %}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "default");
    assert.strictEqual(tmpl.render({ x: 99 }).trim(), "99");
  });

  it("rejects option in bare condition at compile time", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - x = option(str)
---
> {% if x %}

yes

> {% else %}

no

> {% /if %}`),
      /cannot evaluate truthiness of option/,
    );
  });

  it("option renderUnchecked with null", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

{{ x }}

> {% else %}

none

> {% /if %}`);
    assert.strictEqual(tmpl.renderUnchecked({ x: null }).trim(), "none");
  });

  it("option renderUnchecked with value", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

{{ x }}

> {% else %}

none

> {% /if %}`);
    assert.strictEqual(tmpl.renderUnchecked({ x: "hi" }).trim(), "hi");
  });
});

// ===========================================================================
// Flow-sensitive narrowing tests
// ===========================================================================

describe("Flow-sensitive narrowing", () => {
  describe("has() narrows option(T) to T", () => {
    it("option(str) inside has() guard compiles and renders", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - x = option(str)
---
> {% if has(x) %}{{ x }}{% /if %}`,
      );
      assert.strictEqual(tmpl.render({ x: "hello" }).trim(), "hello");
    });

    it("option(int) inside has() guard compiles and renders", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - x = option(int)
---
> {% if has(x) %}{{ x }}{% /if %}`,
      );
      assert.strictEqual(tmpl.render({ x: 42 }).trim(), "42");
    });

    it("option(str) without has() guard is compile error", () => {
      assert.throws(
        () =>
          Template.fromSource(
            `---
params:
  - x = option(str)
---
{{ x }}`,
          ),
        /cannot display.*option/,
      );
    });

    it("option(int) without has() guard is compile error", () => {
      assert.throws(
        () =>
          Template.fromSource(
            `---
params:
  - x = option(int)
---
{{ x }}`,
          ),
        /cannot display.*option/,
      );
    });

    it("else branch does NOT narrow option", () => {
      // In the else branch of has(x), x is still option — not narrowed
      assert.throws(
        () =>
          Template.fromSource(
            `---
params:
  - x = option(str)
---
> {% if has(x) %}present{% else %}{{ x }}{% /if %}`,
          ),
        /cannot display.*option/,
      );
    });

    it("nested has() narrowing works", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - a = option(str)
  - b = option(int)
---
> {% if has(a) %}{% if has(b) %}{{ a }}-{{ b }}{% /if %}{% /if %}`,
      );
      assert.strictEqual(tmpl.render({ a: "x", b: 5 }).trim(), "x-5");
    });

    it("bare option condition is compile error", () => {
      assert.throws(
        () =>
          Template.fromSource(
            `---
params:
  - opt = option(str)
---
> {% if opt %}yes{% /if %}`,
          ),
        /cannot evaluate truthiness of option/,
      );
    });

    it("has(test) && test in same condition works due to left-to-right narrowing", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - test = option(str) := None
---
> {% if has(test) && test %}val: {{ test }}{% /if %}`,
      );
      assert.strictEqual(tmpl.render({}), "");
      assert.strictEqual(tmpl.render({ test: "" }), "");
      assert.strictEqual(tmpl.render({ test: "hello" }), "val: hello");
    });
  });

  describe("match narrows enum to matched variant", () => {
    it("enum field access inside match arm compiles", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
> {% match outcome %}
> {% case Confirmed %}

{{ outcome.evidence }}

> {% case Rejected %}

rejected

> {% /match %}`,
      );
      const result = tmpl.render({
        outcome: { __kind__: "Confirmed", evidence: "proof" },
      });
      assert.ok(result.includes("proof"), `got: ${result}`);
    });

    it("bare unit-variant enum compiles and renders", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - status = enum(Active, Inactive)
---
{{ status }}`,
      );
      assert.strictEqual(tmpl.render({ status: "Active" }).trim(), "Active");
    });

    it("bare struct-variant enum without match is compile error", () => {
      assert.throws(
        () =>
          Template.fromSource(
            `---
params:
  - status = enum(Active(reason = str), Inactive)
---
{{ status }}`,
          ),
        /cannot display.*enum/,
      );
    });

    it("kind() on enum is allowed without narrowing", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - status = enum(Active, Inactive)
---
{{ kind(status) }}`,
      );
      assert.ok(tmpl);
    });
  });

  describe("for-loop introduces element binding", () => {
    it("for-loop binding allows scalar element display", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - items = list(str)
---
> {% for item in items %}{{ item }} {% /for %}`,
      );
      const result = tmpl.render({ items: ["a", "b", "c"] });
      assert.ok(result.includes("a"), `got: ${result}`);
    });

    it("for-loop binding allows struct field access", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - items = list(name = str, score = int)
---
> {% for item in items %}{{ item.name }}: {{ item.score }} {% /for %}`,
      );
      const result = tmpl.render({ items: [{ name: "Alice", score: 95 }] });
      assert.ok(result.includes("Alice"), `got: ${result}`);
      assert.ok(result.includes("95"), `got: ${result}`);
    });

    it("bare list without for-loop is compile error", () => {
      assert.throws(
        () =>
          Template.fromSource(
            `---
params:
  - items = list(name = str)
---
{{ items }}`,
          ),
        /cannot display.*list/,
      );
    });
  });

  describe("struct displayability", () => {
    it("bare struct is compile error", () => {
      assert.throws(
        () =>
          Template.fromSource(
            `---
params:
  - config = struct(timeout = int, name = str)
---
{{ config }}`,
          ),
        /cannot display.*struct/,
      );
    });

    it("struct field access is allowed", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - config = struct(timeout = int, name = str)
---
{{ config.timeout }} {{ config.name }}`,
      );
      const result = tmpl.render({ config: { timeout: 30, name: "test" } });
      assert.ok(result.includes("30"), `got: ${result}`);
      assert.ok(result.includes("test"), `got: ${result}`);
    });
  });

  describe("filters skip displayability check", () => {
    it("list | join() passes compile-time check", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - items = list(str)
---
{{ items | join(", ") }}`,
      );
      assert.strictEqual(
        tmpl.render({ items: ["a", "b", "c"] }).trim(),
        "a, b, c",
      );
    });

    it("str | upper passes compile-time check", () => {
      const tmpl = Template.fromSource(
        `---
params:
  - name = str
---
{{ name | upper }}`,
      );
      assert.strictEqual(tmpl.render({ name: "hello" }).trim(), "HELLO");
    });
  });
});
