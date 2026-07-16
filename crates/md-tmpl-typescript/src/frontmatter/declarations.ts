/**
 * Parsers for individual frontmatter declaration lines (params, type
 * aliases, consts, imports).
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import { type Value } from "../value.js";
import {
  DOT,
  EQUALS,
  ERR_BARE_VARIANT_HINT,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  TYPE_ALIAS,
  TYPE_ENUM,
  TYPE_OPTION,
  unescapeStringLiteral,
} from "../consts.js";
import { type ImportDecl, type VarDecl, type VarType } from "./types.js";
import { isValidPathPrefix, stripStringLiteral } from "./paths.js";
import { parseVarType } from "./var_type.js";
import { parseLiteralOrConst, splitDefault } from "./literals.js";

/**
 * Strip an outer YAML quoted scalar wrapping a whole declaration and unescape
 * its inner content, mirroring the Rust core's `parse_declarations`
 * (`strip_string_literal` followed by `unescape_string_literal`).
 *
 * A declaration may be wrapped in outer YAML quotes (e.g.
 * `"name = str := \"a # b\""`) to protect an inner `#` (or quotes) from YAML
 * inline-comment stripping. When wrapped, the outer quotes are removed and the
 * inner `\\`, `\"`, `\'` escapes are decoded so the underlying md-tmpl
 * declaration is recovered. Unwrapped declarations are returned trimmed and
 * otherwise unchanged.
 */
function stripOuterQuotedDecl(raw: string): string {
  const trimmed = raw.trim();
  const outerQuoted =
    trimmed.length >= 2 &&
    ((trimmed.startsWith(QUOTE_DOUBLE) && trimmed.endsWith(QUOTE_DOUBLE)) ||
      (trimmed.startsWith(QUOTE_SINGLE) && trimmed.endsWith(QUOTE_SINGLE)));
  return outerQuoted
    ? unescapeStringLiteral(trimmed.slice(1, -1)).trim()
    : trimmed;
}

/**
 * Decide whether a dotted default like `Stage.Build` is a (rejected) qualified
 * enum-variant reference for the given declared type.
 *
 * `option(...)` wrappers are unwrapped first. An inline `enum(...)` type always
 * qualifies (any dotted variant form is wrong). A bare type alias qualifies
 * only when the stem matches the alias name (e.g. type `Stage` with default
 * `Stage.Build`), so genuine imported-const references (`lib.MAX`) are left to
 * the defer path.
 */
function isQualifiedEnumVariantContext(
  varType: VarType,
  dottedDefault: string,
): boolean {
  let vt = varType;
  while (vt.kind === TYPE_OPTION) {
    vt = vt.innerType;
  }
  if (vt.kind === TYPE_ENUM) {
    return true;
  }
  if (vt.kind === TYPE_ALIAS) {
    const stem = dottedDefault.slice(0, dottedDefault.indexOf(DOT));
    return stem === vt.name;
  }
  return false;
}

export function parseParamDecl(
  raw: string,
  constValues?: ReadonlyMap<string, Value>,
  isConstant = false,
): VarDecl {
  return parseParamDeclDeferred(raw, constValues, isConstant)[0];
}

export function parseParamDeclDeferred(
  raw: string,
  constValues?: ReadonlyMap<string, Value>,
  isConstant = false,
): [VarDecl, { text: string; varType: VarType } | undefined] {
  const cleaned = stripOuterQuotedDecl(raw);
  const defaultSplit = splitDefault(cleaned);
  const [nameType, defaultLiteral] = defaultSplit;

  const label = isConstant ? "constant" : "param";
  const eqIdx = nameType.indexOf(EQUALS);
  if (eqIdx === -1) {
    // A `:=` default supplied without an explicit type (e.g. `x := "hi"`) is a
    // distinct diagnostic from a bare name with no type at all (e.g.
    // `untyped_param`). Mirror the Rust core's two separate messages exactly.
    if (defaultLiteral !== undefined) {
      throw new TemplateSyntaxError(
        `${label} '${nameType.trim()}' must have an explicit type ` +
          `(expected 'name = type := value')`,
      );
    }
    throw new TemplateSyntaxError(
      `${label} '${nameType.trim()}' is missing a type annotation ` +
        `(expected 'name = type')`,
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
    // A qualified `Type.Variant` default (e.g. `Stage.Build`) is never valid
    // for an enum-typed parameter — the canonical form is the bare variant
    // name. Reject it here, before the imported-const defer logic, so the
    // error is not misattributed to a missing import. Mirrors the Rust core.
    if (isQualifiedEnumVariantContext(varType, trimmedDefault)) {
      // Rust uses the suffix after the LAST separator; mirror it exactly so the
      // shared "invalid enum default" message is byte-for-byte identical.
      const suffix = trimmedDefault
        .slice(trimmedDefault.lastIndexOf(DOT) + 1)
        .trim();
      throw new TemplateSyntaxError(
        `declaration '${name}': invalid enum default '${trimmedDefault}': ` +
          `${ERR_BARE_VARIANT_HINT} '${suffix}' ` +
          `(a qualified 'Type.Variant' is only valid in expressions)`,
      );
    }
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
  return parseParamDecl(raw, constValues, true);
}

/** Parse `"[stem](path.tmpl.md)"` for imports. */
export function parseImportDecl(raw: string): ImportDecl {
  const unquoted = stripStringLiteral(raw);
  const match = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(unquoted);
  if (!match?.[1] || !match[2]) {
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
