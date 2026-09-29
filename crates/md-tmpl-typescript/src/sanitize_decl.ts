/**
 * Frontmatter `| sanitize(...)` declaration parsing, AST extraction,
 * type resolution, and validation.
 *
 * @module
 */

import {
  BACKSLASH,
  COMMA,
  DOT,
  EQUALS,
  FILTER_SANITIZE,
  PAREN_CLOSE,
  PAREN_OPEN,
  PIPE,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  TYPE_ALIAS,
  TYPE_ENUM,
  TYPE_LIST,
  TYPE_OPTION,
  TYPE_SCALAR_LIST,
  TYPE_STR,
  TYPE_STRUCT,
  TYPE_TMPL,
  TYPE_UNTRUSTED,
} from "./consts.js";
import { TemplateSyntaxError } from "./errors.js";
import { splitPipes } from "./evaluator.js";
import { parseFilter } from "./filters.js";
import type { VarDecl, VarType } from "./frontmatter/types.js";
import {
  splitTopLevel,
  startsWithCompoundType,
  stripTypeBrackets,
} from "./frontmatter/var_type.js";
import {
  type SanitizeSpec,
  parseSanitizeFilterArgs,
} from "./sanitize_runtime.js";

function splitPipeAware(s: string): [string, string] {
  let depth = 0;
  let inSingle = false;
  let inDouble = false;
  let escaped = false;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (ch === BACKSLASH && (inSingle || inDouble)) {
      escaped = true;
      continue;
    }
    if (ch === QUOTE_SINGLE && !inDouble) {
      inSingle = !inSingle;
      continue;
    }
    if (ch === QUOTE_DOUBLE && !inSingle) {
      inDouble = !inDouble;
      continue;
    }
    if (!inSingle && !inDouble) {
      if (ch === PAREN_OPEN) depth++;
      else if (ch === PAREN_CLOSE && depth > 0) depth--;
      else if (ch === PIPE && depth === 0) {
        return [s.slice(0, i), s.slice(i + 1)];
      }
    }
  }
  return [s, ""];
}

function parseDeclSanitizeChain(chain: string): SanitizeSpec {
  const filters = splitPipes(chain);
  let result: SanitizeSpec | undefined;
  for (const rawFilter of filters) {
    const trimmed = rawFilter.trim();
    if (trimmed.length === 0) continue;
    const [name, args] = parseFilter(trimmed);
    if (name !== FILTER_SANITIZE) {
      throw new Error(
        `unsupported parameter filter '${name}' in frontmatter (only '| sanitize' is allowed on declarations)`,
      );
    }
    if (result !== undefined) {
      throw new Error("duplicate '| sanitize' on declaration");
    }
    const parsed = parseSanitizeFilterArgs(args);
    result =
      parsed === undefined
        ? { kind: "inline" }
        : { kind: "block", tag: parsed.tagSpec, notice: parsed.notice };
  }
  if (result === undefined) {
    throw new Error("empty filter after '|' in declaration");
  }
  return result;
}

export function joinPrefix(prefix: string, field: string): string {
  if (prefix.length === 0) return field;
  if (field.length === 0) return prefix;
  return `${prefix}${DOT}${field}`;
}

function insertSpec(
  specs: Map<string, SanitizeSpec>,
  prefix: string,
  spec: SanitizeSpec,
): void {
  const existing = specs.get(prefix);
  if (existing !== undefined) {
    if (existing.kind === spec.kind) return;
    if (existing.kind === "inline" && spec.kind === "block") {
      specs.set(prefix, spec);
      return;
    }
    if (existing.kind === "block" && spec.kind === "inline") {
      return;
    }
    throw new Error(
      `conflicting sanitization specifications on '${prefix.length === 0 ? "type" : prefix}'`,
    );
  }
  specs.set(prefix, spec);
}

function findCharAtDepthZero(s: string, target: string): number {
  let depth = 0;
  let inSingle = false;
  let inDouble = false;
  let escaped = false;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (ch === BACKSLASH && (inSingle || inDouble)) {
      escaped = true;
      continue;
    }
    if (ch === QUOTE_SINGLE && !inDouble) {
      inSingle = !inSingle;
      continue;
    }
    if (ch === QUOTE_DOUBLE && !inSingle) {
      inDouble = !inDouble;
      continue;
    }
    if (!inSingle && !inDouble) {
      if (ch === PAREN_OPEN) depth++;
      else if (ch === PAREN_CLOSE && depth > 0) depth--;
      else if (ch === target && depth === 0) return i;
    }
  }
  return -1;
}

