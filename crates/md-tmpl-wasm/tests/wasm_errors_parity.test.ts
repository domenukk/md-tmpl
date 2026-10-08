/**
 * WASM binding tests: WASM vs TS parity, resource disposal, and error reporting.
 *
 * Uses Node.js built-in test runner (`node:test`) and `node:assert/strict`.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template as WasmTemplate } from "../pkg/md_tmpl_wasm.js";
import { Template as TsTemplate } from "../../md-tmpl-typescript/dist/index.js";

/** Normalize whitespace for content-level comparison. */
function normalize(s) {
  return s.replace(/\n+/g, "\n").trim();
}

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
  - items = list(name = str)
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
// 30. API review — errors are real JS Error objects tagged with ErrorKind
// =========================================================================

describe("Errors are real Error instances with stable names", () => {
  function expectError(fn: () => unknown, expectedName: string): void {
    assert.throws(fn, (err: unknown) => {
      assert.ok(err instanceof Error, "thrown value must be an Error instance");
      assert.equal(
        (err as Error).name,
        expectedName,
        `expected error name '${expectedName}', got '${(err as Error).name}'`,
      );
      assert.ok(
        (err as Error).message.length > 0,
        "error message should be non-empty",
      );
      return true;
    });
  }

  it("fromSource syntax error -> name 'syntax'", () => {
    expectError(
      () =>
        WasmTemplate.fromSource(`---
params:
  - = bad
---
Hello`),
      "syntax",
    );
  });

  it("render missing param -> name 'missing_params'", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - username = str
---
Hello {{ username }}!`,
    );
    expectError(() => t.render({}), "missing_params");
  });

  it("render type mismatch -> name 'type_mismatch'", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
{{ n }}`,
    );
    expectError(() => t.render({ n: "nope" }), "type_mismatch");
  });

  it("render extra param -> name 'extra_params'", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    expectError(() => t.render({ name: "world", extra: "oops" }), "extra_params");
  });

  it("panic -> name 'panic'", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
> {% panic("halt") %}
`,
    );
    expectError(() => t.render({}), "panic");
  });

  it("renderFlexbuffers on garbage -> real Error (name 'syntax')", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    expectError(
      () => t.renderFlexbuffers(new Uint8Array([0xde, 0xad, 0xbe, 0xef])),
      "syntax",
    );
  });
});

// =========================================================================
// 31. API review — errors expose a stable machine-readable `kind` property
// =========================================================================

describe("Errors expose a stable machine-readable kind property", () => {
  function expectKind(fn: () => unknown, expectedKind: string): void {
    assert.throws(fn, (err: unknown) => {
      assert.ok(err instanceof Error, "thrown value must be an Error instance");
      assert.equal(
        (err as Error & { kind?: unknown }).kind,
        expectedKind,
        `expected kind '${expectedKind}', got '${
          (err as Error & { kind?: unknown }).kind
        }'`,
      );
      return true;
    });
  }

  it("render missing param -> kind 'missing_params'", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - username = str
---
Hello {{ username }}!`,
    );
    expectKind(() => t.render({}), "missing_params");
  });

  it("render type mismatch -> kind 'type_mismatch'", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
{{ n }}`,
    );
    expectKind(() => t.render({ n: "nope" }), "type_mismatch");
  });

  it("fromSource syntax error -> kind 'syntax'", () => {
    expectKind(
      () =>
        WasmTemplate.fromSource(`---
params:
  - = bad
---
Hello`),
      "syntax",
    );
  });
});
