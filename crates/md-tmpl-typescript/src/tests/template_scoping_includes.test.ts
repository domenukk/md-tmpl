/**
 * Tests for the md-tmpl TypeScript bindings: constants scoping, loops, inline templates, includes and renderUnchecked parity.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

import { Template } from "../index.js";

// ---------------------------------------------------------------------------
// SPEC.md coverage — constants in scope
// ---------------------------------------------------------------------------

describe("Constants scoping", () => {
  it("consts are visible inside for loops", () => {
    const src = [
      `---`,
      "consts:",
      '  - PREFIX = str := ">>"',
      "",
      "params:",
      "  - items = list(name = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "{{ PREFIX }} {{ item.name }}",

      "",
      "> {% /for %}",
    ].join("\n");

    const result = Template.fromSource(src).render({
      items: [{ name: "A" }, { name: "B" }],
    });
    assert.strictEqual(result, ">> A\n>> B\n");
  });

  it("consts are visible inside if blocks", () => {
    const src = [
      `---`,
      "consts:",
      "  - THRESHOLD = int := 10",
      "",
      "params:",
      "  - x = int",
      `---`,
      "> {% if x > 5 %}",
      "",
      "over (threshold={{ THRESHOLD }})",

      "",
      "> {% /if %}",
    ].join("\n");

    assert.strictEqual(
      Template.fromSource(src).render({ x: 7 }),
      "over (threshold=10)\n",
    );
  });

  it("consts are visible inside match blocks", () => {
    const src = [
      `---`,
      "consts:",
      '  - LABEL = str := "status"',
      "",
      "params:",
      "  - s = enum(A, B)",
      `---`,
      "> {% match s %}",
      "> {% case A %}",
      "",
      "{{ LABEL }}: alpha",

      "",
      "> {% case B %}",
      "",
      "{{ LABEL }}: beta",

      "",
      "> {% /match %}",
    ].join("\n");

    assert.strictEqual(
      Template.fromSource(src).render({ s: "A" }),
      "status: alpha\n",
    );
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — for loop empty list
// ---------------------------------------------------------------------------

describe("For loop edge cases", () => {
  it("empty list produces empty output", () => {
    const src = [
      `---`,
      "params:",
      "  - items = list(name = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "{{ item.name }}",

      "",
      "> {% /for %}",
    ].join("\n");

    assert.strictEqual(Template.fromSource(src).render({ items: [] }), "");
  });

  it("single item list has no trailing separator issues", () => {
    const src = [
      `---`,
      "params:",
      "  - items = list(name = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "- {{ item.name }}",

      "",
      "> {% /for %}",
    ].join("\n");

    assert.strictEqual(
      Template.fromSource(src).render({ items: [{ name: "only" }] }),
      "- only\n",
    );
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — inline templates ({% tmpl %})
// ---------------------------------------------------------------------------

describe("Inline templates", () => {
  it("defines and includes an inline template", () => {
    const src = [
      `---`,
      "params:",
      "  - tasks = list(title = str, priority = str)",
      `---`,
      "> {% tmpl task_row %}",
      "",
      `---`,
      "",
      "params:",
      "",
      "- title = str",
      "- priority = str",
      "",
      `---`,
      "",
      "- **{{ title }}** ({{ priority }})",

      "",
      "> {% /tmpl %}",
      "> {% for task in tasks %}",
      "> {% include task_row with title=task.title, priority=task.priority %}",
      "> {% /for %}",
    ].join("\n");

    const result = Template.fromSource(src).render({
      tasks: [
        { title: "Write docs", priority: "high" },
        { title: "Add tests", priority: "medium" },
      ],
    });
    assert.ok(result.includes("Write docs"));
    assert.ok(result.includes("high"));
    assert.ok(result.includes("Add tests"));
    assert.ok(result.includes("medium"));
  });

  it("inline template renders with simple params", () => {
    const src = [
      `---`,
      "params:",
      "  - greeting = str",
      `---`,
      "> {% tmpl msg %}",
      "",
      `---`,
      "",
      "params:",
      "",
      "- text = str",
      "",
      `---`,
      "",
      "[{{ text }}]",

      "",
      "> {% /tmpl %}",
      "> {% include msg with text=greeting %}",
    ].join("\n");

    const result = Template.fromSource(src).render({ greeting: "hello" });
    assert.ok(result.includes("[hello]"));
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — include with params (file-based)
// ---------------------------------------------------------------------------

describe("Include with params (file-based)", () => {
  it("includes a file-based template with explicit params", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-include-"));
    const childPath = path.join(dir, "child.tmpl.md");
    fs.writeFileSync(
      childPath,
      `---
params:
  - msg = str
---
Child says: {{ msg }}`,
    );

    const parentPath = path.join(dir, "parent.tmpl.md");
    fs.writeFileSync(
      parentPath,
      `---
params:
  - greeting = str
---
> {% include [child](./child.tmpl.md) with msg=greeting %}`,
    );

    try {
      const tmpl = Template.fromFile(parentPath);
      const result = tmpl.render({ greeting: "hello" });
      assert.ok(
        result.includes("Child says: hello"),
        `expected 'Child says: hello' but got: ${result}`,
      );
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("include for iteration renders each item", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-incfor-"));
    const rowPath = path.join(dir, "row.tmpl.md");
    fs.writeFileSync(
      rowPath,
      `---
params:
  - item = struct(name = str)
---
- {{ item.name }}`,
    );

    const mainPath = path.join(dir, "main.tmpl.md");
    fs.writeFileSync(
      mainPath,
      `---
params:
  - items = list(name = str)
---
> {% include [row](./row.tmpl.md) for item in items %}`,
    );

    try {
      const tmpl = Template.fromFile(mainPath);
      const result = tmpl.render({
        items: [{ name: "A" }, { name: "B" }, { name: "C" }],
      });
      assert.ok(result.includes("A"), `missing A in: ${result}`);
      assert.ok(result.includes("B"), `missing B in: ${result}`);
      assert.ok(result.includes("C"), `missing C in: ${result}`);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — raw custom delimiter
// ---------------------------------------------------------------------------

describe("Raw custom delimiter", () => {
  it("uses raw=DELIM to escape /raw itself", () => {
    const tmpl = Template.fromSource(
      `---
params: []
---
> {% raw=MYDELIM %}
> {% raw %}...{% /raw %}
> {% /MYDELIM %}`,
    );
    const result = tmpl.render({});
    assert.ok(
      result.includes("{% raw %}"),
      `expected raw syntax preserved, got: ${result}`,
    );
    assert.ok(
      result.includes("{% /raw %}"),
      `expected /raw preserved, got: ${result}`,
    );
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — include depth limit
// ---------------------------------------------------------------------------

describe("Include depth limit", () => {
  it("maxIncludeDepth prevents infinite recursion", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-depth-"));
    const recPath = path.join(dir, "rec.tmpl.md");
    // Self-referencing template
    fs.writeFileSync(
      recPath,
      `---
params:
  - n = int
---
{{ n }}

> {% include [rec](./rec.tmpl.md) with n=n %}`,
    );

    try {
      const tmpl = Template.fromFile(recPath);
      assert.throws(() => tmpl.render({ n: 1 }), /depth/i);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Direct renderer parity — renderUnchecked must produce same output as render
// ---------------------------------------------------------------------------

describe("renderUnchecked parity", () => {
  const parityCheck = (
    label: string,
    src: string,
    params: Record<string, unknown>,
  ) => {
    it(label, () => {
      const tmpl = Template.fromSource(src);
      const checked = tmpl.render(params);
      const unchecked = tmpl.renderUnchecked(params);
      assert.strictEqual(
        unchecked,
        checked,
        `render vs renderUnchecked mismatch for: ${label}`,
      );
    });
  };

  parityCheck(
    "simple string",
    `---
params: [name = str]
---
Hello {{ name }}!`,
    { name: "world" },
  );

  parityCheck(
    "for loop",
    [
      `---`,
      "params:",
      "  - items = list(name = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "- {{ item.name }}",

      "",
      "> {% /for %}",
    ].join("\n"),
    { items: [{ name: "A" }, { name: "B" }] },
  );

  parityCheck(
    "if/elif/else",
    [
      `---`,
      "params:",
      "  - x = int",
      `---`,
      "> {% if x > 10 %}",
      "",
      "big",

      "",
      "> {% elif x > 5 %}",
      "",
      "medium",

      "",
      "> {% else %}",
      "",
      "small",

      "",
      "> {% /if %}",
    ].join("\n"),
    { x: 7 },
  );

  parityCheck(
    "match/case with struct variant",
    [
      `---`,
      "params:",
      "  - status = enum(Ok(msg = str), Error(code = int))",
      `---`,
      "> {% match status %}",
      "> {% case Ok %}",
      "",
      "OK: {{ status.msg }}",

      "",
      "> {% case Error %}",
      "",
      "ERR: {{ status.code }}",

      "",
      "> {% /match %}",
    ].join("\n"),
    { status: { __kind__: "Ok", msg: "done" } },
  );

  parityCheck(
    "constants in scope",
    [
      `---`,
      "consts:",
      '  - PREFIX = str := ">>"',
      "",
      "params:",
      "  - name = str",
      `---`,
      "{{ PREFIX }} {{ name }}",
    ].join("\n"),
    { name: "test" },
  );

  parityCheck(
    "default values",
    [
      `---`,
      "params:",
      '  - name = str := "default"',
      "  - count = int := 0",
      `---`,
      "{{ name }} {{ count }}",
    ].join("\n"),
    {},
  );

  parityCheck(
    "empty list",
    [
      `---`,
      "params:",
      "  - items = list(name = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "- {{ item.name }}",

      "",
      "> {% /for %}",
    ].join("\n"),
    { items: [] },
  );

  parityCheck(
    "whitespace control",
    `---
params:
  - name = str
---
hello  {{- name -}}
bye`,
    { name: "world" },
  );

  parityCheck(
    "match else arm",
    [
      `---`,
      "params:",
      "  - s = enum(A, B, C)",
      `---`,
      "> {% match s %}",
      "> {% case A %}",
      "",
      "alpha",

      "",
      "> {% else %}",
      "",
      "other",

      "",
      "> {% /match %}",
    ].join("\n"),
    { s: "B" },
  );

  parityCheck(
    "match multi-variant arm",
    [
      `---`,
      "params:",
      "  - s = enum(A, B, C)",
      `---`,
      "> {% match s %}",
      "> {% case A | B %}",
      "",
      "ab",

      "",
      "> {% case C %}",
      "",
      "c",

      "",
      "> {% /match %}",
    ].join("\n"),
    { s: "A" },
  );

  parityCheck(
    "filters chain",
    `---
params: [name = str]
---
{{ name | trim | upper }}`,
    { name: "  hello  " },
  );

  parityCheck(
    "comparison operators",
    [
      `---`,
      "params:",
      "  - x = int",
      `---`,
      "> {% if x >= 5 %}",
      "",
      "yes",

      "",
      "> {% else %}",
      "",
      "no",

      "",
      "> {% /if %}",
    ].join("\n"),
    { x: 5 },
  );

  parityCheck(
    "nested for loops",
    [
      `---`,
      "params:",
      "  - rows = list(cells = list(v = str))",
      `---`,
      "> {% for row in rows %}> {% for cell in row.cells %}{{ cell.v }} {% /for %}",
      "> {% /for %}",
    ].join("\n"),
    { rows: [{ cells: [{ v: "a" }, { v: "b" }] }, { cells: [{ v: "c" }] }] },
  );

  parityCheck(
    "idx and len built-ins",
    [
      `---`,
      "params:",
      "  - items = list(v = str)",
      `---`,
      "> {% for item in items %}",
      "",
      "{{ idx(item) }}/{{ len(items) }}: {{ item.v }}",

      "",
      "> {% /for %}",
    ].join("\n"),
    { items: [{ v: "a" }, { v: "b" }, { v: "c" }] },
  );

  parityCheck(
    "comments stripped",
    `---
params: [name = str]
---
> {# comment #}Hello {{ name }}!`,
    { name: "world" },
  );

  parityCheck(
    "raw block",
    `---
params: []
---
> {% raw %}

{{ literal }}

> {% /raw %}`,
    {},
  );
});
