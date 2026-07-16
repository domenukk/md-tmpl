/**
 * Regression tests for "delimiters inside quoted strings" in frontmatter
 * default values.
 *
 * The parser is intentionally quote-aware: commas and the bracket family
 * (`()[]{}<>`) that appear inside quoted string literals (`"..."` / `'...'`)
 * must be treated as literal characters, NOT as field/element separators.
 *
 * These tests lock in that behavior at three levels:
 *   1. `splitTopLevel` — the low-level, quote-aware splitter.
 *   2. `parseLiteral` / `parseListLiteral` / `parseStructLiteral` — the
 *      default-value literal parsers.
 *   3. `generateTypesFromFile` — end-to-end code generation from a
 *      `.tmpl.md` file (matrix cases T1–T8).
 *
 * @module
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

import { splitTopLevel, parseVarType } from "../frontmatter/var_type.js";
import {
  parseLiteral,
  parseListLiteral,
  parseStructLiteral,
} from "../frontmatter/literals.js";
import { generateTypesFromFile, inferTypes } from "../codegen.js";
import { type VarType } from "../frontmatter.js";
import { type Value, TYPE_STR, TYPE_LIST, TYPE_STRUCT } from "../value.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Narrow a `Value` to a string, throwing with context if it is not one. */
function strValue(v: Value | undefined): string {
  if (v === undefined) {
    throw new Error("expected str value, got undefined");
  }
  if (v.type !== TYPE_STR) {
    throw new Error(`expected str value, got '${v.type}'`);
  }
  return v.value;
}

/** Narrow a `Value` to a list's items, throwing with context otherwise. */
function listItems(v: Value | undefined): readonly Value[] {
  if (v === undefined) {
    throw new Error("expected list value, got undefined");
  }
  if (v.type !== TYPE_LIST) {
    throw new Error(`expected list value, got '${v.type}'`);
  }
  return v.items;
}

/** Narrow a `Value` to a struct's field map, throwing with context otherwise. */
function structFields(v: Value | undefined): ReadonlyMap<string, Value> {
  if (v === undefined) {
    throw new Error("expected struct value, got undefined");
  }
  if (v.type !== TYPE_STRUCT) {
    throw new Error(`expected struct value, got '${v.type}'`);
  }
  return v.fields;
}

/** Parse a struct type declaration, asserting it resolves to a struct. */
function structType(
  decl: string,
): Extract<VarType, { kind: typeof TYPE_STRUCT }> {
  const vt = parseVarType(decl);
  if (vt.kind !== TYPE_STRUCT) {
    throw new Error(`expected struct type from '${decl}', got '${vt.kind}'`);
  }
  return vt;
}

/** Build a minimal single-param template source from one param declaration. */
function templateWithParam(paramDecl: string): string {
  return ["---", "params:", `  - ${paramDecl}`, "---", "Body {{ x }}"].join(
    "\n",
  );
}

/**
 * Write `source` to a temporary `.tmpl.md` file, generate types from it, and
 * additionally extract the programmatic default values via `inferTypes`.
 *
 * Returns both the emitted TypeScript source (`code`) and the parsed default
 * values keyed by param name (`defaults`), so tests can assert both the
 * end-to-end codegen output and the semantic parsed value.
 */
function generateAndInfer(source: string): {
  code: string;
  defaults: Record<string, unknown>;
} {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "md-tmpl-delim-"));
  const filePath = path.join(dir, "template.tmpl.md");
  fs.writeFileSync(filePath, source);
  try {
    const code = generateTypesFromFile(filePath);
    const inferred = inferTypes(source);
    const defaults: Record<string, unknown> = {};
    for (const field of inferred.fields) {
      if (field.defaultValue !== undefined) {
        defaults[field.name] = field.defaultValue;
      }
    }
    return { code, defaults };
  } finally {
    fs.rmSync(dir, { recursive: true });
  }
}

// ---------------------------------------------------------------------------
// 1. splitTopLevel — the quote-aware splitter
// ---------------------------------------------------------------------------

