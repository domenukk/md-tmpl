/**
 * Tests for the md-tmpl TypeScript bindings: variant construction, matching, metadata edge cases and complex scenarios.
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
  TemplateCache,
  defineVariants,
  unitVariant,
  variant,
  match,
  isVariant,
} from "../index.js";
import { defined, withTempFile } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Variant edge cases
// ---------------------------------------------------------------------------

describe("Variant edge cases", () => {
  it("unitVariant has correct tag", () => {
    const v = unitVariant("Active");
    assert.strictEqual(v.__kind__, "Active");
  });

  it("variant with missing field throws", () => {
    const MyVariant = variant("MyVariant", ["field1"]);
    assert.throws(
      () => MyVariant({} as unknown as Record<"field1", unknown>),
      /missing/i,
    );
  });

  it("variant with extra fields logs warning", () => {
    const MyVariant = variant("MyVariant", ["field1"]);
    // Extra fields should be caught at runtime
    assert.throws(
      () =>
        (MyVariant as (v: Record<string, unknown>) => unknown)({
          field1: "ok",
          extraField: "bad",
        }),
      /unexpected/i,
    );
  });

  it("defineVariants creates all variants", () => {
    const { Active, Inactive } = defineVariants({
      Active: null,
      Inactive: null,
    });
    assert.strictEqual(
      (Active as unknown as { __kind__: string }).__kind__,
      "Active",
    );
    assert.strictEqual(
      (Inactive as unknown as { __kind__: string }).__kind__,
      "Inactive",
    );
  });

  it("isVariant checks correctly", () => {
    const v = unitVariant("MyType");
    assert.ok(isVariant(v, "MyType"));
    assert.ok(!isVariant(v, "Other"));
  });

  it("match function dispatches correctly", () => {
    const v = unitVariant("Success");
    const result = match(v, {
      Success: () => "ok",
      Error: () => "fail",
    });
    assert.strictEqual(result, "ok");
  });
});

// ---------------------------------------------------------------------------
// Enum variant construction & pattern matching (comprehensive)
// ---------------------------------------------------------------------------

describe("defineVariants — construction", () => {
  it("creates unit + struct variants in one call", () => {
    const Status = defineVariants({
      Approved: null,
      Rejected: null,
      NeedsChanges: ["reason"],
    });

    // Unit sentinels
    assert.strictEqual(Status.Approved.__kind__, "Approved");
    assert.strictEqual(Status.Rejected.__kind__, "Rejected");
    assert.deepStrictEqual({ ...Status.Approved }, { __kind__: "Approved" });

    // Struct constructor
    const nc = Status.NeedsChanges({ reason: "fix tests" });
    assert.strictEqual(nc.__kind__, "NeedsChanges");
    assert.strictEqual(nc.reason, "fix tests");
    assert.deepStrictEqual(
      { ...nc },
      { __kind__: "NeedsChanges", reason: "fix tests" },
    );
  });

  it("struct variant with multiple fields preserves all fields", () => {
    const Event = defineVariants({
      Click: ["x", "y"],
      Scroll: ["delta"],
    });
    const click = Event.Click({ x: 10, y: 20 });
    assert.strictEqual(click.x, 10);
    assert.strictEqual(click.y, 20);
    assert.deepStrictEqual({ ...click }, { __kind__: "Click", x: 10, y: 20 });
  });

  it("unit variant toString returns tag name", () => {
    const { Approved } = defineVariants({ Approved: null });
    // Variant instances define a non-enumerable custom `toString` not
    // surfaced by the type; assert through a `toString`-bearing view.
    assert.strictEqual(
      (Approved as { toString(): string }).toString(),
      "Approved",
    );
  });

  it("struct variant toString includes fields", () => {
    const { NeedsChanges } = defineVariants({
      NeedsChanges: ["reason"],
    });
    const v = NeedsChanges({ reason: "tests fail" });
    // Variant instances carry a non-enumerable custom `toString`.
    const rendered = (v as { toString(): string }).toString();
    assert.ok(rendered.includes("NeedsChanges"));
    assert.ok(rendered.includes("tests fail"));
  });
});

describe("variant() constructor validation", () => {
  it("throws on missing required field", () => {
    const { NeedsChanges } = defineVariants({
      NeedsChanges: ["reason"],
    });
    assert.throws(
      () => NeedsChanges({}),
      (err: Error) => err.message.includes("missing"),
    );
  });

  it("throws on extra unexpected field", () => {
    const { NeedsChanges } = defineVariants({
      NeedsChanges: ["reason"],
    });
    assert.throws(
      () =>
        (NeedsChanges as (f: Record<string, unknown>) => unknown)({
          reason: "ok",
          extra: "bad",
        }),
      (err: Error) => err.message.includes("unexpected"),
    );
  });

  it("variant instances are frozen (immutable)", () => {
    const { NeedsChanges } = defineVariants({
      NeedsChanges: ["reason"],
    });
    const v = NeedsChanges({ reason: "original" });
    assert.throws(() => {
      (v as Record<string, unknown>).reason = "modified";
    });
  });
});

describe("match() — pattern matching", () => {
  const Status = defineVariants({
    Approved: null,
    Rejected: null,
    NeedsChanges: ["reason"],
  });

  it("dispatches on unit variant", () => {
    const result = match(Status.Approved, {
      Approved: () => "Ship it!",
      NeedsChanges: () => "fix",
      Rejected: () => "no",
    });
    assert.strictEqual(result, "Ship it!");
  });

  it("dispatches on struct variant and passes fields", () => {
    const v = Status.NeedsChanges({ reason: "fix tests" });
    const result = match(v, {
      Approved: () => "ok",
      NeedsChanges: (f) => `Please fix: ${String(f.reason)}`,
      Rejected: () => "no",
    });
    assert.strictEqual(result, "Please fix: fix tests");
  });

  it("uses wildcard _ for unmatched variants", () => {
    const result = match(Status.Rejected, {
      Approved: () => "yes",
      _: () => "something else",
    });
    assert.strictEqual(result, "something else");
  });

  it("throws when no handler and no wildcard", () => {
    assert.throws(
      () =>
        match(Status.NeedsChanges({ reason: "x" }), {
          Approved: () => "ok",
        }),
      /no handler for variant 'NeedsChanges'/,
    );
  });

  it("works with __kind__ objects from generated types", () => {
    const result = match(
      { __kind__: "NeedsChanges", reason: "codegen" },
      {
        Approved: () => "yes",
        NeedsChanges: (f) => `fix: ${String(f.reason)}`,
        _: () => "default",
      },
    );
    assert.strictEqual(result, "fix: codegen");
  });

  it("returns typed result", () => {
    const count: number = match(Status.Approved, {
      Approved: () => 1,
      _: () => 0,
    });
    assert.strictEqual(count, 1);
  });
});

describe("isVariant() — type guard", () => {
  const Status = defineVariants({
    Approved: null,
    Rejected: null,
    NeedsChanges: ["reason"],
  });

  it("returns true for matching unit variant", () => {
    assert.ok(isVariant(Status.Approved, "Approved"));
  });

  it("returns false for non-matching unit variant", () => {
    assert.ok(!isVariant(Status.Approved, "Rejected"));
  });

  it("returns true for matching struct variant", () => {
    const v = Status.NeedsChanges({ reason: "fix" });
    assert.ok(isVariant(v, "NeedsChanges"));
  });

  it("returns false for non-matching struct variant", () => {
    const v = Status.NeedsChanges({ reason: "fix" });
    assert.ok(!isVariant(v, "Approved"));
  });

  it("works with __kind__ objects", () => {
    assert.ok(isVariant({ __kind__: "Approved" }, "Approved"));
    assert.ok(!isVariant({ __kind__: "Approved" }, "Rejected"));
  });

  it("works with plain string variants", () => {
    assert.ok(isVariant("Rejected", "Rejected"));
    assert.ok(!isVariant("Rejected", "Approved"));
  });

  it("returns false for null/undefined/non-variant", () => {
    assert.ok(!isVariant(null, "Anything"));
    assert.ok(!isVariant(undefined, "Anything"));
    assert.ok(!isVariant(42, "Anything"));
    assert.ok(!isVariant({ foo: "bar" }, "Anything"));
  });
});

// ---------------------------------------------------------------------------
// Template metadata edge cases
// ---------------------------------------------------------------------------

describe("Template metadata edge cases", () => {
  it("body() returns the raw body text", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.body(), "Hello {{ name }}!");
  });

  it("consts() returns constant values", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - GREETING = str := "Hi"

params: [name = str]
---
{{ GREETING }} {{ name }}!`,
    );
    const consts = tmpl.consts();
    assert.strictEqual(consts.GREETING, "Hi");
  });

  it("declarations() returns all param declarations", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
  - count = int
---
{{ name }} {{ count }}`,
    );
    const decls = tmpl.declarations();
    assert.strictEqual(decls.length, 2);
    assert.strictEqual(defined(decls[0])[0], "name");
    assert.strictEqual(defined(decls[1])[0], "count");
  });

  it("sourceHash is deterministic", () => {
    const src = `---
params: [x = str]
---
{{ x }}`;
    const t1 = Template.fromSource(src);
    const t2 = Template.fromSource(src);
    assert.strictEqual(t1.sourceHash(), t2.sourceHash());
  });

  it("maxIncludeDepth can be set", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    tmpl.setMaxIncludeDepth(8);
    assert.strictEqual(tmpl.maxIncludeDepth, 8);
  });

  it("frontmatter exposes full metadata", () => {
    const tmpl = Template.fromSource(
      `---
name: "Test"
description: "A test template"
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.frontmatter.name, "Test");
    assert.strictEqual(tmpl.frontmatter.description, "A test template");
  });
});

// ---------------------------------------------------------------------------
// TypedTemplate tests
// ---------------------------------------------------------------------------

describe("TypedTemplate", () => {
  it("renders with typed params", () => {
    const tmpl = TypedTemplate.fromSource<{ name: string }>(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.render({ name: "world" }), "Hello world!");
  });

  it("validates template structure", () => {
    const tmpl = TypedTemplate.fromSource<{ name: string }>(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    // First render triggers validation
    tmpl.render({ name: "test" });
    // Second render reuses validation
    assert.strictEqual(tmpl.render({ name: "world" }), "Hello world!");
  });

  it("fromFile creates typed template", () => {
    withTempFile(
      `---
params: [name = str]
---
Hello {{ name }}!`,
      (filepath) => {
        const tmpl = TypedTemplate.fromFile<{ name: string }>(filepath);
        assert.strictEqual(tmpl.render({ name: "file" }), "Hello file!");
      },
    );
  });

  it("exposes inner template methods", () => {
    const tmpl = TypedTemplate.fromSource<{ name: string }>(
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );
    assert.ok(tmpl.declarations().length > 0);
    assert.ok(tmpl.body().includes("{{ name }}"));
    assert.ok(tmpl.sourceHash() > 0);
  });
});

// ---------------------------------------------------------------------------
// TemplateCache edge cases
// ---------------------------------------------------------------------------

describe("TemplateCache edge cases", () => {
  it("returns same template for same file", () => {
    withTempFile(
      `---
params: [name = str]
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

  it("detects changed file content", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-cache-"));
    const filepath = path.join(dir, "test.tmpl.md");
    fs.writeFileSync(
      filepath,
      `---
params: [name = str]
---
Hello {{ name }}!`,
    );

    try {
      const cache = new TemplateCache();
      const t1 = cache.load(filepath);
      // Modify the file
      fs.writeFileSync(
        filepath,
        `---
params: [name = str]
---
Goodbye {{ name }}!`,
      );
      const t2 = cache.load(filepath);
      // After file change, hash should differ
      assert.notStrictEqual(t1.sourceHash(), t2.sourceHash());
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Complex template scenarios
// ---------------------------------------------------------------------------

describe("Complex template scenarios", () => {
  it("deeply nested struct access", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - config = struct(server = struct(host = str, port = int))
---
{{ config.server.host }}:{{ config.server.port }}`,
    );
    assert.strictEqual(
      tmpl.render({
        config: { server: { host: "localhost", port: 8080 } },
      }),
      "localhost:8080",
    );
  });

  it("multiple for loops in sequence", () => {
    const src = [
      `---`,
      "params:",
      "  - fruits = list(name = str)",
      "  - vegs = list(name = str)",
      `---`,
      "Fruits:",

      "",
      "> {% for f in fruits %}",
      "",
      "- {{ f.name }}",

      "",
      "> {% /for %}",
      "",
      "Vegetables:",

      "",
      "> {% for v in vegs %}",
      "",
      "- {{ v.name }}",

      "",
      "> {% /for %}",
    ].join("\n");

    const result = Template.fromSource(src).render({
      fruits: [{ name: "apple" }, { name: "banana" }],
      vegs: [{ name: "carrot" }],
    });
    assert.ok(result.includes("apple"));
    assert.ok(result.includes("banana"));
    assert.ok(result.includes("carrot"));
  });

  it("if inside for loop", () => {
    const src = [
      `---`,
      "params:",
      "  - items = list(name = str, active = bool)",
      `---`,
      "> {% for item in items %}> {% if item.active %}",
      "",
      "✓ {{ item.name }}",
      "",
      "> {% /if %}{% /for %}",
    ].join("\n");

    const result = Template.fromSource(src).render({
      items: [
        { name: "A", active: true },
        { name: "B", active: false },
        { name: "C", active: true },
      ],
    });
    assert.ok(result.includes("✓ A"));
    assert.ok(!result.includes("B"));
    assert.ok(result.includes("✓ C"));
  });

  it("match inside for loop", () => {
    const src = [
      `---`,
      "params:",
      "  - items = list(status = enum(Ok, Error))",
      `---`,
      "> {% for item in items %}> {% match item.status %}> {% case Ok %}",
      "",
      "✓",

      "",
      "> {% case Error %}",
      "",
      "✗",
      "",
      "> {% /match %}{% /for %}",
    ].join("\n");

    const result = Template.fromSource(src).render({
      items: [{ status: "Ok" }, { status: "Error" }, { status: "Ok" }],
    });
    const lines = result.split("\n").filter((l) => l.trim());
    assert.ok(lines.some((l) => l.includes("✓")));
    assert.ok(lines.some((l) => l.includes("✗")));
  });

  it("enum struct variant with nested struct", () => {
    const src = [
      `---`,
      "params:",
      "  - event = enum(Click(x = int, y = int), Scroll(delta = float))",
      `---`,
      "> {% match event %}",
      "> {% case Click %}",
      "",
      "Click at {{ event.x }},{{ event.y }}",

      "",
      "> {% case Scroll %}",
      "",
      "Scroll by {{ event.delta }}",

      "",
      "> {% /match %}",
    ].join("\n");

    assert.strictEqual(
      Template.fromSource(src).render({
        event: { __kind__: "Click", x: 10, y: 20 },
      }),
      "Click at 10,20\n",
    );

    assert.strictEqual(
      Template.fromSource(src).render({
        event: { __kind__: "Scroll", delta: 3.5 },
      }),
      "Scroll by 3.5\n",
    );
  });

  it("template with all types combined", () => {
    const src = [
      `---`,
      "consts:",
      '  - VERSION = str := "1.0"',
      "",
      "params:",
      "  - name = str",
      "  - count = int",
      "  - score = float",
      "  - active = bool",
      "  - tags = list(str)",
      "  - meta = struct(key = str)",
      "  - status = enum(Ok, Error)",
      `---`,
      "v{{ VERSION }} {{ name }} #{{ count }} ({{ score | fixed(1) }})",

      "",
      "> {% if active %}",
      "",
      "Active",

      "",
      "> {% /if %}",
      "",
      'Tags: {{ tags | join(", ") }}',
      "Meta: {{ meta.key }}",

      "",
      "> {% match status %}",
      "> {% case Ok %}",
      "",
      "OK",

      "",
      "> {% case Error %}",
      "",
      "ERR",

      "",
      "> {% /match %}",
    ].join("\n");

    const result = Template.fromSource(src).render({
      name: "test",
      count: 42,
      score: 3.14,
      active: true,
      tags: ["a", "b"],
      meta: { key: "val" },
      status: "Ok",
    });
    assert.ok(result.includes("v1.0"));
    assert.ok(result.includes("test"));
    assert.ok(result.includes("#42"));
    assert.ok(result.includes("3.1"));
    assert.ok(result.includes("Active"));
    assert.ok(result.includes("a, b"));
    assert.ok(result.includes("val"));
    assert.ok(result.includes("OK"));
  });
});
