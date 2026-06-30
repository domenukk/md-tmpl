/**
 * YAML frontmatter parser.
 *
 * Parses the `---` delimited frontmatter block from `.tmpl.md` files.
 * Extracts params, types, consts, and imports declarations.
 *
 * This is a lightweight parser — not a full YAML parser. It handles
 * the subset of YAML used by prompt-templates frontmatter.
 *
 * @module
 */

import { TemplateSyntaxError } from "./errors.js";
import {
  type Value,
  ENUM_TAG_KEY,
  NONE,
  str,
  int,
  float,
  bool,
} from "./value.js";
import {
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
  TYPE_TMPL,
  TYPE_OPTION,
  PAREN_OPEN,
  PAREN_CLOSE,
  ANGLE_OPEN,
  ANGLE_CLOSE,
  BRACKET_OPEN,
  BRACKET_CLOSE,
  BRACE_OPEN,
  BRACE_CLOSE,
  COMMA,
  EQUALS,
  SLASH,
  PATH_PREFIX_CUR,
  PATH_PREFIX_PARENT,
  PATH_PREFIX_CUR_WIN,
  PATH_PREFIX_PARENT_WIN,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  FM_NAME_PREFIX,
  FM_DESC_PREFIX,
  FM_PARAMS_PREFIX,
  FM_TYPES_PREFIX,
  FM_IMPORTS_PREFIX,
  FM_CONSTS_PREFIX,
  FM_ALLOW_UNUSED_PREFIX,
  ERR_COMPOUND_BRACKETS_PROHIBITED,
} from "./consts.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/** A single parameter declaration from frontmatter. */
export interface VarDecl {
  readonly name: string;
  readonly varType: VarType;
  readonly defaultValue?: Value;
}

/** The type of a template variable. */
export type VarType =
  | { kind: "str" }
  | { kind: "bool" }
  | { kind: "int" }
  | { kind: "float" }
  | { kind: "list"; fields: readonly VarDecl[] }
  | { kind: "scalar_list"; elementType: VarType }
  | { kind: "struct"; fields: readonly VarDecl[] }
  | { kind: "enum"; variants: readonly VariantDecl[]; isOption?: boolean }
  | { kind: "option"; innerType: VarType }
  | { kind: "alias"; name: string }
  | { kind: "untyped_list" };

/** A variant in an enum type. */
export interface VariantDecl {
  readonly name: string;
  readonly fields: readonly VarDecl[];
}

/** Parsed frontmatter. */
export interface Frontmatter {
  readonly name?: string;
  readonly description?: string;
  readonly params: readonly VarDecl[];
  readonly allowUnused: boolean;
  readonly typeAliases: ReadonlyMap<string, VarType>;
  readonly consts: readonly VarDecl[];
  readonly imports: readonly ImportDecl[];
  /** Resolved constants from imports, keyed by `stem.NAME`. */
  readonly importedConsts: Readonly<Record<string, unknown>>;
  /**
   * Param defaults that couldn't be resolved during frontmatter parsing
   * (e.g., references to imported consts like `stem.NAME`).
   * Maps param name → raw default text.
   */
  readonly unresolvedDefaults: ReadonlyMap<
    string,
    { text: string; varType: VarType }
  >;
}