describe("splitTopLevel — delimiters inside quoted strings", () => {
  it("ignores a comma inside a double-quoted string", () => {
    const parts = splitTopLevel('a = "x, y", b = 1', ",");
    assert.deepStrictEqual(parts, ['a = "x, y"', " b = 1"]);
  });

  it("ignores a comma inside a single-quoted string", () => {
    const parts = splitTopLevel("a = 'x, y', b = 2", ",");
    assert.deepStrictEqual(parts, ["a = 'x, y'", " b = 2"]);
  });

  it("ignores brackets/braces/parens inside a double-quoted string", () => {
    const parts = splitTopLevel('a = "x[1]{2}(3),y", b = 3', ",");
    assert.deepStrictEqual(parts, ['a = "x[1]{2}(3),y"', " b = 3"]);
  });

  it("does not let closing brackets inside a string skew bracket depth", () => {
    // If quote-awareness regressed, the `]`/`[` inside the strings would
    // drive bracket depth negative/positive and the top-level comma would be
    // mis-detected, collapsing this into a single part.
    const parts = splitTopLevel('a = "]", b = "["', ",");
    assert.deepStrictEqual(parts, ['a = "]"', ' b = "["']);
  });

  it("handles multiple embedded delimiters within one string", () => {
    const parts = splitTopLevel('note = "p, q, r, s", n = 4', ",");
    assert.strictEqual(parts.length, 2);
    assert.strictEqual(parts[0], 'note = "p, q, r, s"');
    assert.strictEqual(parts[1], " n = 4");
  });

  it("treats input as a single part when every delimiter is quoted", () => {
    const parts = splitTopLevel('"a, b, c"', ",");
    assert.deepStrictEqual(parts, ['"a, b, c"']);
  });

  it("still splits ordinary top-level delimiters", () => {
    const parts = splitTopLevel("a = 1, b = 2, c = 3", ",");
    assert.deepStrictEqual(parts, ["a = 1", " b = 2", " c = 3"]);
  });

  it("respects nested bracket depth while ignoring quoted delimiters", () => {
    const parts = splitTopLevel('[a, "b, c"], d', ",");
    assert.deepStrictEqual(parts, ['[a, "b, c"]', " d"]);
  });
});

// ---------------------------------------------------------------------------
// 2. Literal parsers — embedded-comma quoted values
// ---------------------------------------------------------------------------

describe("parseListLiteral — embedded delimiters in quoted items", () => {
  it("preserves commas inside double-quoted scalar items", () => {
    const value = parseListLiteral(
      '["a, b", "c, d"]',
      parseVarType("list(str)"),
    );
    const items = listItems(value);
    assert.strictEqual(items.length, 2);
    assert.strictEqual(strValue(items[0]), "a, b");
    assert.strictEqual(strValue(items[1]), "c, d");
  });

  it("preserves commas inside single-quoted scalar items", () => {
    const value = parseListLiteral("['a, b', 'c']", parseVarType("list(str)"));
    const items = listItems(value);
    assert.strictEqual(items.length, 2);
    assert.strictEqual(strValue(items[0]), "a, b");
    assert.strictEqual(strValue(items[1]), "c");
  });

  it("keeps bracket-family characters inside quoted items intact", () => {
    const value = parseListLiteral(
      '["a[b]c", "d{e}f", "g(h)i"]',
      parseVarType("list(str)"),
    );
    const items = listItems(value);
    assert.deepStrictEqual(items.map(strValue), ["a[b]c", "d{e}f", "g(h)i"]);
  });

  it("splits a list of records on record boundaries, not string commas", () => {
    const value = parseListLiteral(
      '[{name = "x", note = "p, q, r"}, {name = "y", note = "s"}]',
      parseVarType("list(name = str, note = str)"),
    );
    const items = listItems(value);
    assert.strictEqual(items.length, 2);
    assert.strictEqual(strValue(structFields(items[0]).get("name")), "x");
    assert.strictEqual(strValue(structFields(items[0]).get("note")), "p, q, r");
    assert.strictEqual(strValue(structFields(items[1]).get("note")), "s");
  });
});

describe("parseStructLiteral — embedded delimiters in quoted values", () => {
  it("keeps a comma inside a double-quoted struct field value", () => {
    const value = parseStructLiteral(
      '{msg = "a, b", n = 1}',
      structType("struct(msg = str, n = int)"),
    );
    const fields = structFields(value);
    assert.strictEqual(strValue(fields.get("msg")), "a, b");
    const n = fields.get("n");
    assert.ok(n);
    assert.strictEqual(n.type, "int");
  });

  it("keeps empty quoted strings alongside comma-bearing values", () => {
    const value = parseStructLiteral(
      '{name = "", note = "y, z"}',
      structType("struct(name = str, note = str)"),
    );
    const fields = structFields(value);
    assert.strictEqual(strValue(fields.get("name")), "");
    assert.strictEqual(strValue(fields.get("note")), "y, z");
  });
});

