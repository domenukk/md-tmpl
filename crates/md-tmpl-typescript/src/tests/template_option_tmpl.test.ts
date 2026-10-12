/**
 * Tests for the md-tmpl TypeScript bindings: option(T) match/serde/codegen and option(tmpl(...)) parameters.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template } from "../index.js";
import { parseVarType, varTypeToString } from "../frontmatter.js";
import { generateTypes, inferTypes } from "../codegen.js";
import { defined } from "./template_helpers.js";

describe("option(T) match/case", () => {
  it("matches Some case", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% match x %}

> {% case Some %}

found: {{ x }}

> {% case None %}

empty

> {% /match %}`);
    assert.ok(tmpl.render({ x: 42 }).includes("found: 42"));
  });

  it("matches None case", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% match x %}

> {% case Some %}

found

> {% case None %}

empty

> {% /match %}`);
    assert.ok(tmpl.render({ x: null }).includes("empty"));
  });
});

describe("option(T) JSON serde", () => {
  it("null input becomes None", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "None");
  });

  it("undefined input becomes None via default", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := None
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render().trim(), "None");
  });

  it("value input becomes Some", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: 42 }).trim(), "Some");
  });
});

describe("option(T) codegen", () => {
  it("generates T | null for option(int)", () => {
    const code = generateTypes(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.ok(code.includes("number | null"));
  });

  it("generates T | null for option(str)", () => {
    const code = generateTypes(`---
params:
  - x = option(str)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.ok(code.includes("string | null"));
  });

  it("inferTypes reports option correctly", () => {
    const result = inferTypes(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(defined(result.fields[0]).tsType, "number | null");
  });

  it("generates null for None default in codegen", () => {
    const code = generateTypes(`---
params:
  - x = option(int) := None
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.ok(code.includes("null"));
  });

  it("option label in JSDoc", () => {
    const code = generateTypes(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.ok(code.includes("option(int)"));
  });
});

describe("option(T) nested in structs and lists", () => {
  it("option inside struct", () => {
    const tmpl = Template.fromSource(`---
params:
  - config = struct(name = str, timeout = option(int))
---
> {% if has(config.timeout) %}

timeout: {{ config.timeout }}

> {% else %}

no timeout

> {% /if %}`);
    const output1 = tmpl.render({ config: { name: "test", timeout: 42 } });
    assert.ok(output1.includes("timeout: 42"));

    const output2 = tmpl.render({ config: { name: "test", timeout: null } });
    assert.ok(output2.includes("no timeout"));
  });

  it("option in list items", () => {
    const tmpl = Template.fromSource(`---
params:
  - items = list(name = str, value = option(int))
---
> {% for item in items %}

> {% if has(item.value) %}

{{ item.name }}: {{ item.value }}

> {% else %}

{{ item.name }}: -

> {% /if %}

> {% /for %}`);
    const output = tmpl.render({
      items: [
        { name: "a", value: 1 },
        { name: "b", value: null },
      ],
    });
    assert.ok(output.includes("a: 1"));
    assert.ok(output.includes("b: -"));
  });
});

describe("option(T) renderUnchecked", () => {
  it("renders option value directly in unchecked mode", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const output = tmpl.renderUnchecked({ x: 42 });
    assert.strictEqual(output.trim(), "42");
  });

  it("has() works in unchecked mode", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

yes

> {% else %}

no

> {% /if %}`);
    assert.ok(tmpl.renderUnchecked({ x: null }).includes("no"));
    assert.ok(tmpl.renderUnchecked({ x: 42 }).includes("yes"));
  });
});

describe("option(T) error cases", () => {
  it("rejects wrong inner type in render", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.throws(
      () => tmpl.render({ x: "not a number" }),
      (err: Error) => err.message.includes("type mismatch"),
    );
  });

  it("option as reserved keyword prevents naming", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - option = str
---
{{ option }}`),
      (err: Error) => err.message.includes("reserved"),
    );
  });

  it("option variant name rejected in enum", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - x = enum(option, other)
---
{{ kind(x) }}`),
      (err: Error) => err.message.includes("shadows a builtin type keyword"),
    );
  });

  it("option type alias cannot shadow builtin", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
types:
  - Option = enum(A, B)

params:
  - x = Option
---
{{ kind(x) }}`),
      (err: Error) => err.message.includes("shadows built-in type name"),
    );
  });
});

describe("option(T) type alias support", () => {
  it("option type alias in types block", () => {
    const tmpl = Template.fromSource(`---
types:
  - MaybeInt = option(int)

params:
  - x = MaybeInt
---
> {% if has(x) %}

{{ x }}

> {% else %}

none

> {% /if %}`);
    assert.ok(tmpl.render({ x: 42 }).includes("42"));
    assert.ok(tmpl.render({ x: null }).includes("none"));
  });
});

// ---------------------------------------------------------------------------
// option(tmpl(...)) type support
// ---------------------------------------------------------------------------

describe("option(tmpl(...)) parsing", () => {
  // NOTE: tmpl() is a first-class VarType kind (not desugared to struct).
  // option(tmpl(x = str)) preserves the tmpl kind at the VarType level.
  it("parses option(tmpl(x = str)) — tmpl is first-class kind", () => {
    const vt = parseVarType("option(tmpl(x = str))");
    assert.strictEqual(vt.kind, "option");
    // tmpl is a first-class VarType kind
    assert.strictEqual(vt.innerType.kind, "tmpl");
  });

  it("parses option(tmpl()) — empty tmpl", () => {
    const vt = parseVarType("option(tmpl())");
    assert.strictEqual(vt.kind, "option");
    assert.strictEqual(vt.innerType.kind, "tmpl");
  });

  it("formats option(tmpl(x = str)) as option(tmpl(x = str))", () => {
    const vt = parseVarType("option(tmpl(x = str))");
    // tmpl is preserved as first-class kind
    assert.strictEqual(varTypeToString(vt), "option(tmpl(x = str))");
  });

  it("declarations report tmpl type", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(test = str))
---
> {% if has(cb) %}

present

> {% else %}

absent

> {% /if %}`);
    const decls = tmpl.declarations();
    // tmpl is preserved as first-class kind
    assert.strictEqual(defined(decls[0])[1], "option(tmpl(test = str))");
  });
});

describe("option(tmpl(...)) has() guard", () => {
  it("has() returns false for null option(tmpl)", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(test = str))
---
> {% if has(cb) %}

yes

> {% else %}

no

> {% /if %}`);
    assert.ok(tmpl.render({ cb: null }).includes("no"));
  });

  it("has() returns true for present option(tmpl)", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(test = str))
---
> {% if has(cb) %}

yes

> {% else %}

no

> {% /if %}`);
    assert.ok(tmpl.render({ cb: { test: "hello" } }).includes("yes"));
  });
});

describe("option(tmpl(...)) match/case", () => {
  it("matches Some case for option(tmpl)", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(test = str))
---
> {% match cb %}

> {% case Some %}

present

> {% case None %}

absent

> {% /match %}`);
    assert.ok(tmpl.render({ cb: { test: "val" } }).includes("present"));
  });

  it("matches None case for option(tmpl)", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(test = str))
---
> {% match cb %}

> {% case Some %}

present

> {% case None %}

absent

> {% /match %}`);
    assert.ok(tmpl.render({ cb: null }).includes("absent"));
  });
});

describe("option(tmpl(...)) default values", () => {
  it("defaults to None when := None", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(test = str)) := None
---
> {% if has(cb) %}

present

> {% else %}

absent

> {% /if %}`);
    assert.ok(tmpl.render().includes("absent"));
  });

  it("option(tmpl) without default is required", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(test = str))
---
> {% if has(cb) %}

yes

> {% /if %}`);
    assert.throws(
      () => tmpl.render({}),
      (err: Error) =>
        err.message.includes("missing") && err.message.includes("cb"),
    );
  });

  it("rejects field access on an option(tmpl) handle with a None default", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - cb = option(tmpl(test = str)) := None
---
> {% if has(cb) %}

{{ cb.test }}

> {% else %}

absent

> {% /if %}`),
      /cannot access field '[a-z_]+' on tmpl\(/,
    );
  });
});

describe("option(tmpl(...)) field access inside has() guard", () => {
  it("rejects field access on a narrowed option(tmpl) handle", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - cb = option(tmpl(name = str, count = int))
---
> {% if has(cb) %}

{{ cb.name }}: {{ cb.count }}

> {% /if %}`),
      /cannot access field '[a-z_]+' on tmpl\(/,
    );
  });

  it("rejects field access on an option(tmpl) handle in a has() branch with else", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - cb = option(tmpl(name = str, count = int))
---
> {% if has(cb) %}

{{ cb.name }}: {{ cb.count }}

> {% else %}

no callback

> {% /if %}`),
      /cannot access field '[a-z_]+' on tmpl\(/,
    );
  });
});

describe("option(tmpl(...)) kind() builtin", () => {
  it("kind() returns Some for present value", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(x = str))
---
{{ kind(cb) }}`);
    assert.strictEqual(tmpl.render({ cb: { x: "hi" } }).trim(), "Some");
  });

  it("kind() returns None for null value", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(x = str))
---
{{ kind(cb) }}`);
    assert.strictEqual(tmpl.render({ cb: null }).trim(), "None");
  });
});

describe("option(tmpl(...)) nested: option(tmpl(x = option(tmpl(...))))", () => {
  it("parses deeply nested option(tmpl(option(tmpl))) — first-class", () => {
    const vt = parseVarType("option(tmpl(sub = option(tmpl(test = str))))");
    assert.strictEqual(vt.kind, "option");
    // tmpl is a first-class VarType kind
    assert.strictEqual(vt.innerType.kind, "tmpl");
  });

  it("formats deeply nested type — tmpl preserved", () => {
    const vt = parseVarType("option(tmpl(sub = option(tmpl(test = str))))");
    assert.strictEqual(
      varTypeToString(vt),
      "option(tmpl(sub = option(tmpl(test = str))))",
    );
  });

  it("rejects nested field access through tmpl handles", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - outer = option(tmpl(inner = option(tmpl(val = str))))
---
> {% if has(outer) %}

> {% if has(outer.inner) %}

{{ outer.inner.val }}

> {% else %}

no inner

> {% /if %}

> {% else %}

no outer

> {% /if %}`),
      /cannot access field '[a-z_]+' on tmpl\(/,
    );
  });

  it("rejects has() on a field of a tmpl handle", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - outer = option(tmpl(inner = option(tmpl(val = str))))
---
> {% if has(outer) %}

> {% if has(outer.inner) %}

{{ outer.inner.val }}

> {% else %}

no inner

> {% /if %}

> {% else %}

no outer

> {% /if %}`),
      /cannot access field '[a-z_]+' on tmpl\(/,
    );
  });

  it("rejects has() on a nested tmpl field even when the body is static", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - outer = option(tmpl(inner = option(tmpl(val = str))))
---
> {% if has(outer) %}

> {% if has(outer.inner) %}

deep

> {% else %}

no inner

> {% /if %}

> {% else %}

no outer

> {% /if %}`),
      /cannot access field '[a-z_]+' on tmpl\(/,
    );
  });
});

describe("option(tmpl(...)) codegen", () => {
  it("generates correct type for option(tmpl(x = str))", () => {
    const code = generateTypes(`---
params:
  - cb = option(tmpl(x = str))
---
> {% if has(cb) %}

{{ cb.x }}

> {% /if %}`);
    // Should contain null union and the field type
    assert.ok(code.includes("null"), `codegen should include null: ${code}`);
  });

  it("inferTypes reports option(tmpl) correctly", () => {
    const result = inferTypes(`---
params:
  - cb = option(tmpl(x = str))
---
> {% if has(cb) %}

{{ cb.x }}

> {% /if %}`);
    const field = result.fields.find((f) => f.name === "cb");
    assert.ok(field, "cb field should exist in inferred types");
    assert.ok(
      field.tsType.includes("null"),
      `type should include null: ${field.tsType}`,
    );
  });
});

describe("option(tmpl(...)) JSON serde", () => {
  it("null input becomes None for option(tmpl)", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(x = str))
---
{{ kind(cb) }}`);
    assert.strictEqual(tmpl.render({ cb: null }).trim(), "None");
  });

  it("struct input becomes Some for option(tmpl)", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(x = str))
---
{{ kind(cb) }}`);
    assert.strictEqual(tmpl.render({ cb: { x: "hi" } }).trim(), "Some");
  });

  it("undefined input becomes None via default", () => {
    const tmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(x = str)) := None
---
{{ kind(cb) }}`);
    assert.strictEqual(tmpl.render().trim(), "None");
  });
});

describe("option(tmpl(...)) multiple params mix", () => {
  it("rejects field access on option(tmpl) handles mixed with option scalars", () => {
    assert.throws(
      () =>
        Template.fromSource(`---
params:
  - cb = option(tmpl(test = str))
  - target = option(tmpl(test = str))
  - label = option(str) := None
---
> {% if has(cb) %}

cb: {{ cb.test }}

> {% /if %}

> {% if has(target) %}

target: {{ target.test }}

> {% /if %}

> {% if has(label) %}

label: {{ label }}

> {% /if %}`),
      /cannot access field '[a-z_]+' on tmpl\(/,
    );
  });
});
