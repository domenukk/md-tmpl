import { describe, it } from "node:test";
import assert from "node:assert";
import { Template } from "../index.js";

describe("Tier 5 Phase 2 White-box Adversarial Coverage — TypeScript", () => {
  describe("1. Unchecked Rendering Parity with Inline Templates (Bug Verification)", () => {
    it("renderUnchecked silently drops inline template includes compared to checked render", () => {
      const src = `---
params:
  - name = str
---
> {% tmpl greeting %}

Hello {{ name }}!

> {% /tmpl %}

> {% include greeting with name=name %}`;

      const tmpl = Template.fromSource(src);
      const params = { name: "Alice" };

      // 1. Checked render works as expected and matches Rust/WASM parity
      const checkedOutput = tmpl.render(params);
      assert.strictEqual(checkedOutput, "Hello Alice!\n");

      // 2. Unchecked render in TS now correctly renders inline template includes, matching render().
      const uncheckedOutput = tmpl.renderUnchecked(params);
      assert.strictEqual(
        uncheckedOutput,
        checkedOutput,
        "Confirmed match between render() and renderUnchecked() on inline templates",
      );
    });
  });

  describe("2. Cyclic Data Structure Handling (Cycle Detection Verification)", () => {
    it("passing self-referential cyclic object throws TemplateError cleanly instead of stack overflow", () => {
      const src = `---
params:
  - data = struct(self = str)
---
> {% if data %}

Hello!

> {% /if %}`;
      const tmpl = Template.fromSource(src);
      interface Cyclic {
        self?: Cyclic;
      }
      const cyclic: Cyclic = {};
      cyclic.self = cyclic;

      assert.throws(
        () => {
          tmpl.render({ data: cyclic });
        },
        (err: unknown) => {
          assert.ok(err instanceof Error);
          assert.strictEqual(err.name, "TemplateError");
          assert.ok(
            err.message.includes(
              "cyclic object detected in template parameter",
            ),
          );
          return true;
        },
        "Expected cyclic object to throw TemplateError cleanly",
      );
    });
  });

  describe("3. Deeply Nested Struct & Array Stress Tests", () => {
    it("handles 10-level deep struct property access without error", () => {
      const src = `---
params:
  - l1 = struct(l2 = struct(l3 = struct(l4 = struct(l5 = struct(l6 = struct(l7 = struct(l8 = struct(l9 = struct(l10 = str)))))))))
---
Deep value: {{ l1.l2.l3.l4.l5.l6.l7.l8.l9.l10 }}`;
      const tmpl = Template.fromSource(src);
      const data = {
        l1: {
          l2: {
            l3: {
              l4: { l5: { l6: { l7: { l8: { l9: { l10: "success" } } } } } },
            },
          },
        },
      };
      assert.strictEqual(tmpl.render(data), "Deep value: success");
      assert.strictEqual(tmpl.renderUnchecked(data), "Deep value: success");
    });

    it("handles large array iteration (1000 items) in both render paths", () => {
      const src = `---
params:
  - items = list(val = int)
---
> {% for item in items %}

{{ item.val }},

> {% /for %}`;
      const tmpl = Template.fromSource(src);
      const items = Array.from({ length: 1000 }, (_, i) => ({ val: i }));
      const expected = items.map((i) => `${String(i.val)},\n`).join("");
      assert.strictEqual(tmpl.render({ items }), expected);
      assert.strictEqual(tmpl.renderUnchecked({ items }), expected);
    });
  });
});
