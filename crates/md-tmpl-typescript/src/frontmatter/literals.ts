/**
 * Parsers for literal default values (scalars, structs, lists).
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import {
  ENUM_TAG_KEY,
  NONE,
  type Value,
  bool,
  float,
  int,
  list,
  str,
} from "../value.js";
import {
  BRACE_OPEN,
  BRACKET_CLOSE,
  BRACKET_OPEN,
  COMMA,
  DOT,
  EQUALS,
  ERR_BARE_VARIANT_HINT,
  LIT_FALSE,
  LIT_TRUE,
  OPTION_NONE,
  OPTION_SOME,
  PAREN_CLOSE,
  PAREN_OPEN,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  TYPE_ALIAS,
  TYPE_ENUM,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  TYPE_OPTION,
  TYPE_SCALAR_LIST,
  TYPE_STR,
  TYPE_STRUCT,
  unescapeStringLiteral,
} from "../consts.js";
import { type VarType } from "./types.js";
import { splitTopLevel } from "./var_type.js";

/** Split `name = type := default` into `["name = type", "default"]`. */
export function splitDefault(raw: string): [string, string | undefined] {
  const marker = ":=";
  const idx = raw.indexOf(marker);
  if (idx === -1) return [raw, undefined];
  return [raw.slice(0, idx).trim(), raw.slice(idx + marker.length).trim()];
}

/**
 * Try to parse a literal, falling back to const-name lookup.
 *
 * If `parseLiteral()` throws and `constValues` contains a matching
 * key, the const's value is returned instead — enabling declarations
 * like `param = int := MY_CONST`.
 */
export function parseLiteralOrConst(
  literal: string,
  varType: VarType,
  constValues?: ReadonlyMap<string, Value>,
): Value {
  try {
    return parseLiteral(literal, varType, constValues);
  } catch (err) {
    // If the default text matches a known const name, use its value.
    if (constValues) {
      const constVal = constValues.get(literal);
      if (constVal !== undefined) {
        // Validate that the const value is compatible with the param type.
        validateConstDefaultType(literal, constVal, varType);
        return constVal;
      }
    }
    // Re-throw the original parse error.
    throw err;
  }
}

/**
 * Validate that a const value used as a param default is type-compatible.
 *
 * @throws {TemplateSyntaxError} if the const value type doesn't match
 *         the expected param type.
 */
export function validateConstDefaultType(
  constName: string,
  constVal: Value,
  varType: VarType,
): void {
  const expectedKind = varType.kind;
  // For simple scalar types, check the Value's type tag.
  const typeMap: Record<string, string> = {
    str: "str",
    bool: "bool",
    int: "int",
    float: "float",
  };
  const expected = typeMap[expectedKind];
  if (expected !== undefined) {
    if (constVal.type !== expected) {
      // Allow int → float promotion.
      if (expected === TYPE_FLOAT && constVal.type === TYPE_INT) return;
      throw new TemplateSyntaxError(
        `const '${constName}' has type '${constVal.type}' but param expects '${expected}'`,
      );
    }
  }
  // For option(T), validate against the inner type (const can't be None).
  if (expectedKind === TYPE_OPTION) {
    validateConstDefaultType(constName, constVal, varType.innerType);
  }
}

