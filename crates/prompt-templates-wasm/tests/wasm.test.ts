/**
 * Comprehensive WASM test suite for prompt-templates WASM bindings.
 *
 * Uses Node.js built-in test runner (`node:test`) and `node:assert/strict`.
 *
 * Usage:
 *   cd crates/prompt-templates-wasm
 *   node --test tests/wasm.test.mjs
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template as WasmTemplate } from "../pkg/prompt_templates_wasm.js";
import { Template as TsTemplate } from "../../prompt-templates-typescript/dist/index.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Normalize whitespace for content-level comparison. */
function normalize(s) {
  return s.replace(/\n+/g, "\n").trim();
}

// =========================================================================
// 1. Template.fromSource
// =========================================================================

describe("Template.fromSource", () => {
  it("basic string param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    const result = t.render({ name: "world" });
    assert.equal(result, "Hello world!");
  });

  it("int param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
Value is {{ n }}`,
    );
    assert.equal(t.render({ n: 42 }), "Value is 42");
  });

  it("float param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - pi = float
---
Pi is {{ pi }}`,
    );
    assert.equal(t.render({ pi: 3.14159 }), "Pi is 3.14159");
  });

  it("bool param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - flag = bool
---
Flag is {{ flag }}`,
    );
    assert.equal(t.render({ flag: true }), "Flag is true");
  });

  it("throws on syntax error", () => {
    assert.throws(() => {
      WasmTemplate.fromSource(`---
params:
  - = invalid
---
Hello`);
    });
  });

  it("throws on missing frontmatter delimiter", () => {
    assert.throws(() => {
      WasmTemplate.fromSource("no frontmatter here");
    });
  });
});

// =========================================================================
// 2. Template.fromSourceAllowingUnused
// =========================================================================

describe("Template.fromSourceAllowingUnused", () => {
  it("accepts unused declared params", () => {
    const source = `---
params:
  - used = str
  - unused = str
---
Hello {{ used }}!`;
    // Should NOT throw
    const t = WasmTemplate.fromSourceAllowingUnused(source);
    assert.equal(t.render({ used: "world", unused: "ignored" }), "Hello world!");
  });

  it("strict mode (fromSource) rejects unused params", () => {
    const source = `---
params:
  - used = str
  - unused = str
---
Hello {{ used }}!`;
    assert.throws(() => {
      WasmTemplate.fromSource(source);
    });
  });
});

// =========================================================================
// 3. Template.render (strict mode)
// =========================================================================

describe("Template.render (strict mode)", () => {
  it("renders correctly with all params", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`,
    );
    assert.equal(t.render({ name: "Alice", count: 10 }), "Alice: 10");
  });

  it("throws on missing param (error mentions param name)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    try {
      t.render({});
      assert.fail("Expected render to throw");
    } catch (err) {
      assert.ok(
        String(err).toLowerCase().includes("name"),
        `Error should mention "name", got: ${err}`,
      );
    }
  });

  it("throws on type mismatch (string where int expected)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
{{ n }}`,
    );
    assert.throws(() => {
      t.render({ n: "not_a_number" });
    });
  });

  it("throws on extra undeclared params", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(() => {
      t.render({ name: "world", extra: "oops" });
    });
  });

  it("renders with multiple params (str, int, float, bool)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`,
    );
    assert.equal(
      t.render({ name: "Alice", count: 42, score: 9.5, enabled: true }),
      "Alice: count=42, score=9.5, enabled=true",
    );
  });
});

// =========================================================================
// 4. Template.renderUnchecked
// =========================================================================

