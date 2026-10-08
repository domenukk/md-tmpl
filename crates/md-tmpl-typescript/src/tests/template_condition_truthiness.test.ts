/**
 * Tests for the md-tmpl TypeScript bindings: condition truthiness and type checking.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template } from "../index.js";

describe("Condition Truthiness and Type Checking", () => {
  it("accepts scalar str in condition and evaluates truthiness", () => {
    const src = `---
params:
  - name = str
---
> {% if name %}hello {{ name }}{% /if %}`;
    const tmpl = Template.fromSource(src);
    assert.strictEqual(tmpl.render({ name: "" }), "");
    assert.strictEqual(tmpl.render({ name: "Alice" }), "hello Alice");
  });

  it("accepts scalar int in condition and evaluates non-zero truthiness", () => {
    const src = `---
params:
  - count = int
---
> {% if count %}yes{% else %}no{% /if %}`;
    const tmpl = Template.fromSource(src);
    assert.strictEqual(tmpl.render({ count: 0 }), "no");
    assert.strictEqual(tmpl.render({ count: 5 }), "yes");
  });

  it("rejects option(str) in bare condition at compile time", () => {
    const src = `---
params:
  - opt = option(str) := None
---
> {% if opt %} · {{ opt }}{% /if %}`;
    assert.throws(
      () => Template.fromSource(src),
      /cannot evaluate truthiness of option/,
    );
  });

  it("accepts list in bare condition for non-empty truthiness check", () => {
    const src = `---
params:
  - items = list(str)
---
> {% if items %}items present{% else %}empty{% /if %}`;
    const tmpl = Template.fromSource(src);
    assert.strictEqual(tmpl.render({ items: [] }), "empty");
    assert.strictEqual(tmpl.render({ items: ["a"] }), "items present");
  });

  it("rejects has() on non-option types (str, list, int, struct) at compile time", () => {
    const srcStr = `---
params:
  - title = str
---
> {% if has(title) %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcStr),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("'has()' requires an option type"),
    );

    const srcList = `---
params:
  - items = list(str)
---
> {% if has(items) %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcList),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("'has()' requires an option type"),
    );

    const srcInt = `---
params:
  - count = int
---
> {% if has(count) %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcInt),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("'has()' requires an option type"),
    );

    const srcStruct = `---
params:
  - info = struct(id = int)
---
> {% if has(info) %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcStruct),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("'has()' requires an option type"),
    );

    const srcTmpl = `---
params:
  - t = tmpl()
---
> {% if has(t) %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcTmpl),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("'has()' requires an option type"),
    );

    const srcEnum = `---
params:
  - stage = enum(Design, Build)
---
> {% if has(stage) %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcEnum),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("'has()' requires an option type"),
    );
  });

  it("rejects bare struct, enum, or tmpl in a condition (no truthiness)", () => {
    const srcStruct = `---
params:
  - info = struct(id = int)
---
> {% if info %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcStruct),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("cannot evaluate truthiness of struct"),
    );

    const srcEnum = `---
params:
  - stage = enum(Design, Build)
---
> {% if stage %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcEnum),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("cannot evaluate truthiness of enum"),
    );

    const srcTmpl = `---
params:
  - t = tmpl()
---
> {% if t %}hi{% /if %}`;
    assert.throws(
      () => Template.fromSource(srcTmpl),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("cannot evaluate truthiness of template handle"),
    );
  });

  it("rejects limit filter on string at render time", () => {
    const src = `---
params:
  - name = str
---
{{ name | limit(2) }}`;
    const tmpl = Template.fromSource(src);
    assert.throws(
      () => tmpl.render({ name: "hello" }),
      (err: unknown) =>
        err instanceof Error && err.message.includes("'limit' requires a list"),
    );
  });

  it("rejects kind() on plain str parameter at render time", () => {
    const src = `---
params:
  - msg = str
---
{{ kind(msg) }}`;
    const tmpl = Template.fromSource(src);
    assert.throws(
      () => tmpl.render({ msg: "hello" }),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("kind() requires an enum or option value"),
    );
  });

  it("rejects has() on plain str parameter at render time", () => {
    const src = `---
params:
  - msg = str
---
{{ has(msg) }}`;
    const tmpl = Template.fromSource(src);
    assert.throws(
      () => tmpl.render({ msg: "hello" }),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("has() requires an option value"),
    );
  });

  it("rejects wildcard _ in {% case %}", () => {
    const src = `---
params:
  - outcome = enum(Confirmed, NotConfirmed, Pending)
---
> {% match outcome %}
> {% case Confirmed %}

confirmed

> {% case _ %}

fallback

> {% /match %}`;
    assert.throws(
      () => Template.fromSource(src),
      (err: unknown) =>
        err instanceof Error &&
        err.message.includes("wildcard '_' in {% case %} is not supported"),
    );
  });

  it("rejects non-exhaustive option match without else arm", () => {
    const src = `---
params:
  - opt = option(str)
---
> {% match opt %}
> {% case Some && opt == "special" %}

special

> {% case Some %}

has: {{ opt }}

> {% /match %}`;
    assert.throws(
      () => Template.fromSource(src),
      (err: unknown) =>
        err instanceof Error && err.message.includes("non-exhaustive"),
    );
  });

  it("accepts match with else arm fallback", () => {
    const src = `---
params:
  - outcome = enum(Confirmed, NotConfirmed, Pending)
---
> {% match outcome %}
> {% case Confirmed %}

confirmed

> {% else %}

fallback

> {% /match %}`;
    const tmpl = Template.fromSource(src);
    assert.strictEqual(tmpl.render({ outcome: "Pending" }).trim(), "fallback");
  });
});
