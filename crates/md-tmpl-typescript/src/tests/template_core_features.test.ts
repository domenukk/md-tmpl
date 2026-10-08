/**
 * Tests for the md-tmpl TypeScript bindings: filters, built-in functions, variant helpers, value module and frontmatter parsing.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template, defineVariants, unitVariant, variant } from "../index.js";
import { fromJs, display, isTruthy } from "../value.js";
import {
  parseFrontmatter,
  parseVarType,
  varTypeToString,
  TYPE_LIST,
} from "../frontmatter.js";
import { joinContinuationLines } from "../frontmatter/yaml.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------------

describe("Filters", () => {
  it("upper filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [msg = str]
---
{{ msg | upper }}`,
    );
    assert.strictEqual(tmpl.render({ msg: "hello" }), "HELLO");
  });

  it("lower filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [msg = str]
---
{{ msg | lower }}`,
    );
    assert.strictEqual(tmpl.render({ msg: "HELLO" }), "hello");
  });

  it("trim filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [msg = str]
---
{{ msg | trim }}`,
    );
    assert.strictEqual(tmpl.render({ msg: "  hello  " }), "hello");
  });

  it("fixed filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = float]
---
{{ val | fixed(2) }}`,
    );
    assert.strictEqual(tmpl.render({ val: 3.14159 }), "3.14");
  });

  it("join filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [items = list(str)]
---
{{ items | join(", ") }}`,
    );
    assert.strictEqual(tmpl.render({ items: ["a", "b", "c"] }), "a, b, c");
  });

  it("limit filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [items = list(str)]
---
{{ items | limit(2) | join(", ") }}`,
    );
    assert.strictEqual(tmpl.render({ items: ["a", "b", "c"] }), "a, b");
  });

  it("add filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [n = int]
---
{{ n | add(1) }}`,
    );
    assert.strictEqual(tmpl.render({ n: 9 }), "10");
  });

  it("sub filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [n = int]
---
{{ n | sub(1) }}`,
    );
    assert.strictEqual(tmpl.render({ n: 10 }), "9");
  });
});

// ---------------------------------------------------------------------------
// Built-in functions
// ---------------------------------------------------------------------------

describe("Built-in functions", () => {
  it("len() for list", () => {
    const tmpl = Template.fromSource(
      `---
params: [items = list(x = str)]
---
{{ len(items) }}`,
    );
    assert.strictEqual(tmpl.render({ items: [{ x: "a" }, { x: "b" }] }), "2");
  });

  it("len() for string", () => {
    const tmpl = Template.fromSource(
      `---
params: [msg = str]
---
{{ len(msg) }}`,
    );
    assert.strictEqual(tmpl.render({ msg: "hello" }), "5");
  });

  it("idx() in for loop", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(label = str)
---
> {% for item in items %}

{{ idx(item) | add(1) }}. {{ item.label }}

> {% /for %}`,
    );
    const output = tmpl.render({
      items: [{ label: "first" }, { label: "second" }],
    });
    assert.ok(output.includes("1. first"));
    assert.ok(output.includes("2. second"));
  });

  it("kind() extracts variant name", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
{{ kind(outcome) }}`,
    );
    const output = tmpl.render({
      outcome: { __kind__: "Confirmed", evidence: "proof" },
    });
    assert.strictEqual(output.trim(), "Confirmed");
  });
});

// ---------------------------------------------------------------------------
// Variant helpers
// ---------------------------------------------------------------------------

describe("Variant helpers", () => {
  it("unitVariant creates sentinel", () => {
    const v = unitVariant("Rejected");
    assert.strictEqual(v.__kind__, "Rejected");
    assert.deepStrictEqual({ ...v }, { __kind__: "Rejected" });
  });

  it("variant() creates constructor with fields", () => {
    const NeedsChanges = variant("NeedsChanges", ["reason"] as const);
    const v = NeedsChanges({ reason: "fix tests" });
    assert.strictEqual(v.__kind__, "NeedsChanges");
    assert.strictEqual(v.reason, "fix tests");
  });

  it("variant() throws on missing field", () => {
    const Required = variant("Required", ["value"] as const);
    assert.throws(
      () => Required({} as Record<"value", unknown>),
      (err: Error) => err.message.includes("missing"),
    );
  });

  it("variant() throws on unexpected field", () => {
    const Simple = variant("Simple", ["x"] as const);
    assert.throws(
      () =>
        (Simple as (f: Record<string, unknown>) => unknown)({
          x: 1,
          y: 2,
        }),
      (err: Error) => err.message.includes("unexpected"),
    );
  });

  it("defineVariants creates mixed enum", () => {
    const Status = defineVariants({
      Approved: null,
      Rejected: null,
      NeedsChanges: ["reason"],
    });
    assert.strictEqual(Status.Approved.__kind__, "Approved");
    const nc = Status.NeedsChanges({ reason: "fix" });
    assert.strictEqual(nc.__kind__, "NeedsChanges");
    assert.strictEqual(nc.reason, "fix");
  });

  it("variant objects render correctly", () => {
    const Status = defineVariants({
      Confirmed: ["evidence"],
      Rejected: null,
    });

    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - outcome = enum(Confirmed(evidence = str), Rejected)",
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
        "> {% /match %}",
      ].join("\n"),
    );

    assert.strictEqual(tmpl.render({ outcome: Status.Rejected }).trim(), "NO");
    assert.strictEqual(
      tmpl.render({ outcome: Status.Confirmed({ evidence: "proof" }) }).trim(),
      "YES: proof",
    );
  });
});

// ---------------------------------------------------------------------------
// Value module
// ---------------------------------------------------------------------------

describe("Value module", () => {
  it("fromJs converts string", () => {
    const v = fromJs("hello");
    assert.strictEqual(v.type, "str");
    assert.strictEqual(display(v), "hello");
  });

  it("fromJs converts integer", () => {
    const v = fromJs(42);
    assert.strictEqual(v.type, "int");
    assert.strictEqual(display(v), "42");
  });

  it("fromJs converts float", () => {
    const v = fromJs(3.14);
    assert.strictEqual(v.type, "float");
    assert.strictEqual(display(v), "3.14");
  });

  it("fromJs converts boolean", () => {
    const v = fromJs(true);
    assert.strictEqual(v.type, "bool");
    assert.strictEqual(display(v), "true");
  });

  it("fromJs converts array", () => {
    const v = fromJs(["a", "b"]);
    assert.strictEqual(v.type, "list");
  });

  it("fromJs converts object", () => {
    const v = fromJs({ key: "val" });
    assert.strictEqual(v.type, "struct");
  });

  it("isTruthy works correctly", () => {
    assert.strictEqual(isTruthy(fromJs(true)), true);
    assert.strictEqual(isTruthy(fromJs(false)), false);
    assert.strictEqual(isTruthy(fromJs("")), false);
    assert.strictEqual(isTruthy(fromJs("hello")), true);
    assert.strictEqual(isTruthy(fromJs(0)), false);
    assert.strictEqual(isTruthy(fromJs(1)), true);
  });
});

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

describe("Frontmatter parsing", () => {
  it("parses params and body", () => {
    const [fm, body] = parseFrontmatter(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.strictEqual(fm.params.length, 1);
    assert.strictEqual(defined(fm.params[0]).name, "name");
    assert.ok(body.includes("Hello"));
  });

  it("parseVarType round-trips", () => {
    for (const t of ["str", "bool", "int", "float"]) {
      const vt = parseVarType(t);
      assert.strictEqual(varTypeToString(vt), t);
    }
  });

  it("parses inline list syntax", () => {
    const [fm] = parseFrontmatter(
      `---
params: [x = str, y = int]
---
{{ x }} {{ y }}`,
    );
    assert.strictEqual(fm.params.length, 2);
    assert.strictEqual(defined(fm.params[0]).name, "x");
    assert.strictEqual(defined(fm.params[1]).name, "y");
  });

  it("joins a multi-line := default value (list) onto one declaration", () => {
    // Regression: a `:=` value whose list literal spans several physical
    // lines must be captured in full, not truncated to an empty default.
    const [fm] = parseFrontmatter(
      `---
consts:
  - LADDER = list(tier = str, note = str) :=
    [{tier = "L0", note = "analysis"},
    {tier = "L1", note = "trigger"},
    {tier = "L2", note = "control"}]

params: []
---
body`,
    );
    assert.strictEqual(fm.consts.length, 1);
    const ladder = defined(fm.consts[0]);
    assert.strictEqual(ladder.name, "LADDER");
    assert.ok(ladder.defaultValue !== undefined, "default must be parsed");
    assert.strictEqual(ladder.defaultValue.type, TYPE_LIST);
    assert.strictEqual(
      (ladder.defaultValue as { items: readonly unknown[] }).items.length,
      3,
    );
  });

  it("does not let comments/blank lines break a continuation", () => {
    // Mirrors the ARTIST template layout: a documented const whose multi-line
    // value follows `:=` on subsequent indented lines.
    const [fm] = parseFrontmatter(
      `---
consts:
  - FIRST = str := "one"

  # A documented, canonical list.
  # Spanning several comment lines.
  - SECOND = list(k = str) :=
    [{k = "a"},
    {k = "b"}]

params: []
---
body`,
    );
    assert.strictEqual(fm.consts.length, 2);
    assert.strictEqual(defined(fm.consts[0]).name, "FIRST");
    const second = defined(fm.consts[1]);
    assert.strictEqual(second.name, "SECOND");
    assert.strictEqual(defined(second.defaultValue).type, TYPE_LIST);
    assert.strictEqual(
      (second.defaultValue as { items: readonly unknown[] }).items.length,
      2,
    );
  });

  it("joins a multi-line compound type declaration", () => {
    const [fm] = parseFrontmatter(
      `---
params:
  - rows = list(tier = str,
    short = str,
    note = str)
---
body`,
    );
    assert.strictEqual(fm.params.length, 1);
    assert.strictEqual(defined(fm.params[0]).name, "rows");
    const ty = varTypeToString(defined(fm.params[0]).varType);
    assert.ok(ty.startsWith("list("), `expected a list type, got: ${ty}`);
    assert.ok(ty.includes("tier = str"));
    assert.ok(ty.includes("short = str"));
    assert.ok(ty.includes("note = str"));
  });

  it("joinContinuationLines merges only non-item, non-section lines", () => {
    const joined = joinContinuationLines([
      "consts:",
      '  - A = str := "x"',
      "  - B = list(k = str) :=",
      '    [{k = "a"},',
      '    {k = "b"}]',
    ]);
    assert.deepStrictEqual(joined, [
      "consts:",
      '  - A = str := "x"',
      '  - B = list(k = str) := [{k = "a"}, {k = "b"}]',
    ]);
  });
});
