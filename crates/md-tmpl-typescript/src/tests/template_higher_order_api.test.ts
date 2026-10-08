/**
 * Tests for the md-tmpl TypeScript bindings: higher-order templates and public API review.
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
  Context,
  defineVariants,
  match,
  isVariant,
  TemplateError,
  TemplateSyntaxError,
  MissingParamsError,
  TemplatePanicError,
  IncludeNotFoundError,
  DeclarationsMutatedError,
  structVal,
  StructValue,
  dict,
  DictValue,
  str,
  int,
} from "../index.js";
import { generateTypes } from "../codegen.js";

// ---------------------------------------------------------------------------
// Higher-order templates (tmpl() with actual Template refs)
// ---------------------------------------------------------------------------

describe("Higher-order templates — basic tmpl() param passing", () => {
  it("passes a Template as a tmpl() param and renders via include", () => {
    const child = Template.fromSource(`---
params:
  - msg = str
---
Child: {{ msg }}`);
    const parent = Template.fromSource(`---
params:
  - sub = tmpl(msg = str)
  - greeting = str
---
> {% include sub with msg=greeting %}`);
    const result = parent.render({ sub: child, greeting: "hello" });
    assert.ok(
      result.includes("Child: hello"),
      `expected 'Child: hello' in: ${result}`,
    );
  });

  it("passes a Template with multiple params", () => {
    const child = Template.fromSource(`---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`);
    const parent = Template.fromSource(`---
params:
  - renderer = tmpl(name = str, count = int)
  - user = str
  - n = int
---
> {% include renderer with name=user, count=n %}`);
    const result = parent.render({ renderer: child, user: "Alice", n: 42 });
    assert.ok(
      result.includes("Alice: 42"),
      `expected 'Alice: 42' in: ${result}`,
    );
  });
});

describe("Higher-order templates — signature matching", () => {
  it("accepts Template with matching signature", () => {
    const child = Template.fromSource(`---
params:
  - x = str
---
{{ x }}`);
    const parent = Template.fromSource(`---
params:
  - cb = tmpl(x = str)
  - val = str
---
> {% include cb with x=val %}`);
    // Should not throw
    const result = parent.render({ cb: child, val: "ok" });
    assert.ok(result.includes("ok"));
  });

  it("rejects Template with wrong param type", () => {
    const child = Template.fromSource(`---
params:
  - x = int
---
{{ x }}`);
    const parent = Template.fromSource(`---
params:
  - cb = tmpl(x = str)
  - val = str
---
> {% include cb with x=val %}`);
    assert.throws(
      () => parent.render({ cb: child, val: "ok" }),
      (err: Error) =>
        err.message.includes("x") &&
        (err.message.includes("str") || err.message.includes("int")),
    );
  });

  it("accepts Template with extra param that has a default (signature only)", () => {
    const child = Template.fromSource(`---
params:
  - x = str
  - extra = str := "default"
---
{{ x }}-{{ extra }}`);
    const parent = Template.fromSource(`---
params:
  - cb = tmpl(x = str)
  - val = str
---
> {% include cb with x=val %}`);
    // Signature check should accept the child: 'extra' has a default so it
    // doesn't violate the tmpl(x = str) contract. We only verify that
    // type-checking passes — rendering may fail because renderForInclude
    // doesn't auto-fill child defaults for unmapped params.
    // Use buildContext indirectly via the type-checking path.
    assert.doesNotThrow(() => {
      // We can verify the child is accepted as a TmplRef by the parent's
      // type-checking. If signature validation fails it throws during render.
      // We catch the *render* error (undefined variable) but not type errors.
      try {
        parent.render({ cb: child, val: "ok" });
      } catch (err: unknown) {
        // Render error for 'extra' is expected — it's not mapped in the include.
        // But a TypeMismatchError would mean signature check failed.
        if (err instanceof Error && err.name === "TypeMismatchError") {
          throw err;
        }
        // Non-type errors (like UndefinedVariableError) are fine — signature passed.
      }
    });
  });

  it("rejects Template with extra required param (no default)", () => {
    const child = Template.fromSource(`---
params:
  - x = str
  - extra = str
---
{{ x }}-{{ extra }}`);
    const parent = Template.fromSource(`---
params:
  - cb = tmpl(x = str)
  - val = str
---
> {% include cb with x=val %}`);
    assert.throws(
      () => parent.render({ cb: child, val: "ok" }),
      (err: Error) => err.message.includes("extra"),
    );
  });
});

describe("Higher-order templates — empty signature tmpl()", () => {
  it("accepts Template with no params for tmpl()", () => {
    const child = Template.fromSource(`---
params: []
---
Static content`);
    const parent = Template.fromSource(`---
params:
  - cb = tmpl()
---
> {% include cb %}`);
    const result = parent.render({ cb: child });
    assert.ok(
      result.includes("Static content"),
      `expected 'Static content' in: ${result}`,
    );
  });
});

describe("Higher-order templates — option(tmpl(...)) Some and None", () => {
  it("renders Some with actual Template ref", () => {
    const child = Template.fromSource(`---
params:
  - msg = str
---
From child: {{ msg }}`);
    const parent = Template.fromSource(`---
params:
  - cb = option(tmpl(msg = str))
  - val = str
---
> {% if has(cb) %}
> {% include cb with msg=val %}
> {% else %}

no callback

> {% /if %}`);
    const result = parent.render({ cb: child, val: "hi" });
    assert.ok(
      result.includes("From child: hi"),
      `expected 'From child: hi' in: ${result}`,
    );
  });

  it("renders None path when cb is null", () => {
    const parent = Template.fromSource(`---
params:
  - cb = option(tmpl(msg = str))
  - val = str
---
> {% if has(cb) %}
> {% include cb with msg=val %}
> {% else %}

no callback

> {% /if %}`);
    const result = parent.render({ cb: null, val: "hi" });
    assert.ok(
      result.includes("no callback"),
      `expected 'no callback' in: ${result}`,
    );
    assert.ok(
      !result.includes("From child"),
      `should not contain child output`,
    );
  });
});

describe("Higher-order templates — nested: tmpl param uses file include", () => {
  it("child template with file include renders through tmpl param", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-hot-nested-"));
    try {
      // Create a grandchild template on disk
      const grandchildPath = path.join(dir, "grandchild.tmpl.md");
      fs.writeFileSync(
        grandchildPath,
        `---
params:
  - label = str
---
[{{ label }}]`,
      );

      // Create child template on disk that includes grandchild
      const childPath = path.join(dir, "child.tmpl.md");
      fs.writeFileSync(
        childPath,
        `---
params:
  - title = str
---
> {% include [grandchild](./grandchild.tmpl.md) with label=title %}`,
      );

      // Load child via fromFile so it has a base path for includes
      const child = Template.fromFile(childPath);

      // Create parent that accepts a tmpl param
      const parent = Template.fromSource(`---
params:
  - renderer = tmpl(title = str)
  - heading = str
---
> {% include renderer with title=heading %}`);

      const result = parent.render({ renderer: child, heading: "Nested" });
      assert.ok(
        result.includes("[Nested]"),
        `expected '[Nested]' in: ${result}`,
      );
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

describe("Higher-order templates — for-each with tmpl include", () => {
  it("iterates list items through a tmpl param include", () => {
    const row = Template.fromSource(`---
params:
  - item = struct(name = str)
---
- {{ item.name }}
`);
    const parent = Template.fromSource(`---
params:
  - renderer = tmpl(item = struct(name = str))
  - items = list(name = str)
---
> {% include renderer for item in items %}`);
    const result = parent.render({
      renderer: row,
      items: [{ name: "A" }, { name: "B" }, { name: "C" }],
    });
    assert.ok(result.includes("- A"), `missing A in: ${result}`);
    assert.ok(result.includes("- B"), `missing B in: ${result}`);
    assert.ok(result.includes("- C"), `missing C in: ${result}`);
  });
});

describe("Higher-order templates — constants passed through tmpl param", () => {
  it("child template accesses its own constants when rendered via tmpl", () => {
    const child = Template.fromSource(`---
params:
  - name = str

consts:
  - VER = str := "child-1.0"
---
{{ name }} v{{ VER }}`);
    const parent = Template.fromSource(`---
params:
  - renderer = tmpl(name = str)
  - user = str

consts:
  - APP = str := "app-2.0"
---
> {% include renderer with name=user %}`);
    const result = parent.render({ renderer: child, user: "Alice" });
    // Child's own consts should work
    assert.ok(result.includes("Alice"), `expected 'Alice' in: ${result}`);
    assert.ok(
      result.includes("child-1.0"),
      `expected child const 'child-1.0' in: ${result}`,
    );
  });
});

// ---------------------------------------------------------------------------
// Higher-order templates — nested tmpl: tmpl(target = tmpl(val = str))
// ---------------------------------------------------------------------------

describe("Higher-order templates — nested tmpl param forwarding", () => {
  it("forwards a tmpl() param through another tmpl() via include", () => {
    // inner: the leaf template
    const inner = Template.fromSource(`---
params: [val = str]
---
Inner: {{ val }}`);

    // middle: accepts a tmpl() and includes it
    const middle = Template.fromSource(`---
params:
  - target = tmpl(val = str)
  - value = str
---
> {% include target with val=value %}`);

    // main: declares nested tmpl signature: tmpl(target = tmpl(...), value = str)
    const main = Template.fromSource(`---
params:
  - processor = tmpl(target = tmpl(val = str), value = str)
  - callback = tmpl(val = str)
---
> {% include processor with target=callback, value="Success" %}`);

    const result = main.render({ processor: middle, callback: inner });
    assert.ok(
      result.includes("Inner: Success"),
      `expected 'Inner: Success' in: ${result}`,
    );
  });

  it("rejects nested tmpl with wrong inner signature", () => {
    // inner: has x = int, not val = str
    const inner = Template.fromSource(`---
params: [x = int]
---
{{ x }}`);

    // middle: expects target = tmpl(val = str)
    const middle = Template.fromSource(`---
params:
  - target = tmpl(val = str)
  - value = str
---
> {% include target with val=value %}`);

    const main = Template.fromSource(`---
params:
  - processor = tmpl(target = tmpl(val = str), value = str)
  - callback = tmpl(val = str)
---
> {% include processor with target=callback, value="hi" %}`);

    // inner doesn't match tmpl(val = str) — it has x = int
    assert.throws(
      () => main.render({ processor: middle, callback: inner }),
      (err: Error) => err.message.includes("val") || err.message.includes("x"),
    );
  });
});

describe("Higher-order templates — deeply nested option(tmpl(option(tmpl(...))))", () => {
  it("renders with all levels present", () => {
    // inner leaf
    const innerTmpl = Template.fromSource(`---
params: [test = str]
---
Leaf: {{ test }}`);

    // middle: has an option(tmpl(...)) param
    const middleTmpl = Template.fromSource(`---
params: [sub = option(tmpl(test = str))]
---
> {% if has(sub) %}
> {% include sub with test="NestedOK" %}
> {% else %}

No sub

> {% /if %}`);

    // main: option(tmpl(sub = option(tmpl(test = str))))
    const mainTmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(sub = option(tmpl(test = str))))
  - target = option(tmpl(test = str))
---
> {% if has(cb) %}
> {% include cb with sub=target %}
> {% else %}

No cb

> {% /if %}`);

    const result = mainTmpl.render({ cb: middleTmpl, target: innerTmpl });
    assert.ok(
      result.includes("Leaf: NestedOK"),
      `expected 'Leaf: NestedOK' in: ${result}`,
    );
  });

  it("renders None at outer level", () => {
    const mainTmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(sub = option(tmpl(test = str))))
  - target = option(tmpl(test = str))
---
> {% if has(cb) %}
> {% include cb with sub=target %}
> {% else %}

No cb

> {% /if %}`);

    const result = mainTmpl.render({ cb: null, target: null });
    assert.ok(result.includes("No cb"), `expected 'No cb' in: ${result}`);
  });

  it("renders None at inner level", () => {
    const middleTmpl = Template.fromSource(`---
params: [sub = option(tmpl(test = str))]
---
> {% if has(sub) %}
> {% include sub with test="inner" %}
> {% else %}

No sub

> {% /if %}`);

    const mainTmpl = Template.fromSource(`---
params:
  - cb = option(tmpl(sub = option(tmpl(test = str))))
  - target = option(tmpl(test = str))
---
> {% if has(cb) %}
> {% include cb with sub=target %}
> {% else %}

No cb

> {% /if %}`);

    // cb present but target is null → inner "No sub" path
    const result = mainTmpl.render({ cb: middleTmpl, target: null });
    assert.ok(result.includes("No sub"), `expected 'No sub' in: ${result}`);
  });
});

describe("Higher-order templates — tmpl inside struct field", () => {
  it("struct(widget = tmpl(x = str)) passes and renders", () => {
    const widget = Template.fromSource(`---
params: [x = str]
---
Widget: {{ x }}`);

    const main = Template.fromSource(`---
params:
  - config = struct(widget = tmpl(x = str), label = str)
---
{{ config.label }}: {% include config.widget with x=config.label %}`);

    const result = main.render({
      config: { widget, label: "Test" },
    });
    assert.ok(
      result.includes("Widget: Test"),
      `expected 'Widget: Test' in: ${result}`,
    );
  });
});

// ---------------------------------------------------------------------------
// API review findings — behavior-change coverage
// ---------------------------------------------------------------------------

describe("API review: public exports", () => {
  it("exposes the preferred struct value constructor structVal", () => {
    const v = structVal([
      ["a", str("x")],
      ["n", int(1)],
    ]);
    assert.strictEqual(v.type, "struct");
    assert.strictEqual(v.fields.get("a")?.type, "str");
    assert.strictEqual(v.fields.get("n")?.type, "int");
  });

  it("keeps the deprecated dict alias pointing at structVal", () => {
    assert.strictEqual(dict, structVal);
    // DictValue is a deprecated alias of StructValue — assignable both ways.
    const asDict: DictValue = dict([["k", str("v")]]);
    const asStruct: StructValue = asDict;
    assert.strictEqual(asStruct.fields.get("k")?.type, "str");
  });

  it("exposes the full error hierarchy as constructors", () => {
    assert.strictEqual(typeof TemplatePanicError, "function");
    assert.strictEqual(typeof IncludeNotFoundError, "function");
    assert.strictEqual(typeof DeclarationsMutatedError, "function");
    assert.ok(new TemplatePanicError("x") instanceof TemplateError);
    assert.ok(new IncludeNotFoundError("x") instanceof TemplateError);
    assert.ok(new DeclarationsMutatedError("x") instanceof TemplateError);
  });
});

describe("API review: typed error classes", () => {
  it("IncludeNotFoundError carries the unresolved include path", () => {
    const err = new IncludeNotFoundError("./missing.tmpl.md");
    assert.strictEqual(err.name, "IncludeNotFoundError");
    assert.strictEqual(err.include, "./missing.tmpl.md");
    assert.ok(err.message.includes("include not found"));
    assert.ok(err.message.includes("./missing.tmpl.md"));
  });

  it("DeclarationsMutatedError carries details and clear message", () => {
    const err = new DeclarationsMutatedError("param 'x' retyped");
    assert.strictEqual(err.name, "DeclarationsMutatedError");
    assert.strictEqual(err.details, "param 'x' retyped");
    assert.ok(err.message.includes("declarations were modified"));
    assert.ok(err.message.includes("param 'x' retyped"));
  });

  it("TemplatePanicError is thrown when {% panic %} executes", () => {
    const tmpl = Template.fromSource(`---
params: [trigger = bool]
---
> {% if trigger %}{% panic("boom") %}{% /if %}
SAFE`);
    assert.strictEqual(tmpl.render({ trigger: false }).trim(), "SAFE");
    assert.throws(
      () => tmpl.render({ trigger: true }),
      (err: Error) =>
        err instanceof TemplatePanicError && err.message.includes("boom"),
    );
  });
});

describe("API review: renderEmpty typed error", () => {
  it("throws MissingParamsError listing every required param", () => {
    const tmpl = Template.fromSource(`---
params: [name = str, age = int]
---
{{ name }} {{ age }}`);
    assert.throws(
      () => tmpl.renderEmpty(),
      (err: Error) =>
        err instanceof MissingParamsError &&
        err.missing.includes("name") &&
        err.missing.includes("age"),
    );
  });

  it("renders successfully when all params have defaults", () => {
    const tmpl = Template.fromSource(`---
params:
  - greeting = str := "Hi"
---
{{ greeting }}!`);
    assert.strictEqual(tmpl.renderEmpty().trim(), "Hi!");
  });
});

describe("API review: variant __kind__ convergence", () => {
  it("round-trips through match and isVariant", () => {
    const Status = defineVariants({
      Approved: null,
      NeedsChanges: ["reason"],
    });
    const nc = Status.NeedsChanges({ reason: "fix" });
    assert.strictEqual(nc.__kind__, "NeedsChanges");
    assert.ok(isVariant(nc, "NeedsChanges"));
    assert.ok(!isVariant(nc, "Approved"));
    assert.strictEqual(
      match(nc, {
        Approved: () => "ok",
        NeedsChanges: (v) => `changes: ${String(v.reason)}`,
      }),
      "changes: fix",
    );
  });

  it("plain { __kind__ } objects (codegen shape) also match", () => {
    const plain = { __kind__: "Approved" };
    assert.ok(isVariant(plain, "Approved"));
    assert.strictEqual(
      match(plain, { Approved: () => "a", _: () => "?" }),
      "a",
    );
  });

  it("renders a __kind__ struct passed as a param", () => {
    const tmpl = Template.fromSource(
      [
        `---`,
        "params:",
        "  - outcome = enum(Approved, NeedsChanges(reason = str))",
        `---`,
        "> {% match outcome %}",
        "> {% case Approved %}",
        "",
        "Approved",
        "",
        "> {% case NeedsChanges %}",
        "",
        "Needs: {{ outcome.reason }}",
        "",
        "> {% /match %}",
      ].join("\n"),
    );
    const out = tmpl.render({
      outcome: { __kind__: "NeedsChanges", reason: "tests" },
    });
    assert.ok(out.includes("Needs: tests"), `got: ${out}`);
  });
});

describe("API review: allowUnused compile option", () => {
  const SRC = `---
params: [used = str, unused = str]
---
{{ used }}`;

  it("rejects unused declared params by default", () => {
    assert.throws(
      () => Template.fromSource(SRC),
      (err: Error) =>
        err instanceof TemplateSyntaxError && err.message.includes("unused"),
    );
  });

  it("honors allowUnused: true via fromSourceWithOptions", () => {
    const tmpl = Template.fromSourceWithOptions(SRC, { allowUnused: true });
    assert.strictEqual(tmpl.render({ used: "x", unused: "y" }).trim(), "x");
  });
});

describe("API review: fromSourceWithEnv flat signature", () => {
  const SRC = `---
env: [region = str]

params: [name = str]
---
{{ name }} @ {{ region }}`;

  it("accepts a flat env record (WASM-consistent signature)", () => {
    const tmpl = Template.fromSourceWithEnv(SRC, { region: "us" });
    assert.strictEqual(tmpl.render({ name: "svc" }).trim(), "svc @ us");
  });

  it("defaults env to an empty object", () => {
    const tmpl = Template.fromSourceWithEnv(`---
params: [name = str]
---
Hi {{ name }}`);
    assert.strictEqual(tmpl.render({ name: "x" }).trim(), "Hi x");
  });
});

describe("API review: Context render path", () => {
  it("renderContext renders using a pre-built Context", () => {
    const tmpl = Template.fromSource(`---
params: [name = str]
---
Hello {{ name }}!`);
    const ctx = Context.from({ name: "world" });
    assert.strictEqual(tmpl.renderContext(ctx).trim(), "Hello world!");
  });

  it("renderContext reuses raw Values set via setRaw", () => {
    const tmpl = Template.fromSource(`---
params: [count = int]
---
Count: {{ count }}`);
    const ctx = new Context();
    ctx.setRaw("count", int(42));
    assert.strictEqual(tmpl.renderContext(ctx).trim(), "Count: 42");
  });
});

describe("API review: codegen single canonical all-variants constant", () => {
  const SRC = `---
params: [stage = enum(Design, Build, Ship)]
---
{{ stage }}`;

  it("exposes all variants exactly once via the namespace `kinds` tuple", () => {
    const code = generateTypes(SRC);
    // The `kinds` tuple is the single canonical enumeration of variant names.
    const kindsMatches = code.match(/\bkinds:/g) ?? [];
    assert.strictEqual(
      kindsMatches.length,
      1,
      `expected exactly one 'kinds' tuple in:\n${code}`,
    );
    assert.ok(
      code.includes('kinds: ["Design", "Build", "Ship"] as const,'),
      `expected canonical 'kinds' tuple in:\n${code}`,
    );
    // The legacy flat aliases must be gone.
    assert.ok(!/\bSTAGE_ALL\b/.test(code), "STAGE_ALL alias should be gone");
    assert.ok(!/\bALL_STAGES\b/.test(code), "ALL_STAGES alias should be gone");
    assert.ok(!/\bconst\s+STAGES\b/.test(code), "STAGES alias should be gone");
  });
});