/** Parse a literal value for a default. */
export function parseLiteral(
  literal: string,
  varType: VarType,
  constValues?: ReadonlyMap<string, Value>,
): Value {
  // Option types need to be checked first so that quoted strings like
  // "None" are correctly parsed as Some(val="None") rather than being
  // consumed by the generic string literal handler as str("None").
  if (varType.kind === TYPE_OPTION) {
    // Bare None → Value.None
    if (literal === OPTION_NONE) {
      return NONE;
    }
    // Any other literal → parse as the inner type (auto-wrap to Some)
    return parseLiteral(literal, varType.innerType, constValues);
  }

  // Legacy isOption handling (for backward compatibility with old enum-based options)
  if (varType.kind === TYPE_ENUM && varType.isOption) {
    if (literal === OPTION_NONE) {
      return NONE;
    }
    const someVariant = varType.variants.find((v) => v.name === OPTION_SOME);
    if (someVariant?.fields.length === 1) {
      const firstField = someVariant.fields[0];
      if (firstField) {
        return parseLiteral(literal, firstField.varType, constValues);
      }
    }
  }

  // String literals
  if (
    (literal.startsWith(QUOTE_DOUBLE) && literal.endsWith(QUOTE_DOUBLE)) ||
    (literal.startsWith(QUOTE_SINGLE) && literal.endsWith(QUOTE_SINGLE))
  ) {
    return str(unescapeStringLiteral(literal.slice(1, -1)));
  }

  // Boolean
  if (literal === LIT_TRUE) return bool(true);
  if (literal === LIT_FALSE) return bool(false);

  // List literals: [item1, item2]
  if (literal.startsWith(BRACKET_OPEN) && literal.endsWith(BRACKET_CLOSE)) {
    return parseListLiteral(literal, varType, constValues);
  }

  // Struct literals: {KEY = "val", KEY2 = 42}
  if (varType.kind === TYPE_STRUCT && literal.startsWith(BRACE_OPEN)) {
    return parseStructLiteral(literal, varType, constValues);
  }

  // Integer
  if (
    varType.kind === TYPE_INT ||
    (varType.kind !== TYPE_FLOAT && /^-?\d+$/.test(literal))
  ) {
    const n = parseInt(literal, 10);
    if (!Number.isNaN(n)) return int(n);
  }

  // Float
  const f = parseFloat(literal);
  if (!Number.isNaN(f)) return float(f);

  // If the expected type is an Enum, validate variant identifiers.
  if (varType.kind === TYPE_ENUM) {
    // Check for struct variant default: VariantName(field = value, ...)
    const openParen = literal.indexOf(PAREN_OPEN);
    if (openParen !== -1 && literal.endsWith(PAREN_CLOSE)) {
      const variantName = literal.slice(0, openParen).trim();
      const innerFields = literal.slice(openParen + 1, -1).trim();
      const variant = varType.variants.find((v) => v.name === variantName);
      if (!variant) {
        throw new TemplateSyntaxError(`unknown enum variant '${variantName}'`);
      }
      if (variant.fields.length === 0) {
        throw new TemplateSyntaxError(
          `unit variant '${variantName}' cannot have fields`,
        );
      }
      // Parse field values and build a tagged dict.
      const fieldEntries = splitTopLevel(innerFields, COMMA);
      const entries: [string, Value][] = [[ENUM_TAG_KEY, str(variantName)]];
      // Build a lookup for field types.
      const fieldTypeMap = new Map<string, VarType>();
      for (const f of variant.fields) {
        fieldTypeMap.set(f.name, f.varType);
      }
      for (const entry of fieldEntries) {
        const trimmedEntry = entry.trim();
        if (trimmedEntry === "") continue;
        const eqPos = trimmedEntry.indexOf(EQUALS);
        if (eqPos === -1) continue;
        const key = trimmedEntry.slice(0, eqPos).trim();
        const valStr = trimmedEntry.slice(eqPos + 1).trim();
        const fieldType = fieldTypeMap.get(key) ?? { kind: TYPE_STR };
        entries.push([
          key,
          parseLiteralOrConst(valStr, fieldType, constValues),
        ]);
      }
      return { type: TYPE_STRUCT, fields: new Map(entries) };
    }

    // Bare identifier — must be a known variant. A qualified `Type.Variant`
    // form (e.g. `Stage.Build`) is rejected: the canonical default is the
    // bare variant name. Mirrors the Rust core and the top-level check in
    // `parseParamDeclDeferred`.
    if (literal.includes(DOT)) {
      const variantName = literal.slice(literal.indexOf(DOT) + 1);
      throw new TemplateSyntaxError(
        `enum variant default '${literal}' must ${ERR_BARE_VARIANT_HINT}` +
          ` (e.g. '${variantName}'), not the qualified 'Type.Variant' form`,
      );
    }
    const variant = varType.variants.find((v) => v.name === literal);
    if (!variant) {
      throw new TemplateSyntaxError(`unknown enum variant '${literal}'`);
    }
    if (variant.fields.length > 0) {
      throw new TemplateSyntaxError(
        `struct variant '${literal}' requires fields; use ${literal}(field = val
ue)`,
      );
    }
    return str(literal);
  }

  // If the expected type is a type alias, allow unquoted identifiers.
  if (varType.kind === TYPE_ALIAS) {
    // A qualified `Alias.Variant` default (where the stem is the alias itself)
    // is a rejected enum-variant reference — the canonical form is the bare
    // variant name. Other dotted forms are left for later resolution.
    if (
      literal.includes(DOT) &&
      literal.slice(0, literal.indexOf(DOT)) === varType.name
    ) {
      const variantName = literal.slice(literal.indexOf(DOT) + 1);
      throw new TemplateSyntaxError(
        `enum variant default '${literal}' must ${ERR_BARE_VARIANT_HINT}` +
          ` (e.g. '${variantName}'), not the qualified 'Type.Variant' form`,
      );
    }
    return str(literal);
  }

  // Fallback to string
  throw new TemplateSyntaxError(
    `invalid default value: '${literal}' (strings must be quoted)`,
  );
}