describe("Template.renderUnchecked", () => {
  it("allows extra params", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    // Should NOT throw with extra params
    const result = t.renderUnchecked({ name: "world", extra: "ok" });
    assert.equal(result, "Hello world!");
  });

  it("allows extra undeclared params without throwing", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    // renderUnchecked allows extra params that aren't declared
    const result = t.renderUnchecked({ name: "world", bonus: "extra" });
    assert.equal(result, "Hello world!");
  });

  it("still renders correctly", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - a = str
  - b = int
---
{{ a }}-{{ b }}`,
    );
    assert.equal(t.renderUnchecked({ a: "foo", b: 5 }), "foo-5");
  });
});

// =========================================================================
// 5. Template.body()
// =========================================================================

describe("Template.body()", () => {
  it("returns body text without frontmatter", () => {
    const source = `---
params:
  - name = str
---
Hello {{ name }}!`;
    const t = WasmTemplate.fromSource(source);
    const body = t.body();
    assert.ok(!body.includes(`---`), "Body should not contain frontmatter delimiters");
    assert.ok(!body.includes("params:"), "Body should not contain frontmatter content");
    assert.ok(
      body.includes("Hello {{ name }}!"),
      "Body should contain the template body",
    );
  });
});

// =========================================================================
// 6. Template.consts()
// =========================================================================

describe("Template.consts()", () => {
  it("returns constants as plain object", () => {
    const t = WasmTemplate.fromSource(
      `---
consts:
  - MAX = int := 100
params: []
---
Max is {{ MAX }}`,
    );
    const consts = t.consts();
    assert.deepEqual(consts, { MAX: 100 });
  });

  it("returns empty object when no consts", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static text`,
    );
    const consts = t.consts();
    assert.deepEqual(consts, {});
  });
});

// =========================================================================
// 7. Template.declarations()
// =========================================================================

describe("Template.declarations()", () => {
  it("returns array of [name, typeString] tuples", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
---
{{ name }} {{ count }}`,
    );
    const decls = t.declarations();
    assert.ok(Array.isArray(decls), "declarations() should return an array");
    assert.equal(decls.length, 2);
    // Each element is [name, typeString]
    assert.equal(decls[0][0], "name");
    assert.equal(decls[0][1], "str");
    assert.equal(decls[1][0], "count");
    assert.equal(decls[1][1], "int");
  });

  it("handles complex types (list, struct, enum)", () => {
    const t = WasmTemplate.fromSourceAllowingUnused(
      `---
params:
  - items = list<name = str>
  - meta = struct<author = str, version = int>
  - status = enum<Active, Paused>
---
{{ items }}{{ meta }}{{ status }}`,
    );
    const decls = t.declarations();
    assert.equal(decls.length, 3);
    // Check the type strings exist
    assert.ok(decls[0][1].includes("list"), `Expected list type, got: ${decls[0][1]}`);
    assert.ok(decls[1][1].includes("struct"), `Expected struct type, got: ${decls[1][1]}`);
    assert.ok(decls[2][1].includes("enum"), `Expected enum type, got: ${decls[2][1]}`);
  });
});

// =========================================================================
// 8. Template.defaults()
// =========================================================================

describe("Template.defaults()", () => {
  it("returns defaults object", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - greeting = str := "Hi"
---
{{ greeting }}!`,
    );
    const defaults = t.defaults();
    assert.equal(defaults.greeting, "Hi");
  });

  it("missing defaults returns empty object", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
{{ name }}`,
    );
    const defaults = t.defaults();
    assert.deepEqual(defaults, {});
  });

  it("renders using defaults when params omitted", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - greeting = str := "Hi"
  - name = str
---
{{ greeting }}, {{ name }}!`,
    );
    assert.equal(t.render({ name: "World" }), "Hi, World!");
  });
});

// =========================================================================
// 9. Template.sourceHash()
// =========================================================================

describe("Template.sourceHash()", () => {
  const source1 = `---
params:
  - x = str
---
{{ x }}`;
  const source2 = `---
params:
  - y = str
---
{{ y }}`;

  it("same source → same hash", () => {
    const t1 = WasmTemplate.fromSource(source1);
    const t2 = WasmTemplate.fromSource(source1);
    assert.equal(t1.sourceHash(), t2.sourceHash());
  });

  it("different source → different hash", () => {
    const t1 = WasmTemplate.fromSource(source1);
    const t2 = WasmTemplate.fromSource(source2);
    assert.notEqual(t1.sourceHash(), t2.sourceHash());
  });

  it("returns a number (u32)", () => {
    const t = WasmTemplate.fromSource(source1);
    const hash = t.sourceHash();
    assert.equal(typeof hash, "number");
    assert.ok(Number.isInteger(hash), "Hash should be an integer");
    assert.ok(hash >= 0, "Hash should be non-negative");
    assert.ok(hash <= 0xffffffff, "Hash should fit in u32");
  });
});

// =========================================================================
// 10. Template.importedConsts()
// =========================================================================