function extractFieldsSanitize(
  inner: string,
  prefix: string,
  specs: Map<string, SanitizeSpec>,
): string {
  const entries = splitTopLevel(inner, COMMA);
  const cleanedEntries: string[] = [];
  for (const entry of entries) {
    const f = entry.trim();
    if (f.length === 0) continue;
    const eqPos = findCharAtDepthZero(f, EQUALS);
    if (eqPos !== -1) {
      const fieldName = f.slice(0, eqPos).trim();
      const fieldTypeRaw = f.slice(eqPos + 1).trim();
      const fieldPrefix = joinPrefix(prefix, fieldName);
      const cleanedFieldType = extractTypeSanitizeInner(
        fieldTypeRaw,
        fieldPrefix,
        specs,
      );
      cleanedEntries.push(`${fieldName} = ${cleanedFieldType}`);
    } else {
      const cleanedElem = extractTypeSanitizeInner(f, prefix, specs);
      cleanedEntries.push(cleanedElem);
    }
  }
  return cleanedEntries.join(", ");
}

function extractEnumVariantsSanitize(
  inner: string,
  prefix: string,
  specs: Map<string, SanitizeSpec>,
): string {
  const entries = splitTopLevel(inner, COMMA);
  const cleanedVariants: string[] = [];
  for (const entry of entries) {
    const v = entry.trim();
    if (v.length === 0) continue;
    const openPos = v.indexOf(PAREN_OPEN);
    if (openPos !== -1 && v.endsWith(PAREN_CLOSE)) {
      const varName = v.slice(0, openPos).trim();
      const varInner = v.slice(openPos + 1, -1);
      const cleanedFields = extractFieldsSanitize(varInner, prefix, specs);
      cleanedVariants.push(
        `${varName}${PAREN_OPEN}${cleanedFields}${PAREN_CLOSE}`,
      );
    } else {
      cleanedVariants.push(v);
    }
  }
  return cleanedVariants.join(", ");
}

function extractTypeSanitizeInner(
  raw: string,
  prefix: string,
  specs: Map<string, SanitizeSpec>,
): string {
  const [basePart, filterChain] = splitPipeAware(raw);
  let base = basePart.trim();
  if (filterChain.trim().length > 0) {
    const spec = parseDeclSanitizeChain(filterChain);
    insertSpec(specs, prefix, spec);
  }

  if (base.startsWith(TYPE_UNTRUSTED)) {
    const rest = base.slice(TYPE_UNTRUSTED.length);
    if (rest.startsWith(" ") || rest.startsWith("\t")) {
      insertSpec(specs, prefix, { kind: "inline" });
      base = rest.trim();
    } else {
      const inner = stripTypeBrackets(base, TYPE_UNTRUSTED);
      if (inner.length !== base.length) {
        insertSpec(specs, prefix, { kind: "inline" });
        base = inner.trim();
      }
    }
  }

  if (startsWithCompoundType(base, TYPE_OPTION)) {
    const inner = stripTypeBrackets(base, TYPE_OPTION);
    const cleanedInner = extractTypeSanitizeInner(inner, prefix, specs);
    return `${TYPE_OPTION}${PAREN_OPEN}${cleanedInner}${PAREN_CLOSE}`;
  }

  for (const compoundKw of [TYPE_STRUCT, TYPE_LIST, TYPE_TMPL] as const) {
    if (startsWithCompoundType(base, compoundKw)) {
      const inner = stripTypeBrackets(base, compoundKw);
      const cleanedFields = extractFieldsSanitize(inner, prefix, specs);
      return `${compoundKw}${PAREN_OPEN}${cleanedFields}${PAREN_CLOSE}`;
    }
  }

  if (startsWithCompoundType(base, TYPE_ENUM)) {
    const inner = stripTypeBrackets(base, TYPE_ENUM);
    const cleanedVariants = extractEnumVariantsSanitize(inner, prefix, specs);
    return `${TYPE_ENUM}${PAREN_OPEN}${cleanedVariants}${PAREN_CLOSE}`;
  }

  return base;
}

