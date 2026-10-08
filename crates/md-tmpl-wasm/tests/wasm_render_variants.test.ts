/**
 * WASM binding tests: JSON, empty, unchecked, and allowing-extra render entry points.
 *
 * Uses Node.js built-in test runner (`node:test`) and `node:assert/strict`.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template as WasmTemplate } from "../pkg/md_tmpl_wasm.js";

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
  - config = struct(host = str, port = int)
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
  - items = list(name = str)
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
// 27. API review — renderEmpty
// =========================================================================

describe("Template.renderEmpty", () => {
  it("renders a template with no params", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Hello world!`,
    );
    assert.equal(t.renderEmpty(), "Hello world!");
  });

  it("renders using declared defaults", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - greeting = str := "Hi"
  - count = int := 5
---
{{ greeting }} {{ count }}`,
    );
    assert.equal(t.renderEmpty(), "Hi 5");
  });

  it("throws (missing_params) when a required param has no default", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(
      () => t.renderEmpty(),
      (err: unknown) => {
        assert.ok(err instanceof Error, "should be a real Error instance");
        assert.equal((err as Error).name, "missing_params");
        assert.ok((err as Error).message.includes("name"));
        return true;
      },
    );
  });

  it("matches render({}) for an all-defaults template", () => {
    const source = `---
params:
  - greeting = str := "Hey"
---
{{ greeting }}!`;
    const t = WasmTemplate.fromSource(source);
    assert.equal(t.renderEmpty(), t.render({}));
  });
});

// =========================================================================
// 28. API review — renderUnchecked skips ALL validation
// =========================================================================

describe("Template.renderUnchecked (skips all validation)", () => {
  it("does NOT throw on a wrong-typed param and renders it as-is", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
value={{ n }}`,
    );
    // Strict render would reject a string for an int param; unchecked skips it.
    assert.throws(() => t.render({ n: "oops" }));
    assert.equal(t.renderUnchecked({ n: "oops" }), "value=oops");
  });

  it("does NOT throw on extra undeclared params", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.equal(
      t.renderUnchecked({ name: "world", extra: "ignored" }),
      "Hello world!",
    );
  });

  it("renderUncheckedJson mirrors renderUnchecked (wrong type tolerated)", () => {
    const source = `---
params:
  - n = int
---
value={{ n }}`;
    const t = WasmTemplate.fromSource(source);
    const params = { n: "oops" };
    assert.equal(
      t.renderUncheckedJson(JSON.stringify(params)),
      t.renderUnchecked(params),
    );
  });
});

// =========================================================================
// 29. API review — renderAllowingExtra family
// =========================================================================

describe("Template.renderAllowingExtra", () => {
  it("ignores extra params but still renders", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.equal(
      t.renderAllowingExtra({ name: "world", extra: "ok" }),
      "Hello world!",
    );
  });

  it("still validates missing required params (missing_params)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(
      () => t.renderAllowingExtra({ extra: "ok" }),
      (err: unknown) => {
        assert.ok(err instanceof Error);
        assert.equal((err as Error).name, "missing_params");
        return true;
      },
    );
  });

  it("still validates types (type_mismatch)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
{{ n }}`,
    );
    assert.throws(
      () => t.renderAllowingExtra({ n: "not_a_number", extra: "ok" }),
      (err: unknown) => {
        assert.ok(err instanceof Error);
        assert.equal((err as Error).name, "type_mismatch");
        return true;
      },
    );
  });

  it("matches render() when there are no extra params", () => {
    const source = `---
params:
  - name = str
---
Hello {{ name }}!`;
    const t = WasmTemplate.fromSource(source);
    const params = { name: "world" };
    assert.equal(t.renderAllowingExtra(params), t.render(params));
  });

  it("renderAllowingExtraJson ignores extras and validates missing", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.equal(
      t.renderAllowingExtraJson(JSON.stringify({ name: "world", extra: "x" })),
      "Hello world!",
    );
    assert.throws(() =>
      t.renderAllowingExtraJson(JSON.stringify({ extra: "x" })),
    );
  });

  it("renderAllowingExtraFlexbuffers is exposed and throws cleanly on garbage", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.strictEqual(
      typeof t.renderAllowingExtraFlexbuffers,
      "function",
    );
    assert.throws(
      () => t.renderAllowingExtraFlexbuffers(new Uint8Array([0x00, 0x01, 0x02])),
      /flexbuffers/i,
    );
  });
});
