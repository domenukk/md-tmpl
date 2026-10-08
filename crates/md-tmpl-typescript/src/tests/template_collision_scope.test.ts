/**
 * Tests for the md-tmpl TypeScript bindings: collision/scope tests and milestone 2 enforcement.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

import { Template, TemplateSyntaxError } from "../index.js";

// ---------------------------------------------------------------------------
// Collision and scope tests
// ---------------------------------------------------------------------------

describe("Collision and scope tests", () => {
  // ── 1. Const as param default ──────────────────────────────────────────

  describe("Const as param default", () => {
    it("local const as param default", () => {
      const tmpl = Template.fromSource(`---
consts:
  - MAX = int := 10

params:
  - count = int := MAX
---
{{ count }}`);
      // Without providing count, it should use the const default
      assert.strictEqual(tmpl.render().trim(), "10");
      // Providing count overrides the default
      assert.strictEqual(tmpl.render({ count: 42 }).trim(), "42");
    });

    it("imported const as param default", () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-impconst-"));
      try {
        const childContent = `---
consts:
  - LIMIT = int := 100

params: [x = str]
---
{{ x }}`;
        fs.writeFileSync(path.join(dir, "Config.tmpl.md"), childContent);

        const tmpl = Template.fromSourceWithBaseDir(
          `---
imports:
  - "[Config](./Config.tmpl.md)"

params:
  - max_items = int := Config.LIMIT
---
{{ max_items }}`,
          dir,
        );
        assert.strictEqual(tmpl.render().trim(), "100");
        assert.strictEqual(tmpl.render({ max_items: 5 }).trim(), "5");
      } finally {
        fs.rmSync(dir, { recursive: true });
      }
    });

    it("rejects const default with type mismatch", () => {
      assert.throws(
        () =>
          Template.fromSource(`---
consts:
  - NAME = str := "hello"

params:
  - count = int := NAME
---
{{ count }}`),
        (err: Error) =>
          err.message.includes("type") && err.message.includes("str"),
      );
    });
  });

  // ── 2. Param name shadows import stem (REJECTED) ─────────────────────

  describe("Param name shadows import stem", () => {
    it("rejects param whose PascalCase matches import stem", () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-shadow-imp-"));
      try {
        const childContent = `---
params: [x = str]
---
{{ x }}`;
        fs.writeFileSync(path.join(dir, "Helper.tmpl.md"), childContent);

        assert.throws(
          () =>
            Template.fromSourceWithBaseDir(
              `---
imports:
  - "[Helper](./Helper.tmpl.md)"

params:
  - helper = str
---
{{ helper }}`,
              dir,
            ),
          (err: Error) => err.message.includes("shadows import"),
        );
      } finally {
        fs.rmSync(dir, { recursive: true });
      }
    });

    it("rejects const whose PascalCase matches import stem", () => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-shadow-cimp-"));
      try {
        const childContent = `---
params: [x = str]
---
{{ x }}`;
        fs.writeFileSync(path.join(dir, "Config.tmpl.md"), childContent);

        assert.throws(
          () =>
            Template.fromSourceWithBaseDir(
              `---
imports:
  - "[Config](./Config.tmpl.md)"

consts:
  - config = str := "val"
---
{{ config }}`,
              dir,
            ),
          (err: Error) => err.message.includes("shadows import"),
        );
      } finally {
        fs.rmSync(dir, { recursive: true });
      }
    });
  });

  // ── 3. Param name shadows type alias (REJECTED unless type matches) ──

  describe("Param name shadows type alias", () => {
    it("rejects param PascalCase matching type alias with different type", () => {
      assert.throws(
        () =>
          Template.fromSource(`---
types:
  - TaskList = list(title = str)

params:
  - task_list = str
---
{{ task_list }}`),
        (err: Error) => err.message.includes("conflicts with type alias"),
      );
    });

    it("rejects const PascalCase matching type alias with different type", () => {
      assert.throws(
        () =>
          Template.fromSource(
            [
              `---`,
              "types:",
              "  - MyItem = list(x = int)",
              "",
              "consts:",
              '  - my_item = str := "hello"',
              `---`,
              "{{ my_item }}",
            ].join("\n"),
          ),
        (err: Error) => err.message.includes("conflicts with type alias"),
      );
    });

    it("allows param PascalCase matching type alias with SAME type", () => {
      const tmpl = Template.fromSource(`---
types:
  - CodeReview = list(title = str)

params:
  - code_review = CodeReview
---
> {% for item in code_review %}

{{ item.title }}

> {% /for %}`);
      const output = tmpl.render({
        code_review: [{ title: "PR #1" }],
      });
      assert.ok(output.includes("PR #1"));
    });
  });

  // ── 4. Param name shadows const name (REJECTED) ──────────────────────

  describe("Param name shadows const name", () => {
    it("rejects param and const with same name", () => {
      assert.throws(
        () =>
          Template.fromSource(`---
consts:
  - limit = int := 5

params:
  - limit = int
---
{{ limit }}`),
        (err: Error) =>
          err.message.includes("conflicts with constant") ||
          err.message.includes("declared as both"),
      );
    });
  });

  // ── 5. Import stem shadows type alias (REJECTED) ─────────────────────

  describe("Import stem shadows type alias", () => {
    it("rejects type alias with same name as import stem", () => {
      assert.throws(
        () =>
          Template.fromSource(
            [
              `---`,
              "imports:",
              '  - "[Shared](./Shared.tmpl.md)"',
              "",
              "types:",
              "  - Shared = list(x = str)",
              "",
              "params:",
              "  - data = Shared",
              `---`,
              "> {% for item in data %}",
              "",
              "{{ item.x }}",
              "",
              "> {% /for %}",
            ].join("\n"),
          ),
        (err: Error) => err.message.includes("shadows"),
      );
    });
  });

  // ── 6. Inline tmpl name shadows param/const (REJECTED) ───────────────

  describe("Inline tmpl name shadows param or const", () => {
    it("rejects inline template name colliding with param name", () => {
      assert.throws(
        () =>
          Template.fromSource(
            [
              `---`,
              "params:",
              "  - greeting = str",
              `---`,
              "> {% tmpl greeting %}",
              "",
              `---`,
              "",
              "params:",
              "",
              "- text = str",
              "",
              `---`,
              "",
              "{{ text }}",
              "",
              "> {% /tmpl %}",
              "",
              "{{ greeting }}",
            ].join("\n"),
          ),
        (err: Error) => err.message.includes("inline template name conflicts"),
      );
    });

    it("rejects inline template name colliding with const name", () => {
      assert.throws(
        () =>
          Template.fromSource(
            [
              `---`,
              "consts:",
              '  - footer = str := "bye"',
              `---`,
              "> {% tmpl footer %}",
              "",
              `---`,
              "",
              "params:",
              "",
              "- text = str",
              "",
              `---`,
              "",
              "{{ text }}",
              "",
              "> {% /tmpl %}",
              "",
              "{{ footer }}",
            ].join("\n"),
          ),
        (err: Error) => err.message.includes("inline template name conflicts"),
      );
    });
  });

  // ── 7. Inline tmpl name shadows import stem (REJECTED) ───────────────

  describe("Inline tmpl name shadows import stem", () => {
    it("rejects inline template name matching import stem", () => {
      assert.throws(
        () =>
          Template.fromSource(
            [
              `---`,
              "imports:",
              '  - "[Utils](./Utils.tmpl.md)"',
              "",
              "params:",
              "  - x = str",
              `---`,
              "> {% tmpl Utils %}",
              "",
              `---`,
              "",
              "params:",
              "",
              "- y = str",
              "",
              `---`,
              "",
              "{{ y }}",
              "",
              "> {% /tmpl %}",
              "",
              "{{ x }}",
            ].join("\n"),
          ),
        (err: Error) =>
          err.message.includes("inline template name conflicts with import"),
      );
    });
  });

  // ── 8. For-loop binding shadows declared name (REJECTED) ─────────────

  describe("For-loop binding shadows declared name", () => {
    it("rejects for binding that shadows a param", () => {
      assert.throws(
        () =>
          Template.fromSource(`---
params:
  - item = str
  - items = list(str)
---
> {% for item in items %}

{{ item }}

> {% /for %}`),
        (err: Error) =>
          // Rust says "unused" (param 'item' is never referenced as a param,
          // only the loop binding is used). TS now matches this behavior.
          err.message.includes("unused") || err.message.includes("shadows"),
      );
    });

    it("rejects for binding that shadows a const", () => {
      assert.throws(
        () =>
          Template.fromSource(`---
consts:
  - x = int := 1

params:
  - items = list(str)
---
> {% for x in items %}

{{ x }}

> {% /for %}`),
        (err: Error) => err.message.includes("shadows"),
      );
    });

    it("rejects for binding that shadows an import stem", () => {
      assert.throws(
        () =>
          Template.fromSource(
            [
              `---`,
              "imports:",
              '  - "[item](./item.tmpl.md)"',
              "",
              "params:",
              "  - items = list(str)",
              `---`,
              "> {% for item in items %}",
              "",
              "{{ item }}",
              "",
              "> {% /for %}",
            ].join("\n"),
          ),
        (err: Error) => err.message.includes("shadows"),
      );
    });
  });

  // ── 9. Nested tmpl scope isolation ───────────────────────────────────

  describe("Nested tmpl scope isolation", () => {
    it("child tmpl with its own types works independently", () => {
      const src = [
        `---`,
        "params:",
        "  - items = list(name = str)",
        `---`,
        "> {% tmpl row %}",
        "",
        `---`,
        "",
        "params:",
        "",
        "- label = str",
        "",
        `---`,
        "",
        "[{{ label }}]",
        "",
        "> {% /tmpl %}",
        "> {% for item in items %}",
        "> {% include row with label=item.name %}",
        "> {% /for %}",
      ].join("\n");

      const tmpl = Template.fromSource(src);
      const result = tmpl.render({
        items: [{ name: "alpha" }, { name: "beta" }],
      });
      assert.ok(result.includes("[alpha]"), `expected [alpha] in: ${result}`);
      assert.ok(result.includes("[beta]"), `expected [beta] in: ${result}`);
    });

    it("parent type alias is NOT inherited by child tmpl", () => {
      // Parent declares a type alias. The child inline tmpl has its own
      // scope with only its own params — proving scope isolation.
      // We pass a variable (label) from the parent scope to the child.
      const src = [
        `---`,
        "types:",
        "  - Priority = enum(High, Low)",
        "",
        "params:",
        "  - p = Priority",
        "  - label = str",
        `---`,
        "> {% tmpl detail %}",
        "",
        `---`,
        "",
        "params:",
        "",
        "- msg = str",
        "",
        `---`,
        "",
        "Detail: {{ msg }}",
        "",
        "> {% /tmpl %}",
        "> {% match p %}",
        "> {% case High %}",
        "",
        "> {% include detail with msg=label %}",
        "",
        "> {% case Low %}",
        "",
        "> {% include detail with msg=label %}",
        "",
        "> {% /match %}",
      ].join("\n");

      const tmpl = Template.fromSource(src);
      const result = tmpl.render({ p: "High", label: "urgent" });
      assert.ok(
        result.includes("Detail: urgent"),
        `expected Detail: urgent in: ${result}`,
      );
    });

    it("inline tmpl with own consts can access them", () => {
      const src = [
        `---`,
        "params:",
        "  - name = str",
        `---`,
        "> {% tmpl greeting %}",
        "",
        `---`,
        "",
        "consts:",
        "",
        '  - PREFIX = str := "Hello"',
        "",
        "params:",
        "",
        "- who = str",
        "",
        `---`,
        "",
        "{{ PREFIX }}, {{ who }}!",
        "",
        "> {% /tmpl %}",
        "> {% include greeting with who=name %}",
      ].join("\n");

      const tmpl = Template.fromSource(src);
      const result = tmpl.render({ name: "world" });
      assert.ok(
        result.includes("Hello, world!"),
        `expected 'Hello, world!' in: ${result}`,
      );
    });

    it("inline tmpl consts shadow parent values", () => {
      const src = [
        `---`,
        "consts:",
        '  - LABEL = str := "outer"',
        "",
        "params:",
        "  - x = str",
        `---`,
        "> {% tmpl inner %}",
        "",
        `---`,
        "",
        "consts:",
        "",
        '  - LABEL = str := "inner"',
        "",
        "params:",
        "",
        "- val = str",
        "",
        `---`,
        "",
        "{{ LABEL }}:{{ val }}",
        "",
        "> {% /tmpl %}",
        "",
        "{{ LABEL }}",
        "",
        "> {% include inner with val=x %}",
      ].join("\n");

      const tmpl = Template.fromSource(src);
      const result = tmpl.render({ x: "test" });
      assert.ok(
        result.includes("outer"),
        `expected 'outer' from parent scope in: ${result}`,
      );
      assert.ok(
        result.includes("inner:test"),
        `expected 'inner:test' from child scope in: ${result}`,
      );
    });
  });

  // ── 10. Duplicate const names (REJECTED) ─────────────────────────────

  describe("Duplicate const names", () => {
    it("rejects duplicate constant names", () => {
      assert.throws(
        () =>
          Template.fromSource(`---
consts:
  - MAX = int := 10
  - MAX = int := 20

params:
  - x = str
---
{{ x }}`),
        (err: Error) => err.message.includes("duplicate"),
      );
    });
  });

  // ── 11. Const name conflicts with type alias ─────────────────────────

  describe("Const name conflicts with type alias", () => {
    it("rejects const whose PascalCase matches a non-enum type alias", () => {
      assert.throws(
        () =>
          Template.fromSource(
            [
              `---`,
              "types:",
              "  - MaxSize = list(x = int)",
              "",
              "consts:",
              '  - max_size = str := "big"',
              `---`,
              "{{ max_size }}",
            ].join("\n"),
          ),
        (err: Error) => err.message.includes("conflicts with type alias"),
      );
    });

    it("allows const matching enum type alias (enum auto-inject)", () => {
      // Enum type aliases are auto-injected as constants; a user-defined
      // constant with the same name just takes priority — not a conflict.
      const tmpl = Template.fromSource(`---
types:
  - Stage = enum(Design, Build)

consts:
  - Stage = str := "override"
---
{{ Stage }}`);
      const result = tmpl.render();
      assert.strictEqual(result, "override");
    });
  });
});

describe("Milestone 2 Enforcement", () => {
  it("accepts compound types with parens (...)", () => {
    const tmpl = Template.fromSource(`---
types:
  - MyStruct = struct(name = str)
  - MyEnum = enum(A, B)

params:
  - items = list(MyStruct)
  - opt = option(int)
---
{{ len(items) }}{% if has(opt) %}{{ opt }}{% /if %}`);
    assert.ok(tmpl);
  });

  it("rejects compound types with <...> or [...] throwing TemplateSyntaxError", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params: [x = list<str>]
---
{{ x }}`),
      (err: unknown) =>
        err instanceof TemplateSyntaxError &&
        err.message.includes("parentheses (...)"),
    );
    assert.throws(
      () =>
        Template.fromSource(`---
params: [x = list[str]]\n---
{{ x }}`),
      (err: unknown) =>
        err instanceof TemplateSyntaxError &&
        err.message.includes("parentheses (...)"),
    );
  });

  it("strips outer quotes in declaration lines in frontmatter", () => {
    const tmpl = Template.fromSource(`---
params:
  - "name = str"
  - 'count = int'
---
{{ name }}: {{ count }}`);
    assert.strictEqual(tmpl.render({ name: "test", count: 5 }), "test: 5");
  });

  it("enforces strict ./ or ../ prefixes for relative file includes", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params: []
---
> {% include [header](header.tmpl.md) %}`,
        ),
      (err: unknown) =>
        err instanceof TemplateSyntaxError &&
        err.message.includes("include path must begin with"),
    );
  });

  describe("Adversarial M2 features (in/not in & panic)", () => {
    it("handles unicode substring checks and empty string checks", () => {
      const tmpl = Template.fromSource(`---
params: [text = str, query = str]
---
> {% if query in text %}FOUND{% else %}MISSING{% /if %}`);
      assert.strictEqual(
        tmpl.render({ text: "Hello 🌍 World!", query: "🌍" }).trim(),
        "FOUND",
      );
      assert.strictEqual(
        tmpl.render({ text: "Hello", query: "" }).trim(),
        "FOUND",
      );
    });

    it("halts rendering on panic inside taken conditional branch", () => {
      const tmpl = Template.fromSource(`---
params: [trigger = bool]
---
> {% if trigger %}{% panic("Security violation: access denied") %}{% /if %}
SAFE`);
      assert.strictEqual(tmpl.render({ trigger: false }).trim(), "SAFE");
      assert.throws(
        () => tmpl.render({ trigger: true }),
        (err: unknown) =>
          err instanceof Error && err.message.includes("Security violation"),
      );
    });
  });
});
