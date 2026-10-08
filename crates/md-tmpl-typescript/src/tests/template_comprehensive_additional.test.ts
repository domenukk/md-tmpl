/**
 * Tests for the md-tmpl TypeScript bindings: option access, match else, builtins, constants, conditionals and values.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  Template,
  TemplateSyntaxError,
  UndefinedVariableError,
} from "../index.js";
import { fromJs, display, isTruthy } from "../value.js";

// ---------------------------------------------------------------------------
// Option handling with transparent access
// ---------------------------------------------------------------------------

describe("Option transparent access comprehensive", () => {
  it("accesses option value directly (transparent)", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: "hello" }).trim(), "hello");
  });

  it("kind() returns Some for present option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: 42 }).trim(), "Some");
  });

  it("kind() returns None for absent option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "None");
  });

  it("has() returns true for present option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}present{% else %}absent{% /if %}`);
    assert.strictEqual(tmpl.render({ x: "hi" }).trim(), "present");
  });

  it("has() returns false for absent option", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}present{% else %}absent{% /if %}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "absent");
  });

  it("match/case on option Some", () => {
    const tmpl = Template.fromSource(
      [
        "---",
        "params:",
        "  - x = option(int)",
        "---",
        "> {% match x %}",
        "",
        "> {% case Some %}",
        "",
        "got {{ x }}",
        "",
        "> {% case None %}",
        "",
        "nothing",
        "",
        "> {% /match %}",
      ].join("\n"),
    );
    const result = tmpl.render({ x: 42 });
    assert.ok(result.includes("got 42"));
  });

  it("match/case on option None", () => {
    const tmpl = Template.fromSource(
      [
        "---",
        "params:",
        "  - x = option(int)",
        "---",
        "> {% match x %}",
        "",
        "> {% case Some %}",
        "",
        "got {{ x }}",
        "",
        "> {% case None %}",
        "",
        "nothing",
        "",
        "> {% /match %}",
      ].join("\n"),
    );
    const result = tmpl.render({ x: null });
    assert.ok(result.includes("nothing"));
  });

  it("option with default None renders correctly", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str) := None
---
> {% if has(x) %}{{ x }}{% else %}default{% /if %}`);
    assert.strictEqual(tmpl.render({}).trim(), "default");
  });

  it("option with default Some value", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str) := "hello"
---
> {% if has(x) %}{{ x }}{% else %}default{% /if %}`);
    assert.strictEqual(tmpl.render({}).trim(), "hello");
  });
});

// ---------------------------------------------------------------------------
// Match/case with {% else %} keyword
// ---------------------------------------------------------------------------

describe("Match with {% else %} keyword", () => {
  it("else arm catches unmatched variant", () => {
    const tmpl = Template.fromSource(
      [
        "---",
        "params:",
        "  - status = enum(Ok, Err, Pending)",
        "---",
        "> {% match status %}",
        "",
        "> {% case Ok %}",
        "",
        "success",
        "",
        "> {% else %}",
        "",
        "other",
        "",
        "> {% /match %}",
      ].join("\n"),
    );
    assert.ok(tmpl.render({ status: "Ok" }).includes("success"));
    assert.ok(tmpl.render({ status: "Err" }).includes("other"));
    assert.ok(tmpl.render({ status: "Pending" }).includes("other"));
  });

  it("{% else %} works as catch-all in match", () => {
    const tmpl = Template.fromSource(
      [
        "---",
        "params:",
        "  - status = enum(Ok, Err, Pending)",
        "---",
        "> {% match status %}",
        "",
        "> {% case Ok %}",
        "",
        "success",
        "",
        "> {% else %}",
        "",
        "other",
        "",
        "> {% /match %}",
      ].join("\n"),
    );
    assert.ok(tmpl.render({ status: "Ok" }).includes("success"));
    assert.ok(tmpl.render({ status: "Err" }).includes("other"));
  });

  it("match with multiple cases and else", () => {
    const tmpl = Template.fromSource(
      [
        "---",
        "params:",
        "  - color = enum(Red, Green, Blue, Yellow)",
        "---",
        "> {% match color %}",
        "",
        "> {% case Red %}",
        "",
        "hot",
        "",
        "> {% case Blue %}",
        "",
        "cool",
        "",
        "> {% else %}",
        "",
        "neutral",
        "",
        "> {% /match %}",
      ].join("\n"),
    );
    assert.ok(tmpl.render({ color: "Red" }).includes("hot"));
    assert.ok(tmpl.render({ color: "Blue" }).includes("cool"));
    assert.ok(tmpl.render({ color: "Green" }).includes("neutral"));
    assert.ok(tmpl.render({ color: "Yellow" }).includes("neutral"));
  });
});

// ---------------------------------------------------------------------------
// Built-in functions edge cases
// ---------------------------------------------------------------------------

describe("Built-in function edge cases", () => {
  it("idx() starts at 0", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
> {% for item in items %}{{ idx(item) }}{% /for %}`);
    assert.strictEqual(tmpl.render({ items: ["a", "b", "c"] }).trim(), "012");
  });

  it("len() on string", () => {
    const tmpl = Template.fromSource(`---
params:
  - text = str
---
{{ len(text) }}`);
    assert.strictEqual(tmpl.render({ text: "hello" }).trim(), "5");
  });

  it("len() on empty string", () => {
    const tmpl = Template.fromSource(`---
params:
  - text = str
---
{{ len(text) }}`);
    assert.strictEqual(tmpl.render({ text: "" }).trim(), "0");
  });

  it("len() on list", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
{{ len(items) }}`);
    assert.strictEqual(tmpl.render({ items: ["a", "b"] }).trim(), "2");
  });

  it("len() on empty list", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
{{ len(items) }}`);
    assert.strictEqual(tmpl.render({ items: [] }).trim(), "0");
  });

  it("len() on struct is rejected — only list and string are valid", () => {
    const tmpl = Template.fromSource(`---
params:
  - obj = struct(a = str, b = int)
---
{{ len(obj) }}`);
    assert.throws(
      () => tmpl.render({ obj: { a: "x", b: 1 } }),
      (err: Error) => err.message.includes("len()"),
    );
  });

  it("kind() on unit enum variant", () => {
    const tmpl = Template.fromSource(`---
params:
  - color = enum(Red, Green, Blue)
---
{{ kind(color) }}`);
    assert.strictEqual(tmpl.render({ color: "Red" }).trim(), "Red");
  });

  it("idx() throws on non-loop binding", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = str
---
{{ idx(x) }}`);
    assert.throws(() => tmpl.render({ x: "hi" }), TemplateSyntaxError);
  });
});

// ---------------------------------------------------------------------------
// for...else edge cases (additional)
// ---------------------------------------------------------------------------

describe("for...else edge cases (additional)", () => {
  it("for...else with single item list", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
> {% for item in items %}{{ item }}{% else %}empty{% /for %}`);
    assert.strictEqual(tmpl.render({ items: ["only"] }).trim(), "only");
  });

  it("for...else with many items", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(int)
---
> {% for item in items %}{{ item }},{% else %}empty{% /for %}`);
    const result = tmpl.render({ items: [1, 2, 3, 4, 5] }).trim();
    assert.ok(result.includes("1,"));
    assert.ok(result.includes("5,"));
    assert.ok(!result.includes("empty"));
  });

  it("multiple for loops with independent else", () => {
    const tmpl = Template.fromSource(`---
params:
  - a = list(str)
  - b = list(str)
---
> {% for x in a %}{{ x }}{% else %}noA{% /for %}|{% for y in b %}{{ y }}{% else %}noB{% /for %}`);
    assert.strictEqual(tmpl.render({ a: [], b: ["hi"] }).trim(), "noA|hi");
    assert.strictEqual(tmpl.render({ a: ["hi"], b: [] }).trim(), "hi|noB");
  });
});

// ---------------------------------------------------------------------------
// Constants comprehensive
// ---------------------------------------------------------------------------

describe("Constants comprehensive (additional)", () => {
  it("const str parsed in frontmatter", () => {
    const tmpl = Template.fromSource(`---
consts:
  - GREETING = str := "Hello"

params:
  - name = str
---
{{ name }}`);
    assert.strictEqual(tmpl.consts().GREETING, "Hello");
  });

  it("const int parsed in frontmatter", () => {
    const tmpl = Template.fromSource(`---
consts:
  - MAX = int := 100

params:
  - n = int
---
{{ n }}`);
    assert.strictEqual(tmpl.consts().MAX, 100);
  });

  it("const bool parsed in frontmatter", () => {
    const tmpl = Template.fromSource(`---
consts:
  - ENABLED = bool := true

params:
  - name = str
---
{{ name }}`);
    assert.strictEqual(tmpl.consts().ENABLED, true);
  });

  it("multiple consts parsed", () => {
    const tmpl = Template.fromSource(`---
consts:
  - A = str := "alpha"
  - B = int := 42

params:
  - name = str
---
{{ name }}`);
    assert.strictEqual(tmpl.consts().A, "alpha");
    assert.strictEqual(tmpl.consts().B, 42);
  });
});

// ---------------------------------------------------------------------------
// If/elif/else comprehensive
// ---------------------------------------------------------------------------

describe("If/elif/else comprehensive (additional)", () => {
  it("simple if-true", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = bool
---
> {% if x %}yes{% /if %}`);
    assert.strictEqual(tmpl.render({ x: true }).trim(), "yes");
  });

  it("simple if-false produces empty", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = bool
---
> {% if x %}yes{% /if %}`);
    assert.strictEqual(tmpl.render({ x: false }).trim(), "");
  });

  it("if-elif-else selects correct branch", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x == 1 %}one{% elif x == 2 %}two{% else %}other{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 1 }).trim(), "one");
    assert.strictEqual(tmpl.render({ x: 2 }).trim(), "two");
    assert.strictEqual(tmpl.render({ x: 3 }).trim(), "other");
  });

  it("multiple elif chains", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x == 1 %}one{% elif x == 2 %}two{% elif x == 3 %}three{% elif x == 4 %}four{% else %}other{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 1 }).trim(), "one");
    assert.strictEqual(tmpl.render({ x: 4 }).trim(), "four");
    assert.strictEqual(tmpl.render({ x: 5 }).trim(), "other");
  });

  it("nested if blocks", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = bool
  - y = bool
---
> {% if x %}{% if y %}both{% else %}x only{% /if %}{% else %}{% if y %}y only{% else %}neither{% /if %}{% /if %}`);
    assert.strictEqual(tmpl.render({ x: true, y: true }).trim(), "both");
    assert.strictEqual(tmpl.render({ x: true, y: false }).trim(), "x only");
    assert.strictEqual(tmpl.render({ x: false, y: true }).trim(), "y only");
    assert.strictEqual(tmpl.render({ x: false, y: false }).trim(), "neither");
  });

  it("not keyword is not supported — use else branch instead", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = bool
---
> {% if not x %}negated{% else %}normal{% /if %}`);
    // "not x" is treated as a variable name, which doesn't exist
    assert.throws(() => tmpl.render({ x: true }));
  });

  it("negation via else branch works correctly", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = bool
---
> {% if x %}normal{% else %}negated{% /if %}`);
    assert.strictEqual(tmpl.render({ x: true }).trim(), "normal");
    assert.strictEqual(tmpl.render({ x: false }).trim(), "negated");
  });
});

// ---------------------------------------------------------------------------
// Undefined variable errors
// ---------------------------------------------------------------------------

describe("Undefined variable errors (additional)", () => {
  it("throws on undeclared variable in expression at compile time", () => {
    // A fully-undeclared top-level variable is a compile-time error in both
    // backends (Rust's `check_undeclared` / TS's `checkUndeclaredVariables`),
    // so construction — not render — is where it is rejected.
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - x = str
---
{{ x }} {{ y }}`),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(
          err.message.includes("undeclared variable(s) referenced in body: y"),
        );
        return true;
      },
    );
  });

  it("throws on undefined nested field", () => {
    const tmpl = Template.fromSource(`---
params:
  - obj = struct(name = str)
---
{{ obj.nonexistent }}`);
    assert.throws(
      () => tmpl.render({ obj: { name: "hi" } }),
      UndefinedVariableError,
    );
  });
});

// ---------------------------------------------------------------------------
// Raw blocks comprehensive
// ---------------------------------------------------------------------------

describe("Raw block comprehensive (additional)", () => {
  it("raw block preserves template syntax literally", () => {
    const tmpl = Template.fromSource(`---
params: []
---
> {% raw %}{{ x }}{% /raw %}`);
    assert.strictEqual(tmpl.render({}).trim(), "{{ x }}");
  });

  it("raw block preserves statements literally", () => {
    const tmpl = Template.fromSource(`---
params: []
---
> {% raw %}{% if x %}yes{% /if %}{% /raw %}`);
    assert.strictEqual(tmpl.render({}).trim(), "{% if x %}yes{% /if %}");
  });
});

// ---------------------------------------------------------------------------
// Comments comprehensive
// ---------------------------------------------------------------------------

describe("Comment comprehensive (additional)", () => {
  it("comment block is omitted from output", () => {
    const tmpl = Template.fromSource(`---
params:
---
before{# this is a comment #}after`);
    const result = tmpl.render({});
    assert.ok(result.includes("before"));
    assert.ok(result.includes("after"));
    assert.ok(!result.includes("this is a comment"));
  });

  it("comment blockquote rules enforced", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: []
---
{# line start #}`),
      /blockquote prefix/,
    );
    assert.throws(
      () =>
        Template.fromSource(`---
params: []
---
> {#nospace#}`),
      /spaces around the content/,
    );
  });

  it("statement tag spacing enforced", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [x = bool]
---
> {%if x%}hello{%/if%}`),
      /Statement tags must have spaces around the content/,
    );
  });

  it("comment mixed with statement tag on same line", () => {
    const tmpl = Template.fromSource(`---
params: [flag = bool]
---
> {# comment explaining complex if #}{% if flag %}hello{% /if %}`);
    assert.strictEqual(tmpl.render({ flag: true }), "hello");
  });

  it("multiple comments on same line", () => {
    const tmpl = Template.fromSource(`---
params: [a = str, b = str]
---
> {# first comment: {{ a }} #}text{# second comment: {{ b }} #}`);
    assert.strictEqual(tmpl.render({ a: "1", b: "2" }), "text");
  });
});

// ---------------------------------------------------------------------------
// Value module comprehensive
// ---------------------------------------------------------------------------

describe("Value module comprehensive (additional)", () => {
  it("fromJs converts null to none", () => {
    const val = fromJs(null);
    assert.strictEqual(val.type, "none");
    assert.strictEqual(display(val), "");
  });

  it("fromJs converts undefined to none", () => {
    const val = fromJs(undefined);
    assert.strictEqual(val.type, "none");
    assert.strictEqual(display(val), "");
  });

  it("fromJs converts string", () => {
    const val = fromJs("hello");
    assert.strictEqual(val.type, "str");
    assert.strictEqual(display(val), "hello");
  });

  it("fromJs converts integer", () => {
    const val = fromJs(42);
    assert.strictEqual(val.type, "int");
    assert.strictEqual(display(val), "42");
  });

  it("fromJs converts float", () => {
    const val = fromJs(3.14);
    assert.strictEqual(val.type, "float");
    assert.strictEqual(display(val), "3.14");
  });

  it("fromJs converts boolean true", () => {
    const val = fromJs(true);
    assert.strictEqual(val.type, "bool");
    assert.strictEqual(isTruthy(val), true);
  });

  it("fromJs converts boolean false", () => {
    const val = fromJs(false);
    assert.strictEqual(val.type, "bool");
    assert.strictEqual(isTruthy(val), false);
  });

  it("fromJs converts array to list", () => {
    const val = fromJs([1, 2, 3]);
    assert.strictEqual(val.type, "list");
    assert.strictEqual(val.items.length, 3);
  });

  it("fromJs converts object to dict", () => {
    const val = fromJs({ a: 1, b: "two" });
    assert.strictEqual(val.type, "struct");
    assert.strictEqual(val.fields.size, 2);
  });

  it("display of list throws", () => {
    const val = fromJs([1, 2, 3]);
    assert.throws(() => display(val), /cannot display list/);
  });

  it("display of dict throws", () => {
    const val = fromJs({ a: 1, b: 2 });
    assert.throws(() => display(val), /cannot display struct/);
  });

  it("isTruthy: empty dict is falsy", () => {
    const val = fromJs({});
    assert.strictEqual(isTruthy(val), false);
  });

  it("isTruthy: non-empty dict is truthy", () => {
    const val = fromJs({ a: 1 });
    assert.strictEqual(isTruthy(val), true);
  });
});
