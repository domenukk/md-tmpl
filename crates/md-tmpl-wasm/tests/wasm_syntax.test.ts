/**
 * WASM binding tests: Template language features: loops, conditionals, match, filters, structs, defaults, raw, options, panic.
 *
 * Uses Node.js built-in test runner (`node:test`) and `node:assert/strict`.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template as WasmTemplate } from "../pkg/md_tmpl_wasm.js";

// =========================================================================
// 11. For loops
// =========================================================================

describe("For loops", () => {
  it("simple list iteration", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - items = list(name = str)
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
  - items = list(name = str)
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
  - tasks = list(title = str, priority = str)
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
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
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
  - meta = struct(author = str, version = int)
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
  - config = struct(db = struct(host = str, port = int))
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
  - meta = struct(author = str)
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

// ---------------------------------------------------------------------------
// option(T) support
// ---------------------------------------------------------------------------

describe("option(T)", () => {
  const matchTemplate = WasmTemplate.fromSource(
    `---
params:
  - label = option(str)
---
> {% match label %}
> {% case Some %}

got:{{ label }}

> {% case None %}

empty

> {% /match %}`,
  );

  const hasTemplate = WasmTemplate.fromSource(
    `---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

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
      label: "hello",
    });
    assert.ok(result.includes("got:hello"), `expected 'got:hello', got '${result}'`);
  });

  it("null renders else branch via has()", () => {
    const result = hasTemplate.render({ label: null });
    assert.equal(result.trim(), "empty");
  });

  it("Some struct renders if branch via has()", () => {
    const result = hasTemplate.render({
      label: "world",
    });
    assert.ok(result.includes("got:world"), `expected 'got:world', got '${result}'`);
  });

  it("option(int) with null via has()", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}`,
    );
    const result = t.render({ count: null });
    assert.equal(result.trim(), "no-count");
  });

  it("option(int) with value via has()", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}`,
    );
    const result = t.render({
      count: 42,
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
  - items = list(name = str)
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
  - items = list(name = str)
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
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
    );
    assert.ok(t.render({ items: [{ name: "Bob" }] }).includes("Bob"));
  });
});

// =========================================================================
// 26. Milestone M2 — panic(...) and in/not in
// =========================================================================

describe("Milestone M2 — panic(...) and in/not in", () => {
  it("panic literal throws error cleanly", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
> {% panic("halt") %}
`,
    );
    assert.throws(() => {
      t.render({});
    }, /template panic: halt/);
  });

  it("panic interpolation throws error cleanly", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - reason = str
---
> {% panic(reason) %}
`,
    );
    assert.throws(() => {
      t.render({ reason: "fatal: bad state" });
    }, /template panic: fatal: bad state/);
  });

  it("in operator string substring", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - role = str
---
> {% if "admin" in role %}

YES

> {% else %}

NO

> {% /if %}`,
    );
    assert.equal(t.render({ role: "superadmin user" }).trim(), "YES");
    assert.equal(t.render({ role: "guest" }).trim(), "NO");
  });

  it("not in operator string substring", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - role = str
---
> {% if !("err" in role) %}

OK

> {% else %}

ERROR

> {% /if %}`,
    );
    assert.equal(t.render({ role: "status_ok" }).trim(), "OK");
    assert.equal(t.render({ role: "has_err_flag" }).trim(), "ERROR");
  });

  it("in operator list membership", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - roles = list(str)
---
> {% if "admin" in roles %}

ALLOWED

> {% else %}

DENIED

> {% /if %}`,
    );
    assert.equal(t.render({ roles: ["user", "admin"] }).trim(), "ALLOWED");
    assert.equal(t.render({ roles: ["guest", "user"] }).trim(), "DENIED");
  });

  it("not in operator list membership", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - roles = list(str)
---
> {% if !("banned" in roles) %}

WELCOME

> {% else %}

GO AWAY

> {% /if %}`,
    );
    assert.equal(t.render({ roles: ["guest", "user"] }).trim(), "WELCOME");
    assert.equal(t.render({ roles: ["user", "banned"] }).trim(), "GO AWAY");
  });

  it("in operator enum kinds", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - status = enum(Active, Inactive, Pending)

allow_unused: true
---
> {% if "Active" in kinds(Status) %}

VALID

> {% /if %}`,
    );
    assert.equal(t.render({ status: "Active" }).trim(), "VALID");
  });
});