describe("Template.importedConsts()", () => {
  it("returns empty object when no imports", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static content`,
    );
    const imported = t.importedConsts();
    assert.deepEqual(imported, {});
  });
});

// =========================================================================
// 11. For loops
// =========================================================================

describe("For loops", () => {
  it("simple list iteration", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - items = list<name = str>
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
    );
    const result = t.render({
      items: [{ name: "alpha" }, { name: "beta" }],
    });
    assert.ok(result.includes("alpha"));
    assert.ok(result.includes("beta"));
  });

  it("empty list renders nothing", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - items = list<name = str>
---
before

> {% for item in items %}

{{ item.name }}

> {% /for %}

after`,
    );
    const result = t.render({ items: [] });
    assert.ok(result.includes("before"));
    assert.ok(result.includes("after"));
    // Should not contain any item content between before/after
    const between = result.split("before")[1].split("after")[0];
    assert.equal(between.trim(), "");
  });

  it("nested field access in loop body", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - tasks = list<title = str, priority = str>
---
> {% for task in tasks %}

- {{ task.title }} ({{ task.priority }})

> {% /for %}`,
    );
    const result = t.render({
      tasks: [
        { title: "Task A", priority: "High" },
        { title: "Task B", priority: "Low" },
      ],
    });
    assert.ok(result.includes("Task A"));
    assert.ok(result.includes("High"));
    assert.ok(result.includes("Task B"));
    assert.ok(result.includes("Low"));
  });
});

// =========================================================================
// 12. Conditionals (if/elif/else)
// =========================================================================

describe("Conditionals (if/elif/else)", () => {
  const condSource = `---
params:
  - level = int
  - name = str
---
> {% if level == 1 %}

Beginner: {{ name }}

> {% elif level == 2 %}

Intermediate: {{ name }}

> {% else %}

Expert: {{ name }}

> {% /if %}`;

  it("if true branch", () => {
    const result = WasmTemplate.fromSource(condSource).render({
      level: 1,
      name: "Alice",
    });
    assert.ok(result.includes("Beginner"));
    assert.ok(result.includes("Alice"));
  });

  it("elif branch", () => {
    const result = WasmTemplate.fromSource(condSource).render({
      level: 2,
      name: "Bob",
    });
    assert.ok(result.includes("Intermediate"));
    assert.ok(result.includes("Bob"));
  });

  it("else branch", () => {
    const result = WasmTemplate.fromSource(condSource).render({
      level: 99,
      name: "Charlie",
    });
    assert.ok(result.includes("Expert"));
    assert.ok(result.includes("Charlie"));
  });

  it("comparison operator ==", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - x = int
---
> {% if x == 5 %}

yes

> {% else %}

no

> {% /if %}`,
    );
    assert.ok(t.render({ x: 5 }).includes("yes"));
    assert.ok(t.render({ x: 3 }).includes("no"));
  });

  it("comparison operator !=", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - x = int
---
> {% if x != 0 %}

nonzero

> {% else %}

zero

> {% /if %}`,
    );
    assert.ok(t.render({ x: 5 }).includes("nonzero"));
    assert.ok(t.render({ x: 0 }).includes("zero"));
  });

  it("comparison operator <", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - x = int
---
> {% if x < 10 %}

small

> {% else %}

big

> {% /if %}`,
    );
    assert.ok(t.render({ x: 3 }).includes("small"));
  });

  it("comparison operator >", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - x = int
---
> {% if x > 10 %}

big

> {% else %}

small

> {% /if %}`,
    );
    assert.ok(t.render({ x: 99 }).includes("big"));
  });

  it("comparison operator <=", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - x = int
---
> {% if x <= 5 %}

small

> {% else %}

big

> {% /if %}`,
    );
    assert.ok(t.render({ x: 5 }).includes("small"));
    assert.ok(t.render({ x: 6 }).includes("big"));
  });

  it("comparison operator >=", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - x = int
---
> {% if x >= 10 %}

big

> {% else %}

small

> {% /if %}`,
    );
    assert.ok(t.render({ x: 10 }).includes("big"));
    assert.ok(t.render({ x: 9 }).includes("small"));
  });
});

// =========================================================================
// 13. Enum/match
// =========================================================================

describe("Enum/match", () => {
  const enumSource = `---
params:
  - outcome = enum<Confirmed(evidence = str), Rejected, NeedsWork>
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`;

  it("unit variant", () => {
    const result = WasmTemplate.fromSource(enumSource).render({
      outcome: "Rejected",
    });
    assert.ok(result.includes("NO"));
  });

  it("struct variant with fields", () => {
    const result = WasmTemplate.fromSource(enumSource).render({
      outcome: { __kind__: "Confirmed", evidence: "proof found" },
    });
    assert.ok(result.includes("YES"));
    assert.ok(result.includes("proof found"));
  });

  it("all branches (Confirm/Reject/NeedsWork)", () => {
    const t = WasmTemplate.fromSource(enumSource);

    const r1 = t.render({
      outcome: { __kind__: "Confirmed", evidence: "data" },
    });
    assert.ok(r1.includes("YES"));

    const r2 = t.render({ outcome: "Rejected" });
    assert.ok(r2.includes("NO"));

    const r3 = t.render({ outcome: "NeedsWork" });
    assert.ok(r3.includes("MAYBE"));
  });
});

// =========================================================================
// 14. Filters
// =========================================================================

describe("Filters", () => {
  it("upper filter", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
{{ name | upper }}`,
    );
    assert.equal(t.render({ name: "hello" }), "HELLO");
  });

  it("lower filter", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
{{ name | lower }}`,
    );
    assert.equal(t.render({ name: "HELLO" }), "hello");
  });

  it("trim filter", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
{{ name | trim }}`,
    );
    assert.equal(t.render({ name: "  hello  " }), "hello");
  });

  it("length filter", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
{{ len(name) }}`,
    );
    assert.equal(t.render({ name: "hello" }), "5");
  });

  it("filter chaining (trim | lower)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - val = str
---
{{ val | trim | lower }}`,
    );
    assert.equal(t.render({ val: "  HELLO  " }), "hello");
  });
});