/** An import declaration. */
export interface ImportDecl {
  readonly stem: string;
  readonly path: string;
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/** Format a VarType as a string (for declarations output). */
export function varTypeToString(vt: VarType): string {
  switch (vt.kind) {
    case TYPE_STR:
    case TYPE_BOOL:
    case TYPE_INT:
    case TYPE_FLOAT:
      return vt.kind;
    case TYPE_LIST:
      if (vt.fields.length === 0) return "list()";
      return `list(${vt.fields.map((f) => `${f.name} = ${varTypeToString(f.varType)}`).join(", ")})`;
    case "scalar_list":
      return `list(${varTypeToString(vt.elementType)})`;
    case TYPE_STRUCT:
      if (vt.fields.length === 0) return "struct()";
      return `struct(${vt.fields.map((f) => `${f.name} = ${varTypeToString(f.varType)}`).join(", ")})`;
    case TYPE_ENUM: {
      if (vt.isOption) {
        const someVariant = vt.variants.find((v) => v.name === "Some");
        if (someVariant && someVariant.fields.length === 1) {
          return `option(${varTypeToString(someVariant.fields[0]!.varType)})`;
        }
      }
      const parts = vt.variants.map((v) => {
        if (v.fields.length === 0) return v.name;
        return `${v.name}(${v.fields.map((f) => `${f.name} = ${varTypeToString(f.varType)}`).join(", ")})`;
      });
      return `enum(${parts.join(", ")})`;
    }
    case TYPE_OPTION:
      return `option(${varTypeToString(vt.innerType)})`;
    case "alias":
      return vt.name;
    case "untyped_list":
      return "list()";
  }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/**
 * Parse the frontmatter and body from a template source string.
 *
 * Returns `[frontmatter, body]` where body is everything after the
 * closing `---` delimiter.
 */
export function parseFrontmatter(source: string): [Frontmatter, string] {
  const lines = source.split("\n");

  // Find opening ---
  let startIdx = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i]!.trim() === `---`) {
      startIdx = i;
      break;
    }
  }
  if (startIdx === -1) {
    throw new TemplateSyntaxError(
      "missing mandatory YAML frontmatter block (starts with ---)",
    );
  }

  // Find closing ---
  let endIdx = -1;
  for (let i = startIdx + 1; i < lines.length; i++) {
    if (lines[i]!.trim() === `---`) {
      endIdx = i;
      break;
    }
  }
  if (endIdx === -1) {
    throw new TemplateSyntaxError("unclosed YAML frontmatter block");
  }

  const fmLines = lines.slice(startIdx + 1, endIdx);
  const body = lines.slice(endIdx + 1).join("\n");

  // Parse the frontmatter YAML subset
  const fm = parseFrontmatterYaml(fmLines);
  return [fm, body];
}

/**
 * Strip frontmatter from source, returning just the body.
 */
export function stripFrontmatter(source: string): string {
  const [, body] = parseFrontmatter(source);
  return body;
}

// ---------------------------------------------------------------------------
// Internal YAML-subset parser
// ---------------------------------------------------------------------------