/**
 * Strip any `| sanitize(...)` annotations from a type expression, returning
 * `[cleanedTypeStr, relSpecs]`.
 */
export function extractTypeSanitizeSpecs(
  rawType: string,
): [string, Map<string, SanitizeSpec>] {
  const specs = new Map<string, SanitizeSpec>();
  const cleaned = extractTypeSanitizeInner(rawType.trim(), "", specs);
  return [cleaned, specs];
}

function resolveAlias(
  vt: VarType,
  typeAliases?: ReadonlyMap<string, VarType>,
): VarType {
  let cur = vt;
  const seen = new Set<string>();
  while (cur.kind === TYPE_ALIAS && typeAliases) {
    if (seen.has(cur.name)) break;
    seen.add(cur.name);
    const target = typeAliases.get(cur.name);
    if (!target) break;
    cur = target;
  }
  return cur;
}

function allowsSanitize(
  vt: VarType,
  typeAliases?: ReadonlyMap<string, VarType>,
): boolean {
  const resolved = resolveAlias(vt, typeAliases);
  if (resolved.kind === TYPE_STR) return true;
  if (resolved.kind === TYPE_OPTION) {
    return resolveAlias(resolved.innerType, typeAliases).kind === TYPE_STR;
  }
  if (resolved.kind === TYPE_SCALAR_LIST) {
    return resolveAlias(resolved.elementType, typeAliases).kind === TYPE_STR;
  }
  if (resolved.kind === TYPE_ALIAS) {
    // Imported type alias not yet resolved at frontmatter parse time
    return true;
  }
  return false;
}

function resolveRelativeType(
  varType: VarType,
  relPath: string,
  typeAliases?: ReadonlyMap<string, VarType>,
): VarType | undefined {
  const resolved = resolveAlias(varType, typeAliases);
  if (relPath.length === 0) {
    return resolved;
  }
  const dotIdx = relPath.indexOf(DOT);
  const head = dotIdx === -1 ? relPath : relPath.slice(0, dotIdx);
  const tail = dotIdx === -1 ? "" : relPath.slice(dotIdx + 1);

  switch (resolved.kind) {
    case TYPE_OPTION:
      return resolveRelativeType(resolved.innerType, relPath, typeAliases);
    case TYPE_SCALAR_LIST:
      return resolveRelativeType(resolved.elementType, relPath, typeAliases);
    case TYPE_STRUCT:
    case TYPE_LIST:
    case TYPE_TMPL: {
      const field = resolved.fields.find((f) => f.name === head);
      if (!field) return undefined;
      return resolveRelativeType(field.varType, tail, typeAliases);
    }
    case TYPE_ENUM: {
      for (const v of resolved.variants) {
        const field = v.fields.find((f) => f.name === head);
        if (field) {
          return resolveRelativeType(field.varType, tail, typeAliases);
        }
      }
      return undefined;
    }
    case TYPE_ALIAS:
      return resolved;
    default:
      return undefined;
  }
}

/** Validate that all `| sanitize` specs on a declaration resolve to `str`, `option(str)`, or `list(str)`. */
export function validateSanitizeSpecsOnType(
  declName: string,
  varType: VarType,
  specs: ReadonlyMap<string, SanitizeSpec>,
  typeAliases?: ReadonlyMap<string, VarType>,
  formatVarType?: (vt: VarType) => string,
): void {
  for (const relPath of specs.keys()) {
    const fullPath = joinPrefix(declName, relPath);
    const targetType = resolveRelativeType(varType, relPath, typeAliases);
    if (!targetType) {
      throw new TemplateSyntaxError(
        `declaration '${declName}': cannot resolve field '${fullPath}' for '| sanitize'`,
      );
    }
    if (!allowsSanitize(targetType, typeAliases)) {
      const resolvedTarget = resolveAlias(targetType, typeAliases);
      const typeLabel = formatVarType
        ? formatVarType(resolvedTarget)
        : resolvedTarget.kind;
      throw new TemplateSyntaxError(
        `declaration '${declName}': '| sanitize' is only supported on 'str', 'option(str)', or 'list(str)', got '${typeLabel}' on '${fullPath}'`,
      );
    }
  }
}