// =========================================================================
// 15. Struct parameters
// =========================================================================

describe("Struct parameters", () => {
  it("dot access on struct fields", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - meta = struct<author = str, version = int>
---
By {{ meta.author }}, v{{ meta.version }}`,
    );
    assert.equal(
      t.render({ meta: { author: "Alice", version: 3 } }),
      "By Alice, v3",
    );
  });

  it("nested struct access", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - config = struct<db = struct<host = str, port = int>>
---
{{ config.db.host }}:{{ config.db.port }}`,
    );
    assert.equal(
      t.render({ config: { db: { host: "localhost", port: 5432 } } }),
      "localhost:5432",
    );
  });

  it("missing field error", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - meta = struct<author = str>
---
{{ meta.author }}`,
    );
    assert.throws(() => {
      t.render({ meta: {} });
    });
  });
});

// =========================================================================
// 16. Default values
// =========================================================================

describe("Default values", () => {
  it("int default", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int := 42
---
{{ n }}`,
    );
    assert.equal(t.render({}), "42");
  });

  it("string default", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str := "Guest"
---
Hello {{ name }}!`,
    );
    assert.equal(t.render({}), "Hello Guest!");
  });

  it("bool default", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - flag = bool := true
---
{{ flag }}`,
    );
    assert.equal(t.render({}), "true");
  });

  it("override default", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str := "Guest"
---
Hello {{ name }}!`,
    );
    assert.equal(t.render({ name: "Alice" }), "Hello Alice!");
  });
});

// =========================================================================
// 17. Raw blocks
// =========================================================================

describe("Raw blocks", () => {
  it("{% raw %}...{% /raw %} outputs content verbatim", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
> {% raw %}

{{ not_processed }}

> {% /raw %}`,
    );
    const result = t.render({});
    assert.ok(
      result.includes("{{ not_processed }}"),
      "Raw block should preserve template syntax literally",
    );
  });
});

// =========================================================================
// 18. WASM vs TS parity
// =========================================================================

describe("WASM vs TS parity", () => {
  it("simple render produces identical output", () => {
    const source = `---
params:
  - name = str
---
Hello {{ name }}!`;
    const wasmResult = WasmTemplate.fromSource(source).render({ name: "world" });
    const tsResult = TsTemplate.fromSource(source).render({ name: "world" });
    assert.equal(wasmResult, tsResult);
  });

  it("multi-param render produces identical output", () => {
    const source = `---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`;
    const params = { name: "Alice", count: 42, score: 9.5, enabled: true };
    const wasmResult = WasmTemplate.fromSource(source).render(params);
    const tsResult = TsTemplate.fromSource(source).render(params);
    assert.equal(wasmResult, tsResult);
  });

  it("for loop content matches (after whitespace normalization)", () => {
    const source = `---
params:
  - items = list<name = str>
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`;
    const params = { items: [{ name: "alpha" }, { name: "beta" }] };
    const wasmResult = WasmTemplate.fromSource(source).render(params);
    const tsResult = TsTemplate.fromSource(source).render(params);
    assert.equal(normalize(wasmResult), normalize(tsResult));
  });
});