function parseFrontmatterYaml(lines: string[]): Frontmatter {
  // Validate Frontmatter List Termination Rule:
  // A blank line is strictly required after a block list before starting a new top-level
  // section keyword, so raw markdown renders correctly.
  let inBlockList = false;
  let hadBlankLine = true;
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) {
      hadBlankLine = true;
      continue;
    }
    const startsWithSection =
      trimmed.startsWith(FM_NAME_PREFIX) ||
      trimmed.startsWith(FM_DESC_PREFIX) ||
      trimmed.startsWith(FM_TYPES_PREFIX) ||
      trimmed.startsWith(FM_IMPORTS_PREFIX) ||
      trimmed.startsWith(FM_PARAMS_PREFIX) ||
      trimmed.startsWith(FM_CONSTS_PREFIX) ||
      trimmed.startsWith(FM_ALLOW_UNUSED_PREFIX);

    if (startsWithSection) {
      if (inBlockList && !hadBlankLine) {
        throw new TemplateSyntaxError(
          `A blank line is required after a block list before '${trimmed}' so raw markdown renders correctly`,
        );
      }
      inBlockList = false;
    } else if (trimmed.startsWith("-")) {
      inBlockList = true;
    }
    hadBlankLine = false;
  }

  let name: string | undefined;
  let description: string | undefined;
  let allowUnused = false;
  const typeAliases = new Map<string, VarType>();
  const imports: ImportDecl[] = [];

  // Two-pass approach: first collect raw items per block, then resolve.
  // This allows consts to be parsed before params, enabling const names
  // as param defaults.
  const rawParams: string[] = [];
  const rawConsts: string[] = [];
  let inlineParamsRaw: string[] | undefined;

  let currentBlock: "none" | "params" | "types" | "consts" | "imports" = "none";

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;

    // Top-level keys
    if (trimmed.startsWith(FM_NAME_PREFIX)) {
      name = trimmed
        .slice(FM_NAME_PREFIX.length)
        .trim()
        .replace(/^["']|["']$/g, "");
      currentBlock = "none";
      continue;
    }
    if (trimmed.startsWith(FM_DESC_PREFIX)) {
      description = trimmed
        .slice(FM_DESC_PREFIX.length)
        .trim()
        .replace(/^["']|["']$/g, "");
      currentBlock = "none";
      continue;
    }
    if (trimmed.startsWith(FM_ALLOW_UNUSED_PREFIX)) {
      allowUnused =
        trimmed.slice(FM_ALLOW_UNUSED_PREFIX.length).trim() === "true";
      currentBlock = "none";
      continue;
    }

    // Block starts
    if (trimmed.startsWith(FM_PARAMS_PREFIX)) {
      currentBlock = "params";
      // Inline params: [x = str, y = int]
      const rest = trimmed.slice(FM_PARAMS_PREFIX.length).trim();
      if (rest.startsWith("[")) {
        inlineParamsRaw = parseInlineList(rest);
        currentBlock = "none";
      }
      continue;
    }
    if (trimmed.startsWith(FM_TYPES_PREFIX)) {
      currentBlock = "types";
      continue;
    }
    if (trimmed.startsWith(FM_CONSTS_PREFIX)) {
      currentBlock = "consts";
      continue;
    }
    if (trimmed.startsWith(FM_IMPORTS_PREFIX)) {
      currentBlock = "imports";
      continue;
    }

    // List items
    if (trimmed.startsWith("- ")) {
      const item = trimmed.slice(2).trim();
      switch (currentBlock) {
        case "params":
          rawParams.push(item);
          break;
        case "types": {
          const [aliasName, aliasType] = parseTypeAlias(item);
          if (typeAliases.has(aliasName)) {
            throw new TemplateSyntaxError(
              `duplicate type alias '${aliasName}'`,
            );
          }
          typeAliases.set(aliasName, aliasType);
          break;
        }
        case "consts":
          rawConsts.push(item);
          break;
        case "imports":
          imports.push(parseImportDecl(item));
          break;
        default:
          break;
      }
    }
  }

  // Phase 1: Parse consts (they only use literal defaults)
  const consts: VarDecl[] = [];
  for (const raw of rawConsts) {
    consts.push(parseConstDecl(raw));
  }

  // Build resolved const values map for param default resolution
  const constValues = new Map<string, Value>();
  for (const decl of consts) {
    if (decl.defaultValue !== undefined) {
      constValues.set(decl.name, decl.defaultValue);
    }
  }

  // Phase 2: Parse params with const values available for defaults.
  // For imported consts (dotted names like stem.NAME), the default is
  // deferred — stored in unresolvedDefaults for later resolution.
  const params: VarDecl[] = [];
  const unresolvedDefaults = new Map<
    string,
    { text: string; varType: VarType }
  >();
  const allRawParams = inlineParamsRaw ?? rawParams;
  for (const raw of allRawParams) {
    const [decl, unresolved] = parseParamDeclDeferred(raw, constValues);
    params.push(decl);
    if (unresolved !== undefined) {
      unresolvedDefaults.set(decl.name, unresolved);
    }
  }

  return {
    name,
    description,
    params,
    allowUnused,
    typeAliases,
    consts,
    imports,
    importedConsts: {},
    unresolvedDefaults,
  };
}

// ---------------------------------------------------------------------------
// Declaration parsers
// ---------------------------------------------------------------------------

/**
 * Parse `name = type` or `name = type := default`.
 *
 * If `constValues` is provided, unrecognised default literals are
 * looked up as const names before raising an error.  This enables
 * `param = int := MY_CONST` where `MY_CONST` is a previously
 * declared constant.
 */
export function stripStringLiteral(s: string): string {
  const trimmed = s.trim();
  if (
    (trimmed.startsWith(QUOTE_DOUBLE) &&
      trimmed.endsWith(QUOTE_DOUBLE) &&
      trimmed.length >= 2) ||
    (trimmed.startsWith(QUOTE_SINGLE) &&
      trimmed.endsWith(QUOTE_SINGLE) &&
      trimmed.length >= 2)
  ) {
    return trimmed.slice(1, -1).trim();
  }
  return trimmed;
}

export function isValidPathPrefix(path: string): boolean {
  return (
    path.startsWith(PATH_PREFIX_CUR) ||
    path.startsWith(PATH_PREFIX_PARENT) ||
    path.startsWith(PATH_PREFIX_CUR_WIN) ||
    path.startsWith(PATH_PREFIX_PARENT_WIN) ||
    path.startsWith(SLASH)
  );
}

function parseParamDecl(
  raw: string,
  constValues?: ReadonlyMap<string, Value>,
): VarDecl {
  const cleaned = stripStringLiteral(raw);
  const defaultSplit = splitDefault(cleaned);
  const [nameType, defaultLiteral] = defaultSplit;

  const eqIdx = nameType!.indexOf(EQUALS);
  if (eqIdx === -1) {
    throw new TemplateSyntaxError(
      `parameter must have explicit type: '${raw}'`,
    );
  }

  const name = stripStringLiteral(nameType!.slice(0, eqIdx).trim());
  const typeStr = nameType!.slice(eqIdx + 1).trim();

  const varType = parseVarType(typeStr);
  let defaultValue: Value | undefined;
  if (defaultLiteral !== undefined) {
    const trimmedDefault = defaultLiteral.trim();
    defaultValue = parseLiteralOrConst(trimmedDefault, varType, constValues);
  }

  return { name, varType, defaultValue };
}

/**
 * Like `parseParamDecl` but defers unresolvable defaults (e.g., imported
 * consts like `stem.NAME`) instead of throwing.
 *
 * Returns `[VarDecl, unresolved]` where `unresolved` is either undefined
 * (default was resolved) or `{ text, varType }` for later resolution.
 */
function parseParamDeclDeferred(
  raw: string,
  constValues?: ReadonlyMap<string, Value>,
): [VarDecl, { text: string; varType: VarType } | undefined] {
  const cleaned = stripStringLiteral(raw);
  const defaultSplit = splitDefault(cleaned);
  const [nameType, defaultLiteral] = defaultSplit;

  const eqIdx = nameType!.indexOf(EQUALS);
  if (eqIdx === -1) {
    throw new TemplateSyntaxError(
      `parameter must have explicit type: '${raw}'`,
    );
  }

  const name = stripStringLiteral(nameType!.slice(0, eqIdx).trim());
  const typeStr = nameType!.slice(eqIdx + 1).trim();

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
    } catch {
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
function parseTypeAlias(raw: string): [string, VarType] {
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
function parseConstDecl(raw: string): VarDecl {
  return parseParamDecl(raw);
}

/** Parse `"[stem](path.tmpl.md)"` for imports. */
function parseImportDecl(raw: string): ImportDecl {
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

// ---------------------------------------------------------------------------
// Type parser
// ---------------------------------------------------------------------------

/** Parse a type annotation string into a VarType. */
function startsWithCompoundType(s: string, keyword: string): boolean {
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

  if (t === TYPE_STR) return { kind: "str" };
  if (t === TYPE_BOOL) return { kind: "bool" };
  if (t === TYPE_INT) return { kind: "int" };
  if (t === TYPE_FLOAT) return { kind: "float" };

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
        return { kind: "list", fields: elementType.fields };
      }
      return { kind: "scalar_list", elementType };
    }
    const fields = parseFieldList(inner);
    return { kind: "list", fields };
  }

  if (startsWithCompoundType(t, TYPE_STRUCT)) {
    const inner = stripTypeBrackets(t, TYPE_STRUCT);
    if (inner === "") {
      throw new TemplateSyntaxError(
        "untyped struct() is not allowed; must specify fields (e.g., struct(name = str))",
      );
    }
    const fields = parseFieldList(inner);
    return { kind: "struct", fields };
  }

  if (startsWithCompoundType(t, TYPE_OPTION)) {
    const inner = stripTypeBrackets(t, TYPE_OPTION);
    const innerType = parseVarType(inner);
    return { kind: "option", innerType };
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
    return { kind: "enum", variants };
  }

  if (startsWithCompoundType(t, TYPE_TMPL)) {
    const inner = stripTypeBrackets(t, TYPE_TMPL);
    if (inner === "") return { kind: "struct", fields: [] };
    const fields = parseFieldList(inner);
    return { kind: "struct", fields };
  }

  // Type alias reference
  return { kind: "alias", name: t };
}

/** Extract content between parentheses: `list(...)` → `...`. */
function stripTypeBrackets(s: string, keyword: string): string {
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
function parseFieldList(inner: string): VarDecl[] {
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
function parseVariantList(inner: string): VariantDecl[] {
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
 * Split a string by a delimiter, respecting nested angle brackets, curly braces, and parens.
 */
function splitTopLevel(s: string, delimiter: string): string[] {
  const result: string[] = [];
  let depth = 0;
  let current = "";

  for (let i = 0; i < s.length; i++) {
    const ch = s[i]!;
    if (
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
function parseInlineList(s: string): string[] {
  const trimmed = s.trim();
  if (!trimmed.startsWith(BRACKET_OPEN) || !trimmed.endsWith(BRACKET_CLOSE)) {
    throw new TemplateSyntaxError(`expected inline list: ${s}`);
  }
  const inner = trimmed.slice(1, -1).trim();
  if (inner === "") return [];
  return splitTopLevel(inner, COMMA);
}

/** Split `name = type := default` into `["name = type", "default"]`. */
function splitDefault(raw: string): [string, string | undefined] {
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
function parseLiteralOrConst(
  literal: string,
  varType: VarType,
  constValues?: ReadonlyMap<string, Value>,
): Value {
  try {
    return parseLiteral(literal, varType);
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
function validateConstDefaultType(
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
  if (expectedKind in typeMap) {
    const expected = typeMap[expectedKind]!;
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
    validateConstDefaultType(
      constName,
      constVal,
      (varType as Extract<VarType, { kind: "option" }>).innerType,
    );
  }
}

/** Parse a literal value for a default. */
export function parseLiteral(literal: string, varType: VarType): Value {
  // Option types need to be checked first so that quoted strings like
  // "None" are correctly parsed as Some(val="None") rather than being
  // consumed by the generic string literal handler as str("None").
  if (varType.kind === TYPE_OPTION) {
    // Bare None → Value.None
    if (literal === "None") {
      return NONE;
    }
    // Any other literal → parse as the inner type (auto-wrap to Some)
    return parseLiteral(literal, varType.innerType);
  }

  // Legacy isOption handling (for backward compatibility with old enum-based options)
  if (varType.kind === TYPE_ENUM && varType.isOption) {
    if (literal === "None") {
      return NONE;
    }
    const someVariant = varType.variants.find((v) => v.name === "Some");
    if (someVariant && someVariant.fields.length === 1) {
      return parseLiteral(literal, someVariant.fields[0]!.varType);
    }
  }

  // String literals
  if (
    (literal.startsWith(QUOTE_DOUBLE) && literal.endsWith(QUOTE_DOUBLE)) ||
    (literal.startsWith(QUOTE_SINGLE) && literal.endsWith(QUOTE_SINGLE))
  ) {
    return str(literal.slice(1, -1));
  }

  // Boolean
  if (literal === "true") return bool(true);
  if (literal === "false") return bool(false);

  // Struct literals: {KEY = "val", KEY2 = 42}
  if (varType.kind === TYPE_STRUCT && literal.startsWith(BRACE_OPEN)) {
    return parseStructLiteral(literal, varType);
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
        const fieldType = fieldTypeMap.get(key) ?? { kind: "str" as const };
        entries.push([key, parseLiteral(valStr, fieldType)]);
      }
      return { type: "dict", fields: new Map(entries) };
    }

    // Bare identifier — must be a known variant.
    const variant = varType.variants.find((v) => v.name === literal);
    if (!variant) {
      throw new TemplateSyntaxError(`unknown enum variant '${literal}'`);
    }
    if (variant.fields.length > 0) {
      throw new TemplateSyntaxError(
        `struct variant '${literal}' requires fields; use ${literal}(field = value)`,
      );
    }
    return str(literal);
  }

  // If the expected type is a type alias, allow unquoted identifiers.
  if (varType.kind === "alias") {
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
function parseStructLiteral(
  literal: string,
  varType: Extract<VarType, { kind: "struct" }>,
): Value {
  const inner = literal.slice(1, -1).trim();
  if (inner === "") {
    return { type: "dict", fields: new Map() };
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
    const fieldType = fieldTypeMap.get(key) ?? { kind: "str" as const };
    entries.push([key, parseLiteral(valStr, fieldType)]);
  }
  return { type: "dict", fields: new Map(entries) };
}
