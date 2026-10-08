/**
 * Tests for the md-tmpl TypeScript bindings: duck typing and match on strings / match type safety.
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
// Duck typing / structural typing — extra fields in struct values
// ---------------------------------------------------------------------------

describe("Duck typing / structural typing", () => {
  it("struct param with extra fields renders correctly", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`,
    );
    // Extra field "protocol" should be silently ignored
    const output = tmpl.render({
      config: { host: "localhost", port: 8080, protocol: "https" },
    });
    assert.strictEqual(output, "localhost:8080");
  });

  it("struct with many extra fields still renders declared fields", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - user = struct(name = str, active = bool)
---
{{ user.name }} is {{ user.active }}`,
    );
    const output = tmpl.render({
      user: {
        name: "Alice",
        active: true,
        email: "alice@example.com",
        age: 30,
        role: "admin",
      },
    });
    assert.strictEqual(output, "Alice is true");
  });

  it("list items with extra fields render correctly", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - tasks = list(title = str, priority = str)
---
> {% for task in tasks %}

- {{ task.title }} ({{ task.priority }})

> {% /for %}`,
    );
    const output = tmpl.render({
      tasks: [
        { title: "Write docs", priority: "High", assignee: "Alice", id: 1 },
        {
          title: "Add tests",
          priority: "Medium",
          status: "open",
          tags: ["test"],
        },
      ],
    });
    assert.ok(output.includes("Write docs"));
    assert.ok(output.includes("High"));
    assert.ok(output.includes("Add tests"));
    assert.ok(output.includes("Medium"));
    // Extra fields should not appear in output
    assert.ok(!output.includes("Alice"));
    assert.ok(!output.includes("assignee"));
  });

  it("type alias (types: block) with duck typing", () => {
    const tmpl = Template.fromSource(`---
types:
  - Config = struct(host = str, port = int)

params:
  - server = Config
---
{{ server.host }}:{{ server.port }}`);
    // Extra "timeout" field should be silently ignored
    const output = tmpl.render({
      server: { host: "example.com", port: 443, timeout: 30 },
    });
    assert.strictEqual(output, "example.com:443");
  });

  it("type alias list with extra fields in items", () => {
    const tmpl = Template.fromSource(`---
types:
  - Entry = struct(label = str, value = int)

params:
  - entries = list(Entry)
---
> {% for e in entries %}{{ e.label }}={{ e.value }} {% /for %}`);
    const output = tmpl.render({
      entries: [
        { label: "x", value: 10, color: "red" },
        { label: "y", value: 20, extra: true },
      ],
    });
    assert.ok(output.includes("x=10"));
    assert.ok(output.includes("y=20"));
  });

  it("nested struct with extra fields at every depth", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - outer = struct(inner = struct(value = str))
---
{{ outer.inner.value }}`,
    );
    const output = tmpl.render({
      outer: {
        inner: { value: "deep", extra_inner: 999, debug: true },
        extra_outer: "ignored",
        metadata: { foo: "bar" },
      },
    });
    assert.strictEqual(output, "deep");
  });

  it("deeply nested struct (3 levels) with extra fields", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - a = struct(b = struct(c = struct(name = str)))
---
{{ a.b.c.name }}`,
    );
    const output = tmpl.render({
      a: {
        b: {
          c: { name: "leaf", z: 42 },
          extra_b: "bval",
        },
        extra_a: "aval",
      },
    });
    assert.strictEqual(output, "leaf");
  });

  it("struct in conditional with extra fields", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - config = struct(enabled = bool, label = str)
---
> {% if config.enabled %}

{{ config.label }}

> {% /if %}`,
    );
    const output = tmpl.render({
      config: { enabled: true, label: "ON", priority: 1, debug: false },
    });
    assert.ok(output.includes("ON"));
  });

  it("struct with extra fields still rejects missing declared fields", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`,
    );
    // Has extra "protocol" but is missing required "port"
    assert.throws(
      () => tmpl.render({ config: { host: "localhost", protocol: "https" } }),
      (err: Error) => err.message.includes("missing"),
    );
  });

  it("list with nested struct items containing extra fields", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(meta = struct(key = str))
---
> {% for item in items %}{{ item.meta.key }} {% /for %}`,
    );
    const output = tmpl.render({
      items: [
        { meta: { key: "a", extra: true }, other: 1 },
        { meta: { key: "b", debug: "yes" }, tag: "x" },
      ],
    });
    assert.ok(output.includes("a"));
    assert.ok(output.includes("b"));
  });

  it("enum variant struct with extra fields", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - result = enum(Ok(value = str), Err)
---
> {% match result %}
> {% case Ok %}

{{ result.value }}

> {% case Err %}

error

> {% /match %}`,
    );
    // Extra "code" field on the Ok variant should be silently ignored
    const output = tmpl.render({
      result: { __kind__: "Ok", value: "success", code: 200 },
    });
    assert.ok(output.trim().includes("success"));
  });
});

// ---------------------------------------------------------------------------
// Duck typing — include with partial struct (child uses subset of fields)
// ---------------------------------------------------------------------------

describe("Duck typing — include with partial struct", () => {
  it("include child that declares subset of parent struct fields", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-duck-inc-"));
    try {
      // Child only declares "name" but parent passes a struct with name + age
      fs.writeFileSync(
        path.join(dir, "child.tmpl.md"),
        `---
params:
  - user = struct(name = str)
---
Hello {{ user.name }}`,
      );
      const src = [
        `---`,
        "params:",
        "  - user = struct(name = str, age = int)",
        `---`,
        "> {% include [child](./child.tmpl.md) with user=user %}",
      ].join("\n");
      const tmpl = Template.fromSourceWithBaseDir(src, dir);
      const result = tmpl.render({
        user: { name: "Alice", age: 30 },
      });
      assert.ok(
        result.includes("Hello Alice"),
        `expected 'Hello Alice' but got: ${result}`,
      );
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("for-each include where child uses subset of list item fields", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-duck-for-"));
    try {
      // Child only declares name (subset of name + score)
      fs.writeFileSync(
        path.join(dir, "row.tmpl.md"),
        `---
params:
  - item = struct(name = str)
---
- {{ item.name }}`,
      );
      const mainSrc = [
        `---`,
        "params:",
        "  - items = list(name = str, score = int)",
        `---`,
        "> {% include [row](./row.tmpl.md) for item in items %}",
      ].join("\n");
      const tmpl = Template.fromSourceWithBaseDir(mainSrc, dir);
      const result = tmpl.render({
        items: [
          { name: "Alpha", score: 100, extra: "ignored" },
          { name: "Beta", score: 200, debug: true },
        ],
      });
      assert.ok(result.includes("Alpha"), `missing Alpha in: ${result}`);
      assert.ok(result.includes("Beta"), `missing Beta in: ${result}`);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("include child with extra fields on value passed to subset struct", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-duck-extra-"));
    try {
      // Child declares struct with 1 field
      fs.writeFileSync(
        path.join(dir, "detail.tmpl.md"),
        `---
params:
  - info = struct(title = str)
---
Title: {{ info.title }}`,
      );
      const src = [
        `---`,
        "params:",
        "  - info = struct(title = str, author = str, year = int)",
        `---`,
        "> {% include [detail](./detail.tmpl.md) with info=info %}",
        "",
        "By: {{ info.author }} ({{ info.year }})",
      ].join("\n");
      const tmpl = Template.fromSourceWithBaseDir(src, dir);
      const result = tmpl.render({
        info: { title: "The Book", author: "Jane", year: 2025, isbn: "123" },
      });
      assert.ok(
        result.includes("Title: The Book"),
        `expected title in: ${result}`,
      );
      assert.ok(result.includes("By: Jane"), `expected author in: ${result}`);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Match / Case on strings (quoted case labels)
// ---------------------------------------------------------------------------

describe("Match on strings (quoted case labels)", () => {
  it("matches a multi-arm string match", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - status = str",
        `---`,
        `> {% match status %}`,
        `> {% case "Active" %}`,
        "",
        "currently active",
        "",
        `> {% case "Paused" %}`,
        "",
        "on hold",
        "",
        `> {% /match %}`,
      ].join("\n"),
    );
    assert.strictEqual(
      tmpl.render({ status: "Active" }).trim(),
      "currently active",
    );
    assert.strictEqual(tmpl.render({ status: "Paused" }).trim(), "on hold");
  });

  it("inline match with quoted case", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - role = str
---
> {% match role case "Admin" %}admin panel{% /match %}`,
    );
    assert.strictEqual(tmpl.render({ role: "Admin" }).trim(), "admin panel");
    assert.strictEqual(tmpl.render({ role: "User" }).trim(), "");
  });

  it("multi-value string arm", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - level = str",
        `---`,
        `> {% match level %}`,
        `> {% case "High" | "Critical" %}`,
        "",
        "urgent",
        "",
        `> {% case "Low" %}`,
        "",
        "fine",
        "",
        `> {% /match %}`,
      ].join("\n"),
    );
    assert.strictEqual(tmpl.render({ level: "High" }).trim(), "urgent");
    assert.strictEqual(tmpl.render({ level: "Critical" }).trim(), "urgent");
    assert.strictEqual(tmpl.render({ level: "Low" }).trim(), "fine");
  });

  it("string match with else fallback", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - color = str",
        `---`,
        `> {% match color %}`,
        `> {% case "red" %}`,
        "",
        "stop",
        "",
        `> {% case "green" %}`,
        "",
        "go",
        "",
        `> {% else %}`,
        "",
        "unknown color",
        "",
        `> {% /match %}`,
      ].join("\n"),
    );
    assert.strictEqual(tmpl.render({ color: "red" }).trim(), "stop");
    assert.strictEqual(tmpl.render({ color: "green" }).trim(), "go");
    assert.strictEqual(tmpl.render({ color: "blue" }).trim(), "unknown color");
  });

  it("single-quoted string case labels", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - mode = str",
        `---`,
        `> {% match mode %}`,
        `> {% case 'fast' %}`,
        "",
        "speed mode",
        "",
        `> {% case 'slow' %}`,
        "",
        "safe mode",
        "",
        `> {% /match %}`,
      ].join("\n"),
    );
    assert.strictEqual(tmpl.render({ mode: "fast" }).trim(), "speed mode");
    assert.strictEqual(tmpl.render({ mode: "slow" }).trim(), "safe mode");
  });
});

// ---------------------------------------------------------------------------
// Match type safety — compile-time errors for str↔enum confusion
// ---------------------------------------------------------------------------

describe("Match type safety (compile-time)", () => {
  it("allows unquoted case labels on str param (matches literally)", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - status = str",
        `---`,
        `> {% match status %}`,
        `> {% case Active %}`,
        "",
        "active",
        "",
        `> {% /match %}`,
      ].join("\n"),
    );
    assert.strictEqual(tmpl.render({ status: "Active" }).trim(), "active");
    assert.strictEqual(tmpl.render({ status: "Other" }).trim(), "");
  });

  it("allows unquoted inline case on str param (matches literally)", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - role = str
---
> {% match role case Admin %}admin{% /match %}`,
    );
    assert.strictEqual(tmpl.render({ role: "Admin" }).trim(), "admin");
    assert.strictEqual(tmpl.render({ role: "User" }).trim(), "");
  });

  it("counts an unquoted case label that matches a declared param as a reference", () => {
    // `{% case expected %}` on a str match reads the param `expected`'s value
    // at runtime for comparison — the param IS used. The collector must count
    // it, so no `allow_unused` is needed and no unused-param error is thrown.
    const tmpl = Template.fromSource(
      `---
params:
  - status = str
  - expected = str
---
> {% match status %}{% case expected %}matched{% else %}nope{% /match %}`,
    );
    // Runtime: param-ref match — `expected`'s value is compared to `status`.
    assert.strictEqual(
      tmpl.render({ status: "Active", expected: "Active" }).trim(),
      "matched",
    );
    assert.strictEqual(
      tmpl.render({ status: "Active", expected: "Paused" }).trim(),
      "nope",
    );
  });

  it("rejects quoted case labels on enum param", () => {
    assert.throws(
      () =>
        Template.fromSource(
          [
            `---`,
            "params:",
            "  - status = enum(Active, Inactive)",
            `---`,
            `> {% match status %}`,
            `> {% case "Active" %}`,
            "",
            "active",
            "",
            `> {% /match %}`,
          ].join("\n"),
        ),
      (err: Error) =>
        err.message.includes("quoted string") &&
        err.message.includes("enum variants"),
    );
  });

  it("allows unquoted case labels on enum param", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - status = enum(Active, Inactive)",
        `---`,
        `> {% match status %}`,
        `> {% case Active %}`,
        "",
        "on",
        "",
        `> {% case Inactive %}`,
        "",
        "off",
        "",
        `> {% /match %}`,
      ].join("\n"),
    );
    assert.strictEqual(tmpl.render({ status: "Active" }).trim(), "on");
  });

  it("allows quoted case labels on str param", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - role = str",
        `---`,
        `> {% match role %}`,
        `> {% case "Admin" %}`,
        "",
        "admin",
        "",
        `> {% case "User" %}`,
        "",
        "user",
        "",
        `> {% /match %}`,
      ].join("\n"),
    );
    assert.strictEqual(tmpl.render({ role: "Admin" }).trim(), "admin");
  });
});
