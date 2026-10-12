/**
 * Tests for the md-tmpl TypeScript bindings: nested types, TypedTemplate, renderUnchecked, match() and isVariant().
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  Template,
  TypedTemplate,
  defineVariants,
  match,
  isVariant,
} from "../index.js";
import { generateTypes, inferTypes } from "../codegen.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Nested types
// ---------------------------------------------------------------------------

describe("Nested types", () => {
  it("renders nested list of structs with inner fields", () => {
    const src = [
      `---`,
      "params:",
      "  - sections = list(heading = str, items = list(label = str))",
      `---`,
      "> {% for section in sections %}",
      "",
      "## {{ section.heading }}",

      "",
      "> {% for item in section.items %}",
      "",
      "- {{ item.label }}",

      "",
      "> {% /for %}",
      "> {% /for %}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    const result = tmpl.render({
      sections: [
        { heading: "A", items: [{ label: "one" }, { label: "two" }] },
        { heading: "B", items: [{ label: "three" }] },
      ],
    });
    assert.ok(result.includes("## A"));
    assert.ok(result.includes("- one"));
    assert.ok(result.includes("- two"));
    assert.ok(result.includes("## B"));
    assert.ok(result.includes("- three"));
  });

  it("renders struct with nested struct", () => {
    const src = [
      `---`,
      "params:",
      "  - config = struct(db = struct(host = str, port = int))",
      `---`,
      "{{ config.db.host }}:{{ config.db.port }}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    const result = tmpl.render({
      config: { db: { host: "localhost", port: 5432 } },
    });
    assert.strictEqual(result, "localhost:5432");
  });
});

// ---------------------------------------------------------------------------
// TypedTemplate
// ---------------------------------------------------------------------------

describe("TypedTemplate", () => {
  it("renders with typed params", () => {
    interface MyParams {
      readonly name: string;
      readonly count: number;
    }
    const tmpl = TypedTemplate.fromSource<MyParams>(
      `---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`,
    );
    const result = tmpl.render({ name: "Alice", count: 42 });
    assert.strictEqual(result, "Alice: 42");
  });

  it("wraps an existing Template", () => {
    interface P {
      readonly x: string;
    }
    const inner = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    const typed = TypedTemplate.wrap<P>(inner);
    assert.strictEqual(typed.render({ x: "hello" }), "hello");
  });

  it("exposes metadata via inner template", () => {
    interface P {
      readonly x: string;
    }
    const tmpl = TypedTemplate.fromSource<P>(
      `---
params: [x = str]
---
{{ x }}`,
    );
    assert.strictEqual(tmpl.declarations().length, 1);
    assert.ok(tmpl.sourceHash() > 0);
    assert.ok(tmpl.body().includes("{{ x }}"));
    assert.ok(tmpl.toString().includes("Template"));
  });

  it("defaults returns partial of P", () => {
    interface P {
      readonly name: string;
      readonly count: number;
    }
    const tmpl = TypedTemplate.fromSource<P>(
      `---
params:
  - name = str := "World"
  - count = int
---
{{ name }} {{ count }}`,
    );
    const defs = tmpl.defaults();
    assert.strictEqual(defs.name, "World");
    assert.strictEqual(defs.count, undefined);
  });

  it("renderUnchecked produces correct output", () => {
    interface P {
      readonly x: string;
    }
    const tmpl = TypedTemplate.fromSource<P>(
      `---
params: [x = str]
---
{{ x }}`,
    );
    assert.strictEqual(tmpl.renderUnchecked({ x: "fast" }), "fast");
  });

  it("renderTrusted validates on first call", () => {
    interface P {
      readonly x: string;
    }
    const tmpl = TypedTemplate.fromSource<P>(
      `---
params: [x = str]
---
{{ x }}`,
    );
    // First call: validated
    assert.strictEqual(tmpl.renderTrusted({ x: "first" }), "first");
    // Subsequent calls: unchecked (but still correct)
    assert.strictEqual(tmpl.renderTrusted({ x: "second" }), "second");
    assert.strictEqual(tmpl.renderTrusted({ x: "third" }), "third");
  });

  it("renderTrusted rejects bad types on first call", () => {
    interface P {
      readonly x: string;
    }
    const tmpl = TypedTemplate.fromSource<P>(
      `---
params: [x = bool]
---
{{ x }}`,
    );
    // First call with wrong type should throw
    assert.throws(() => tmpl.renderTrusted({ x: "not a bool" }));
  });
});

// ---------------------------------------------------------------------------
// Template.renderUnchecked
// ---------------------------------------------------------------------------

describe("Template.renderUnchecked", () => {
  it("renders without type checking", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }}: {{ count }}`,
    );
    assert.strictEqual(
      tmpl.renderUnchecked({ name: "Bob", count: 7 }),
      "Bob: 7",
    );
  });

  it("renders with defaults when params omitted", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str := "World"
---
Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.renderUnchecked({}), "Hello World!");
  });

  it("is faster than render for list templates", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str, score = int)
---
> {% for item in items %}

{{ item.name }}: {{ item.score }}

> {% /for %}`,
    );
    const data = {
      items: Array.from({ length: 10 }, (_, i) => ({
        name: `item${String(i)}`,
        score: i * 10,
      })),
    };

    // Warmup JIT and inline caches
    for (let i = 0; i < 100; i++) {
      tmpl.render(data);
      tmpl.renderUnchecked(data);
    }

    const start = performance.now();
    for (let i = 0; i < 500; i++) {
      tmpl.renderUnchecked(data);
    }
    const uncheckedMs = performance.now() - start;

    const start2 = performance.now();
    for (let i = 0; i < 500; i++) {
      tmpl.render(data);
    }
    const checkedMs = performance.now() - start2;

    // renderUnchecked should be faster for list templates
    assert.ok(
      uncheckedMs < checkedMs,
      `renderUnchecked (${uncheckedMs.toFixed(2)}ms) should be faster than render (${checkedMs.toFixed(2)}ms) for list templates`,
    );
  });
});

// ---------------------------------------------------------------------------
// Concurrent / reuse
// ---------------------------------------------------------------------------

describe("Template reuse", () => {
  it("renders same template many times with different params", () => {
    const tmpl = Template.fromSource(`---
params: [n = int]
---
{{ n }}`);
    for (let i = 0; i < 100; i++) {
      assert.strictEqual(tmpl.render({ n: i }), String(i));
    }
  });

  it("multiple templates can coexist", () => {
    const t1 = Template.fromSource(`---
params: [x = str]
---
a={{ x }}`);
    const t2 = Template.fromSource(`---
params: [y = int]
---
b={{ y }}`);
    assert.strictEqual(t1.render({ x: "hi" }), "a=hi");
    assert.strictEqual(t2.render({ y: 42 }), "b=42");
  });
});

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

describe("Comments", () => {
  it("strips inline comments", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = str]
---
Before{# this is a comment #}After {{ x }}`,
    );
    const result = tmpl.render({ x: "!" });
    assert.ok(result.includes("BeforeAfter"));
    assert.ok(!result.includes("this is a comment"));
  });

  it("strips multi-word comments", () => {
    const tmpl = Template.fromSource(`---
params: []
---
A{# remove me #}B`);
    assert.strictEqual(tmpl.render({}), "AB");
  });
});

// ---------------------------------------------------------------------------
// End-to-end: generate types → use with TypedTemplate
// ---------------------------------------------------------------------------

describe("End-to-end type workflow", () => {
  const TEMPLATE_SRC = [
    `---`,
    "params:",
    "  - title = str",
    "  - count = int",
    "  - items = list(label = str, done = bool)",
    "  - outcome = enum(Success(msg = str), Failure)",
    `---`,
    "# {{ title }} ({{ count }})",

    "",
    "> {% for item in items %}",
    "",
    "- {{ item.label }}: {{ item.done }}",

    "",
    "> {% /for %}",
    "> {% match outcome %}",
    "> {% case Success %}",
    "",
    "OK: {{ outcome.msg }}",

    "",
    "> {% case Failure %}",
    "",
    "FAIL",

    "",
    "> {% /match %}",
  ].join("\n");

  it("generateTypes produces valid code for complex template", () => {
    const code = generateTypes(TEMPLATE_SRC);
    // Check all types are generated
    assert.ok(code.includes("interface Params"));
    assert.ok(code.includes("readonly title: string"));
    assert.ok(code.includes("readonly count: number"));
    assert.ok(code.includes("interface ItemsItem"));
    assert.ok(code.includes("readonly label: string"));
    assert.ok(code.includes("readonly done: boolean"));
    assert.ok(code.includes("type Outcome"));
    assert.ok(code.includes('"Failure"'));
    assert.ok(code.includes("Outcome_Success"));
    assert.ok(code.includes('readonly __kind__: "Success"'));
    assert.ok(code.includes("readonly msg: string"));
  });

  it("inferTypes matches template structure", () => {
    const result = inferTypes(TEMPLATE_SRC);
    assert.strictEqual(result.fields.length, 4);
    assert.strictEqual(defined(result.fields[0]).name, "title");
    assert.strictEqual(defined(result.fields[0]).tsType, "string");
    assert.strictEqual(defined(result.fields[1]).name, "count");
    assert.strictEqual(defined(result.fields[1]).tsType, "number");
    // items should be an array type
    assert.ok(defined(result.fields[2]).tsType.includes("label: string"));
    assert.ok(defined(result.fields[2]).tsType.includes("done: boolean"));
    // outcome should be a union type
    assert.ok(defined(result.fields[3]).tsType.includes('"Failure"'));
    assert.ok(defined(result.fields[3]).tsType.includes('__kind__: "Success"'));
  });

  it("TypedTemplate works with inferred type structure", () => {
    // Simulate what a user would do after running generateTypes
    interface ItemsItem {
      readonly label: string;
      readonly done: boolean;
    }
    interface Outcome_Success {
      readonly __kind__: "Success";
      readonly msg: string;
    }
    type Outcome = Outcome_Success | "Failure";
    interface Params {
      readonly title: string;
      readonly count: number;
      readonly items: readonly ItemsItem[];
      readonly outcome: Outcome;
    }

    const tmpl = TypedTemplate.fromSource<Params>(TEMPLATE_SRC);

    const result = tmpl.render({
      title: "Report",
      count: 2,
      items: [
        { label: "Task A", done: true },
        { label: "Task B", done: false },
      ],
      outcome: { __kind__: "Success", msg: "all good" },
    });

    assert.ok(result.includes("# Report (2)"));
    assert.ok(result.includes("- Task A: true"));
    assert.ok(result.includes("- Task B: false"));
    assert.ok(result.includes("OK: all good"));
  });

  it("TypedTemplate rejects wrong params at runtime", () => {
    interface Params {
      readonly title: string;
      readonly count: number;
      readonly items: readonly {
        readonly label: string;
        readonly done: boolean;
      }[];
      readonly outcome:
        { readonly __kind__: "Success"; readonly msg: string } | "Failure";
    }

    const tmpl = TypedTemplate.fromSource<Params>(TEMPLATE_SRC);

    // Missing required param
    assert.throws(() =>
      tmpl.render({
        title: "X",
        count: 1,
        items: [],
      } as unknown as Params),
    );
  });

  it("frontmatter accessor exposes parsed type info", () => {
    const tmpl = Template.fromSource(TEMPLATE_SRC);
    const fm = tmpl.frontmatter;
    assert.strictEqual(fm.params.length, 4);
    const titleParam = defined(fm.params[0]);
    assert.strictEqual(titleParam.name, "title");
    assert.strictEqual(titleParam.varType.kind, "str");
    const itemsParam = defined(fm.params[2]);
    assert.strictEqual(itemsParam.name, "items");
    assert.strictEqual(itemsParam.varType.kind, "list");
    const outcomeParam = defined(fm.params[3]);
    assert.strictEqual(outcomeParam.name, "outcome");
    // strictEqual narrows the discriminant, so `.variants` is accessible.
    assert.strictEqual(outcomeParam.varType.kind, "enum");
    assert.strictEqual(outcomeParam.varType.variants.length, 2);
    assert.strictEqual(
      defined(outcomeParam.varType.variants[0]).name,
      "Success",
    );
    assert.strictEqual(
      defined(outcomeParam.varType.variants[1]).name,
      "Failure",
    );
  });
});

// ---------------------------------------------------------------------------
// Error messages
// ---------------------------------------------------------------------------

describe("Error messages", () => {
  it("MissingParamsError lists missing param names", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - a = str
  - b = int
---
{{ a }} {{ b }}`,
    );
    try {
      tmpl.render({});
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(err instanceof Error);
      assert.ok(err.message.includes("a"));
      assert.ok(err.message.includes("b"));
    }
  });

  it("TypeMismatchError includes path and types", () => {
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
      assert.ok(err instanceof Error);
      assert.ok(err.message.includes("flag"));
      assert.ok(err.message.includes("bool"));
    }
  });

  it("ExtraParamsError lists extra param names", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    try {
      tmpl.render({ x: "hi", bogus: "extra" });
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(err instanceof Error);
      assert.ok(err.message.includes("bogus"));
    }
  });
});

// ---------------------------------------------------------------------------
// match() and isVariant()
// ---------------------------------------------------------------------------

describe("match()", () => {
  const Status = defineVariants({
    Done: ["summary"],
    InProgress: null,
    Blocked: ["reason"],
  });

  it("matches unit variant", () => {
    const result = match(Status.InProgress, {
      Done: () => "done",
      InProgress: () => "working",
      Blocked: () => "blocked",
    });
    assert.strictEqual(result, "working");
  });

  it("matches struct variant with fields", () => {
    const v = Status.Done({ summary: "All tests pass" });
    const result = match(v, {
      Done: (f) => `✅ ${String(f.summary)}`,
      InProgress: () => "working",
      Blocked: () => "blocked",
    });
    assert.strictEqual(result, "✅ All tests pass");
  });

  it("uses wildcard fallback", () => {
    const result = match(Status.InProgress, {
      Done: () => "done",
      _: () => "other",
    });
    assert.strictEqual(result, "other");
  });

  it("throws without handler or wildcard", () => {
    assert.throws(
      () => match(Status.InProgress, { Done: () => "done" }),
      /no handler for variant 'InProgress'/,
    );
  });

  it("works with __kind__ tagged objects", () => {
    const v = { __kind__: "Confirmed", evidence: "proof" };
    const result = match(v, {
      Confirmed: (f) => `yes: ${String(f.evidence)}`,
      _: () => "no",
    });
    assert.strictEqual(result, "yes: proof");
  });

  it("works with string unit variants", () => {
    const result = match("Rejected", {
      Confirmed: () => "yes",
      Rejected: () => "no",
    });
    assert.strictEqual(result, "no");
  });
});

describe("isVariant()", () => {
  const Status = defineVariants({
    Done: ["summary"],
    InProgress: null,
  });

  it("detects unit variant", () => {
    assert.ok(isVariant(Status.InProgress, "InProgress"));
    assert.ok(!isVariant(Status.InProgress, "Done"));
  });

  it("detects struct variant", () => {
    const v = Status.Done({ summary: "ok" });
    assert.ok(isVariant(v, "Done"));
    assert.ok(!isVariant(v, "InProgress"));
  });

  it("detects __kind__ objects", () => {
    assert.ok(isVariant({ __kind__: "Confirmed" }, "Confirmed"));
    assert.ok(!isVariant({ __kind__: "Confirmed" }, "Rejected"));
  });

  it("detects string variants", () => {
    assert.ok(isVariant("Rejected", "Rejected"));
    assert.ok(!isVariant("Rejected", "Confirmed"));
  });
});

// ---------------------------------------------------------------------------
// Edge cases & consistency
// ---------------------------------------------------------------------------

describe("Edge cases", () => {
  it("empty template (no params, no body)", () => {
    const tmpl = Template.fromSource(`---
params: []
---
`);
    assert.strictEqual(tmpl.render(), "");
    assert.strictEqual(tmpl.renderUnchecked(), "");
    assert.deepStrictEqual(tmpl.defaults(), {});
    assert.deepStrictEqual(tmpl.consts(), {});
  });

  it("consts-only template (no params)", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - NAME = str := "test"

params: []
---
Hello {{ NAME }}!`,
    );
    assert.strictEqual(tmpl.render(), "Hello test!");
    assert.strictEqual(tmpl.renderUnchecked(), "Hello test!");
    assert.deepStrictEqual(tmpl.consts(), { NAME: "test" });
  });

  it("TypedTemplate.consts() with multiple consts", () => {
    const tmpl = TypedTemplate.fromSource<{ x: string }>(
      `---
consts:
  - A = int := 1
  - B = str := "two"
  - C = bool := true

params: [x = str]
---
{{ A }} {{ B }} {{ C }} {{ x }}`,
    );
    const c = tmpl.consts();
    assert.strictEqual(c.A, 1);
    assert.strictEqual(c.B, "two");
    assert.strictEqual(c.C, true);
    assert.strictEqual(tmpl.render({ x: "done" }), "1 two true done");
  });

  it("{{ list }} is a compile error", () => {
    // Bare {{ items }} should fail to compile when items is a list type.
    assert.throws(
      () =>
        Template.fromSourceAllowingUnused(
          `---
params:
  - items = list(name = str)
---
{{ items }}`,
        ),
      /cannot display.*list/,
    );
  });

  it("defaults are injected in renderUnchecked", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - greeting = str := "Hi"
  - name = str
---
{{ greeting }} {{ name }}`,
    );
    assert.strictEqual(tmpl.renderUnchecked({ name: "Bob" }), "Hi Bob");
  });

  it("render and renderUnchecked produce same output for simple templates", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - x = str
  - n = int
---
{{ x }} has {{ n }} items`,
    );
    const params = { x: "Alice", n: 42 };
    assert.strictEqual(tmpl.render(params), tmpl.renderUnchecked(params));
  });
});
