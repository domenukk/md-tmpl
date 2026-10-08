/**
 * Tests for the md-tmpl TypeScript bindings: for...else, enforcement gaps, filters, comparisons and truthiness.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  Template,
  TemplateSyntaxError,
  MissingParamsError,
  TypeMismatchError,
  ExtraParamsError,
  UnknownFilterError,
} from "../index.js";

// ---------------------------------------------------------------------------
// for...else
// ---------------------------------------------------------------------------

describe("for...else", () => {
  it("renders else body when list is empty", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str)
---
> {% for item in items %}{{ item.name }}{% else %}No items{% /for %}`,
    );
    const output = tmpl.render({ items: [] });
    assert.strictEqual(output.trim(), "No items");
  });

  it("renders loop body when list is non-empty", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str)
---
> {% for item in items %}{{ item.name }}{% else %}No items{% /for %}`,
    );
    const output = tmpl.render({ items: [{ name: "Alice" }] });
    assert.ok(output.includes("Alice"));
    assert.ok(!output.includes("No items"));
  });

  it("nested for...else renders correctly", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - outer = list(name = str)
  - inner = list(name = str)
---
> {% for o in outer %}{% for i in inner %}{{ i.name }}{% else %}no inner{% /for %}{% else %}no outer{% /for %}`,
    );
    const output = tmpl.render({
      outer: [{ name: "A" }],
      inner: [],
    });
    assert.ok(output.includes("no inner"));
    assert.ok(!output.includes("no outer"));
  });

  it("for...else with if/else nesting", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str)
  - show = bool
---
> {% for item in items %}{% if show %}{{ item.name }}{% else %}hidden{% /if %}{% else %}No items{% /for %}`,
    );
    const output = tmpl.render({ items: [], show: true });
    assert.strictEqual(output.trim(), "No items");
  });

  it("for without else still works", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - items = list(name = str)
---
> {% for item in items %}{{ item.name }}{% /for %}`,
    );
    const output = tmpl.render({ items: [{ name: "Bob" }] });
    assert.strictEqual(output.trim(), "Bob");
  });
});

// ---------------------------------------------------------------------------
// Duplicate type alias detection
// ---------------------------------------------------------------------------

describe("Duplicate type alias detection", () => {
  it("throws on duplicate type alias", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
types:
  - Greeting = str
  - Greeting = int

params:
  - msg = Greeting
---
{{ msg }}`),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(err.message.includes("duplicate type alias"));
        assert.ok(err.message.includes("Greeting"));
        return true;
      },
    );
  });

  it("accepts distinct type aliases", () => {
    const tmpl = Template.fromSource(`---
types:
  - Greeting = str
  - Count = int

params:
  - msg = Greeting
  - n = Count
---
{{ msg }} {{ n }}`);
    assert.strictEqual(tmpl.render({ msg: "hello", n: 5 }).trim(), "hello 5");
  });
});

// ---------------------------------------------------------------------------
// Enforcement — missing params, type mismatches, extra params
// ---------------------------------------------------------------------------

describe("Enforcement gaps", () => {
  it("rejects missing required param", () => {
    const tmpl = Template.fromSource(`---
params:
  - name = str
  - age = int
---
{{ name }} {{ age }}`);
    assert.throws(
      () => tmpl.render({ name: "Alice" }),
      (err: Error) => {
        assert.ok(err instanceof MissingParamsError);
        return true;
      },
    );
  });

  it("rejects wrong type for int param", () => {
    const tmpl = Template.fromSource(`---
params:
  - count = int
---
{{ count }}`);
    assert.throws(
      () => tmpl.render({ count: "not a number" }),
      (err: Error) => {
        assert.ok(err instanceof TypeMismatchError);
        return true;
      },
    );
  });

  it("rejects wrong type for bool param", () => {
    const tmpl = Template.fromSource(`---
params:
  - flag = bool
---
> {% if flag %}yes{% else %}no{% /if %}`);
    assert.throws(
      () => tmpl.render({ flag: "true" }),
      (err: Error) => {
        assert.ok(err instanceof TypeMismatchError);
        return true;
      },
    );
  });

  it("rejects wrong type for list param", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
> {% for item in items %}{{ item }}{% /for %}`);
    assert.throws(
      () => tmpl.render({ items: "not a list" }),
      (err: Error) => {
        assert.ok(err instanceof TypeMismatchError);
        return true;
      },
    );
  });

  it("rejects extra params by default", () => {
    const tmpl = Template.fromSource(`---
params:
  - name = str
---
{{ name }}`);
    assert.throws(
      () => tmpl.render({ name: "Alice", extra: "bad" }),
      (err: Error) => {
        assert.ok(err instanceof ExtraParamsError);
        return true;
      },
    );
  });

  it("allows extra params with allowExtra option", () => {
    const tmpl = Template.fromSource(`---
params:
  - name = str
---
{{ name }}`);
    const result = tmpl.render(
      { name: "Alice", extra: "ok" },
      { allowExtra: true },
    );
    assert.strictEqual(result.trim(), "Alice");
  });

  it("rejects wrong struct field type", () => {
    const tmpl = Template.fromSource(`---
params:
  - person = struct(name = str, age = int)
---
{{ person.name }} is {{ person.age }}`);
    assert.throws(
      () => tmpl.render({ person: { name: "Alice", age: "thirty" } }),
      (err: Error) => {
        assert.ok(err instanceof TypeMismatchError);
        return true;
      },
    );
  });

  it("rejects missing struct field", () => {
    const tmpl = Template.fromSource(`---
params:
  - person = struct(name = str, age = int)
---
{{ person.name }} is {{ person.age }}`);
    assert.throws(
      () => tmpl.render({ person: { name: "Alice" } }),
      (err: Error) => {
        assert.ok(
          err instanceof MissingParamsError || err instanceof TypeMismatchError,
          `expected MissingParamsError or TypeMismatchError, got ${err.constructor.name}: ${err.message}`,
        );
        return true;
      },
    );
  });

  it("rejects wrong list element type", () => {
    const tmpl = Template.fromSource(`---
params:
  - nums = list(int)
---
> {% for n in nums %}{{ n }}{% /for %}`);
    assert.throws(
      () => tmpl.render({ nums: [1, "two", 3] }),
      (err: Error) => {
        assert.ok(err instanceof TypeMismatchError);
        return true;
      },
    );
  });

  it("rejects unknown enum variant", () => {
    const tmpl = Template.fromSource(`---
params:
  - color = enum(Red, Green, Blue)
---
{{ kind(color) }}`);
    assert.throws(
      () => tmpl.render({ color: "Yellow" }),
      (err: Error) => {
        assert.ok(err instanceof TypeMismatchError);
        return true;
      },
    );
  });
});

// ---------------------------------------------------------------------------
// Filter comprehensive tests
// ---------------------------------------------------------------------------

describe("Filter comprehensive tests", () => {
  it("upper filter", () => {
    const tmpl = Template.fromSource(`---
params:
  - name = str
---
{{ name | upper }}`);
    assert.strictEqual(tmpl.render({ name: "hello" }).trim(), "HELLO");
  });

  it("lower filter", () => {
    const tmpl = Template.fromSource(`---
params:
  - name = str
---
{{ name | lower }}`);
    assert.strictEqual(tmpl.render({ name: "HELLO" }).trim(), "hello");
  });

  it("trim filter", () => {
    const tmpl = Template.fromSource(`---
params:
  - name = str
---
{{ name | trim }}`);
    assert.strictEqual(tmpl.render({ name: "  hello  " }).trim(), "hello");
  });

  it("join filter", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
{{ items | join(", ") }}`);
    assert.strictEqual(
      tmpl.render({ items: ["a", "b", "c"] }).trim(),
      "a, b, c",
    );
  });

  it("join filter with empty list", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
{{ items | join(", ") }}`);
    assert.strictEqual(tmpl.render({ items: [] }).trim(), "");
  });

  it("limit filter truncates long list", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
{{ items | limit(2) | join(", ") }}`);
    assert.strictEqual(
      tmpl.render({ items: ["a", "b", "c", "d"] }).trim(),
      "a, b",
    );
  });

  it("limit filter keeps short list unchanged", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
{{ items | limit(50) | join(", ") }}`);
    assert.strictEqual(tmpl.render({ items: ["a", "b"] }).trim(), "a, b");
  });

  it("add filter on int", () => {
    const tmpl = Template.fromSource(`---
params:
  - n = int
---
{{ n | add(5) }}`);
    assert.strictEqual(tmpl.render({ n: 10 }).trim(), "15");
  });

  it("sub filter on int", () => {
    const tmpl = Template.fromSource(`---
params:
  - n = int
---
{{ n | sub(3) }}`);
    assert.strictEqual(tmpl.render({ n: 10 }).trim(), "7");
  });

  it("fixed filter on float", () => {
    const tmpl = Template.fromSource(`---
params:
  - val = float
---
{{ val | fixed(2) }}`);
    assert.strictEqual(tmpl.render({ val: 3.14159 }).trim(), "3.14");
  });

  it("filter chain: upper then join", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
{{ items | join(", ") | upper }}`);
    assert.strictEqual(
      tmpl.render({ items: ["hello", "world"] }).trim(),
      "HELLO, WORLD",
    );
  });

  it("filter chain: trim then lower", () => {
    const tmpl = Template.fromSource(`---
params:
  - text = str
---
{{ text | trim | lower }}`);
    assert.strictEqual(tmpl.render({ text: "  HELLO  " }).trim(), "hello");
  });

  it("unknown filter throws UnknownFilterError", () => {
    assert.throws(
      () =>
        Template.fromSource(
          `---
params:
  - text = str
---
{{ text | nonexistent }}`,
        ).render({ text: "hello" }),
      (err: Error) => {
        assert.ok(err instanceof UnknownFilterError);
        return true;
      },
    );
  });
});

// ---------------------------------------------------------------------------
// Comparison operators comprehensive tests
// ---------------------------------------------------------------------------

describe("Comparison operators comprehensive", () => {
  it("== with equal ints", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x == 5 %}yes{% else %}no{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 5 }).trim(), "yes");
  });

  it("== with unequal ints", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x == 5 %}yes{% else %}no{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 3 }).trim(), "no");
  });

  it("!= with unequal ints", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x != 5 %}yes{% else %}no{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 3 }).trim(), "yes");
  });

  it("!= with equal ints", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x != 5 %}yes{% else %}no{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 5 }).trim(), "no");
  });

  it("< comparison", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x < 5 %}less{% else %}not{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 3 }).trim(), "less");
    assert.strictEqual(tmpl.render({ x: 5 }).trim(), "not");
  });

  it("> comparison", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x > 5 %}more{% else %}not{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 7 }).trim(), "more");
    assert.strictEqual(tmpl.render({ x: 5 }).trim(), "not");
  });

  it("<= comparison", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x <= 5 %}yes{% else %}no{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 5 }).trim(), "yes");
    assert.strictEqual(tmpl.render({ x: 6 }).trim(), "no");
  });

  it(">= comparison", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = int
---
> {% if x >= 5 %}yes{% else %}no{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 5 }).trim(), "yes");
    assert.strictEqual(tmpl.render({ x: 4 }).trim(), "no");
  });

  it("string == comparison", () => {
    const tmpl = Template.fromSource(`---
params:
  - mode = str
---
> {% if mode == "debug" %}debugging{% else %}normal{% /if %}`);
    assert.strictEqual(tmpl.render({ mode: "debug" }).trim(), "debugging");
    assert.strictEqual(tmpl.render({ mode: "release" }).trim(), "normal");
  });

  it("bool == comparison", () => {
    const tmpl = Template.fromSource(`---
params:
  - flag = bool
---
> {% if flag == true %}on{% else %}off{% /if %}`);
    assert.strictEqual(tmpl.render({ flag: true }).trim(), "on");
    assert.strictEqual(tmpl.render({ flag: false }).trim(), "off");
  });
});

// ---------------------------------------------------------------------------
// Truthiness comprehensive tests
// ---------------------------------------------------------------------------

describe("Truthiness comprehensive", () => {
  it("empty string is falsy", () => {
    const tmpl = Template.fromSource(`---
params:
  - text = str
---
> {% if text %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ text: "" }).trim(), "falsy");
  });

  it("non-empty string is truthy", () => {
    const tmpl = Template.fromSource(`---
params:
  - text = str
---
> {% if text %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ text: "hello" }).trim(), "truthy");
  });

  it("zero int is falsy", () => {
    const tmpl = Template.fromSource(`---
params:
  - n = int
---
> {% if n != 0 %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ n: 0 }).trim(), "falsy");
  });

  it("non-zero int is truthy", () => {
    const tmpl = Template.fromSource(`---
params:
  - n = int
---
> {% if n != 0 %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ n: 42 }).trim(), "truthy");
  });

  it("negative int is truthy", () => {
    const tmpl = Template.fromSource(`---
params:
  - n = int
---
> {% if n != 0 %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ n: -1 }).trim(), "truthy");
  });

  it("zero float is falsy", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = float
---
> {% if x != 0.0 %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 0.0 }).trim(), "falsy");
  });

  it("non-zero float is truthy", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = float
---
> {% if x != 0.0 %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ x: 0.001 }).trim(), "truthy");
  });

  it("true bool is truthy", () => {
    const tmpl = Template.fromSource(`---
params:
  - flag = bool
---
> {% if flag %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ flag: true }).trim(), "truthy");
  });

  it("false bool is falsy", () => {
    const tmpl = Template.fromSource(`---
params:
  - flag = bool
---
> {% if flag %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ flag: false }).trim(), "falsy");
  });

  it("empty list is falsy", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
> {% if items %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ items: [] }).trim(), "falsy");
  });

  it("non-empty list is truthy", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
> {% if items %}truthy{% else %}falsy{% /if %}`);
    assert.strictEqual(tmpl.render({ items: ["a"] }).trim(), "truthy");
  });
});

// ---------------------------------------------------------------------------
// Whitespace control comprehensive
// ---------------------------------------------------------------------------

describe("Whitespace control comprehensive", () => {
  it("for loop with compact output", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(str)
---
[{% for item in items %}{{ item }}{% /for %}]`);
    const result = tmpl.render({ items: ["a", "b", "c"] });
    assert.strictEqual(result.trim(), "[abc]");
  });

  it("if/else with compact output", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = bool
---
[{% if x %}yes{% else %}no{% /if %}]`);
    assert.strictEqual(tmpl.render({ x: true }).trim(), "[yes]");
    assert.strictEqual(tmpl.render({ x: false }).trim(), "[no]");
  });
});