describe("parseLiteral — dispatches quoted-delimiter defaults correctly", () => {
  it("parses a scalar list default with embedded commas", () => {
    const value = parseLiteral('["a, b", "c, d"]', parseVarType("list(str)"));
    assert.deepStrictEqual(listItems(value).map(strValue), ["a, b", "c, d"]);
  });

  it("parses a struct default with an embedded comma", () => {
    const value = parseLiteral(
      '{msg = "a, b", n = 1}',
      parseVarType("struct(msg = str, n = int)"),
    );
    assert.strictEqual(strValue(structFields(value).get("msg")), "a, b");
  });

  it("keeps a bare quoted string with commas as a single str", () => {
    const value = parseLiteral('"a, b, c"', parseVarType("str"));
    assert.strictEqual(strValue(value), "a, b, c");
  });
});

// ---------------------------------------------------------------------------
// 3. End-to-end generateTypesFromFile — matrix cases T1–T8
// ---------------------------------------------------------------------------

describe("generateTypesFromFile — embedded-delimiter defaults (T1–T8)", () => {
  it("T1: list(str) with commas inside double quotes", () => {
    const { code, defaults } = generateAndInfer(
      templateWithParam('x = list(str) := ["a, b", "c, d"]'),
    );
    assert.deepStrictEqual(defaults.x, ["a, b", "c, d"]);
    assert.ok(code.includes('"a, b"'), `missing "a, b" in:\n${code}`);
    assert.ok(code.includes('"c, d"'), `missing "c, d" in:\n${code}`);
  });

  it("T2: struct with a comma inside a double-quoted field", () => {
    const { code, defaults } = generateAndInfer(
      templateWithParam(
        'x = struct(msg = str, n = int) := {msg = "a, b", n = 1}',
      ),
    );
    assert.deepStrictEqual(defaults.x, { msg: "a, b", n: 1 });
    assert.ok(code.includes('"a, b"'), `missing "a, b" in:\n${code}`);
  });

  it("T3: list of records with comma-prose in a field", () => {
    const { code, defaults } = generateAndInfer(
      templateWithParam(
        'x = list(name = str, note = str) := [{name = "x", note = "p, q, r"}, {name = "y", note = "s"}]',
      ),
    );
    assert.deepStrictEqual(defaults.x, [
      { name: "x", note: "p, q, r" },
      { name: "y", note: "s" },
    ]);
    assert.ok(code.includes('"p, q, r"'), `missing "p, q, r" in:\n${code}`);
  });

  it("T4: list(str) with bracket-family characters inside items", () => {
    const { code, defaults } = generateAndInfer(
      templateWithParam('x = list(str) := ["a[b]c", "d{e}f", "g(h)i"]'),
    );
    assert.deepStrictEqual(defaults.x, ["a[b]c", "d{e}f", "g(h)i"]);
    assert.ok(code.includes('"a[b]c"'), `missing "a[b]c" in:\n${code}`);
    assert.ok(code.includes('"d{e}f"'), `missing "d{e}f" in:\n${code}`);
    assert.ok(code.includes('"g(h)i"'), `missing "g(h)i" in:\n${code}`);
  });

  it("T5: list(str) with commas inside single quotes", () => {
    const { defaults } = generateAndInfer(
      templateWithParam("x = list(str) := ['a, b', 'c']"),
    );
    assert.deepStrictEqual(defaults.x, ["a, b", "c"]);
  });

  it("T6: list(str) with unicode (em-dash, emoji) and commas intact", () => {
    const { code, defaults } = generateAndInfer(
      templateWithParam(
        'x = list(str) := ["Theory — not a finding", "✅ done, ok"]',
      ),
    );
    assert.deepStrictEqual(defaults.x, [
      "Theory — not a finding",
      "✅ done, ok",
    ]);
    assert.ok(
      code.includes("Theory — not a finding"),
      `missing em-dash string in:\n${code}`,
    );
    assert.ok(
      code.includes("✅ done, ok"),
      `missing emoji string in:\n${code}`,
    );
  });

  it("T7: list of records with empty strings and a comma value", () => {
    const { defaults } = generateAndInfer(
      templateWithParam(
        'x = list(name = str, note = str) := [{name = "", note = ""}, {name = "a", note = "y, z"}]',
      ),
    );
    assert.deepStrictEqual(defaults.x, [
      { name: "", note: "" },
      { name: "a", note: "y, z" },
    ]);
  });

  it("T8: SEVERITY_LADDER-style records with comma/em-dash/emoji prose", () => {
    const { code, defaults } = generateAndInfer(
      templateWithParam(
        "x = list(tier = str, short = str, proves = str) := " +
          '[{tier = "P0", short = "Critical, immediate action", ' +
          'proves = "RCE, memory corruption — proven exploit ✅"}, ' +
          '{tier = "P1", short = "High, urgent", ' +
          'proves = "auth bypass, data exfiltration"}]',
      ),
    );
    assert.deepStrictEqual(defaults.x, [
      {
        tier: "P0",
        short: "Critical, immediate action",
        proves: "RCE, memory corruption — proven exploit ✅",
      },
      {
        tier: "P1",
        short: "High, urgent",
        proves: "auth bypass, data exfiltration",
      },
    ]);
    assert.ok(
      code.includes("RCE, memory corruption — proven exploit ✅"),
      `missing comma-prose field in:\n${code}`,
    );
    assert.ok(
      code.includes("auth bypass, data exfiltration"),
      `missing second comma-prose field in:\n${code}`,
    );
  });
});

