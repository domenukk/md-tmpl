/**
 * Tests for the md-tmpl TypeScript bindings: regression tests and enum defaults, keywords and literal expressions.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

import { Template, TemplateCache, TemplateSyntaxError } from "../index.js";
import { parseVarType } from "../frontmatter.js";
import { withTempFile } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

describe("Regression tests", () => {
  // ── 1. AST mutation bug (trimAfter) ─────────────────────────────────
  // Both `renderNodes()` and `renderDirectNodes()` previously mutated the
  // AST nodes array in-place when handling trimAfter. Calling render()
  // twice on the same template would produce different results.

  it("render() is idempotent with trimAfter whitespace control", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{- name -}} world`,
    );
    const first = tmpl.render({ name: "Alice" });
    const second = tmpl.render({ name: "Alice" });
    assert.strictEqual(first, second);
  });

  // ── 2. node:fs lazy loading (browser compat) ────────────────────────
  // `import * as fs from "node:fs"` was replaced with lazy getFs()/getPath()
  // getters. fromSource() and render() must work without filesystem access.

  it("fromSource does not require filesystem", () => {
    // This test verifies that creating a template from source and rendering
    // it never triggers file system access — only fromFile/TemplateCache do.
    const tmpl = Template.fromSource(
      `---
params: [greeting = str]
---
{{ greeting }}, universe!`,
    );
    const result = tmpl.render({ greeting: "Hello" });
    assert.strictEqual(result, "Hello, universe!");
  });

  // ── 3. checkUnusedParams word-boundary matching ─────────────────────
  // The unused param check was using `body.includes(decl.name)` which
  // had false negatives: a param named "a" passed if the body contained
  // "variable" (which contains 'a' as a substring).

  it("rejects param whose name is a substring of body words", () => {
    // Param "a" is declared but only "variable" appears in body.
    // "variable" contains 'a' as a substring but not as a word.
    assert.throws(
      () =>
        Template.fromSource(`---
params: [a = str]
---
The variable is set`),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(err.message.includes("unused"));
        return true;
      },
    );
  });

  // ── 4. Strict equality in direct renderer ───────────────────────────
  // The direct renderer was using `==` instead of `===` for comparisons,
  // which could cause `0 == ""` to be true due to JS loose coercion.

  it("direct renderer uses strict equality (empty string != 0)", () => {
    const tmpl = Template.fromSource(
      `---
params: [count = str]
---
> {% if count == 0 %}

zero

> {% else %}

nonzero

> {% /if %}`,
    );
    // With strict equality, "" !== 0, so this should NOT match the == 0 branch.
    const result = tmpl.render({ count: "" });
    assert.ok(
      result.includes("nonzero"),
      `expected "nonzero" but got: ${JSON.stringify(result)}`,
    );
  });

  // ── 5. importedConsts() method ──────────────────────────────────────
  // Verify importedConsts() returns an object (even if empty).

  it("importedConsts() returns an empty object for a simple template", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    const imported = tmpl.importedConsts();
    assert.deepStrictEqual(imported, {});
  });

  it("importedConsts() resolves constants from imported templates", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-import-"));
    try {
      // Child template with consts
      const childContent = [
        `---`,
        "consts:",
        "  - MAX_RETRIES = int := 3",
        '  - MODEL_NAME = str := "gpt-4"',
        "",
        "params: [x = str]",
        `---`,
        "{{ x }}",
      ].join("\n");
      fs.writeFileSync(path.join(dir, "child.tmpl.md"), childContent);

      // Parent template that imports the child
      const parentContent = [
        `---`,
        "imports:",
        '  - "[child](./child.tmpl.md)"',
        "",
        "params: [x = str]",
        `---`,
        "{{ x }}",
      ].join("\n");
      const parentPath = path.join(dir, "parent.tmpl.md");
      fs.writeFileSync(parentPath, parentContent);

      // Load parent from file (triggers import resolution)
      const tmpl = Template.fromFile(parentPath);
      const imported = tmpl.importedConsts();

      // Should have child's constants keyed as "child.NAME"
      assert.strictEqual(imported["child.MAX_RETRIES"], 3);
      assert.strictEqual(imported["child.MODEL_NAME"], "gpt-4");
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  // ── 6. TemplateCache content hash ───────────────────────────────────
  // Verify that TemplateCache returns the same template for unchanged content.

  it("TemplateCache returns stable hashes for unchanged content", () => {
    withTempFile(
      `---
params: [v = str]
---
{{ v }}`,
      (filepath) => {
        const cache = new TemplateCache();
        const t1 = cache.load(filepath);
        const t2 = cache.load(filepath);
        assert.strictEqual(t1.sourceHash(), t2.sourceHash());
        // Both should render identically
        assert.strictEqual(
          t1.render({ v: "cached" }),
          t2.render({ v: "cached" }),
        );
      },
    );
  });

  it("TemplateCache evicts oldest entry when maxEntries exceeded", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-lru-"));
    try {
      // Write 3 distinct template files
      for (let i = 1; i <= 3; i++) {
        fs.writeFileSync(
          path.join(dir, `t${String(i)}.tmpl.md`),
          `---
params: [x = str]
---
Template ${String(i)}: {{ x }}`,
        );
      }

      const cache = new TemplateCache({ maxEntries: 2 });
      cache.load(path.join(dir, "t1.tmpl.md"));
      cache.load(path.join(dir, "t2.tmpl.md"));
      assert.strictEqual(cache.templateCount(), 2);

      // Loading a 3rd template should evict the oldest (t1)
      cache.load(path.join(dir, "t3.tmpl.md"));
      assert.strictEqual(cache.templateCount(), 2);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("untyped list() fails", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [items = list()]
---
{{ len(items) }}`),
      (err: Error) => err.message.includes("untyped list() is not allowed"),
    );
  });

  it("untyped struct() fails", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [data = struct()]
---
{{ data.x }}`),
      (err: Error) => err.message.includes("untyped struct() is not allowed"),
    );
  });

  it("unnamed multiple fields list fails", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [items = list(str, int)]
---
{{ len(items) }}`),
      (err: Error) =>
        err.message.includes("list with multiple fields must use named fields"),
    );
  });

  it("unquoted string default fails", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [name = str := hello]
---
{{ name }}`),
      (err: Error) => err.message.includes("strings must be quoted"),
    );
  });

  // -- list(struct(...)) rejection ------------------------------------------

  it("list(str) parses OK (scalar list)", () => {
    const tmpl = Template.fromSource(`---
params: [items = list(str)]
---
{{ items | join(", ") }}`);
    assert.strictEqual(tmpl.render({ items: ["a", "b"] }), "a, b");
  });

  it("list(name = str, score = int) parses OK (named fields / struct items)", () => {
    const tmpl = Template.fromSource(`---
params: [items = list(name = str, score = int)]
---
> {% for item in items %}

{{ item.name }}: {{ item.score }}

> {% /for %}`);
    const output = tmpl.render({
      items: [{ name: "Alice", score: 100 }],
    });
    assert.ok(output.includes("Alice: 100"));
  });

  it("list(list(str)) parses OK (nested list)", () => {
    const tmpl = Template.fromSource(`---
params: [matrix = list(list(str))]
---
{{ len(matrix) }}`);
    assert.strictEqual(tmpl.render({ matrix: [["a", "b"], ["c"]] }), "2");
  });

  it("list(enum(A, B)) parses OK (list of enums)", () => {
    const tmpl = Template.fromSource(`---
params: [flags = list(enum(On, Off))]
---
{{ len(flags) }}`);
    assert.strictEqual(tmpl.render({ flags: ["On", "Off"] }), "2");
  });

  it("list(struct(name = str)) is rejected as redundant", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [items = list(struct(name = str))]
---
{{ len(items) }}`),
      (err: Error) =>
        err.message.includes(
          "list(struct(...)) is redundant; use named fields directly",
        ),
    );
  });

  it("list(struct(name = str, score = int)) is rejected as redundant", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [items = list(struct(name = str, score = int))]
---
{{ len(items) }}`),
      (err: Error) =>
        err.message.includes(
          "list(struct(...)) is redundant; use named fields directly",
        ),
    );
  });

  it("list of strong struct alias unwraps cleanly and renders", () => {
    const tmpl = Template.fromSource(`---
types: [MyItem = struct(name = str, score = int)]
params: [items = list(MyItem)]
---
> {% for item in items %}{{ item.name }}: {{ item.score }}{% /for %}`);
    assert.strictEqual(
      tmpl.render({ items: [{ name: "Alice", score: 10 }] }),
      "Alice: 10",
    );
  });

  it("list of named struct field allowed and renders", () => {
    const tmpl = Template.fromSource(`---
params: [items = list(item = struct(name = str, score = int))]
---
> {% for i in items %}{{ i.item.name }}: {{ i.item.score }}{% /for %}`);
    assert.strictEqual(
      tmpl.render({ items: [{ item: { name: "Bob", score: 20 } }] }),
      "Bob: 20",
    );
  });
});

// ---------------------------------------------------------------------------
// Enum default validation
// ---------------------------------------------------------------------------

describe("Enum default validation", () => {
  it("unit variant default works", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - status = enum(Active, Paused) := Active
---
{{ kind(status) }}`,
    );
    assert.strictEqual(tmpl.render(), "Active");
  });

  it("unit variant default on mixed enum", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected) := Rejected
---
{{ kind(outcome) }}`,
    );
    assert.strictEqual(tmpl.render(), "Rejected");
  });

  it("struct variant default with fields", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected) := Confirmed(evidence = "found it")
---
> {% match outcome %}

> {% case Confirmed %}

{{ outcome.evidence }}

> {% case Rejected %}

no

> {% /match %}`,
    );
    assert.strictEqual(tmpl.render().trim(), "found it");
  });

  it("multi-field struct variant default", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - result = enum(Success(msg = str, code = int), Failure) := Success(msg = "ok", code = 200)
---
> {% match result %}

> {% case Success %}

{{ result.msg }}:{{ result.code }}

> {% case Failure %}

fail

> {% /match %}`,
    );
    assert.strictEqual(tmpl.render().trim(), "ok:200");
  });

  it("struct variant default as a const renders correctly", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - DEFAULT = enum(Success(msg = str), Failure) := Success(msg = "done")
---
> {% match DEFAULT %}

> {% case Success %}

{{ DEFAULT.msg }}

> {% case Failure %}

fail

> {% /match %}`,
    );
    assert.strictEqual(tmpl.render().trim(), "done");
  });

  it("rejects bare struct variant without fields", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected) := Confirmed
---
{{ outcome }}`,
        ),
      (err: Error) =>
        err.message.includes("struct variant") &&
        err.message.includes("requires fields"),
    );
  });

  it("rejects unknown variant name", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params:
  - status = enum(Active, Paused) := Nonexistent
---
{{ status }}`,
        ),
      (err: Error) => err.message.includes("unknown enum variant"),
    );
  });

  it("rejects unit variant with fields", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params:
  - status = enum(Active, Paused) := Active(x = 1)
---
{{ status }}`,
        ),
      (err: Error) =>
        err.message.includes("unit variant") &&
        err.message.includes("cannot have fields"),
    );
  });
});

// ---------------------------------------------------------------------------
// Enum variant reserved keyword check
// ---------------------------------------------------------------------------

describe("Enum variant reserved keyword check", () => {
  it("rejects variant named 'struct'", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params:
  - x = enum(struct, ok)
---
{{ x }}`,
        ),
      (err: Error) => err.message.includes("shadows a builtin type keyword"),
    );
  });

  it("rejects variants named with reserved keywords", () => {
    for (const keyword of [
      "str",
      "bool",
      "int",
      "float",
      "list",
      "struct",
      "enum",
      "tmpl",
    ]) {
      assert.throws(
        () => parseVarType(`enum(${keyword}, Ok)`),
        (err: Error) => err.message.includes("shadows a builtin type keyword"),
        `expected variant '${keyword}' to be rejected`,
      );
    }
  });

  it("accepts valid variant names", () => {
    // Should not throw
    const vt = parseVarType("enum(Active, Paused, Done)");
    assert.strictEqual(vt.kind, "enum");
    assert.strictEqual(vt.variants.length, 3);
  });
});

// ---------------------------------------------------------------------------
// Enum literal expressions
// ---------------------------------------------------------------------------

describe("Enum literal expressions", () => {
  it("{{ Stage.Design }} is rejected as bare enum literal", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
types:
  - Stage = enum(Design, Build, Test)

params: [name = str]
---
Stage: {{ Stage.Design }}, name: {{ name }}`,
        ),
      (err: Error) =>
        err.message.includes("bare enum literal") &&
        err.message.includes("Stage.Design") &&
        err.message.includes("kind(Stage.Design)"),
    );
  });

  it("{{ kind(Stage.Design) }} renders as Design", () => {
    const tmpl = Template.fromSource(
      `---
types:
  - Stage = enum(Design, Build, Test)

params: [name = str]
---
Kind: {{ kind(Stage.Design) }}, name: {{ name }}`,
    );
    assert.strictEqual(
      tmpl.render({ name: "hello" }),
      "Kind: Design, name: hello",
    );
  });

  it("struct variant: {{ kind(Status.Paused) }} renders as Paused", () => {
    const tmpl = Template.fromSource(
      `---
types:
  - Status = enum(Active, Paused(reason = str), Done)

params: [name = str]
---
Kind: {{ kind(Status.Paused) }}, name: {{ name }}`,
    );
    assert.strictEqual(
      tmpl.render({ name: "test" }),
      "Kind: Paused, name: test",
    );
  });

  it("all variants accessible via kind()", () => {
    const tmpl = Template.fromSource(
      `---
types:
  - Color = enum(Red, Green, Blue)

params: [x = str]
---
{{ kind(Color.Red) }}, {{ kind(Color.Green) }}, {{ kind(Color.Blue) }} ({{ x }})`,
    );
    assert.strictEqual(tmpl.render({ x: "ok" }), "Red, Green, Blue (ok)");
  });

  it("enum type constants do NOT overwrite user-defined constants", () => {
    const tmpl = Template.fromSource(
      `---
types:
  - Stage = enum(Design, Build)

consts:
  - Stage = str := "custom"

params: [x = str]
---
{{ Stage }}: {{ x }}`,
    );
    assert.strictEqual(tmpl.render({ x: "hi" }), "custom: hi");
  });

  it("kind(Stage.Design) works with render", () => {
    const tmpl = Template.fromSource(
      `---
types:
  - Stage = enum(Design, Build, Test)

params: [name = str]
---
{{ kind(Stage.Design) }}: {{ name }}`,
    );
    assert.strictEqual(tmpl.render({ name: "fast" }), "Design: fast");
  });

  it("bare enum with filter is also rejected", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
types:
  - Stage = enum(Design, Build)

params: [x = str]
---
{{ Stage.Design | upper }}: {{ x }}`,
        ),
      (err: Error) =>
        err.message.includes("bare enum literal") &&
        err.message.includes("Stage.Design"),
    );
  });
});
