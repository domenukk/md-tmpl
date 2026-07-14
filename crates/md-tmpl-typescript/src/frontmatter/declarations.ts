/**
 * Parsers for individual frontmatter declaration lines (params, type
 * aliases, consts, imports).
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import { type Value } from "../value.js";
import { EQUALS } from "../consts.js";
import { type ImportDecl, type VarDecl, type VarType } from "./types.js";
import { isValidPathPrefix, stripStringLiteral } from "./paths.js";
import { parseVarType } from "./var_type.js";
import { parseLiteralOrConst, splitDefault } from "./literals.js";

export function parseParamDecl(
  raw: string,
  constValues?: ReadonlyMap<string, Value>,
): VarDecl {
  return parseParamDeclDeferred(raw, constValues)[0];
}

export function parseParamDeclDeferred(
  raw: string,
  constValues?: ReadonlyMap<string, Value>,
): [VarDecl, { text: string; varType: VarType } | undefined] {
  const cleaned = stripStringLiteral(raw);
  const defaultSplit = splitDefault(cleaned);
  const [nameType, defaultLiteral] = defaultSplit;

  const eqIdx = nameType.indexOf(EQUALS);
  if (eqIdx === -1) {
    throw new TemplateSyntaxError(
      `parameter must have explicit type: '${raw}'`,
    );
  }

  const name = stripStringLiteral(nameType.slice(0, eqIdx).trim());
  const typeStr = nameType.slice(eqIdx + 1).trim();

  const varType = parseVarType(typeStr);
  if (defaultLiteral === undefined) {
    return [{ name, varType }, undefined];
  }
  const trimmedDefault = defaultLiteral.trim();

  // Check first: if it looks like a dotted reference (stem.NAME), and it's
  // not resolvable as a local const, defer resolution for imported consts.
  if (/^[a-zA-Z_]\w*\.[A-Z_]\w*$/.test(trimmedDefault)) {
    // Try to resolve as literal or local const first
    try {
      const defaultValue = parseLiteralOrConst(
        trimmedDefault,
        varType,
        constValues,
      );
      return [{ name, varType, defaultValue }, undefined];
    } catch (err: unknown) {
      if (!(err instanceof TemplateSyntaxError)) {
        throw err;
      }
      // Defer to imported const resolution
      return [
        { name, varType },
        { text: trimmedDefault, varType },
      ];
    }
  }

  // Not a dotted reference — parse normally, let errors propagate as-is
  const defaultValue = parseLiteralOrConst(
    trimmedDefault,
    varType,
    constValues,
  );
  return [{ name, varType, defaultValue }, undefined];
}

/** Parse `Name = type` for type aliases. */
export function parseTypeAlias(raw: string): [string, VarType] {
  const cleaned = stripStringLiteral(raw);
  const eqIdx = cleaned.indexOf(EQUALS);
  if (eqIdx === -1) {
    throw new TemplateSyntaxError(
      `type alias must have form 'Name = type': '${raw}'`,
    );
  }
  const name = stripStringLiteral(cleaned.slice(0, eqIdx).trim());
  const typeStr = cleaned.slice(eqIdx + 1).trim();
  return [name, parseVarType(typeStr)];
}

/** Parse `NAME = type := value` for constants. */
export function parseConstDecl(
  raw: string,
  constValues?: ReadonlyMap<string, Value>,
): VarDecl {
  return parseParamDecl(raw, constValues);
}

/** Parse `"[stem](path.tmpl.md)"` for imports. */
export function parseImportDecl(raw: string): ImportDecl {
  const unquoted = stripStringLiteral(raw);
  const match = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(unquoted);
  if (!match || !match[1] || !match[2]) {
    throw new TemplateSyntaxError(
      `import must be in format "[stem](path.tmpl.md)": '${raw}'`,
    );
  }
  const stem = match[1];
  const path = match[2].trim();
  if (!isValidPathPrefix(path)) {
    throw new TemplateSyntaxError(
      `import path must begin with './', '../', or '/': '${path}'`,
    );
  }
  return { stem, path };
}
