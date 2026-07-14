/**
 * Type-annotation parser: converts type strings into VarType trees.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import {
  ANGLE_CLOSE,
  ANGLE_OPEN,
  BRACE_CLOSE,
  BRACE_OPEN,
  BRACKET_CLOSE,
  BRACKET_OPEN,
  COMMA,
  EQUALS,
  ERR_COMPOUND_BRACKETS_PROHIBITED,
  PAREN_CLOSE,
  PAREN_OPEN,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  TYPE_ALIAS,
  TYPE_BOOL,
  TYPE_ENUM,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  TYPE_OPTION,
  TYPE_SCALAR_LIST,
  TYPE_STR,
  TYPE_STRUCT,
  TYPE_TMPL,
} from "../consts.js";
import { type VarDecl, type VarType, type VariantDecl } from "./types.js";
import { stripStringLiteral } from "./paths.js";

// ---------------------------------------------------------------------------
// Type parser
// ---------------------------------------------------------------------------

/** Parse a type annotation string into a VarType. */
export function startsWithCompoundType(s: string, keyword: string): boolean {
  if (!s.startsWith(keyword)) return false;
  const rest = s.slice(keyword.length).trimStart();
  if (rest.startsWith(ANGLE_OPEN) || rest.startsWith(BRACKET_OPEN)) {
    throw new TemplateSyntaxError(
      `compound type '${keyword}': ${ERR_COMPOUND_BRACKETS_PROHIBITED}`,
    );
  }
  return rest.startsWith(PAREN_OPEN);
}

export function parseVarType(typeStr: string): VarType {
  const t = stripStringLiteral(typeStr);

  if (t === TYPE_STR) return { kind: TYPE_STR };
  if (t === TYPE_BOOL) return { kind: TYPE_BOOL };
  if (t === TYPE_INT) return { kind: TYPE_INT };
  if (t === TYPE_FLOAT) return { kind: TYPE_FLOAT };

  if (startsWithCompoundType(t, TYPE_LIST)) {
    const inner = stripTypeBrackets(t, TYPE_LIST);
    if (inner === "") {
      throw new TemplateSyntaxError(
        "untyped list() is not allowed; must specify element type or fields (e.g., list(str) or list(name = str))",
      );
    }
    // Reject literal raw struct declarations inside list definitions (e.g. list(struct(name = str, count = int))).
    // Users should write named fields directly (e.g. list(name = str, count = int)) or reference a strong Type alias.
    const topItems = splitTopLevel(inner, COMMA);
    const hasTopLevelEquals = topItems.some(
      (item) =>
        item.indexOf(EQUALS) !== -1 &&
        !startsWithCompoundType(item.trim(), TYPE_STRUCT) &&
        !startsWithCompoundType(item.trim(), TYPE_ENUM) &&
        !startsWithCompoundType(item.trim(), TYPE_LIST) &&
        !startsWithCompoundType(item.trim(), TYPE_TMPL),
    );
    if (!hasTopLevelEquals) {
      if (topItems.length > 1) {
        throw new TemplateSyntaxError(
          "list with multiple fields must use named fields (e.g. list(name = str, count = int))",
        );
      }
      // Simple type list like list(str), list(int), list(enum(A, B)), list(MyStruct)
      const innerTrimmed = inner.trim();
      if (
        startsWithCompoundType(innerTrimmed, TYPE_STRUCT) ||
        innerTrimmed.startsWith(`${TYPE_STRUCT} `)
      ) {
        throw new TemplateSyntaxError(
          "list(struct(...)) is redundant; use named fields directly: list(name = str, count = int)",
        );
      }
      const elementType = parseVarType(innerTrimmed);
      if (elementType.kind === TYPE_STRUCT) {
        return { kind: TYPE_LIST, fields: elementType.fields };
      }
      return { kind: TYPE_SCALAR_LIST, elementType };
    }
    const fields = parseFieldList(inner);
    return { kind: TYPE_LIST, fields };
  }

  if (startsWithCompoundType(t, TYPE_STRUCT)) {
    const inner = stripTypeBrackets(t, TYPE_STRUCT);
    if (inner === "") {
      throw new TemplateSyntaxError(
        "untyped struct() is not allowed; must specify fields (e.g., struct(name = str))",
      );
    }
    const fields = parseFieldList(inner);
    return { kind: TYPE_STRUCT, fields };
  }

  if (startsWithCompoundType(t, TYPE_OPTION)) {
    const inner = stripTypeBrackets(t, TYPE_OPTION);
    const innerType = parseVarType(inner);
    return { kind: TYPE_OPTION, innerType };
  }

  if (startsWithCompoundType(t, TYPE_ENUM)) {
    const inner = stripTypeBrackets(t, TYPE_ENUM);
    const variants = parseVariantList(inner);
    // Reject variant names that shadow builtin type keywords.
    const RESERVED_TYPE_KEYWORDS = [
      TYPE_STR,
      TYPE_INT,
      TYPE_FLOAT,
      TYPE_BOOL,
      TYPE_LIST,
      TYPE_STRUCT,
      TYPE_ENUM,
      TYPE_OPTION,
      TYPE_TMPL,
    ];
    for (const v of variants) {
      if (RESERVED_TYPE_KEYWORDS.includes(v.name)) {
        throw new TemplateSyntaxError(
          `enum variant name '${v.name}' shadows a builtin type keyword`,
        );
      }
    }
    return { kind: TYPE_ENUM, variants };
  }

  if (startsWithCompoundType(t, TYPE_TMPL)) {
    const inner = stripTypeBrackets(t, TYPE_TMPL);
    const fields = inner === "" ? [] : parseFieldList(inner);
    return { kind: TYPE_TMPL, fields };
  }

  // Type alias reference
  return { kind: TYPE_ALIAS, name: t };
}

