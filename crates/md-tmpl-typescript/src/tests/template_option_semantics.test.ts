/**
 * Tests for the md-tmpl TypeScript bindings: option(T) parsing, defaults, rendering, has() and declaration diagnostics.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template, TemplateSyntaxError } from "../index.js";
import {
  parseFrontmatter,
  parseVarType,
  varTypeToString,
} from "../frontmatter.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// option(T) type support
// ---------------------------------------------------------------------------

describe("option(T) parsing", () => {
  it("parses option(int) as dedicated option type", () => {
    const vt = parseVarType("option(int)");
    assert.strictEqual(vt.kind, "option");
    assert.deepStrictEqual(vt.innerType, { kind: "int" });
  });

  it("parses option(str)", () => {
    const vt = parseVarType("option(str)");
    assert.strictEqual(vt.kind, "option");
    assert.deepStrictEqual(vt.innerType, { kind: "str" });
  });

  it("formats option(int) back to string", () => {
    const vt = parseVarType("option(int)");
    assert.strictEqual(varTypeToString(vt), "option(int)");
  });

  it("formats option(str) back to string", () => {
    const vt = parseVarType("option(str)");
    assert.strictEqual(varTypeToString(vt), "option(str)");
  });

  it("declarations report option(T) correctly", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const decls = tmpl.declarations();
    assert.strictEqual(defined(decls[0])[1], "option(int)");
  });
});

describe("option(T) default values", () => {
  it("default None renders correctly", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := None
---
> {% if has(x) %}

present

> {% else %}

absent

> {% /if %}`);
    const output = tmpl.render();
    assert.ok(output.includes("absent"));
  });

  it("default auto-wrap integer", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := 42
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const output = tmpl.render();
    assert.ok(output.includes("42"));
  });

  it("default auto-wrap quoted string None", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str) := "None"
---
> {% if has(x) %}

{{ x }}

> {% else %}

absent

> {% /if %}`);
    const output = tmpl.render();
    assert.ok(output.includes("None"));
    assert.ok(!output.includes("absent"));
  });

  it("default auto-wrap quoted regular string", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str) := "hello"
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render().trim(), "hello");
  });

  it("defaults() returns null for None default", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := None
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const defs = tmpl.defaults();
    assert.strictEqual(defs.x, null);
  });

  it("defaults() returns unwrapped value for Some default", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := 42
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    const defs = tmpl.defaults();
    assert.strictEqual(defs.x, 42);
  });
});

describe("option(T) required vs optional", () => {
  it("option without default is required — missing param throws", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.throws(
      () => tmpl.render({}),
      (err: Error) =>
        err.message.includes("missing") && err.message.includes("x"),
    );
  });

  it("option without default — providing value works", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.ok(tmpl.render({ x: 42 }).includes("42"));
  });

  it("option without default — providing null works", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

present

> {% else %}

absent

> {% /if %}`);
    assert.ok(tmpl.render({ x: null }).includes("absent"));
  });

  it("option with := None default — omitting param is OK", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := None
---
> {% if has(x) %}

present

> {% else %}

absent

> {% /if %}`);
    // No error: default None is applied
    assert.ok(tmpl.render().includes("absent"));
  });

  it("option with := None default — can override with value", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int) := None
---
> {% if has(x) %}

{{ x }}

> {% else %}

absent

> {% /if %}`);
    assert.ok(tmpl.render({ x: 99 }).includes("99"));
  });

  it("defaults() does not include option without default", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
  - y = option(str) := None
  - z = option(str) := "hi"
---
> {% if has(x) %}

{{ x }}

> {% /if %}

> {% if has(y) %}

{{ y }}

> {% /if %}

> {% if has(z) %}

{{ z }}

> {% /if %}`);
    const defs = tmpl.defaults();
    assert.ok(!("x" in defs), "x should not have a default");
    assert.strictEqual(defs.y, null, "y should default to null (None)");
    assert.strictEqual(defs.z, "hi", "z should default to 'hi'");
  });
});

describe("option(T) rendering with auto-unwrap", () => {
  it("auto-unwraps Some(val=42) to 42 in {{ x }}", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: 42 }).trim(), "42");
  });

  it("auto-unwraps Some(val=str) to str", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(str)
---
> {% if has(x) %}

{{ x }}

> {% /if %}`);
    assert.strictEqual(tmpl.render({ x: "hello" }).trim(), "hello");
  });

  it("renders None as str(None) for kind()", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: null }).trim(), "None");
  });

  it("renders Some kind", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
{{ kind(x) }}`);
    assert.strictEqual(tmpl.render({ x: 42 }).trim(), "Some");
  });
});

describe("option(T) has() builtin", () => {
  it("has() returns false for None", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

yes

> {% else %}

no

> {% /if %}`);
    assert.ok(tmpl.render({ x: null }).includes("no"));
  });

  it("has() returns true for Some", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = option(int)
---
> {% if has(x) %}

yes

> {% else %}

no

> {% /if %}`);
    assert.ok(tmpl.render({ x: 42 }).includes("yes"));
  });

  it("bare truthiness is true for non-empty string", () => {
    const tmpl = Template.fromSource(`---
params:
  - x = str
---
> {% if x %}

yes

> {% /if %}`);
    assert.ok(tmpl.render({ x: "hello" }).includes("yes"));
  });
});

// ===========================================================================
// D1 — option has()/None handling and leak-safety (regression)
//
// Locks in the divergence-contract behavior shared with the Rust core:
//   * has() is false only for the None sentinel, true for any present value.
//   * The literal string "None" is a *present* value (has() == true), so it
//     never masquerades as the None sentinel — both as a parsed default and as
//     a runtime-supplied value.
//   * The inner value of an option must not "leak" into a branch where it is
//     statically known to be absent: reading it in the else-branch of
//     `if has(x)`, the body of `if !has(x)`, or a `case None` arm is rejected.
// ===========================================================================
describe("D1: option has()/None + leak-safety (regression)", () => {
  const renders = (src: string, ctx: Record<string, unknown>): string =>
    Template.fromSource(src).render(ctx);

  const ifHasElse = `---
params:
  - x = option(int)
---
> {% if has(x) %}

present

> {% else %}

absent

> {% /if %}`;

  it("has() is false for the None sentinel (absent)", () => {
    assert.ok(renders(ifHasElse, { x: null }).includes("absent"));
  });

  it("has() is true for a present Some value", () => {
    assert.ok(renders(ifHasElse, { x: 42 }).includes("present"));
  });

  it('the literal string "None" is present (has() == true) via default', () => {
    const src = `---
params:
  - x = option(str) := "None"
---
> {% if has(x) %}

present:{{ x }}

> {% else %}

absent

> {% /if %}`;
    assert.strictEqual(renders(src, {}).trim(), "present:None");
  });

  it('the literal string "None" is present (has() == true) at runtime', () => {
    const src = `---
params:
  - x = option(str)
---
> {% if has(x) %}

present:{{ x }}

> {% else %}

absent

> {% /if %}`;
    assert.strictEqual(renders(src, { x: "None" }).trim(), "present:None");
  });

  it("rejects reading the inner value in the else-branch of `if has(x)`", () => {
    const src = `---
params:
  - x = option(int)
---
> {% if has(x) %}

present

> {% else %}

val={{ x }}

> {% /if %}`;
    assert.throws(
      () => renders(src, { x: null }),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(err.message.includes("cannot display value of type option"));
        return true;
      },
    );
  });

  it("rejects reading the inner value in the body of `if !has(x)`", () => {
    const src = `---
params:
  - x = option(int)
---
> {% if !has(x) %}

val={{ x }}

> {% /if %}`;
    assert.throws(
      () => renders(src, { x: null }),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(err.message.includes("cannot display value of type option"));
        return true;
      },
    );
  });

  it("rejects reading the inner value in a `case None` arm", () => {
    const src = `---
params:
  - x = option(int)
---
> {% match x %}

> {% case Some %}

some

> {% case None %}

val={{ x }}

> {% /match %}`;
    assert.throws(
      () => renders(src, { x: null }),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(err.message.includes("cannot display value of type option"));
        return true;
      },
    );
  });
});

// ===========================================================================
// D2 / D4 — declaration diagnostics (byte-for-byte parity with the Rust core)
//
// The exact wording of these diagnostics is part of the cross-backend
// divergence contract, so the assertions pin the full core message. The parser
// appends a " (line N, --> ...)" location suffix, which `includes` tolerates.
// ===========================================================================
describe("D2/D4: declaration diagnostics (byte-for-byte)", () => {
  const expectError = (fm: string, coreMessage: string): void => {
    assert.throws(
      () => parseFrontmatter(fm),
      (err: Error) => {
        assert.ok(err instanceof TemplateSyntaxError);
        assert.ok(
          err.message.includes(coreMessage),
          `expected message to include:\n  ${coreMessage}\ngot:\n  ${err.message}`,
        );
        return true;
      },
    );
  };

  // --- D2: qualified enum-variant defaults are rejected ---

  it("D2 rejects a qualified default on an inline enum type", () => {
    expectError(
      `---
params:
  - s = enum(Design, Build) := MyEnum.Build
---
body`,
      "declaration 's': invalid enum default 'MyEnum.Build': use the bare " +
        "variant name 'Build' (a qualified 'Type.Variant' is only valid in " +
        "expressions)",
    );
  });

  it("D2 rejects a qualified default on an enum type alias", () => {
    expectError(
      `---
types:
  - Stage = enum(Design, Build, Deploy)

params:
  - s = Stage := Stage.Build
---
body`,
      "declaration 's': invalid enum default 'Stage.Build': use the bare " +
        "variant name 'Build' (a qualified 'Type.Variant' is only valid in " +
        "expressions)",
    );
  });

  it("D2 rejects a qualified default on an option(enum) type", () => {
    expectError(
      `---
types:
  - Stage = enum(Design, Build, Deploy)

params:
  - o = option(Stage) := Stage.Deploy
---
body`,
      "declaration 'o': invalid enum default 'Stage.Deploy': use the bare " +
        "variant name 'Deploy' (a qualified 'Type.Variant' is only valid in " +
        "expressions)",
    );
  });

  // --- D4: implicit-typed declarations with a default are rejected ---

  it("D4 rejects an implicit-typed param (`x := ...`)", () => {
    expectError(
      `---
params:
  - x := "hello"
---
body`,
      "param 'x' must have an explicit type (expected 'name = type := value')",
    );
  });

  it("D4 rejects an implicit-typed param with no spaces (`x:=...`)", () => {
    expectError(
      `---
params:
  - x:="hello"
---
body`,
      "param 'x' must have an explicit type (expected 'name = type := value')",
    );
  });

  it("D4 rejects an implicit-typed const (`X := ...`)", () => {
    expectError(
      `---
consts:
  - X := "hello"
---
body`,
      "constant 'X' must have an explicit type (expected 'name = type := value')",
    );
  });

  it("D4 reports a missing type annotation for a bare name", () => {
    expectError(
      `---
params:
  - untyped_param
---
body`,
      "param 'untyped_param' is missing a type annotation " +
        "(expected 'name = type')",
    );
  });
});
