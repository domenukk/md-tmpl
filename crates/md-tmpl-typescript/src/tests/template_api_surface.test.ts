/**
 * Tests for the md-tmpl TypeScript bindings: declaration validation, Context, error types and render options.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

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
import { stripFrontmatter } from "../frontmatter.js";

// ---------------------------------------------------------------------------
// validateDeclarationsAgainst — thorough coverage
// ---------------------------------------------------------------------------

describe("validateDeclarationsAgainst (thorough)", () => {
  it("accepts matching declarations", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }} {{ count }}`,
    );
    tmpl.validateDeclarationsAgainst([
      ["name", "str"],
      ["count", "int"],
    ]);
  });

  it("rejects when count differs (added)", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }} {{ count }}`,
    );
    assert.throws(() => {
      tmpl.validateDeclarationsAgainst([["name", "str"]]);
    }, TemplateError);
  });

  it("rejects when count differs (removed)", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
{{ name }}`,
    );
    assert.throws(() => {
      tmpl.validateDeclarationsAgainst([
        ["name", "str"],
        ["count", "int"],
      ]);
    }, TemplateError);
  });

  it("rejects retyped parameter", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }} {{ count }}`,
    );
    assert.throws(() => {
      tmpl.validateDeclarationsAgainst([
        ["name", "str"],
        ["count", "float"],
      ]);
    }, TemplateError);
  });

  it("rejects renamed parameter", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
{{ name }}`,
    );
    assert.throws(() => {
      tmpl.validateDeclarationsAgainst([["different", "str"]]);
    }, TemplateError);
  });
});

// ---------------------------------------------------------------------------
// stripFrontmatter
// ---------------------------------------------------------------------------

describe("stripFrontmatter", () => {
  it("returns body without frontmatter", () => {
    const body = stripFrontmatter(
      `---
params: [x = str]
---
Hello {{ x }}!`,
    );
    assert.ok(body.includes("Hello {{ x }}!"));
    assert.ok(!body.includes("params"));
  });

  it("handles template with no body", () => {
    const body = stripFrontmatter(`---
params: []
---
`);
    assert.strictEqual(body.trim(), "");
  });

  it("preserves multi-line body", () => {
    const body = stripFrontmatter(
      `---
params: []
---
Line 1
Line 2
Line 3`,
    );
    assert.ok(body.includes("Line 1"));
    assert.ok(body.includes("Line 2"));
    assert.ok(body.includes("Line 3"));
  });
});

// ---------------------------------------------------------------------------
// Context class
// ---------------------------------------------------------------------------

describe("Context", () => {
  it("set and get string value", () => {
    const ctx = new Context();
    ctx.set("name", "Alice");
    const val = ctx.get("name");
    assert.ok(val !== undefined);
    assert.strictEqual(val.type, "str");
    assert.strictEqual(val.value, "Alice");
  });

  it("set and get int value", () => {
    const ctx = new Context();
    ctx.set("count", 42);
    const val = ctx.get("count");
    assert.ok(val !== undefined);
    assert.strictEqual(val.type, "int");
  });

  it("set and get bool value", () => {
    const ctx = new Context();
    ctx.set("flag", true);
    const val = ctx.get("flag");
    assert.ok(val !== undefined);
    assert.strictEqual(val.type, "bool");
  });

  it("set and get float value", () => {
    const ctx = new Context();
    ctx.set("score", 3.14);
    const val = ctx.get("score");
    assert.ok(val !== undefined);
    assert.strictEqual(val.type, "float");
  });

  it("returns undefined for missing key", () => {
    const ctx = new Context();
    assert.strictEqual(ctx.get("nonexistent"), undefined);
  });

  it("overwrites existing value", () => {
    const ctx = new Context();
    ctx.set("x", "first");
    ctx.set("x", "second");
    const val = ctx.get("x");
    assert.ok(val !== undefined);
    if (val.type === "str") assert.strictEqual(val.value, "second");
  });

  it("set and get list value", () => {
    const ctx = new Context();
    ctx.set("items", [{ label: "a" }, { label: "b" }]);
    const val = ctx.get("items");
    assert.ok(val !== undefined);
    assert.strictEqual(val.type, "list");
  });

  it("set and get dict value", () => {
    const ctx = new Context();
    ctx.set("config", { host: "localhost", port: 8080 });
    const val = ctx.get("config");
    assert.ok(val !== undefined);
    assert.strictEqual(val.type, "struct");
  });

  it("values property returns the internal map", () => {
    const ctx = new Context();
    ctx.set("a", "hello");
    ctx.set("b", 42);
    assert.ok(ctx.values instanceof Map);
    assert.ok(ctx.values.has("a"));
    assert.ok(ctx.values.has("b"));
  });
});

// ---------------------------------------------------------------------------
// Error type properties
// ---------------------------------------------------------------------------

describe("Error type properties", () => {
  it("MissingParamsError exposes missing names", () => {
    const err = new MissingParamsError(["a", "b"]);
    assert.deepStrictEqual(err.missing, ["a", "b"]);
    assert.ok(err instanceof TemplateError);
    assert.ok(err.message.includes("a"));
    assert.ok(err.message.includes("b"));
    assert.strictEqual(err.name, "MissingParamsError");
  });

  it("TypeMismatchError exposes path, expected, actual", () => {
    const err = new TypeMismatchError("flag", "bool", "str");
    assert.strictEqual(err.path, "flag");
    assert.strictEqual(err.expected, "bool");
    assert.strictEqual(err.actual, "str");
    assert.ok(err instanceof TemplateError);
    assert.ok(err.message.includes("flag"));
    assert.ok(err.message.includes("bool"));
    assert.strictEqual(err.name, "TypeMismatchError");
  });

  it("ExtraParamsError exposes extra names", () => {
    const err = new ExtraParamsError(["bogus", "unknown"]);
    assert.deepStrictEqual(err.extra, ["bogus", "unknown"]);
    assert.ok(err instanceof TemplateError);
    assert.ok(err.message.includes("bogus"));
    assert.strictEqual(err.name, "ExtraParamsError");
  });

  it("UndefinedVariableError exposes variable name", () => {
    const err = new UndefinedVariableError("myVar");
    assert.strictEqual(err.variable, "myVar");
    assert.ok(err instanceof TemplateError);
    assert.ok(err.message.includes("myVar"));
    assert.strictEqual(err.name, "UndefinedVariableError");
  });

  it("UnknownFilterError exposes filter name", () => {
    const err = new UnknownFilterError("bogusFilter");
    assert.strictEqual(err.filter, "bogusFilter");
    assert.ok(err instanceof TemplateError);
    assert.ok(err.message.includes("bogusFilter"));
    assert.strictEqual(err.name, "UnknownFilterError");
  });

  it("TemplateSyntaxError has line, column and snippet", () => {
    const err = new TemplateSyntaxError("bad syntax", 5, 3, "{{ unclosed");
    assert.strictEqual(err.line, 5);
    assert.strictEqual(err.column, 3);
    assert.strictEqual(err.snippet, "{{ unclosed");
    assert.ok(err instanceof TemplateError);
    assert.strictEqual(err.name, "TemplateSyntaxError");
  });

  it("TemplateError is the base class", () => {
    const err = new TemplateError("generic error");
    assert.strictEqual(err.name, "TemplateError");
    assert.ok(err instanceof Error);
    assert.strictEqual(err.message, "generic error");
  });
});

// ---------------------------------------------------------------------------
// TemplateSyntaxError cases
// ---------------------------------------------------------------------------

describe("TemplateSyntaxError on invalid source", () => {
  it("rejects source without frontmatter", () => {
    assert.throws(
      () => Template.fromSource("no frontmatter at all"),
      (err: unknown) => err instanceof TemplateSyntaxError,
    );
  });

  it("rejects unused params in strict mode", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params: [name = str, unused = int]
---
Hello {{ name }}!`,
        ),
      (err: unknown) => err instanceof TemplateSyntaxError,
    );
  });

  it("fromSourceAllowingUnused permits unused params", () => {
    const tmpl = Template.fromSourceAllowingUnused(
      `---
params: [name = str, unused = int]
---
Hello {{ name }}!`,
    );
    assert.strictEqual(
      tmpl.render({ name: "world", unused: 42 }),
      "Hello world!",
    );
  });
});

// ---------------------------------------------------------------------------
// Template.render with allowExtra option
// ---------------------------------------------------------------------------

describe("Template.render allowExtra option", () => {
  it("render rejects extra params by default", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    assert.throws(() => tmpl.render({ x: "ok", extra: "bad" }));
  });

  it("render accepts extra params with allowExtra", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    const result = tmpl.render(
      { x: "ok", extra: "ignored" },
      { allowExtra: true },
    );
    assert.strictEqual(result, "ok");
  });
});

// ---------------------------------------------------------------------------
// renderDict with Map
// ---------------------------------------------------------------------------

describe("renderDict with Map", () => {
  it("accepts Map<string, unknown>", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }}: {{ count }}`,
    );
    const params = new Map<string, unknown>([
      ["name", "Alice"],
      ["count", 42],
    ]);
    assert.strictEqual(tmpl.renderDict(params), "Alice: 42");
  });

  it("accepts Map with allowExtra", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    const params = new Map<string, unknown>([
      ["name", "World"],
      ["extra", "ignored"],
    ]);
    assert.strictEqual(
      tmpl.renderDict(params, { allowExtra: true }),
      "Hello World!",
    );
  });
});

// ---------------------------------------------------------------------------
// setMaxIncludeDepth
// ---------------------------------------------------------------------------

describe("setMaxIncludeDepth", () => {
  it("does not break rendering", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    tmpl.setMaxIncludeDepth(5);
    assert.strictEqual(tmpl.render({ x: "works" }), "works");
  });

  it("maxIncludeDepth accessor returns value", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    assert.strictEqual(tmpl.maxIncludeDepth, 16);
    tmpl.setMaxIncludeDepth(3);
    assert.strictEqual(tmpl.maxIncludeDepth, 3);
  });
});

// ---------------------------------------------------------------------------
// basePath accessor
// ---------------------------------------------------------------------------

describe("basePath accessor", () => {
  it("returns undefined for fromSource", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    assert.strictEqual(tmpl.basePath, undefined);
  });

  it("returns the dir for fromFile", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-bp-"));
    try {
      const fp = path.join(dir, "test.tmpl.md");
      fs.writeFileSync(
        fp,
        `---
params: [x = str]
---
{{ x }}`,
      );
      const tmpl = Template.fromFile(fp);
      assert.ok(tmpl.basePath !== undefined);
      assert.ok(tmpl.basePath.includes(dir));
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});