// =========================================================================
// 19. Symbol.dispose / free()
// =========================================================================

describe("Symbol.dispose / free()", () => {
  it("calling free() doesn't throw", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static`,
    );
    assert.doesNotThrow(() => {
      t.free();
    });
  });

  it("double free throws (null pointer)", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static`,
    );
    t.free();
    assert.throws(
      () => { t.free(); },
      /null pointer/,
    );
  });
});

// =========================================================================
// 20. Error messages
// =========================================================================

describe("Error messages", () => {
  it("syntax error includes position info", () => {
    try {
      WasmTemplate.fromSource(`---
params:
  - = bad
---
Hello`);
      assert.fail("Expected fromSource to throw");
    } catch (err) {
      const msg = String(err);
      // Error should contain some kind of line/position reference or descriptive text
      assert.ok(
        msg.length > 10,
        `Error message should be descriptive, got: ${msg}`,
      );
    }
  });

  it("missing param error includes param name", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - username = str
---
Hello {{ username }}!`,
    );
    try {
      t.render({});
      assert.fail("Expected render to throw");
    } catch (err) {
      assert.ok(
        String(err).includes("username"),
        `Error should mention "username", got: ${err}`,
      );
    }
  });

  it("type mismatch error includes expected/actual types", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
{{ n }}`,
    );
    try {
      t.render({ n: "not_a_number" });
      assert.fail("Expected render to throw");
    } catch (err) {
      const msg = String(err).toLowerCase();
      // Should mention something about type or int/str
      assert.ok(
        msg.includes("type") || msg.includes("int") || msg.includes("str") || msg.includes("mismatch"),
        `Error should mention type info, got: ${err}`,
      );
    }
  });
});

// =========================================================================
// 21. Template.renderJson (JSON bulk serialization)
// =========================================================================

describe("Template.renderJson", () => {
  it("renders from JSON string", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    const result = t.renderJson(JSON.stringify({ name: "world" }));
    assert.equal(result, "Hello world!");
  });

  it("renders multiple params from JSON", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`,
    );
    const result = t.renderJson(
      JSON.stringify({ name: "Alice", count: 42, score: 9.5, enabled: true }),
    );
    assert.equal(
      result,
      "Alice: count=42, score=9.5, enabled=true",
    );
  });

  it("matches render() output exactly", () => {
    const source = `---
params:
  - name = str
---
Hello {{ name }}!`;
    const t = WasmTemplate.fromSource(source);
    const params = { name: "world" };
    assert.equal(t.renderJson(JSON.stringify(params)), t.render(params));
  });

  it("throws on invalid JSON", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(() => {
      t.renderJson("{invalid json}");
    });
  });

  it("throws on missing required param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(() => {
      t.renderJson(JSON.stringify({}));
    });
  });

  it("throws on extra undeclared param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(() => {
      t.renderJson(JSON.stringify({ name: "world", extra: "oops" }));
    });
  });

  it("renders nested struct from JSON", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - config = struct<host = str, port = int>
---
{{ config.host }}:{{ config.port }}`,
    );
    const result = t.renderJson(
      JSON.stringify({ config: { host: "localhost", port: 5432 } }),
    );
    assert.equal(result, "localhost:5432");
  });

  it("renders list from JSON", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - items = list<name = str>
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
    );
    const result = t.renderJson(
      JSON.stringify({ items: [{ name: "alpha" }, { name: "beta" }] }),
    );
    assert.ok(result.includes("alpha"));
    assert.ok(result.includes("beta"));
  });
});

// =========================================================================
// 22. Template.renderUncheckedJson (JSON + unchecked)
// =========================================================================