/** Collect inherited `| sanitize` specs from referenced `types:` aliases. */
export function collectInheritedAliasSanitize(
  rawCleanedType: string,
  prefix: string,
  typeAliasSanitize: ReadonlyMap<string, ReadonlyMap<string, SanitizeSpec>>,
  out: Map<string, SanitizeSpec>,
): void {
  if (typeAliasSanitize.size === 0) return;
  const t = rawCleanedType.trim();
  const aliasSpecs = typeAliasSanitize.get(t);
  if (aliasSpecs) {
    for (const [rel, spec] of aliasSpecs) {
      const key = joinPrefix(prefix, rel);
      if (!out.has(key)) {
        out.set(key, spec);
      }
    }
    return;
  }
  if (startsWithCompoundType(t, TYPE_OPTION)) {
    const inner = stripTypeBrackets(t, TYPE_OPTION);
    collectInheritedAliasSanitize(inner, prefix, typeAliasSanitize, out);
    return;
  }
  for (const compoundKw of [TYPE_STRUCT, TYPE_LIST, TYPE_TMPL] as const) {
    if (startsWithCompoundType(t, compoundKw)) {
      const inner = stripTypeBrackets(t, compoundKw);
      for (const entry of splitTopLevel(inner, COMMA)) {
        const f = entry.trim();
        if (f.length === 0) continue;
        const eqPos = findCharAtDepthZero(f, EQUALS);
        if (eqPos !== -1) {
          const fieldName = f.slice(0, eqPos).trim();
          const fieldType = f.slice(eqPos + 1).trim();
          const fieldPrefix = joinPrefix(prefix, fieldName);
          collectInheritedAliasSanitize(
            fieldType,
            fieldPrefix,
            typeAliasSanitize,
            out,
          );
        } else {
          collectInheritedAliasSanitize(f, prefix, typeAliasSanitize, out);
        }
      }
      return;
    }
  }
}

/**
 * Extract `| sanitize(...)` annotations from a declaration's `typeStr` and optional `defaultLiteral`.
 */
export function extractDeclarationSanitize(
  name: string,
  typeStr: string,
  defaultLiteral: string | undefined,
  isConstant: boolean,
  typeAliasSanitize?: ReadonlyMap<string, ReadonlyMap<string, SanitizeSpec>>,
): [string, string | undefined, Map<string, SanitizeSpec>] {
  let cleanedType: string;
  let relSpecs: Map<string, SanitizeSpec>;
  try {
    [cleanedType, relSpecs] = extractTypeSanitizeSpecs(typeStr);
  } catch (err) {
    throw new TemplateSyntaxError(
      `declaration '${name}': ${err instanceof Error ? err.message : String(err)}`,
    );
  }

  let cleanedDefault = defaultLiteral;
  if (defaultLiteral !== undefined) {
    const [baseDp, dpFilter] = splitPipeAware(defaultLiteral);
    if (dpFilter.trim().length > 0) {
      try {
        const spec = parseDeclSanitizeChain(dpFilter);
        insertSpec(relSpecs, "", spec);
      } catch (err) {
        throw new TemplateSyntaxError(
          `declaration '${name}': ${err instanceof Error ? err.message : String(err)}`,
        );
      }
      cleanedDefault = baseDp.trim();
    }
  }

  if (isConstant && relSpecs.size > 0) {
    throw new TemplateSyntaxError(
      `constant '${name}': '| sanitize' is only supported on 'params' and 'types', not 'consts'`,
    );
  }

  if (!isConstant && typeAliasSanitize) {
    collectInheritedAliasSanitize(cleanedType, "", typeAliasSanitize, relSpecs);
  }

  return [cleanedType, cleanedDefault, relSpecs];
}

/** Merge a declaration's relative sanitize specs into `paramSanitize`. */
export function registerDeclSanitizeSpecs(
  decl: VarDecl,
  relSpecs: ReadonlyMap<string, SanitizeSpec>,
  paramSanitize: Map<string, SanitizeSpec>,
  typeAliases?: ReadonlyMap<string, VarType>,
  formatVarType?: (vt: VarType) => string,
): void {
  validateSanitizeSpecsOnType(
    decl.name,
    decl.varType,
    relSpecs,
    typeAliases,
    formatVarType,
  );
  for (const [rel, spec] of relSpecs) {
    const fullKey = joinPrefix(decl.name, rel);
    paramSanitize.set(fullKey, spec);
  }
}
