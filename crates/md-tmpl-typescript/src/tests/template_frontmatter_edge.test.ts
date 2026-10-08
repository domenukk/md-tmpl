/**
 * Tests for the md-tmpl TypeScript bindings: frontmatter quoting regressions, edge cases and if/elif/else.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template, DeclarationsMutatedError } from "../index.js";
import { type Value } from "../value.js";
import { parseFrontmatter, parseVarType, TYPE_LIST } from "../frontmatter.js";
import { splitTopLevel } from "../frontmatter/var_type.js";
import {
  parseLiteral,
  parseListLiteral,
  parseStructLiteral,
} from "../frontmatter/literals.js";
import { TYPE_STR, TYPE_INT, TYPE_STRUCT } from "../consts.js";
import { defined } from "./template_helpers.js";

// ---------------------------------------------------------------------------
// Regression: delimiters inside quoted strings in frontmatter defaults
//
// In frontmatter default values (`:= ...`), delimiters — commas and the
// bracket family `()[]{}<>` — that appear INSIDE quoted string literals must
// be treated as literal characters, NOT as field/element separators. This
// holds for both double (`"..."`) and single (`'...'`) quotes.
//
// The quote-aware root parser is `splitTopLevel` (var_type.ts), consumed by
// `parseLiteral` / `parseListLiteral` / `parseStructLiteral` (literals.ts).
// These tests LOCK IN that behavior at the unit and end-to-end levels; they
// mirror the canonical cross-language matrix (cases T1–T8) so the TypeScript
// backend stays byte-for-byte aligned with the Rust core.
// ---------------------------------------------------------------------------

describe("Frontmatter: delimiters inside quoted strings (regression)", () => {
  // Narrowing helpers: assert the Value shape and return the inner data so the
  // individual assertions read clearly.
  const asStr = (v: Value | undefined): string => {
    const val = defined(v);
    if (val.type !== TYPE_STR) {
      assert.fail(`expected str, got '${val.type}'`);
    }
    return val.value;
  };
  const asInt = (v: Value | undefined): number => {
    const val = defined(v);
    if (val.type !== TYPE_INT) {
      assert.fail(`expected int, got '${val.type}'`);
    }
    return val.value;
  };
  const asList = (v: Value | undefined): readonly Value[] => {
    const val = defined(v);
    if (val.type !== TYPE_LIST) {
      assert.fail(`expected list, got '${val.type}'`);
    }
    return val.items;
  };
  const asStruct = (v: Value | undefined): ReadonlyMap<string, Value> => {
    const val = defined(v);
    if (val.type !== TYPE_STRUCT) {
      assert.fail(`expected struct, got '${val.type}'`);
    }
    return val.fields;
  };

  // Parse a single-param frontmatter and return that param's parsed default.
  const paramDefault = (decl: string): Value => {
    const [fm] = parseFrontmatter(`---
params:
  - ${decl}
---
body`);
    assert.strictEqual(fm.params.length, 1, "expected exactly one param");
    return defined(
      defined(fm.params[0]).defaultValue,
      "param default must be parsed",
    );
  };

  // -------------------------------------------------------------------------
  // Low-level split unit tests (mirror the Rust `split_at_depth_zero` cases)
  // -------------------------------------------------------------------------
  describe("splitTopLevel ignores delimiters inside quoted strings", () => {
    it("does not split on a comma inside a double-quoted string", () => {
      assert.deepStrictEqual(splitTopLevel('a = "x, y", b = 1', ","), [
        'a = "x, y"',
        " b = 1",
      ]);
    });

    it("does not split on a comma inside a single-quoted string", () => {
      assert.deepStrictEqual(splitTopLevel("a = 'x, y', b = 2", ","), [
        "a = 'x, y'",
        " b = 2",
      ]);
    });

    it("treats brackets inside a string as literal (no depth change)", () => {
      // The `[` `]` `1` inside the quotes must not open bracket depth, so the
      // following top-level comma still splits into exactly two parts.
      assert.deepStrictEqual(splitTopLevel('a = "x[1],y", b = 3', ","), [
        'a = "x[1],y"',
        " b = 3",
      ]);
    });

    it("never splits mid-quote even for many embedded delimiters", () => {
      assert.deepStrictEqual(splitTopLevel('"a, b, c, d"', ","), [
        '"a, b, c, d"',
      ]);
    });

    it("keeps unbalanced brackets inside strings from corrupting depth", () => {
      // A lone `[` in one string and a lone `]` in another are literal; the
      // top-level comma between them still separates the two fields.
      assert.deepStrictEqual(splitTopLevel('x = "[", y = "]"', ","), [
        'x = "["',
        ' y = "]"',
      ]);
    });

    it("respects escaped quotes when scanning string literals", () => {
      // The escaped `\"` does not close the string, so the enclosed comma is
      // still literal and no top-level split happens inside it.
      assert.deepStrictEqual(splitTopLevel('a = "x \\", y", b = 4', ","), [
        'a = "x \\", y"',
        " b = 4",
      ]);
    });

    it("splits every brace family delimiter only at depth zero", () => {
      // Commas inside the nested struct/list literals stay grouped; only the
      // top-level comma separates the two outer fields.
      assert.deepStrictEqual(
        splitTopLevel('a = {p = "1, 2", q = 3}, b = [4, 5]', ","),
        ['a = {p = "1, 2", q = 3}', " b = [4, 5]"],
      );
    });
  });

  // -------------------------------------------------------------------------
  // Literal parsers: parseLiteral / parseListLiteral / parseStructLiteral
  // -------------------------------------------------------------------------
  describe("parseLiteral keeps embedded delimiters intact", () => {
    it("parses a scalar string containing commas verbatim", () => {
      assert.strictEqual(
        asStr(parseLiteral('"a, b, c"', parseVarType("str"))),
        "a, b, c",
      );
    });

    it("parseListLiteral splits elements, not commas inside strings (T1)", () => {
      const items = asList(
        parseListLiteral('["a, b", "c, d"]', parseVarType("list(str)")),
      );
      assert.strictEqual(items.length, 2);
      assert.strictEqual(asStr(items[0]), "a, b");
      assert.strictEqual(asStr(items[1]), "c, d");
    });

    it("parseListLiteral keeps bracket-family chars inside strings (T4)", () => {
      const items = asList(
        parseListLiteral(
          '["a[b]c", "d{e}f", "g(h)i"]',
          parseVarType("list(str)"),
        ),
      );
      assert.deepStrictEqual(
        items.map((i) => asStr(i)),
        ["a[b]c", "d{e}f", "g(h)i"],
      );
    });

    it("parseListLiteral handles single-quoted elements (T5)", () => {
      const items = asList(
        parseListLiteral("['a, b', 'c']", parseVarType("list(str)")),
      );
      assert.strictEqual(items.length, 2);
      assert.strictEqual(asStr(items[0]), "a, b");
      assert.strictEqual(asStr(items[1]), "c");
    });

    it("parseStructLiteral keeps commas inside a field's string (T2)", () => {
      const structType = parseVarType("struct(msg = str, n = int)");
      if (structType.kind !== TYPE_STRUCT) {
        throw new Error(`expected a struct type, got '${structType.kind}'`);
      }
      const fields = asStruct(
        parseStructLiteral('{msg = "a, b", n = 1}', structType),
      );
      assert.strictEqual(asStr(fields.get("msg")), "a, b");
      assert.strictEqual(asInt(fields.get("n")), 1);
    });

    it("parseLiteral dispatches struct literals with embedded commas", () => {
      const fields = asStruct(
        parseLiteral(
          '{msg = "hello, world", n = 7}',
          parseVarType("struct(msg = str, n = int)"),
        ),
      );
      assert.strictEqual(asStr(fields.get("msg")), "hello, world");
      assert.strictEqual(asInt(fields.get("n")), 7);
    });
  });

  // -------------------------------------------------------------------------
  // End-to-end frontmatter defaults (canonical matrix T1–T8)
  // -------------------------------------------------------------------------
  describe("end-to-end parseFrontmatter defaults (T1–T8)", () => {
    it("T1: list(str) with comma-bearing string elements", () => {
      const items = asList(paramDefault('x = list(str) := ["a, b", "c, d"]'));
      assert.deepStrictEqual(
        items.map((i) => asStr(i)),
        ["a, b", "c, d"],
      );
    });

    it("T2: struct with a comma-bearing string field", () => {
      const fields = asStruct(
        paramDefault('x = struct(msg = str, n = int) := {msg = "a, b", n = 1}'),
      );
      assert.strictEqual(asStr(fields.get("msg")), "a, b");
      assert.strictEqual(asInt(fields.get("n")), 1);
    });

    it("T3: list of records; record note keeps its commas", () => {
      const items = asList(
        paramDefault(
          "x = list(name = str, note = str) := " +
            '[{name = "x", note = "p, q, r"}, {name = "y", note = "s"}]',
        ),
      );
      assert.strictEqual(items.length, 2);
      const rec0 = asStruct(items[0]);
      assert.strictEqual(asStr(rec0.get("name")), "x");
      assert.strictEqual(asStr(rec0.get("note")), "p, q, r");
      const rec1 = asStruct(items[1]);
      assert.strictEqual(asStr(rec1.get("name")), "y");
      assert.strictEqual(asStr(rec1.get("note")), "s");
    });

    it("T4: list(str) with bracket-family chars inside strings", () => {
      const items = asList(
        paramDefault('x = list(str) := ["a[b]c", "d{e}f", "g(h)i"]'),
      );
      assert.deepStrictEqual(
        items.map((i) => asStr(i)),
        ["a[b]c", "d{e}f", "g(h)i"],
      );
    });

    it("T5: list(str) with single-quoted, comma-bearing elements", () => {
      const items = asList(paramDefault("x = list(str) := ['a, b', 'c']"));
      assert.deepStrictEqual(
        items.map((i) => asStr(i)),
        ["a, b", "c"],
      );
    });

    it("T6: list(str) preserves unicode (em-dash, emoji) and commas", () => {
      const items = asList(
        paramDefault(
          'x = list(str) := ["Theory — not a finding", "✅ done, ok"]',
        ),
      );
      assert.deepStrictEqual(
        items.map((i) => asStr(i)),
        ["Theory — not a finding", "✅ done, ok"],
      );
    });

    it("T7: list of records tolerates empty strings and comma-bearing notes", () => {
      const items = asList(
        paramDefault(
          "x = list(name = str, note = str) := " +
            '[{name = "", note = ""}, {name = "a", note = "y, z"}]',
        ),
      );
      assert.strictEqual(items.length, 2);
      const rec0 = asStruct(items[0]);
      assert.strictEqual(asStr(rec0.get("name")), "");
      assert.strictEqual(asStr(rec0.get("note")), "");
      const rec1 = asStruct(items[1]);
      assert.strictEqual(asStr(rec1.get("name")), "a");
      assert.strictEqual(asStr(rec1.get("note")), "y, z");
    });

    it("T8: real-world SEVERITY_LADDER shape with prose fields", () => {
      // A list of records whose prose fields contain commas, an em-dash, and
      // an emoji — the exact shape that originally exposed the bug.
      const items = asList(
        paramDefault(
          "x = list(tier = str, short = str, proves = str) := " +
            '[{tier = "L0", short = "static, dynamic", ' +
            'proves = "Reachable — but not exploitable, yet ⚠️"}, ' +
            '{tier = "L1", short = "trigger", ' +
            'proves = "Crashes, reliably"}]',
        ),
      );
      assert.strictEqual(items.length, 2);
      const rec0 = asStruct(items[0]);
      assert.strictEqual(asStr(rec0.get("tier")), "L0");
      assert.strictEqual(asStr(rec0.get("short")), "static, dynamic");
      assert.strictEqual(
        asStr(rec0.get("proves")),
        "Reachable — but not exploitable, yet ⚠️",
      );
      const rec1 = asStruct(items[1]);
      assert.strictEqual(asStr(rec1.get("tier")), "L1");
      assert.strictEqual(asStr(rec1.get("proves")), "Crashes, reliably");
    });
  });
});

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

describe("Edge cases", () => {
  it("empty params", () => {
    const tmpl = Template.fromSource(`---
params: []
---
Static content`);
    assert.strictEqual(tmpl.render(), "Static content");
  });

  it("unicode params", () => {
    const tmpl = Template.fromSource(
      `---
params: [msg = str]
---
{{ msg }}`,
    );
    assert.strictEqual(
      tmpl.render({ msg: "🎉 Hello 世界!" }),
      "🎉 Hello 世界!",
    );
  });

  it("multiple vars same template", () => {
    const tmpl = Template.fromSource(
      `---
params: [a = str, b = str]
---
{{ a }} and {{ b }}`,
    );
    assert.strictEqual(tmpl.render({ a: "X", b: "Y" }), "X and Y");
  });

  it("validateDeclarationsAgainst matches", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
{{ name }}`,
    );
    const decls = tmpl.declarations();
    // Should not throw
    tmpl.validateDeclarationsAgainst(decls);
  });

  it("validateDeclarationsAgainst rejects mismatch", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
{{ name }}`,
    );
    assert.throws(
      () => {
        tmpl.validateDeclarationsAgainst([["different", "int"]]);
      },
      (err: Error) =>
        err instanceof DeclarationsMutatedError &&
        err.message.includes("declarations were modified"),
    );
  });

  it("multiline template", () => {
    const tmpl = Template.fromSource(
      `---
params: [title = str]
---
# {{ title }}

Body text.`,
    );
    const output = tmpl.render({ title: "Test" });
    assert.strictEqual(output, "# Test\n\nBody text.");
  });

  it("comments are stripped", () => {
    const tmpl = Template.fromSource(
      `---
params: [name = str]
---
Hello {# this is a comment #}{{ name }}!`,
    );
    assert.strictEqual(tmpl.render({ name: "world" }), "Hello world!");
  });
});

// ---------------------------------------------------------------------------
// Multiple param types
// ---------------------------------------------------------------------------

describe("Multiple param types", () => {
  it("all primitive types together", () => {
    const tmpl = Template.fromSource(
      `---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`,
    );
    const output = tmpl.render({
      name: "Alice",
      count: 42,
      score: 9.5,
      enabled: true,
    });
    assert.strictEqual(output, "Alice: count=42, score=9.5, enabled=true");
  });
});

// ---------------------------------------------------------------------------
// If / elif / else
// ---------------------------------------------------------------------------

describe("If/elif/else", () => {
  const SRC = [
    `---`,
    "params:",
    "  - level = int",
    "  - name = str",
    `---`,
    "> {% if level == 1 %}",
    "",
    "Beginner: {{ name }}",

    "",
    "> {% elif level == 2 %}",
    "",
    "Intermediate: {{ name }}",

    "",
    "> {% else %}",
    "",
    "Expert: {{ name }}",

    "",
    "> {% /if %}",
  ].join("\n");

  it("renders first branch", () => {
    const tmpl = Template.fromSource(SRC);
    assert.strictEqual(
      tmpl.render({ level: 1, name: "Alice" }).trim(),
      "Beginner: Alice",
    );
  });

  it("renders elif branch", () => {
    const tmpl = Template.fromSource(SRC);
    assert.strictEqual(
      tmpl.render({ level: 2, name: "Bob" }).trim(),
      "Intermediate: Bob",
    );
  });

  it("renders else branch", () => {
    const tmpl = Template.fromSource(SRC);
    assert.strictEqual(
      tmpl.render({ level: 99, name: "Carol" }).trim(),
      "Expert: Carol",
    );
  });
});