// ---------------------------------------------------------------------------
// 4. Extra edge cases — mixed quotes, nesting, whitespace, hash-as-literal
// ---------------------------------------------------------------------------

describe("splitTopLevel — mixed quotes and nesting edge cases", () => {
  it("E1: a single-quote inside a double-quoted string does not end it", () => {
    const parts = splitTopLevel('"it\'s, fine", "ok"', ",");
    assert.deepStrictEqual(parts, ['"it\'s, fine"', ' "ok"']);
  });

  it("E2: double-quotes inside a single-quoted string do not end it", () => {
    const parts = splitTopLevel('\'say "hi", bye\', "z"', ",");
    assert.deepStrictEqual(parts, ["'say \"hi\", bye'", ' "z"']);
  });

  it("E3: nested brackets keep quoted commas inside inner lists literal", () => {
    const parts = splitTopLevel('["a, b"], ["c, d", "e"]', ",");
    assert.deepStrictEqual(parts, ['["a, b"]', ' ["c, d", "e"]']);
  });

  it("E4: whitespace inside a quoted item is preserved, not trimmed", () => {
    const parts = splitTopLevel('" a, b "', ",");
    assert.deepStrictEqual(parts, ['" a, b "']);
  });

  it("E5: a hash inside a quoted string is a literal, not a comment", () => {
    const parts = splitTopLevel('"a # b, c"', ",");
    assert.deepStrictEqual(parts, ['"a # b, c"']);
  });
});

describe("generateTypesFromFile — extra edge cases (E1–E5)", () => {
  it("E1: double-quoted item containing a single-quote and comma", () => {
    const { defaults } = generateAndInfer(
      templateWithParam('x = list(str) := ["it\'s, fine", "ok"]'),
    );
    assert.deepStrictEqual(defaults.x, ["it's, fine", "ok"]);
  });

  it("E2: single-quoted item containing double-quotes and comma", () => {
    const { defaults } = generateAndInfer(
      templateWithParam('x = list(str) := [\'say "hi", bye\', "z"]'),
    );
    assert.deepStrictEqual(defaults.x, ['say "hi", bye', "z"]);
  });

  it("E3: nested list(list(str)) with embedded commas per inner item", () => {
    const { defaults } = generateAndInfer(
      templateWithParam('x = list(list(str)) := [["a, b"], ["c, d", "e"]]'),
    );
    assert.deepStrictEqual(defaults.x, [["a, b"], ["c, d", "e"]]);
  });

  it("E4: leading/trailing whitespace inside a quoted item is preserved", () => {
    const { defaults } = generateAndInfer(
      templateWithParam('x = list(str) := [" a, b "]'),
    );
    assert.deepStrictEqual(defaults.x, [" a, b "]);
  });

  it("E5a: a `#` with no leading space is a literal, not a comment", () => {
    const { code, defaults } = generateAndInfer(
      templateWithParam('x = str := "a#b,c"'),
    );
    assert.strictEqual(defaults.x, "a#b,c");
    assert.ok(code.includes('"a#b,c"'), `missing hash string in:\n${code}`);
  });

  it("E5b: a ` #` (space-hash) starts a YAML comment and truncates the scalar", () => {
    // In a block list item, ` #` is a YAML inline comment (the core is not
    // md-tmpl-string-aware), so the scalar becomes `x = str := "a` — an
    // unterminated string literal rejected as an invalid default. To keep a
    // literal ` #`, wrap the whole declaration in outer YAML quotes (see E5c).
    assert.throws(
      () => generateAndInfer(templateWithParam('x = str := "a # b, c"')),
      /strings must be quoted/,
    );
  });

  it("E5c: outer YAML quotes protect an inner ` #` from comment stripping", () => {
    const { defaults } = generateAndInfer(
      templateWithParam('"x = str := \\"a # b, c\\""'),
    );
    assert.strictEqual(defaults.x, "a # b, c");
  });

  it("E5d: a ` #` outside any string literal is stripped as a comment", () => {
    const { defaults } = generateAndInfer(
      templateWithParam("x = int := 3 # the retry count"),
    );
    assert.strictEqual(defaults.x, 3);
  });
});