/**
 * Parse a struct literal like `{KEY = "value", KEY2 = 42}`.
 * Supports string, int, float, and bool values.
 */
export function parseStructLiteral(
  literal: string,
  varType: Extract<VarType, { kind: typeof TYPE_STRUCT }>,
  constValues?: ReadonlyMap<string, Value>,
): Value {
  const inner = literal.slice(1, -1).trim();
  if (inner === "") {
    return { type: TYPE_STRUCT, fields: new Map() };
  }

  // Build a lookup of field name → VarType from the struct declaration
  const fieldTypeMap = new Map<string, VarType>();
  for (const field of varType.fields) {
    fieldTypeMap.set(field.name, field.varType);
  }

  const entries: [string, Value][] = [];
  // Split top-level by comma, respecting nested brackets/quotes
  const pairs = splitTopLevel(inner, COMMA);
  for (const pair of pairs) {
    const trimmed = pair.trim();
    const eqIdx = trimmed.indexOf(EQUALS);
    if (eqIdx === -1) continue;
    const key = trimmed.slice(0, eqIdx).trim();
    const valStr = trimmed.slice(eqIdx + 1).trim();
    const fieldType = fieldTypeMap.get(key) ?? { kind: TYPE_STR };
    entries.push([key, parseLiteralOrConst(valStr, fieldType, constValues)]);
  }
  return { type: TYPE_STRUCT, fields: new Map(entries) };
}

/**
 * Parse a list literal like `["rust", "go"]` or `[{name = "Alice", active = true}]`.
 */
export function parseListLiteral(
  literal: string,
  varType: VarType,
  constValues?: ReadonlyMap<string, Value>,
): Value {
  const inner = literal.slice(1, -1).trim();
  if (inner === "") {
    return list([]);
  }
  const entries = splitTopLevel(inner, COMMA);
  const items: Value[] = [];

  // Determine the expected type for individual list elements
  let elemType: VarType;
  if (varType.kind === TYPE_SCALAR_LIST) {
    elemType = varType.elementType;
  } else if (varType.kind === TYPE_LIST) {
    // For a struct list like list(name = str, count = int), each element
    // in the default literal is a struct literal matching these fields.
    elemType = { kind: TYPE_STRUCT, fields: varType.fields };
  } else {
    // Fallback for aliases or untyped lists: pass varType or string
    elemType = varType;
  }

  for (const entry of entries) {
    const trimmed = entry.trim();
    if (trimmed !== "") {
      items.push(parseLiteralOrConst(trimmed, elemType, constValues));
    }
  }
  return list(items);
}
