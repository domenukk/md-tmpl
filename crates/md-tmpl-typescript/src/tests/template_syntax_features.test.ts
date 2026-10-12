/**
 * Tests for the md-tmpl TypeScript bindings: filters, builtins, whitespace, match arms, comparisons, raw blocks and comments.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template } from "../index.js";

// ---------------------------------------------------------------------------
// Multiple filter types
// ---------------------------------------------------------------------------

describe("All filter types", () => {
  it("upper filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = str]
---
{{ val | upper }}`,
    );
    assert.strictEqual(tmpl.render({ val: "hello" }), "HELLO");
  });

  it("lower filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = str]
---
{{ val | lower }}`,
    );
    assert.strictEqual(tmpl.render({ val: "HELLO" }), "hello");
  });

  it("trim filter", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = str]
---
{{ val | trim }}`,
    );
    assert.strictEqual(tmpl.render({ val: "  hello  " }), "hello");
  });

  it("fixed filter on float", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = float]
---
{{ val | fixed(2) }}`,
    );
    assert.strictEqual(tmpl.render({ val: 3.14159 }), "3.14");
  });

  it("join filter on list of strings", () => {
    const tmpl = Template.fromSourceAllowingUnused(
      `---
params:
  - items = list(str)
---
{{ items | join(", ") }}`,
    );
    const result = tmpl.render({ items: ["a", "b", "c"] });
    assert.strictEqual(result, "a, b, c");
  });

  it("join filter on list of structs throws at render", () => {
    // join() is valid at compile time (transforms list→str), but
    // at render time display() on each struct item throws.
    const tmpl = Template.fromSourceAllowingUnused(
      `---
params:
  - items = list(name = str)
---
{{ items | join(", ") }}`,
    );
    assert.throws(
      () =>
        tmpl.render({
          items: [{ name: "a" }, { name: "b" }],
        }),
      /cannot display struct/,
    );
  });

  it("limit and truncate filters on list throw at compile time without join or tojson", () => {
    const srcLimit = `---
params:
  - items = list(name = str)
---
{{ items | limit(2) }}`;
    assert.throws(
      () => Template.fromSourceAllowingUnused(srcLimit),
      /cannot display value of type list/,
    );
    const srcTruncate = `---
params:
  - items = list(name = str)
---
{{ items | truncate(2) }}`;
    assert.throws(
      () => Template.fromSourceAllowingUnused(srcTruncate),
      /cannot display value of type list/,
    );
  });

  it("add filter on int", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = int]
---
{{ val | add(10) }}`,
    );
    assert.strictEqual(tmpl.render({ val: 5 }), "15");
  });

  it("sub filter on int", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = int]
---
{{ val | sub(3) }}`,
    );
    assert.strictEqual(tmpl.render({ val: 10 }), "7");
  });

  it("unknown filter throws UnknownFilterError", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params: [val = str]
---
{{ val | nonexistent_filter }}`,
        ).render({ val: "test" }),
      (err: unknown) =>
        err instanceof Error && err.message.includes("nonexistent_filter"),
    );
  });
});

// ---------------------------------------------------------------------------
// Built-in functions: idx, len, kind
// ---------------------------------------------------------------------------

describe("Built-in functions (idx, len, kind)", () => {
  it("idx returns loop index", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ idx(item) }}: {{ item.name }}

> {% /for %}`,
    );
    const result = tmpl.render({
      items: [{ name: "a" }, { name: "b" }, { name: "c" }],
    });
    assert.ok(result.includes("0: a"));
    assert.ok(result.includes("1: b"));
    assert.ok(result.includes("2: c"));
  });

  it("len returns list length", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str)
---
Count: {{ len(items) }}`,
    );
    assert.strictEqual(
      tmpl.render({
        items: [{ name: "a" }, { name: "b" }],
      }),
      "Count: 2",
    );
  });

  it("kind returns variant kind", () => {
    const src = [
      `---`,
      "params:",
      "  - outcome = enum(Confirmed(evidence = str), Rejected)",
      `---`,
      "Kind: {{ kind(outcome) }}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    assert.strictEqual(tmpl.render({ outcome: "Rejected" }), "Kind: Rejected");
  });
});

// ---------------------------------------------------------------------------
// Whitespace trimming
// ---------------------------------------------------------------------------

describe("Whitespace trimming", () => {
  it("trims with {{- -}} syntax", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = str]
---
  {{- x -}}  `,
    );
    const result = tmpl.render({ x: "hello" });
    assert.strictEqual(result.trim(), "hello");
  });
});

// ---------------------------------------------------------------------------
// Empty list edge case
// ---------------------------------------------------------------------------

describe("Empty list rendering", () => {
  it("for loop over empty list produces empty output", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
    );
    const result = tmpl.render({ items: [] });
    assert.strictEqual(result.trim(), "");
  });
});

// ---------------------------------------------------------------------------
// Defaults introspection
// ---------------------------------------------------------------------------

describe("Defaults introspection (additional)", () => {
  it("defaults() returns empty for no-default params", () => {
    const tmpl = Template.fromSource(
      `---
params: [x = str, y = int]
---
{{ x }} {{ y }}`,
    );
    assert.deepStrictEqual(tmpl.defaults(), {});
  });

  it("defaults() returns correct values for multiple types", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str := "World"
  - count = int := 5
  - flag = bool := true
---
{{ name }} {{ count }} {{ flag }}`,
    );
    const defs = tmpl.defaults();
    assert.strictEqual(defs.name, "World");
    assert.strictEqual(defs.count, 5);
    assert.strictEqual(defs.flag, true);
  });

  it("defaults can be overridden", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str := "World"
---
Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.render({ name: "Alice" }), "Hello Alice!");
    assert.strictEqual(tmpl.render({}), "Hello World!");
  });
});

// ---------------------------------------------------------------------------
// Type alias rendering
// ---------------------------------------------------------------------------

describe("Type alias edge cases", () => {
  it("type alias with struct type", () => {
    const src = [
      `---`,
      "types:",
      "  - Config = struct(host = str, port = int)",
      "",
      "params:",
      "  - cfg = Config",
      `---`,
      "{{ cfg.host }}:{{ cfg.port }}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    assert.strictEqual(
      tmpl.render({ cfg: { host: "localhost", port: 3000 } }),
      "localhost:3000",
    );
  });

  it("typeAliases map is populated", () => {
    const src = [
      `---`,
      "types:",
      "  - Priority = enum(High, Medium, Low)",
      "",
      "params:",
      "  - p = Priority",
      `---`,
      "{{ kind(p) }}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    assert.ok(tmpl.frontmatter.typeAliases.has("Priority"));
  });
});

// ---------------------------------------------------------------------------
// Inline match (guard syntax)
// ---------------------------------------------------------------------------

describe("Inline match guard", () => {
  it("renders inline match case Variant", () => {
    const src = [
      `---`,
      "params:",
      "  - status = enum(Active, Inactive)",
      `---`,
      "> {% match status case Active %}",
      "",
      "User is active",

      "",
      "> {% /match %}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    const result = tmpl.render({ status: "Active" });
    assert.ok(
      result.includes("User is active"),
      `expected 'User is active' in: ${result}`,
    );
  });
});

// ---------------------------------------------------------------------------
// toString representation
// ---------------------------------------------------------------------------

describe("Template toString", () => {
  it("includes param info", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str, count = int]
---
{{ name }} {{ count }}`,
    );
    const str = tmpl.toString();
    assert.ok(str.includes("Template"));
    assert.ok(str.includes("name"));
    assert.ok(str.includes("count"));
  });
});

// ---------------------------------------------------------------------------
// sourceHash stability and uniqueness
// ---------------------------------------------------------------------------

describe("sourceHash (additional)", () => {
  it("same source produces same hash", () => {
    const source = `---
params: [x = str]
---
{{ x }}`;
    const t1 = Template.fromSource(source);
    const t2 = Template.fromSource(source);
    assert.strictEqual(t1.sourceHash(), t2.sourceHash());
  });

  it("different source produces different hash", () => {
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

  it("hash is a positive number", () => {
    const tmpl = Template.fromSource(`---
params: [x = str]
---
{{ x }}`);
    assert.ok(tmpl.sourceHash() > 0);
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — match else arm
// ---------------------------------------------------------------------------

describe("Match else arm", () => {
  const src = [
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
  ].join("\n");

  it("renders matching case", () => {
    assert.strictEqual(Template.fromSource(src).render({ s: "A" }), "alpha\n");
  });

  it("renders else for non-matching variant", () => {
    assert.strictEqual(Template.fromSource(src).render({ s: "B" }), "other\n");
  });

  it("renders else for another non-matching variant", () => {
    assert.strictEqual(Template.fromSource(src).render({ s: "C" }), "other\n");
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — match multi-variant arm
// ---------------------------------------------------------------------------

describe("Match multi-variant arm", () => {
  const src = [
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
  ].join("\n");

  it("renders shared body for first variant", () => {
    assert.strictEqual(Template.fromSource(src).render({ s: "A" }), "ab\n");
  });

  it("renders shared body for second variant", () => {
    assert.strictEqual(Template.fromSource(src).render({ s: "B" }), "ab\n");
  });

  it("renders specific case", () => {
    assert.strictEqual(Template.fromSource(src).render({ s: "C" }), "c\n");
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — nested idx()
// ---------------------------------------------------------------------------

describe("Nested idx()", () => {
  it("tracks each loop variable independently", () => {
    const src = [
      `---`,
      "params:",
      "  - outer = list(label = str)",
      "  - inner = list(label = str)",
      `---`,
      "> {% for a in outer %}{% for b in inner %}{{ idx(a) }}.{{ idx(b) }} {% /for %}{% /for %}",
    ].join("\n");

    const result = Template.fromSource(src).render({
      outer: [{ label: "x" }, { label: "y" }],
      inner: [{ label: "p" }, { label: "q" }],
    });
    assert.strictEqual(result, "0.0 0.1 1.0 1.1 ");
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — comparison operators
// ---------------------------------------------------------------------------

describe("Comparison operators", () => {
  const src = (op: string) =>
    [
      `---`,
      "params:",
      "  - x = int",
      `---`,
      `> {% if x ${op} 5 %}`,
      "",
      "yes",

      "",
      "> {% else %}",
      "",
      "no",

      "",
      "> {% /if %}",
    ].join("\n");

  it("!= operator", () => {
    assert.strictEqual(
      Template.fromSource(src("!=")).render({ x: 3 }),
      "yes\n",
    );
    assert.strictEqual(Template.fromSource(src("!=")).render({ x: 5 }), "no\n");
  });

  it("< operator", () => {
    assert.strictEqual(Template.fromSource(src("<")).render({ x: 3 }), "yes\n");
    assert.strictEqual(Template.fromSource(src("<")).render({ x: 5 }), "no\n");
  });

  it("> operator", () => {
    assert.strictEqual(Template.fromSource(src(">")).render({ x: 7 }), "yes\n");
    assert.strictEqual(Template.fromSource(src(">")).render({ x: 5 }), "no\n");
  });

  it("<= operator", () => {
    assert.strictEqual(
      Template.fromSource(src("<=")).render({ x: 5 }),
      "yes\n",
    );
    assert.strictEqual(Template.fromSource(src("<=")).render({ x: 6 }), "no\n");
  });

  it(">= operator", () => {
    assert.strictEqual(
      Template.fromSource(src(">=")).render({ x: 5 }),
      "yes\n",
    );
    assert.strictEqual(Template.fromSource(src(">=")).render({ x: 4 }), "no\n");
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — whitespace control with {{- -}}
// ---------------------------------------------------------------------------

describe("Whitespace control", () => {
  it("trims whitespace with {{- -}}", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
---
hello  {{- name -}}
bye`,
    );
    assert.strictEqual(tmpl.render({ name: "world" }), "helloworldbye");
  });

  it("trims only before with {{-", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
---
hello  {{- name }}!`,
    );
    assert.strictEqual(tmpl.render({ name: "world" }), "helloworld!");
  });

  it("trims only after with -}}", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
---
hello {{ name -}}  !`,
    );
    assert.strictEqual(tmpl.render({ name: "world" }), "hello world!");
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — raw blocks
// ---------------------------------------------------------------------------

describe("Raw blocks", () => {
  it("preserves template syntax", () => {
    const tmpl = Template.fromSource(
      `---
params: []
---
> {% raw %}
{{ not_processed }}
> {% /raw %}`,
    );
    const result = tmpl.render({});
    assert.ok(result.includes("{{ not_processed }}"));
  });
});

// ---------------------------------------------------------------------------
// SPEC.md coverage — comments
// ---------------------------------------------------------------------------

describe("Comments", () => {
  it("strips {# #} from output", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
---
> {# This is a comment #}Hello {{ name }}!`,
    );
    assert.strictEqual(tmpl.render({ name: "world" }), "Hello world!");
  });

  it("comments suppress unused-param errors", () => {
    // The param 'unused' is only referenced in a comment, which should count
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
  - unused = str
---
> {# {{ unused }} #}Hello {{ name }}!`,
    );
    assert.strictEqual(
      tmpl.render({ name: "world", unused: "x" }),
      "Hello world!",
    );
  });
});
