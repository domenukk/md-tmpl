/**
 * Tests for the md-tmpl TypeScript bindings: code generation, constants, filter chains, type aliases and includes.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template, TypedTemplate } from "../index.js";
import { generateTypes, inferTypes } from "../codegen.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

describe("Code generation", () => {
  const CODEGEN_SRC = [
    `---`,
    "params:",
    "  - name = str",
    "  - count = int",
    "  - tasks = list(title = str, priority = str)",
    "  - outcome = enum(Confirmed(evidence = str), Rejected)",
    `---`,
    "Hello {{ name }}!",
  ].join("\n");

  it("generateTypes produces valid TypeScript", () => {
    const code = generateTypes(CODEGEN_SRC);
    assert.ok(code.includes("interface Params"));
    assert.ok(code.includes("readonly name: string"));
    assert.ok(code.includes("readonly count: number"));
  });

  it("generates list item interfaces", () => {
    const code = generateTypes(CODEGEN_SRC);
    assert.ok(code.includes("interface TasksItem"));
    assert.ok(code.includes("readonly title: string"));
    assert.ok(code.includes("readonly priority: string"));
  });

  it("generates enum types", () => {
    const code = generateTypes(CODEGEN_SRC);
    assert.ok(code.includes("type Outcome"));
    assert.ok(code.includes('"Rejected"'));
    assert.ok(code.includes("Outcome_Confirmed"));
    assert.ok(code.includes('readonly __kind__: "Confirmed"'));
  });

  it("emits a companion namespace with a data-variant constructor", () => {
    const code = generateTypes(CODEGEN_SRC);
    // The enum's companion object is a single `const Outcome = { ... }`.
    assert.ok(
      code.includes("export const Outcome = {"),
      `expected companion namespace 'Outcome' in:\n${code}`,
    );
    // Data variant → factory taking a typed `fields` object.
    assert.ok(
      code.includes(
        "Confirmed: (fields: { evidence: string }): Outcome_Confirmed =>",
      ),
      `expected typed 'Confirmed' constructor in:\n${code}`,
    );
    // The constructor body attaches the reserved discriminant tag + spread.
    assert.ok(
      code.includes('{ __kind__: "Confirmed", ...fields }'),
      `expected __kind__-tagged body in:\n${code}`,
    );
  });

  it("emits unit variants as bare string constants in the namespace", () => {
    const code = generateTypes(CODEGEN_SRC);
    // Unit variant → a `V: "V" as const` entry (matches the bare-string wire form).
    assert.ok(
      code.includes('Rejected: "Rejected" as const,'),
      `expected unit constant 'Rejected' in:\n${code}`,
    );
  });

  it("emits kinds tuple, kindOf, is, and an exhaustive match", () => {
    const code = generateTypes(CODEGEN_SRC);
    assert.ok(
      code.includes('kinds: ["Confirmed", "Rejected"] as const,'),
      `expected 'kinds' tuple in:\n${code}`,
    );
    assert.ok(
      code.includes("kindOf: (v: Outcome): OutcomeKind =>"),
      `expected 'kindOf' helper in:\n${code}`,
    );
    assert.ok(
      code.includes("is: (v: Outcome, k: OutcomeKind): boolean =>"),
      `expected 'is' helper in:\n${code}`,
    );
    assert.ok(
      code.includes("match: <R>(v: Outcome, arms: {"),
      `expected generic 'match' helper in:\n${code}`,
    );
    // Both arms must be present and typed to their precise variant shapes.
    assert.ok(
      code.includes("Confirmed: (v: Outcome_Confirmed) => R"),
      `expected typed 'Confirmed' match arm in:\n${code}`,
    );
    assert.ok(
      code.includes('Rejected: (v: "Rejected") => R'),
      `expected typed 'Rejected' match arm in:\n${code}`,
    );
  });

  it("handles an enum with multiple data + unit variants", () => {
    const code = generateTypes(
      `---
params:
  - result = enum(Success(value = str), Failure(error = str, code = int), Unknown)
---
{{ result }}`,
    );
    // Both data-variant constructors, each typed to its own field object.
    assert.ok(
      code.includes("Success: (fields: { value: string }): Result_Success =>"),
      `expected Success constructor in:\n${code}`,
    );
    assert.ok(
      code.includes(
        "Failure: (fields: { error: string; code: number }): Result_Failure =>",
      ),
      `expected Failure constructor with both fields in:\n${code}`,
    );
    // Unit variant const.
    assert.ok(
      code.includes('Unknown: "Unknown" as const,'),
      `expected 'Unknown' unit constant in:\n${code}`,
    );
    // kinds enumerates every tag in declaration order.
    assert.ok(
      code.includes('kinds: ["Success", "Failure", "Unknown"] as const,'),
      `expected ordered 'kinds' tuple in:\n${code}`,
    );
  });

  it("companion namespace respects exportTypes: false", () => {
    const code = generateTypes(CODEGEN_SRC, { exportTypes: false });
    // No `export` on any generated declaration.
    assert.ok(
      !code.includes("export const Outcome"),
      `expected no export on Outcome namespace:\n${code}`,
    );
    assert.ok(
      !code.includes("export type Outcome"),
      `expected no export on Outcome type:\n${code}`,
    );
    assert.ok(!/\bexport\b/.test(code), `expected zero exports in:\n${code}`);
    // But the namespace itself is still emitted.
    assert.ok(
      code.includes("const Outcome = {"),
      `expected Outcome namespace:\n${code}`,
    );
  });

  it("generates render helper", () => {
    const code = generateTypes(CODEGEN_SRC);
    assert.ok(code.includes("function render"));
  });

  it("respects options", () => {
    const code = generateTypes(CODEGEN_SRC, {
      paramsName: "MyParams",
      exportTypes: false,
      includeRenderHelper: false,
    });
    assert.ok(code.includes("interface MyParams"));
    assert.ok(!code.includes("export "));
    assert.ok(!code.includes("function render"));
  });

  it("marks optional params with defaults", () => {
    const code = generateTypes(
      `---
params:
  - name = str := "World"
---
{{ name }}`,
    );
    assert.ok(code.includes("readonly name?: string"));
  });

  it("inferTypes returns structured result", () => {
    const result = inferTypes(CODEGEN_SRC);
    assert.strictEqual(result.fields.length, 4);
    assert.strictEqual(defined(result.fields[0]).name, "name");
    assert.strictEqual(defined(result.fields[0]).tsType, "string");
    assert.strictEqual(defined(result.fields[1]).name, "count");
    assert.strictEqual(defined(result.fields[1]).tsType, "number");
  });

  it("inferTypes returns correct enum type", () => {
    const result = inferTypes(CODEGEN_SRC);
    const outcomeField = result.fields.find((f) => f.name === "outcome");
    assert.ok(outcomeField);
    assert.ok(outcomeField.tsType.includes('"Rejected"'));
    assert.ok(outcomeField.tsType.includes('__kind__: "Confirmed"'));
  });

  it("handles struct params", () => {
    const code = generateTypes(
      `---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}`,
    );
    assert.ok(code.includes("interface Config"));
    assert.ok(code.includes("readonly host: string"));
    assert.ok(code.includes("readonly port: number"));
  });

  it("emits type aliases from types: block", () => {
    const src = [
      `---`,
      "types:",
      "  - Status = enum(Active, Inactive, Pending)",
      "",
      "params:",
      "  - status = Status",
      `---`,
      "{{ status }}",
    ].join("\n");
    const code = generateTypes(src);
    // Should define the Status type
    assert.ok(
      code.includes("type Status"),
      `expected 'type Status' in:\n${code}`,
    );
    assert.ok(code.includes('"Active"'));
    assert.ok(code.includes('"Inactive"'));
    assert.ok(code.includes('"Pending"'));
    // Params should reference Status by name
    assert.ok(code.includes("readonly status: Status"));
  });

  it("emits struct type aliases", () => {
    const src = [
      `---`,
      "types:",
      "  - Config = struct(host = str, port = int)",
      "",
      "params:",
      "  - cfg = Config",
      `---`,
      "{{ cfg.host }}",
    ].join("\n");
    const code = generateTypes(src);
    assert.ok(
      code.includes("interface Config") || code.includes("type Config"),
    );
    assert.ok(code.includes("readonly cfg: Config"));
  });

  it("inferTypes resolves type aliases", () => {
    const src = [
      `---`,
      "types:",
      "  - Priority = enum(High, Medium, Low)",
      "",
      "params:",
      "  - p = Priority",
      `---`,
      "{{ p }}",
    ].join("\n");
    const result = inferTypes(src);
    assert.strictEqual(result.fields.length, 1);
    // inferTypes should resolve the alias inline
    assert.ok(defined(result.fields[0]).tsType.includes('"High"'));
    assert.ok(defined(result.fields[0]).tsType.includes('"Low"'));
    // typeAliases should also be returned
    assert.strictEqual(result.typeAliases.length, 1);
    assert.strictEqual(defined(result.typeAliases[0]).name, "Priority");
  });
});

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

describe("Constants", () => {
  it("renders constant values", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - MAX = int := 100

params: []
---
Max: {{ MAX }}`,
    );
    assert.strictEqual(tmpl.render({}), "Max: 100");
  });

  it("renders string constant", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - GREETING = str := "hello"

params: []
---
{{ GREETING }}`,
    );
    assert.strictEqual(tmpl.render({}), "hello");
  });

  it("constants coexist with params", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - PREFIX = str := "Hello"

params:
  - name = str
---
{{ PREFIX }} {{ name }}!`,
    );
    assert.strictEqual(tmpl.render({ name: "World" }), "Hello World!");
  });

  it("consts() accessor returns constant values", () => {
    const tmpl = Template.fromSource(
      `---
consts:
  - MAX = int := 100
  - LABEL = str := "test"

params: []
---
{{ MAX }} {{ LABEL }}`,
    );
    const c = tmpl.consts();
    assert.strictEqual(c.MAX, 100);
    assert.strictEqual(c.LABEL, "test");
  });

  it("generateTypes emits CONSTANTS object", () => {
    const src = [
      `---`,
      "consts:",
      "  - MAX = int := 100",
      '  - PREFIX = str := "hello"',
      "",
      "params: [name = str]",
      `---`,
      "{{ PREFIX }} {{ name }} {{ MAX }}",
    ].join("\n");
    const code = generateTypes(src);
    // Should have a CONSTANTS block
    assert.ok(code.includes("CONSTANTS"), `expected CONSTANTS in:\n${code}`);
    assert.ok(code.includes("MAX: 100"));
    assert.ok(code.includes('PREFIX: "hello"'));
    assert.ok(code.includes("as const"));
  });

  it("generateTypes emits DEFAULTS object", () => {
    const src = [
      `---`,
      "params:",
      '  - greeting = str := "Hello"',
      "  - name = str",
      `---`,
      "{{ greeting }} {{ name }}",
    ].join("\n");
    const code = generateTypes(src);
    // greeting should be optional
    assert.ok(
      code.includes("greeting?:"),
      `expected 'greeting?:' in:\n${code}`,
    );
    // name should NOT be optional
    assert.ok(code.includes("readonly name:"));
    assert.ok(!code.includes("name?:"));
    // DEFAULTS block
    assert.ok(code.includes("DEFAULTS"), `expected DEFAULTS in:\n${code}`);
    assert.ok(code.includes('"Hello"'));
  });

  it("inferTypes returns consts", () => {
    const src = [
      `---`,
      "consts:",
      "  - VERSION = int := 42",
      "",
      "params: [x = str]",
      `---`,
      "v{{ VERSION }}: {{ x }}",
    ].join("\n");
    const result = inferTypes(src);
    assert.strictEqual(result.consts.length, 1);
    assert.strictEqual(defined(result.consts[0]).name, "VERSION");
    assert.strictEqual(defined(result.consts[0]).value, 42);
    assert.strictEqual(defined(result.consts[0]).tsType, "number");
  });

  it("inferTypes returns defaults", () => {
    const src = [
      `---`,
      "params:",
      '  - name = str := "World"',
      "  - count = int",
      `---`,
      "{{ name }} {{ count }}",
    ].join("\n");
    const result = inferTypes(src);
    assert.strictEqual(result.fields.length, 2);
    const nameField = defined(result.fields.find((f) => f.name === "name"));
    assert.ok(nameField.optional);
    assert.strictEqual(nameField.defaultValue, "World");
    const countField = defined(result.fields.find((f) => f.name === "count"));
    assert.ok(!countField.optional);
    assert.strictEqual(countField.defaultValue, undefined);
  });

  it("TypedTemplate.consts() delegates correctly", () => {
    const tmpl = TypedTemplate.fromSource<{ name: string }>(
      `---
consts:
  - VER = int := 3

params: [name = str]
---
{{ name }} v{{ VER }}`,
    );
    const c = tmpl.consts();
    assert.strictEqual(c.VER, 3);
    assert.strictEqual(tmpl.render({ name: "test" }), "test v3");
  });
});

// ---------------------------------------------------------------------------
// Filter chains
// ---------------------------------------------------------------------------

describe("Filter chains", () => {
  it("chains trim and upper", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = str]
---
{{ val | trim | upper }}`,
    );
    assert.strictEqual(tmpl.render({ val: "  hello world  " }), "HELLO WORLD");
  });

  it("chains trim and lower", () => {
    const tmpl = Template.fromSource(
      `---
params: [val = str]
---
{{ val | trim | lower }}`,
    );
    assert.strictEqual(tmpl.render({ val: "  HELLO  " }), "hello");
  });
});

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

describe("Type aliases", () => {
  it("resolves type aliases in params", () => {
    const src = [
      `---`,
      "types:",
      "  - Status = enum(Active, Inactive)",
      "",
      "params:",
      "  - user_status = Status",
      `---`,
      "> {% match user_status %}",
      "> {% case Active %}",
      "",
      "ACTIVE",

      "",
      "> {% case Inactive %}",
      "",
      "INACTIVE",

      "",
      "> {% /match %}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    assert.strictEqual(tmpl.render({ user_status: "Active" }).trim(), "ACTIVE");
  });

  it("resolves type aliases for type checking", () => {
    const src = [
      `---`,
      "types:",
      "  - Status = enum(Active, Inactive)",
      "",
      "params:",
      "  - status = Status",
      `---`,
      "> {% match status %}",
      "> {% case Active %}",
      "",
      "ok",

      "",
      "> {% case Inactive %}",
      "",
      "no",

      "",
      "> {% /match %}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    // Invalid variant should fail
    assert.throws(() => tmpl.render({ status: "Unknown" }));
  });

  it("type aliases appear in declarations", () => {
    const src = [
      `---`,
      "types:",
      "  - Priority = enum(High, Low)",
      "",
      "params:",
      "  - p = Priority",
      `---`,
      "{{ kind(p) }}",
    ].join("\n");
    const tmpl = Template.fromSource(src);
    const decls = tmpl.declarations();
    assert.strictEqual(decls.length, 1);
    assert.strictEqual(defined(decls[0])[0], "p");
    assert.strictEqual(defined(decls[0])[1], "Priority");
  });
});

// ---------------------------------------------------------------------------
// Includes (stub behavior)
// ---------------------------------------------------------------------------

describe("Includes", () => {
  it("parses include syntax without error", () => {
    // Include is parsed but rendering is a no-op in pure TS
    const src = [
      `---`,
      "params:",
      "  - title = str",
      `---`,
      "> {% include [header](./header.tmpl.md) with title=title %}",
      "",
      "Body: {{ title }}",
    ].join("\n");
    const tmpl = Template.fromSourceAllowingUnused(src);
    assert.ok(tmpl);
  });
});