describe("parseListLiteral — extra edge cases (E1–E4)", () => {
  it("E1: preserves a single-quote and comma inside a double-quoted item", () => {
    const value = parseListLiteral(
      '["it\'s, fine", "ok"]',
      parseVarType("list(str)"),
    );
    assert.deepStrictEqual(listItems(value).map(strValue), [
      "it's, fine",
      "ok",
    ]);
  });

  it("E2: preserves double-quotes and comma inside a single-quoted item", () => {
    const value = parseListLiteral(
      '[\'say "hi", bye\', "z"]',
      parseVarType("list(str)"),
    );
    assert.deepStrictEqual(listItems(value).map(strValue), [
      'say "hi", bye',
      "z",
    ]);
  });

  it("E3: parses a nested list-of-lists keeping inner quoted commas", () => {
    const value = parseListLiteral(
      '[["a, b"], ["c, d", "e"]]',
      parseVarType("list(list(str))"),
    );
    const outer = listItems(value);
    assert.strictEqual(outer.length, 2);
    assert.deepStrictEqual(listItems(outer[0]).map(strValue), ["a, b"]);
    assert.deepStrictEqual(listItems(outer[1]).map(strValue), ["c, d", "e"]);
  });

  it("E4: keeps interior whitespace of a quoted item verbatim", () => {
    const value = parseListLiteral('[" a, b "]', parseVarType("list(str)"));
    const items = listItems(value);
    assert.strictEqual(items.length, 1);
    assert.strictEqual(strValue(items[0]), " a, b ");
  });
});

// ---------------------------------------------------------------------------
// S4. Backslash-escaped quote before a delimiter must not split
//
// Mirrors Rust's `s4_backslash_before_delimiter_does_not_split_list`: an
// escaped quote (`\"` / `\'`) inside a string literal does not close the
// literal, so a following comma stays part of the element. The parsed value
// is then unescaped (`a\", b` → `a", b`).
// ---------------------------------------------------------------------------

describe("S4 — escaped quote before delimiter does not split", () => {
  it("splitTopLevel: keeps an element whose escaped quote precedes a comma", () => {
    const parts = splitTopLevel('"a\\", b", "c"', ",");
    assert.deepStrictEqual(parts, ['"a\\", b"', ' "c"']);
  });

  it("splitTopLevel: escaped single quote does not close a single-quoted element", () => {
    const parts = splitTopLevel("'x\\', y', 'z'", ",");
    assert.deepStrictEqual(parts, ["'x\\', y'", " 'z'"]);
  });

  it('list: `["a\\", b", "c"]` parses to two unescaped items', () => {
    const value = parseListLiteral(
      '["a\\", b", "c"]',
      parseVarType("list(str)"),
    );
    const items = listItems(value);
    assert.strictEqual(items.length, 2);
    assert.strictEqual(strValue(items[0]), 'a", b');
    assert.strictEqual(strValue(items[1]), "c");
  });

  it('struct: `{msg = "a\\", b", n = 1}` keeps msg intact and unescaped', () => {
    const st = structType("struct(msg = str, n = int)");
    const value = parseStructLiteral('{msg = "a\\", b", n = 1}', st);
    const fields = structFields(value);
    assert.strictEqual(strValue(fields.get("msg")), 'a", b');
    const n = fields.get("n");
    assert.strictEqual(n?.type === "int" ? n.value : undefined, 1);
  });
});
