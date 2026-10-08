/**
 * Tests for the md-tmpl TypeScript bindings: filter edge cases, Context API, value module, frontmatter edge cases and errors.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  Template,
  Context,
  TemplateError,
  TemplateSyntaxError,
  MissingParamsError,
  TypeMismatchError,
  ExtraParamsError,
  UndefinedVariableError,
  UnknownFilterError,
} from "../index.js";
import { fromJs, display, isTruthy } from "../value.js";
import {
  parseFrontmatter,
  parseVarType,
  varTypeToString,
  stripFrontmatter,
} from "../frontmatter.js";
import { generateTypes, inferTypes } from "../codegen.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Filter edge cases
// ---------------------------------------------------------------------------

describe("Filter edge cases", () => {
  it("upper on empty string", () => {
    const tmpl = Template.fromSource(
      `---
params: [s = str]
---
{{ s | upper }}`,
    );
    assert.strictEqual(tmpl.render({ s: "" }), "");
  });

  it("lower on mixed case", () => {
    const tmpl = Template.fromSource(
      `---
params: [s = str]
---
{{ s | lower }}`,
    );
    assert.strictEqual(tmpl.render({ s: "HeLLo WoRLD" }), "hello world");
  });

  it("trim on string with only whitespace", () => {
    const tmpl = Template.fromSource(
      `---
params: [s = str]
---
[{{ s | trim }}]`,
    );
    assert.strictEqual(tmpl.render({ s: "   " }), "[]");
  });

  it("fixed(0) on float", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = float]
---
{{ x | fixed(0) }}`,
    );
    assert.strictEqual(tmpl.render({ x: 3.7 }), "4");
  });

  it("fixed(4) on integer", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = int]
---
{{ x | fixed(4) }}`,
    );
    assert.strictEqual(tmpl.render({ x: 42 }), "42.0000");
  });

  it("add(0) is identity", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = int]
---
{{ x | add(0) }}`,
    );
    assert.strictEqual(tmpl.render({ x: 42 }), "42");
  });

  it("sub with negative result", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = int]
---
{{ x | sub(100) }}`,
    );
    assert.strictEqual(tmpl.render({ x: 42 }), "-58");
  });

  it("add on float", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = float]
---
{{ x | add(0.5) | fixed(1) }}`,
    );
    assert.strictEqual(tmpl.render({ x: 1.5 }), "2.0");
  });

  it("limit on list shorter than N", () => {
    const tmpl = Template.fromSource(
      `---
params: [items = list(str)]
---
{{ items | limit(10) | join(", ") }}`,
    );
    assert.strictEqual(tmpl.render({ items: ["a", "b"] }), "a, b");
  });

  it("join with empty separator", () => {
    const tmpl = Template.fromSource(
      `---
params: [items = list(str)]
---
{{ items | join("") }}`,
    );
    assert.strictEqual(tmpl.render({ items: ["a", "b", "c"] }), "abc");
  });

  it("chained filters: limit then join", () => {
    const tmpl = Template.fromSource(
      `---
params: [items = list(str)]
---
{{ items | limit(2) | join(", ") }}`,
    );
    assert.strictEqual(tmpl.render({ items: ["a", "b", "c", "d"] }), "a, b");
  });

  it("unknown filter throws UnknownFilterError", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params: [x = str]
---
{{ x | nonexistent }}`,
        ).render({ x: "test" }),
      (err: unknown) => err instanceof UnknownFilterError,
    );
  });
});

// ---------------------------------------------------------------------------
// Context API tests
// ---------------------------------------------------------------------------

describe("Context API", () => {
  it("set and get string", () => {
    const ctx = new Context();
    ctx.set("name", "world");
    const val = ctx.get("name");
    assert.ok(val);
    assert.strictEqual(val.type, "str");
  });

  it("set and get int", () => {
    const ctx = new Context();
    ctx.set("count", 42);
    const val = ctx.get("count");
    assert.ok(val);
    assert.strictEqual(val.type, "int");
  });

  it("has returns true for existing key", () => {
    const ctx = new Context();
    ctx.set("key", "value");
    assert.ok(ctx.has("key"));
  });

  it("has returns false for missing key", () => {
    const ctx = new Context();
    assert.ok(!ctx.has("missing"));
  });

  it("size reflects number of entries", () => {
    const ctx = new Context();
    assert.strictEqual(ctx.size, 0);
    ctx.set("a", 1);
    ctx.set("b", 2);
    assert.strictEqual(ctx.size, 2);
  });

  it("keys returns all key names", () => {
    const ctx = new Context();
    ctx.set("alpha", 1);
    ctx.set("beta", 2);
    const keys = [...ctx.keys()];
    assert.deepStrictEqual(keys.sort(), ["alpha", "beta"]);
  });

  it("entries returns all entries", () => {
    const ctx = new Context();
    ctx.set("x", "hello");
    const entries = [...ctx.entries()];
    assert.strictEqual(entries.length, 1);
    assert.strictEqual(defined(entries[0])[0], "x");
  });

  it("Context.from creates from plain object", () => {
    const ctx = Context.from({ name: "world", count: 42 });
    assert.strictEqual(ctx.size, 2);
    assert.ok(ctx.has("name"));
    assert.ok(ctx.has("count"));
  });

  it("overwriting a key updates the value", () => {
    const ctx = new Context();
    ctx.set("x", "old");
    ctx.set("x", "new");
    assert.strictEqual(ctx.size, 1);
  });
});

// ---------------------------------------------------------------------------
// Value module tests
// ---------------------------------------------------------------------------

describe("Value module", () => {
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

  it("fromJs converts nested objects", () => {
    const val = fromJs({ a: { b: 42 } });
    assert.strictEqual(val.type, "struct");
  });

  it("display of int", () => {
    const val = fromJs(42);
    assert.strictEqual(display(val), "42");
  });

  it("display of float", () => {
    const val = fromJs(3.14);
    assert.strictEqual(display(val), "3.14");
  });

  it("display of bool true", () => {
    const val = fromJs(true);
    assert.strictEqual(display(val), "true");
  });

  it("display of bool false", () => {
    const val = fromJs(false);
    assert.strictEqual(display(val), "false");
  });

  it("display of list", () => {
    const val = fromJs(["a", "b"]);
    assert.strictEqual(val.type, "list");
    assert.ok(val.items.length === 2);
  });

  it("isTruthy for non-empty string", () => {
    assert.ok(isTruthy(fromJs("hello")));
  });

  it("isTruthy for empty string is false", () => {
    assert.ok(!isTruthy(fromJs("")));
  });

  it("isTruthy for non-zero int", () => {
    assert.ok(isTruthy(fromJs(42)));
  });

  it("isTruthy for zero is false", () => {
    assert.ok(!isTruthy(fromJs(0)));
  });

  it("isTruthy for true bool", () => {
    assert.ok(isTruthy(fromJs(true)));
  });

  it("isTruthy for false bool is false", () => {
    assert.ok(!isTruthy(fromJs(false)));
  });

  it("isTruthy for non-empty list", () => {
    assert.ok(isTruthy(fromJs([1])));
  });

  it("isTruthy for empty list is false", () => {
    assert.ok(!isTruthy(fromJs([])));
  });
});

// ---------------------------------------------------------------------------
// Frontmatter edge cases
// ---------------------------------------------------------------------------

describe("Frontmatter edge cases", () => {
  it("parses inline param list syntax", () => {
    const [fm] = parseFrontmatter(
      `---
params: [name = str, count = int]
---
`,
    );
    assert.strictEqual(fm.params.length, 2);
    assert.strictEqual(defined(fm.params[0]).name, "name");
    assert.strictEqual(defined(fm.params[1]).name, "count");
  });

  it("handles empty params list", () => {
    const [fm] = parseFrontmatter(`---
params: []
---
`);
    assert.strictEqual(fm.params.length, 0);
  });

  it("parses consts with default values", () => {
    const [fm] = parseFrontmatter(
      `---
consts:
  - GREETING = str := "Hello"
---
`,
    );
    assert.strictEqual(fm.consts.length, 1);
    assert.strictEqual(defined(fm.consts[0]).name, "GREETING");
    assert.ok(defined(fm.consts[0]).defaultValue !== undefined);
  });

  it("parses type aliases", () => {
    const [fm] = parseFrontmatter(
      `---
types:
  - Item = struct(name = str, score = int)

params:
  - items = list(Item)
---
`,
    );
    assert.ok(fm.typeAliases);
    assert.ok(fm.typeAliases.has("Item"));
  });

  it("stripFrontmatter returns body only", () => {
    const body = stripFrontmatter(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.strictEqual(body, "Hello {{ name }}!");
  });

  it("parseVarType and varTypeToString round-trip", () => {
    const types = ["str", "int", "float", "bool"];
    for (const t of types) {
      assert.strictEqual(varTypeToString(parseVarType(t)), t);
    }
  });

  it("parseVarType handles typed list", () => {
    const vt = parseVarType("list(name = str)");
    assert.strictEqual(vt.kind, "list");
  });

  it("parseVarType handles struct", () => {
    const vt = parseVarType("struct(name = str, score = int)");
    assert.strictEqual(vt.kind, "struct");
  });

  it("parseVarType handles enum", () => {
    const vt = parseVarType("enum(A, B(x = str))");
    assert.strictEqual(vt.kind, "enum");
  });

  it("frontmatter name and description", () => {
    const [fm] = parseFrontmatter(
      `---
name: "My Template"
description: "A greeting"
params: [name = str]
---
`,
    );
    assert.strictEqual(fm.name, "My Template");
    assert.strictEqual(fm.description, "A greeting");
  });

  it("allow_unused flag", () => {
    const [fm] = parseFrontmatter(
      `---
allow_unused: true
params: [name = str]
---
`,
    );
    assert.strictEqual(fm.allowUnused, true);
  });

  it("rejects missing blank line after block list in frontmatter", () => {
    assert.throws(
      () =>
        parseFrontmatter(`---
consts:
  - FOO = str := 'bar'
params:
  - x = int
---
body`),
      (err: Error) =>
        err.message.includes("A blank line is required after a block list"),
    );

    assert.throws(
      () =>
        parseFrontmatter(`---
types:
  - Priority = enum(High, Low)
consts:
  - FOO = str := 'bar'
---
body`),
      (err: Error) =>
        err.message.includes("A blank line is required after a block list"),
    );

    assert.throws(
      () =>
        parseFrontmatter(`---
consts:
  - FOO = str := 'bar'
allow_unused: true
---
body`),
      (err: Error) =>
        err.message.includes("A blank line is required after a block list"),
    );
  });
});

// ---------------------------------------------------------------------------
// Error hierarchy tests
// ---------------------------------------------------------------------------

describe("Error hierarchy", () => {
  it("all error types extend TemplateError", () => {
    const errors = [
      new TemplateSyntaxError("test"),
      new MissingParamsError(["x"]),
      new TypeMismatchError("x", "str", "int"),
      new ExtraParamsError(["y"]),
      new UndefinedVariableError("z"),
      new UnknownFilterError("bad"),
    ];
    for (const err of errors) {
      assert.ok(err instanceof TemplateError);
      assert.ok(err instanceof Error);
    }
  });

  it("MissingParamsError lists missing names", () => {
    const err = new MissingParamsError(["a", "b"]);
    assert.deepStrictEqual(err.missing, ["a", "b"]);
    assert.ok(err.message.includes("a"));
    assert.ok(err.message.includes("b"));
  });

  it("TypeMismatchError exposes path, expected, actual", () => {
    const err = new TypeMismatchError("items[0].score", "int", "str");
    assert.strictEqual(err.path, "items[0].score");
    assert.strictEqual(err.expected, "int");
    assert.strictEqual(err.actual, "str");
  });

  it("ExtraParamsError lists extra names", () => {
    const err = new ExtraParamsError(["extra1", "extra2"]);
    assert.deepStrictEqual(err.extra, ["extra1", "extra2"]);
  });

  it("UndefinedVariableError exposes variable name", () => {
    const err = new UndefinedVariableError("unknown_var");
    assert.strictEqual(err.variable, "unknown_var");
  });

  it("UnknownFilterError exposes filter name", () => {
    const err = new UnknownFilterError("badfilter");
    assert.strictEqual(err.filter, "badfilter");
  });
});

// ---------------------------------------------------------------------------
// Codegen tests
// ---------------------------------------------------------------------------

describe("Codegen", () => {
  it("generateTypes produces interface with correct fields", () => {
    const code = generateTypes(
      `---
params:
  - name = str
  - count = int
---
Hello {{ name }}!`,
    );
    assert.ok(code.includes("name: string"));
    assert.ok(code.includes("count: number"));
  });

  it("generateTypes handles list type", () => {
    const code = generateTypes(
      `---
params:
  - items = list(label = str)
---
{{ items }}`,
    );
    assert.ok(code.includes("items:"));
    assert.ok(code.includes("label: string"));
  });

  it("generateTypes handles enum type", () => {
    const code = generateTypes(
      `---
params:
  - status = enum(Ok, Error(msg = str))
---
{{ status }}`,
    );
    assert.ok(code.includes("Ok") || code.includes("ok"));
    assert.ok(code.includes("Error") || code.includes("error"));
  });

  it("inferTypes produces structured result", () => {
    const result = inferTypes(
      `---
params:
  - name = str
  - count = int
---
Hello {{ name }}!`,
    );
    assert.ok(result.fields.length === 2);
    assert.strictEqual(defined(result.fields[0]).name, "name");
    assert.ok(defined(result.fields[0]).tsType.includes("string"));
    assert.strictEqual(defined(result.fields[1]).name, "count");
    assert.ok(defined(result.fields[1]).tsType.includes("number"));
  });
});