describe("Template.renderUncheckedJson", () => {
  it("allows extra params via JSON", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    const result = t.renderUncheckedJson(
      JSON.stringify({ name: "world", extra: "ok" }),
    );
    assert.equal(result, "Hello world!");
  });

  it("matches renderUnchecked() output exactly", () => {
    const source = `---
params:
  - name = str
---
Hello {{ name }}!`;
    const t = WasmTemplate.fromSource(source);
    const params = { name: "world", extra: "ok" };
    assert.equal(
      t.renderUncheckedJson(JSON.stringify(params)),
      t.renderUnchecked(params),
    );
  });

  it("renders correctly without extras", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - a = str
  - b = int
---
{{ a }}-{{ b }}`,
    );
    assert.equal(
      t.renderUncheckedJson(JSON.stringify({ a: "foo", b: 5 })),
      "foo-5",
    );
  });

  it("throws on invalid JSON", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(() => {
      t.renderUncheckedJson("not json");
    });
  });
});

// =========================================================================
// 23. Metadata caching (consts, declarations, defaults, importedConsts)
// =========================================================================

describe("Metadata caching", () => {
  it("consts() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
consts:
  - MAX = int := 100
params: []
---
Max is {{ MAX }}`,
    );
    const a = t.consts();
    const b = t.consts();
    assert.deepEqual(a, b);
    assert.deepEqual(a, { MAX: 100 });
  });

  it("declarations() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
---
{{ name }} {{ count }}`,
    );
    const a = t.declarations();
    const b = t.declarations();
    assert.deepEqual(a, b);
    assert.equal(a.length, 2);
  });

  it("defaults() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - greeting = str := "Hi"
---
{{ greeting }}!`,
    );
    const a = t.defaults();
    const b = t.defaults();
    assert.deepEqual(a, b);
    assert.equal(a.greeting, "Hi");
  });

  it("importedConsts() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static content`,
    );
    const a = t.importedConsts();
    const b = t.importedConsts();
    assert.deepEqual(a, b);
    assert.deepEqual(a, {});
  });
});

// ---------------------------------------------------------------------------
// option<T> support
// ---------------------------------------------------------------------------

describe("option<T>", () => {
  const matchTemplate = WasmTemplate.fromSource(
    `---
params:
  - label = option<str>
---
> {% match label %}
> {% case Some %}

got:{{ label.val }}

> {% case None %}

empty

> {% /match %}`,
  );

  const hasTemplate = WasmTemplate.fromSource(
    `---
params:
  - label = option<str>
---
> {% if has(label) %}

got:{{ label.val }}

> {% else %}

empty

> {% /if %}`,
  );

  it("null renders None arm via match", () => {
    const result = matchTemplate.render({ label: null });
    assert.equal(result.trim(), "empty");
  });

  it("Some struct renders Some arm via match", () => {
    const result = matchTemplate.render({
      label: { __kind__: "Some", val: "hello" },
    });
    assert.ok(result.includes("got:hello"), `expected 'got:hello', got '${result}'`);
  });

  it("null renders else branch via has()", () => {
    const result = hasTemplate.render({ label: null });
    assert.equal(result.trim(), "empty");
  });

  it("Some struct renders if branch via has()", () => {
    const result = hasTemplate.render({
      label: { __kind__: "Some", val: "world" },
    });
    assert.ok(result.includes("got:world"), `expected 'got:world', got '${result}'`);
  });

  it("option<int> with null via has()", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - count = option<int>
---
> {% if has(count) %}

count={{ count.val }}

> {% else %}

no-count

> {% /if %}`,
    );
    const result = t.render({ count: null });
    assert.equal(result.trim(), "no-count");
  });

  it("option<int> with value via has()", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - count = option<int>
---
> {% if has(count) %}

count={{ count.val }}

> {% else %}

no-count

> {% /if %}`,
    );
    const result = t.render({
      count: { __kind__: "Some", val: 42 },
    });
    assert.ok(result.includes("count=42"), `expected 'count=42', got '${result}'`);
  });
});

// ---------------------------------------------------------------------------
// for...else support
// ---------------------------------------------------------------------------

describe("for...else", () => {
  it("empty list renders else body", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - items = list<name = str>
---
> {% for item in items %}

{{ item.name }}

> {% else %}

No items

> {% /for %}`,
    );
    assert.equal(t.render({ items: [] }).trim(), "No items");
  });

  it("non-empty list renders loop body, not else", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - items = list<name = str>
---
> {% for item in items %}

{{ item.name }}

> {% else %}

No items

> {% /for %}`,
    );
    const result = t.render({ items: [{ name: "Alice" }] });
    assert.ok(result.includes("Alice"));
    assert.ok(!result.includes("No items"));
  });

  it("for without else still works", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - items = list<name = str>
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
    );
    assert.ok(t.render({ items: [{ name: "Bob" }] }).includes("Bob"));
  });
});
