import { describe, it } from "node:test";
import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import {
  Context,
  ExtraParamsError,
  MissingParamsError,
  Template,
} from "../index.js";

describe("Tier 5 Phase 2 White-box Adversarial Coverage — TypeScript", () => {
  describe("1. Rendering Parity with Inline Templates", () => {
    it("renders inline template includes identically across render() and renderContext()", () => {
      const src = `---
params:
  - name = str
---
> {% tmpl greeting %}
---
params:
  - name = str
---
Hello {{ name }}!

> {% /tmpl %}

> {% include greeting with name=name %}`;

      const tmpl = Template.fromSource(src);
      const params = { name: "Alice" };

      const checkedOutput = tmpl.render(params);
      assert.strictEqual(checkedOutput, "Hello Alice!\n");

      const ctxOutput = tmpl.renderContext(Context.from(params));
      assert.strictEqual(
        ctxOutput,
        checkedOutput,
        "Confirmed match between render() and renderContext() on inline templates",
      );
    });
  });

  describe("2. Cyclic Data Structure Handling (Cycle Detection Verification)", () => {
    it("passing self-referential cyclic object throws TemplateError cleanly instead of stack overflow", () => {
      const src = `---
params:
  - data = option(struct(self = str)) := None
---
> {% if has(data) %}

{{ data.self }}

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
      assert.strictEqual(
        tmpl.renderContext(Context.from(data)),
        "Deep value: success",
      );
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
      assert.strictEqual(tmpl.renderContext(Context.from({ items })), expected);
    });
  });

  describe("4. Direct Renderer Parity with Imported Constants, Enums, kinds(), and Prefixes", () => {
    it("renders imported constants, imported enums, kinds(), and consts./params. prefixes identically in render() and renderEmpty()", () => {
      const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "md-tmpl-direct-"));
      try {
        const sharedPath = path.join(tmpDir, "shared_consts.tmpl.md");
        fs.writeFileSync(
          sharedPath,
          `---
types:
  - Stage = enum(Design, Build, Verify(owner = str))

consts:
  - MAX_RETRIES = int := 5
  - APP_NAME = str := "ARTIST"
---
`,
        );

        const childPath = path.join(tmpDir, "child.tmpl.md");
        fs.writeFileSync(
          childPath,
          `---
params:
  - label = str
---
[{{ label }}:{{ shared_consts.APP_NAME }}:{{ shared_consts.MAX_RETRIES }}]`,
        );

        const hostSrc = `---
imports:
  - "[shared_consts](./shared_consts.tmpl.md)"

types:
  - LocalMode = enum(Fast, Safe)

consts:
  - LOCAL_LIMIT = int := 10

params:
  - user = str := "alice"
---
App={{ shared_consts.APP_NAME }} Retries={{ consts.LOCAL_LIMIT }}/{{ shared_consts.MAX_RETRIES }}
Stage={{ kind(shared_consts.Stage.Design) }} Stages={{ kinds(shared_consts.Stage) | join(",") }}
LocalModes={{ kinds(LocalMode) | join("|") }} User={{ params.user }}

> {% include [child](./child.tmpl.md) with label=user %}`;

        const tmpl = Template.fromSourceWithBaseDir(hostSrc, tmpDir);
        const expected =
          "App=ARTIST Retries=10/5\n" +
          "Stage=Design Stages=Design,Build,Verify\n" +
          "LocalModes=Fast|Safe User=alice\n" +
          "[alice:ARTIST:5]";

        assert.strictEqual(tmpl.render({ user: "alice" }), expected);
        assert.strictEqual(tmpl.renderEmpty(), expected);
      } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      }
    });

    it("propagates imported type alias and enum variant sanitize policies into paramSanitize", () => {
      const tmpDir = fs.mkdtempSync(
        path.join(os.tmpdir(), "md-tmpl-sanitize-import-"),
      );
      try {
        const sharedPath = path.join(tmpDir, "shared_types.tmpl.md");
        fs.writeFileSync(
          sharedPath,
          `---
types:
  - UntrustedBio = untrusted str
  - SafeCard = struct(body = str | sanitize("card", ""))
  - Verdict = enum(Confirmed(evidence = untrusted str), Rejected)
---
`,
        );

        const hostSrc = `---
imports:
  - "[shared_types](./shared_types.tmpl.md)"

params:
  - bio = shared_types.UntrustedBio
  - card = shared_types.SafeCard
  - verdict = shared_types.Verdict
---
<profile>{{ bio }}</profile>
{{ card.body }}

> {% match verdict %}
> {% case Confirmed %}

<proof>{{ verdict.evidence }}</proof>

> {% case Rejected %}

Rejected

> {% /match %}`;

        const tmpl = Template.fromSourceWithBaseDir(hostSrc, tmpDir);
        const input = {
          bio: "evil </profile> text",
          card: { body: "inside </card> card" },
          verdict: { __kind__: "Confirmed", evidence: "bad </proof> proof" },
        };

        const rendered = tmpl.render(input);
        assert.ok(
          !rendered.includes("</profile> text"),
          `Expected imported UntrustedBio to sanitize closing </profile>, got: ${rendered}`,
        );
        assert.ok(
          rendered.includes("<card>") &&
            !rendered.includes("inside </card> card"),
          `Expected imported SafeCard.body to wrap and escape </card>, got: ${rendered}`,
        );
        assert.ok(
          !rendered.includes("bad </proof> proof"),
          `Expected imported Verdict.Confirmed.evidence to sanitize closing </proof>, got: ${rendered}`,
        );
        assert.strictEqual(tmpl.renderContext(Context.from(input)), rendered);
      } finally {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      }
    });

    it("applies sanitize policies and inherits parent type alias sanitize inside inline {% tmpl %} blocks", () => {
      const src = `---
types:
  - SafeItem = struct(label = untrusted str)

params:
  - item = SafeItem
  - raw_note = str
---
> {% tmpl badge %}
---
params:
  - entry = SafeItem
  - note = untrusted str
---
<badge>{{ entry.label }}|{{ note }}</badge>

> {% /tmpl %}
> {% include badge with entry=item, note=raw_note %}`;

      const tmpl = Template.fromSource(src);
      const input = {
        item: { label: "escape </badge> label" },
        raw_note: "escape </badge> note",
      };
      const rendered = tmpl.render(input);
      assert.ok(
        !rendered.includes("</badge> label") &&
          !rendered.includes("</badge> note"),
        `Expected render() to sanitize inline {% tmpl %} params, got: ${rendered}`,
      );
      assert.strictEqual(tmpl.renderContext(Context.from(input)), rendered);
    });
  });

  describe("5. Fast-Path Validator, DirectScope Loop Stack, and Compiled Instance Isolation", () => {
    it("isolates mutable instance state across Template.fromSource instances", () => {
      const src = `---
params:
  - msg = str
---
{{ msg }}`;
      const t1 = Template.fromSource(src);
      const t2 = Template.fromSource(src);
      assert.notStrictEqual(t1, t2);
      t1.setMaxIncludeDepth(3);
      assert.strictEqual(t1.maxIncludeDepth, 3);
      assert.strictEqual(t2.maxIncludeDepth, 16);
    });

    it("resolves 3-level nested for-loop bindings, idx(), and loop metadata identically in render and renderContext", () => {
      const src = `---
params:
  - groups = list(name = str, rows = list(tag = str, cells = list(str)))
---
> {% for g in groups %}
> {% for r in g.rows %}
> {% for c in r.cells %}

[{{ idx(g) }}.{{ idx(r) }}.{{ idx(c) | add(1) }}/{{ loop.len }}:{{ g.name }}-{{ r.tag }}-{{ c }}]

> {% /for %}
> {% /for %}
> {% /for %}`;
      const tmpl = Template.fromSource(src);
      const data = {
        groups: [
          {
            name: "G0",
            rows: [
              { tag: "R0", cells: ["a", "b"] },
              { tag: "R1", cells: ["c"] },
            ],
          },
        ],
      };
      const expected =
        "[0.0.1/2:G0-R0-a]\n[0.0.2/2:G0-R0-b]\n[0.1.1/1:G0-R1-c]\n";
      assert.strictEqual(tmpl.render(data), expected);
      assert.strictEqual(tmpl.renderContext(Context.from(data)), expected);
    });

    it("rejects extra keys, missing keys with prototype properties, and cyclic extra struct fields", () => {
      const src = `---
params:
  - item = struct(name = str)
---
{{ item.name }}`;
      const tmpl = Template.fromSource(src);

      assert.throws(() => {
        tmpl.render({ wrong: { name: "x" } });
      }, ExtraParamsError);

      const protoObj = Object.create({ item: { name: "inherited" } }) as Record<
        string,
        unknown
      >;
      assert.throws(() => {
        tmpl.render(protoObj);
      }, MissingParamsError);

      interface CyclicExtra {
        name: string;
        extra?: CyclicExtra;
      }
      const cyclicItem: CyclicExtra = { name: "valid" };
      cyclicItem.extra = cyclicItem;
      assert.throws(
        () => {
          tmpl.renderContext(Context.from({ item: cyclicItem }));
        },
        (err: unknown) =>
          err instanceof Error &&
          err.message.includes("cyclic object detected"),
      );
    });

    it("matches parameter-reference case labels without throwing on non-matching arms", () => {
      const src = `---
params:
  - actual = str
  - expected = str
---
> {% match actual %}
> {% case "literal_miss" %}

MISS

> {% case expected %}

HIT:{{ actual }}

> {% else %}

FALLBACK

> {% /match %}`;
      const tmpl = Template.fromSource(src);
      assert.strictEqual(
        tmpl.render({ actual: "target", expected: "target" }).trim(),
        "HIT:target",
      );
      assert.strictEqual(
        tmpl
          .renderContext(Context.from({ actual: "target", expected: "target" }))
          .trim(),
        "HIT:target",
      );
    });
  });
});
