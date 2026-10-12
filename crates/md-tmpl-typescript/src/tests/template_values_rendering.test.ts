/**
 * Tests for the md-tmpl TypeScript bindings: frontmatter metadata, numbers, unicode, fromFile and rendering error paths.
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
  TypedTemplate,
  TemplateError,
  MissingParamsError,
  TypeMismatchError,
  ExtraParamsError,
} from "../index.js";
import { fromJs, TYPE_NONE } from "../value.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Frontmatter metadata fields
// ---------------------------------------------------------------------------

describe("Frontmatter metadata", () => {
  it("exposes name and description", () => {
    const tmpl = Template.fromSourceAllowingUnused(
      `---
name: greeting
description: A greeting template
params: [name = str]
---
{{ name }}`,
    );
    const fm = tmpl.frontmatter;
    assert.strictEqual(fm.name, "greeting");
    assert.strictEqual(fm.description, "A greeting template");
  });

  it("exposes allowUnused flag", () => {
    const src = `---
allow_unused: true
params: [x = str, y = int]
---
{{ x }}`;
    const tmpl = Template.fromSource(src);
    assert.strictEqual(tmpl.frontmatter.allowUnused, true);
  });

  it("name and description are undefined when not set", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    const fm = tmpl.frontmatter;
    assert.strictEqual(fm.name, undefined);
    assert.strictEqual(fm.description, undefined);
  });

  it("params list exposes correct types", () => {
    const src = [
      `---`,
      "params:",
      "  - name = str",
      "  - count = int",
      "  - score = float",
      "  - flag = bool",
      `---`,
      "{{ name }} {{ count }} {{ score }} {{ flag }}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    const fm = tmpl.frontmatter;
    assert.strictEqual(fm.params.length, 4);
    assert.strictEqual(defined(fm.params[0]).name, "name");
    assert.strictEqual(defined(fm.params[0]).varType.kind, "str");
    assert.strictEqual(defined(fm.params[1]).name, "count");
    assert.strictEqual(defined(fm.params[1]).varType.kind, "int");
    assert.strictEqual(defined(fm.params[2]).name, "score");
    assert.strictEqual(defined(fm.params[2]).varType.kind, "float");
    assert.strictEqual(defined(fm.params[3]).name, "flag");
    assert.strictEqual(defined(fm.params[3]).varType.kind, "bool");
  });

  it("consts are accessible via frontmatter", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - MAX = int := 100
  - TAG = str := "v1"

params: []
---
{{ MAX }} {{ TAG }}`,
    );
    const fm = tmpl.frontmatter;
    assert.strictEqual(fm.consts.length, 2);
    assert.strictEqual(defined(fm.consts[0]).name, "MAX");
    assert.strictEqual(defined(fm.consts[1]).name, "TAG");
  });

  it("imports are empty for simple template", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    assert.strictEqual(tmpl.frontmatter.imports.length, 0);
  });
});

// ---------------------------------------------------------------------------
// Float parameters
// ---------------------------------------------------------------------------

describe("Float parameters", () => {
  it("renders float value", () => {
    const tmpl = Template.fromSource(
      `---
params: [score = float]
---
{{ score }}`,
    );
    assert.strictEqual(tmpl.render({ score: 3.14 }), "3.14");
  });

  it("float accepts integer value", () => {
    const tmpl = Template.fromSource(
      `---
params: [score = float]
---
{{ score }}`,
    );
    // An int value should be accepted where float is expected
    assert.strictEqual(tmpl.render({ score: 42 }), "42");
  });

  it("float with fixed filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = float]
---
{{ val | fixed(2) }}`,
    );
    assert.strictEqual(tmpl.render({ val: 3.14159 }), "3.14");
  });

  it("negative float", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = float]
---
{{ val }}`,
    );
    assert.strictEqual(tmpl.render({ val: -2.5 }), "-2.5");
  });
});

// ---------------------------------------------------------------------------
// Unicode content
// ---------------------------------------------------------------------------

describe("Unicode content", () => {
  it("renders unicode in params", () => {
    const tmpl = Template.fromSource(
      `---
params: [msg = str]
---
{{ msg }}`,
    );
    const unicode = "Hello 🌍 こんにちは 世界 🦀";
    assert.strictEqual(tmpl.render({ msg: unicode }), unicode);
  });

  it("renders unicode in template body", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
🎉 Hello {{ name }}! 🎉`,
    );
    assert.strictEqual(tmpl.render({ name: "世界" }), "🎉 Hello 世界! 🎉");
  });
});

// ---------------------------------------------------------------------------
// Boundary numbers
// ---------------------------------------------------------------------------

describe("Boundary numbers", () => {
  it("zero int", () => {
    const tmpl = Template.fromSource(`---
params: [n = int]
---
{{ n }}`);
    assert.strictEqual(tmpl.render({ n: 0 }), "0");
  });

  it("negative int", () => {
    const tmpl = Template.fromSource(`---
params: [n = int]
---
{{ n }}`);
    assert.strictEqual(tmpl.render({ n: -42 }), "-42");
  });

  it("large positive int", () => {
    const tmpl = Template.fromSource(`---
params: [n = int]
---
{{ n }}`);
    assert.strictEqual(
      tmpl.render({ n: 9007199254740991 }),
      "9007199254740991",
    );
  });

  it("empty string param", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
[{{ x }}]`);
    assert.strictEqual(tmpl.render({ x: "" }), "[]");
  });

  it("false bool in if condition", () => {
    const tmpl = Template.fromSource(
      `---
params: [flag = bool]
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}`,
    );
    assert.strictEqual(tmpl.render({ flag: false }).trim(), "no");
  });
});

// ---------------------------------------------------------------------------
// fromFile tests
// ---------------------------------------------------------------------------

describe("Template.fromFile", () => {
  it("loads and renders from file", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-file-"));
    try {
      const fp = path.join(dir, "greeting.tmpl.md");
      fs.writeFileSync(
        fp,
        `---
params: [name = str]
---
Hello {{ name }}!`,
      );
      const tmpl = Template.fromFile(fp);
      assert.strictEqual(tmpl.render({ name: "file" }), "Hello file!");
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("throws for missing file", () => {
    assert.throws(
      () => Template.fromFile("/nonexistent/path.tmpl.md"),
      TemplateError,
    );
  });

  it("fromFile resolves includes relative to file directory", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-file-inc-"));
    try {
      // Create included template
      fs.writeFileSync(
        path.join(dir, "header.tmpl.md"),
        `---
params: [title = str]
---
# {{ title }}`,
      );
      // Create main template that includes it
      fs.writeFileSync(
        path.join(dir, "main.tmpl.md"),
        `---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body: {{ title }}`,
      );
      const tmpl = Template.fromFile(path.join(dir, "main.tmpl.md"));
      assert.ok(tmpl.basePath !== undefined);
      const result = tmpl.render({ title: "Test" });
      assert.ok(
        result.includes("# Test"),
        `expected included header in: ${result}`,
      );
      assert.ok(result.includes("Body: Test"), `expected body in: ${result}`);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

// ---------------------------------------------------------------------------
// TypedTemplate.fromFile
// ---------------------------------------------------------------------------

describe("TypedTemplate.fromFile", () => {
  it("loads a typed template from file", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-typed-"));
    try {
      const fp = path.join(dir, "typed.tmpl.md");
      fs.writeFileSync(
        fp,
        `---
params: [x = str]
---
{{ x }}`,
      );
      const tmpl = TypedTemplate.fromFile<{ x: string }>(fp);
      assert.strictEqual(tmpl.render({ x: "typed" }), "typed");
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Error paths from rendering
// ---------------------------------------------------------------------------

describe("Error paths in rendering", () => {
  it("MissingParamsError thrown by render", () => {
    const tmpl = Template.fromSource(
      `---
params: [a = str, b = int]
---
{{ a }} {{ b }}`,
    );
    try {
      tmpl.render({});
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(err instanceof MissingParamsError);
      assert.ok(err.missing.includes("a"));
      assert.ok(err.missing.includes("b"));
    }
  });

  it("TypeMismatchError thrown by render", () => {
    const tmpl = Template.fromSource(
      `---
params: [flag = bool]
---
{{ flag }}`,
    );
    try {
      tmpl.render({ flag: "not a bool" });
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(err instanceof TypeMismatchError);
      assert.strictEqual(err.path, "flag");
      assert.strictEqual(err.expected, "bool");
    }
  });

  it("ExtraParamsError thrown by render", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    try {
      tmpl.render({ x: "hi", bogus: "extra" });
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(err instanceof ExtraParamsError);
      assert.ok(err.extra.includes("bogus"));
    }
  });

  it("list item missing field produces MissingParamsError", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str, score = int)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
    );
    assert.throws(
      () => tmpl.render({ items: [{ name: "ok" }] }),
      MissingParamsError,
    );
  });

  it("struct missing field produces MissingParamsError", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}`,
    );
    assert.throws(
      () => tmpl.render({ config: { host: "localhost" } }),
      MissingParamsError,
    );
  });

  it("invalid enum variant produces TypeMismatchError", () => {
    const src = [
      `---`,
      "params:",
      "  - outcome = enum(Confirmed(evidence = str), Rejected)",
      `---`,
      "> {% match outcome %}",
      "> {% case Confirmed %}",
      "",
      "YES",

      "",
      "> {% case Rejected %}",
      "",
      "NO",

      "",
      "> {% /match %}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    assert.throws(() => tmpl.render({ outcome: "Unknown" }), TypeMismatchError);
  });
});

// ---------------------------------------------------------------------------
// fromJs edge cases
// ---------------------------------------------------------------------------

describe("fromJs edge cases", () => {
  it("converts nested objects correctly", () => {
    const val = fromJs({ host: "localhost", port: 8080 });
    assert.strictEqual(val.type, "struct");
    assert.ok(val.fields.has("host"));
    assert.ok(val.fields.has("port"));
  });

  it("converts arrays of objects", () => {
    const val = fromJs([{ name: "a" }, { name: "b" }]);
    assert.strictEqual(val.type, "list");
    assert.strictEqual(val.items.length, 2);
  });

  it("converts null to str empty", () => {
    const val = fromJs(null);
    // null and undefined convert to the NONE sentinel.
    assert.strictEqual(val.type, TYPE_NONE);
  });

  it("converts undefined to str empty", () => {
    const val = fromJs(undefined);
    assert.strictEqual(val.type, TYPE_NONE);
  });
});

// ---------------------------------------------------------------------------
// Raw blocks
// ---------------------------------------------------------------------------

describe("Raw blocks", () => {
  it("preserves template syntax inside raw", () => {
    const tmpl = Template.fromSource(
      `---
params: []
---
> {% raw %}
{{ not_a_variable }}
> {% /raw %}`,
    );
    const result = tmpl.render({});
    assert.ok(
      result.includes("{{ not_a_variable }}"),
      `expected raw content preserved, got: ${result}`,
    );
  });
});

// ---------------------------------------------------------------------------
// Fast-validated direct render and fallback paths
// ---------------------------------------------------------------------------

describe("Fast-validated direct render and fallback paths", () => {
  it("renders compound types, aliases, enums, and options via fast validation", () => {
    const tmpl = Template.fromSource(
      `---
types:
  - Priority = enum(Critical, High, Low)
  - Outcome = enum(Confirmed(evidence = str), Rejected)
  - Meta = struct(type = str, retries = int)

params:
  - title = str
  - score = float
  - active = bool
  - priority = Priority
  - outcome = Outcome
  - meta = Meta
  - tags = list(str)
  - items = list(name = str, count = int)
  - note = option(str) := None
---
{{ title }}|{{ score }}|{{ active }}|{{ kind(priority) }}|{{ meta.type }}:{{ meta.retries }}|{{ tags | join(",") }}

> {% for item in items %}

{{ item.name }}={{ item.count }}

> {% /for %}
> {% if has(note) %}

Note:{{ note }}

> {% /if %}
> {% match outcome %}
> {% case Confirmed %}

Confirmed:{{ outcome.evidence }}

> {% case Rejected %}

Rejected

> {% /match %}`,
    );

    const out1 = tmpl.render({
      title: "audit",
      score: 9.5,
      active: true,
      priority: "Critical",
      outcome: { __kind__: "Confirmed", evidence: "poc.rs" },
      meta: { type: "custom-kind", retries: 2 },
      tags: ["a", "b"],
      items: [{ name: "check", count: 3 }],
      note: "urgent",
    });
    assert.ok(out1.includes("audit|9.5|true|Critical|custom-kind:2|a,b"));
    assert.ok(out1.includes("check=3"));
    assert.ok(out1.includes("Note:urgent"));
    assert.ok(out1.includes("Confirmed:poc.rs"));

    const out2 = tmpl.render(
      {
        title: "second",
        score: 1,
        active: false,
        priority: { __kind__: "Low" },
        outcome: "Rejected",
        meta: { type: "fallback", retries: 0 },
        tags: [],
        items: [],
      },
      { allowExtra: true },
    );
    assert.ok(out2.includes("second|1|false|Low|fallback:0|"));
    assert.ok(out2.includes("Rejected"));
  });

  it("falls back to buildContext for boxed Values, Map renderDict, and option kind()", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - cfg = struct(host = str, port = int)
  - opt = option(str) := None
---
{{ cfg.host }}:{{ cfg.port }} ({{ kind(opt) }})`,
    );

    const mapParams = new Map<string, unknown>([
      ["cfg", fromJs({ host: "db.internal", port: 5432 })],
      ["opt", "ready"],
    ]);
    assert.strictEqual(
      tmpl.renderDict(mapParams).trim(),
      "db.internal:5432 (Some)",
    );
  });
});
