/**
 * Tests for the md-tmpl TypeScript bindings: basic loading, rendering, validation, defaults, cache and metadata.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template, TemplateCache } from "../index.js";
import { withTempFile } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Template.fromSource — basic rendering
// ---------------------------------------------------------------------------

describe("Template.fromSource", () => {
  it("renders a basic string param", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.render({ name: "world" }), "Hello world!");
  });

  it("renders an int param", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - count = int
---
Count: {{ count }}`,
    );
    assert.strictEqual(tmpl.render({ count: 42 }), "Count: 42");
  });

  it("renders a bool param in an if block", () => {
    const tmpl = Template.fromSource(
      `---
params: [flag = bool]
---
> {% if flag %}

yes

> {% /if %}`,
    );
    const output = tmpl.render({ flag: true });
    assert.ok(output.includes("yes"));
  });

  it("renders a float param", () => {
    const tmpl = Template.fromSource(
      `---
params: [score = float]
---
{{ score }}`,
    );
    assert.strictEqual(tmpl.render({ score: 3.14 }), "3.14");
  });

  it("throws on missing frontmatter", () => {
    assert.throws(
      () => Template.fromSource("no frontmatter at all"),
      (err: Error) => err.message.includes("frontmatter"),
    );
  });

  it("throws on missing param", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, age = int]
---
{{ name }} {{ age }}`,
    );
    assert.throws(
      () => tmpl.render({ name: "Alice" }),
      (err: Error) => err.message.includes("missing"),
    );
  });

  it("throws on type mismatch", () => {
    const tmpl = Template.fromSource(
      `---
params: [flag = bool]
---
{{ flag }}`,
    );
    assert.throws(
      () => tmpl.render({ flag: "not a bool" }),
      (err: Error) => err.message.includes("type mismatch"),
    );
  });
});

// ---------------------------------------------------------------------------
// Template.fromFile
// ---------------------------------------------------------------------------

describe("Template.fromFile", () => {
  it("loads and renders a file", () => {
    withTempFile(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
      (filepath) => {
        const tmpl = Template.fromFile(filepath);
        assert.strictEqual(tmpl.render({ name: "world" }), "Hello world!");
      },
    );
  });

  it("throws on missing file", () => {
    assert.throws(
      () => Template.fromFile("/nonexistent/path.tmpl.md"),
      (err: Error) => err.message.includes("failed to load"),
    );
  });
});

// ---------------------------------------------------------------------------
// Template.fromSourceAllowingUnused
// ---------------------------------------------------------------------------

describe("Template.fromSourceAllowingUnused", () => {
  it("accepts unused params", () => {
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

  it("strict mode rejects unused params", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params: [name = str, unused = int]
---
Hello {{ name }}!`,
        ),
      (err: Error) => err.message.includes("unused"),
    );
  });
});

// ---------------------------------------------------------------------------
// Strict validation — extra params
// ---------------------------------------------------------------------------

describe("Strict validation", () => {
  it("rejects extra params by default", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.throws(
      () => tmpl.render({ name: "world", bogus: "unexpected" }),
      (err: Error) =>
        err.message.includes("extra") || err.message.includes("undeclared"),
    );
  });

  it("allows extra with allowExtra", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    const result = tmpl.render(
      { name: "world", bogus: "ignored" },
      { allowExtra: true },
    );
    assert.strictEqual(result, "Hello world!");
  });

  it("renderDict rejects extra params", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.throws(
      () => tmpl.renderDict({ name: "world", bogus: "unexpected" }),
      (err: Error) =>
        err.message.includes("extra") || err.message.includes("undeclared"),
    );
  });

  it("renderDict allows extra with allowExtra", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    const result = tmpl.renderDict(
      { name: "world", bogus: "ignored" },
      { allowExtra: true },
    );
    assert.strictEqual(result, "Hello world!");
  });
});

// ---------------------------------------------------------------------------
// renderDict
// ---------------------------------------------------------------------------

describe("renderDict", () => {
  it("renders from a plain object", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.renderDict({ name: "dict" }), "Hello dict!");
  });

  it("validates types", () => {
    const tmpl = Template.fromSource(
      `---
params: [count = int]
---
{{ count }}`,
    );
    assert.throws(
      () => tmpl.renderDict({ count: "not an int" }),
      (err: Error) => err.message.includes("type mismatch"),
    );
  });
});

// ---------------------------------------------------------------------------
// renderEmpty
// ---------------------------------------------------------------------------

describe("renderEmpty", () => {
  it("renders no-param template", () => {
    const tmpl = Template.fromSource(`---
---
Hello world!`);
    assert.strictEqual(tmpl.renderEmpty(), "Hello world!");
  });

  it("renders with all-default params", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - greeting = str := "Hi"
  - count = int := 5
---
{{ greeting }} {{ count }}`,
    );
    assert.strictEqual(tmpl.renderEmpty(), "Hi 5");
  });

  it("renders with consts only", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - VERSION = str := "1.0"

params: []
---
v{{ VERSION }}`,
    );
    assert.strictEqual(tmpl.renderEmpty(), "v1.0");
  });

  it("throws for required params", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(
      () => tmpl.renderEmpty(),
      (err: Error) => err.message.includes("name"),
    );
  });

  it("throws for mixed defaults and required", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - greeting = str := "Hi"
  - name = str
---
{{ greeting }} {{ name }}!`,
    );
    assert.throws(
      () => tmpl.renderEmpty(),
      (err: Error) => err.message.includes("name"),
    );
  });
});

// ---------------------------------------------------------------------------
// Typed lists
// ---------------------------------------------------------------------------

describe("Typed lists", () => {
  const SRC = `---
params:
  - tasks = list(title = str, priority = str)
---
> {% for task in tasks %}

- **{{ task.title }}** ({{ task.priority }})

> {% /for %}`;

  it("renders a list of structs", () => {
    const tmpl = Template.fromSource(SRC);
    const output = tmpl.render({
      tasks: [
        { title: "Write documentation", priority: "High" },
        { title: "Add unit tests", priority: "Medium" },
      ],
    });
    assert.ok(output.includes("Write documentation"));
    assert.ok(output.includes("Add unit tests"));
  });

  it("renders an empty list", () => {
    const tmpl = Template.fromSource(SRC);
    const output = tmpl.render({ tasks: [] });
    assert.strictEqual(output.trim(), "");
  });

  it("throws on wrong item type", () => {
    const tmpl = Template.fromSource(SRC);
    assert.throws(() => tmpl.render({ tasks: ["not a struct"] }));
  });
});

// ---------------------------------------------------------------------------
// Struct parameters
// ---------------------------------------------------------------------------

describe("Struct parameters", () => {
  const SRC = `---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`;

  it("renders a struct param", () => {
    const tmpl = Template.fromSource(SRC);
    const output = tmpl.render({
      config: { host: "localhost", port: 8080 },
    });
    assert.strictEqual(output, "localhost:8080");
  });

  it("throws on missing field", () => {
    const tmpl = Template.fromSource(SRC);
    assert.throws(
      () => tmpl.render({ config: { host: "localhost" } }),
      (err: Error) => err.message.includes("missing"),
    );
  });
});

// ---------------------------------------------------------------------------
// Enum dispatch
// ---------------------------------------------------------------------------

describe("Enum dispatch", () => {
  const SRC = [
    `---`,
    "params:",
    "  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)",
    `---`,
    "> {% match outcome %}",
    "> {% case Confirmed %}",
    "",
    "YES: {{ outcome.evidence }}",

    "",
    "> {% case Rejected %}",
    "",
    "NO",

    "",
    "> {% case NeedsWork %}",
    "",
    "MAYBE",

    "",
    "> {% /match %}",
  ].join("\n");

  it("renders a unit variant", () => {
    const tmpl = Template.fromSource(SRC);
    const output = tmpl.render({ outcome: "Rejected" });
    assert.strictEqual(output.trim(), "NO");
  });

  it("renders a struct variant as dict", () => {
    const tmpl = Template.fromSource(SRC);
    const output = tmpl.render({
      outcome: { __kind__: "Confirmed", evidence: "found it" },
    });
    assert.strictEqual(output.trim(), "YES: found it");
  });

  it("throws on invalid variant", () => {
    const tmpl = Template.fromSource(SRC);
    assert.throws(
      () => tmpl.render({ outcome: "Unknown" }),
      (err: Error) =>
        err.message.includes("type mismatch") || err.message.includes("enum"),
    );
  });
});

// ---------------------------------------------------------------------------
// Default values
// ---------------------------------------------------------------------------

describe("Default values", () => {
  const SRC = `---
params:
  - name = str := "World"
  - count = int := 1
---
Hello {{ name }}, count={{ count }}!`;

  it("uses defaults when params omitted", () => {
    const tmpl = Template.fromSource(SRC);
    assert.strictEqual(tmpl.render(), "Hello World, count=1!");
  });

  it("overrides defaults", () => {
    const tmpl = Template.fromSource(SRC);
    assert.strictEqual(
      tmpl.render({ name: "Alice", count: 99 }),
      "Hello Alice, count=99!",
    );
  });

  it("returns defaults dict", () => {
    const tmpl = Template.fromSource(SRC);
    const defs = tmpl.defaults();
    assert.ok("name" in defs);
    assert.ok("count" in defs);
  });

  it("supports list default values", () => {
    const tmpl = Template.fromSource(`---
params:
  - tags = list(str) := ["rust", "go"]
---
{{ tags | join(", ") }}`);
    assert.strictEqual(tmpl.render(), "rust, go");
  });

  it("supports empty list default values", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(int) := []
---
{{ len(items) }}`);
    assert.strictEqual(tmpl.render(), "0");
  });

  it("supports struct list default values", () => {
    const tmpl = Template.fromSource(`---
params:
  - users = list(name = str, active = bool) := [{name = "Alice", active = true}]
---
> {% for u in users %}{{ u.name }}:{{ u.active }}{% /for %}`);
    assert.strictEqual(tmpl.render().trim(), "Alice:true");
  });
});

// ---------------------------------------------------------------------------
// TemplateCache
// ---------------------------------------------------------------------------

describe("TemplateCache", () => {
  it("loads and caches a template", () => {
    withTempFile(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
      (filepath) => {
        const cache = new TemplateCache();
        const t1 = cache.load(filepath);
        assert.strictEqual(t1.render({ name: "cached" }), "Hello cached!");
      },
    );
  });

  it("returns same hash on repeated loads", () => {
    withTempFile(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
      (filepath) => {
        const cache = new TemplateCache();
        const t1 = cache.load(filepath);
        const t2 = cache.load(filepath);
        assert.strictEqual(t1.sourceHash(), t2.sourceHash());
      },
    );
  });
});

// ---------------------------------------------------------------------------
// Template metadata
// ---------------------------------------------------------------------------

describe("Template metadata", () => {
  it("returns declarations", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }} {{ count }}`,
    );
    const decls = tmpl.declarations();
    const names = decls.map((d) => d[0]);
    assert.ok(names.includes("name"));
    assert.ok(names.includes("count"));
  });

  it("returns correct types in declarations", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }} {{ count }}`,
    );
    const decls = tmpl.declarations();
    const typeMap = Object.fromEntries(decls);
    assert.strictEqual(typeMap.name, "str");
    assert.strictEqual(typeMap.count, "int");
  });

  it("sourceHash is stable", () => {
    const source = `---
params: [x = str]
---
{{ x }}`;
    const t1 = Template.fromSource(source);
    const t2 = Template.fromSource(source);
    assert.strictEqual(t1.sourceHash(), t2.sourceHash());
  });

  it("sourceHash changes with content", () => {
    const t1 = Template.fromSource(
      `---
params: [x = str]
---
Hello {{ x }}`,
    );
    const t2 = Template.fromSource(
      `---
params: [x = str]
---
Goodbye {{ x }}`,
    );
    assert.notStrictEqual(t1.sourceHash(), t2.sourceHash());
  });

  it("toString includes Template", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
{{ name }}`,
    );
    assert.ok(tmpl.toString().includes("Template"));
    assert.ok(tmpl.toString().includes("name"));
  });
});
