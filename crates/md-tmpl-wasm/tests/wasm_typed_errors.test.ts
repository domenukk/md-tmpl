/**
 * WASM binding: errors carry the typed payload (`MdTmplError`) with the same
 * property names and values as the pure-TS engine's error classes.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  Template as WasmTemplate,
  isMdTmplError,
  type MdTmplError,
} from "../pkg/md_tmpl_wasm.js";
import { Template as TsTemplate } from "../../md-tmpl-typescript/dist/index.js";

/** Run `fn`, assert it throws a typed `MdTmplError`, and return it. */
function catchWasm(fn: () => unknown): MdTmplError {
  try {
    fn();
  } catch (err) {
    assert.ok(isMdTmplError(err), "expected an MdTmplError");
    return err;
  }
  assert.fail("expected an error");
}

/** Run `fn` and return the thrown TS-engine error as a property bag. */
function catchTs(fn: () => unknown): Record<string, unknown> {
  try {
    fn();
  } catch (err) {
    return err as Record<string, unknown>;
  }
  assert.fail("expected an error");
}

const UNKNOWN_FILTER = `---
params:
  - name = str
---
intro
{{ name | bogus }}`;

const TWO_PARAMS = `---
params:
  - a = str
  - b = str
---
{{ a }}{{ b }}`;

const INT_PARAM = `---
params:
  - n = int
---
{{ n }}`;

describe("WASM typed errors", () => {
  it("syntax errors carry a file-absolute line and snippet (TS parity)", () => {
    const w = catchWasm(() => WasmTemplate.fromSource(UNKNOWN_FILTER));
    const t = catchTs(() => TsTemplate.fromSource(UNKNOWN_FILTER));
    assert.equal(w.kind, "syntax");
    assert.equal(t.kind, "syntax");
    assert.equal(w.line, 6);
    assert.equal(t.line, 6);
    assert.equal(w.snippet, "{{ name | bogus }}");
    assert.equal(t.snippet, "{{ name | bogus }}");
  });

  it("missing_params carries the missing names (TS parity)", () => {
    const w = catchWasm(() => WasmTemplate.fromSource(TWO_PARAMS).render({}));
    const t = catchTs(() => TsTemplate.fromSource(TWO_PARAMS).render({}));
    assert.equal(w.kind, "missing_params");
    assert.deepEqual([...(w.missing ?? [])], ["a", "b"]);
    assert.deepEqual([...((t.missing as string[] | undefined) ?? [])], ["a", "b"]);
  });

  it("extra_params carries the extra names", () => {
    const w = catchWasm(() =>
      WasmTemplate.fromSource(TWO_PARAMS).render({ a: "x", b: "y", zz: 1 }),
    );
    assert.equal(w.kind, "extra_params");
    assert.deepEqual([...(w.extra ?? [])], ["zz"]);
  });

  it("type_mismatch carries path / expected / actual (TS parity)", () => {
    const w = catchWasm(() =>
      WasmTemplate.fromSource(INT_PARAM).render({ n: "nope" }),
    );
    const t = catchTs(() => TsTemplate.fromSource(INT_PARAM).render({ n: "nope" }));
    assert.equal(w.kind, "type_mismatch");
    assert.equal(w.path, "n");
    assert.equal(w.expected, "int");
    assert.equal(w.actual, "str");
    assert.equal(t.path, "n");
    assert.equal(t.expected, "int");
    assert.equal(t.actual, "str");
  });

  it("isMdTmplError rejects foreign errors", () => {
    assert.equal(isMdTmplError(new Error("x")), false);
    assert.equal(isMdTmplError("x"), false);
  });
});
