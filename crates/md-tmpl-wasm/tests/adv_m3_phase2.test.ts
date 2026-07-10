/**
 * Tier 5 White-box Adversarial Coverage Tests for Milestone M3 Phase 2 (WASM bindings).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template as WasmTemplate } from "../pkg/md_tmpl_wasm.js";

describe("Tier 5 Phase 2 White-box Adversarial Coverage — WASM", () => {
  describe("1. Include Depth Configuration API", () => {
    it("WasmTemplate instances support setMaxIncludeDepth", () => {
      const tmpl = WasmTemplate.fromSource("---\nparams:\n  - x = int\n---\nHello world {{ x }}");
      assert.strictEqual(
        // NOLINT: testing WASM binding name variants requires dynamic property access
        typeof (tmpl as any).setMaxIncludeDepth,
        "function",
        "WasmTemplate should have setMaxIncludeDepth method",
      );
      assert.strictEqual(
        // NOLINT: testing WASM binding name variants requires dynamic property access
        typeof (tmpl as any).set_max_include_depth,
        "function",
        "WasmTemplate should have set_max_include_depth method",
      );
      // NOLINT: testing WASM binding name variants requires dynamic property access
      (tmpl as any).setMaxIncludeDepth(10);
      // NOLINT: testing WASM binding name variants requires dynamic property access
      (tmpl as any).set_max_include_depth(5);
    });
  });

  describe("2. Flexbuffers API Malformed Buffer Handling", () => {
    it("renderFlexbuffers handles malformed byte buffers cleanly without panicking", () => {
      const src = `---
params:
  - name = str
---
Hello {{ name }}`;
      const tmpl = WasmTemplate.fromSource(src);
      
      // Empty buffer
      assert.throws(
        () => {
          tmpl.renderFlexbuffers(new Uint8Array([]));
        },
        /flexbuffers/i,
        "Expected renderFlexbuffers to throw clean error on empty buffer",
      );

      // Garbage bytes
      assert.throws(
        () => {
          tmpl.renderFlexbuffers(new Uint8Array([0xde, 0xad, 0xbe, 0xef, 0x00, 0xff]));
        },
        /flexbuffers/i,
        "Expected renderFlexbuffers to throw clean error on garbage bytes",
      );
    });

    it("renderUncheckedFlexbuffers handles malformed byte buffers cleanly without panicking", () => {
      const src = `---
params:
  - name = str
---
Hello {{ name }}`;
      const tmpl = WasmTemplate.fromSource(src);

      assert.throws(
        () => {
          tmpl.renderUncheckedFlexbuffers(new Uint8Array([0x00, 0x01, 0x02]));
        },
        /flexbuffers/i,
        "Expected renderUncheckedFlexbuffers to throw clean error on garbage bytes",
      );
    });
  });

  describe("3. Unchecked Rendering with Inline Templates (Parity Verification)", () => {
    it("renderUnchecked in WASM preserves inline template includes (unlike TS)", () => {
      const src = `---
params:
  - name = str
---
> {% tmpl greeting %}

Hello {{ name }}!

> {% /tmpl %}

> {% include greeting with name=name %}`;

      const tmpl = WasmTemplate.fromSource(src);
      const params = { name: "Alice" };

      const checked = tmpl.render(params);
      assert.strictEqual(checked, "Hello Alice!\n");

      // In WASM, renderUnchecked delegates directly to Rust render_ctx_unchecked,
      // so it correctly outputs the inline template (unlike TS renderUnchecked).
      const unchecked = tmpl.renderUnchecked(params);
      assert.strictEqual(unchecked, "Hello Alice!\n");
    });
  });
});