/** Extract content between parentheses: `list(...)` → `...`. */
export function stripTypeBrackets(s: string, keyword: string): string {
  const keywordIdx = s.indexOf(keyword);
  if (keywordIdx === -1) return "";
  let openIdx = -1;
  for (let i = keywordIdx + keyword.length; i < s.length; i++) {
    const ch = s[i]!;
    if (ch === PAREN_OPEN) {
      openIdx = i;
      break;
    }
    if (ch !== " " && ch !== "\t") break;
  }
  if (openIdx === -1) return "";

  let depth = 1;
  let i = openIdx + 1;
  while (i < s.length && depth > 0) {
    if (s[i] === PAREN_OPEN) depth++;
    else if (s[i] === PAREN_CLOSE) depth--;
    i++;
  }
  return s.slice(openIdx + 1, i - 1).trim();
}

/** Parse a comma-separated field list: `name = str, score = int`. */
export function parseFieldList(inner: string): VarDecl[] {
  const items = splitTopLevel(inner, COMMA);
  return items.map((item) => {
    const trimmed = stripStringLiteral(item);
    const eqIdx = trimmed.indexOf(EQUALS);
    if (eqIdx === -1) {
      throw new TemplateSyntaxError(
        `field must have form 'name = type': '${trimmed}'`,
      );
    }
    const name = stripStringLiteral(trimmed.slice(0, eqIdx).trim());
    const typeStr = trimmed.slice(eqIdx + 1).trim();
    return { name, varType: parseVarType(typeStr) };
  });
}

/** Parse a comma-separated variant list: `Confirmed(evidence = str), Rejected`. */
export function parseVariantList(inner: string): VariantDecl[] {
  const items = splitTopLevel(inner, COMMA);
  return items.map((item) => {
    const trimmed = stripStringLiteral(item);
    const parenIdx = trimmed.indexOf("(");
    if (parenIdx === -1) {
      return { name: trimmed, fields: [] };
    }
    const name = stripStringLiteral(trimmed.slice(0, parenIdx).trim());
    const fieldsStr = trimmed.slice(parenIdx + 1, -1).trim();
    const fields = parseFieldList(fieldsStr);
    return { name, fields };
  });
}

/**
 * Split `s` on `delimiter` at bracket-depth 0, ignoring delimiters that appear
 * inside quoted string literals (`"..."` or `'...'`).
 *
 * Brackets, braces, parens, angle brackets, and the delimiter itself are treated
 * as literal characters while inside a string literal. This lets struct/list
 * default values contain quoted strings with embedded commas or brackets
 * (e.g. `{msg = "a, b", n = 1}`) without the field separator being misdetected.
 */
export function splitTopLevel(s: string, delimiter: string): string[] {
  const result: string[] = [];
  let depth = 0;
  let current = "";
  // When inside a string literal, holds the opening quote char; delimiters are
  // ignored until the matching closing quote is seen.
  let inQuote: string | undefined;

  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!;
    if (inQuote !== undefined) {
      if (ch === inQuote) inQuote = undefined;
      current += ch;
    } else if (ch === QUOTE_DOUBLE || ch === QUOTE_SINGLE) {
      inQuote = ch;
      current += ch;
    } else if (
      ch === ANGLE_OPEN ||
      ch === PAREN_OPEN ||
      ch === BRACE_OPEN ||
      ch === BRACKET_OPEN
    ) {
      depth++;
      current += ch;
    } else if (
      ch === ANGLE_CLOSE ||
      ch === PAREN_CLOSE ||
      ch === BRACE_CLOSE ||
      ch === BRACKET_CLOSE
    ) {
      depth--;
      current += ch;
    } else if (ch === delimiter && depth === 0) {
      result.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  if (current.trim().length > 0) {
    result.push(current);
  }
  return result;
}

/** Parse inline list like `[x = str, y = int]`. */
export function parseInlineList(s: string): string[] {
  const trimmed = s.trim();
  if (!trimmed.startsWith(BRACKET_OPEN) || !trimmed.endsWith(BRACKET_CLOSE)) {
    throw new TemplateSyntaxError(`expected inline list: ${s}`);
  }
  const inner = trimmed.slice(1, -1).trim();
  if (inner === "") return [];
  return splitTopLevel(inner, COMMA);
}
