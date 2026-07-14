/**
 * The lightweight YAML-subset frontmatter parser.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import type { Value } from "../value.js";
import {
  FM_ALLOW_UNUSED_PREFIX,
  FM_CONSTS_PREFIX,
  FM_DELIMITER,
  FM_DESC_PREFIX,
  FM_ENV_PREFIX,
  FM_IMPORTS_PREFIX,
  FM_NAME_PREFIX,
  FM_PARAMS_PREFIX,
  FM_TYPES_PREFIX,
  LIT_TRUE,
} from "../consts.js";
import {
  type Frontmatter,
  type ImportDecl,
  type VarType,
  type VarDecl,
} from "./types.js";
import { interpolateImports } from "./paths.js";
import {
  parseTypeAlias,
  parseConstDecl,
  parseImportDecl,
  parseParamDeclDeferred,
} from "./declarations.js";
import { parseInlineList } from "./var_type.js";

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
    if (lines[i]!.trim() === FM_DELIMITER) {
      startIdx = i;
      break;
    }
  }
  if (startIdx === -1) {
    throw new TemplateSyntaxError(
      "missing mandatory YAML frontmatter block (starts with ---)",
      1,
      1,
      lines[0] ?? "",
    );
  }

  // Find closing ---
  let endIdx = -1;
  for (let i = startIdx + 1; i < lines.length; i++) {
    if (lines[i]!.trim() === FM_DELIMITER) {
      endIdx = i;
      break;
    }
  }
  if (endIdx === -1) {
    throw new TemplateSyntaxError(
      "unclosed YAML frontmatter block",
      startIdx + 1,
      1,
      lines[startIdx] ?? "",
    );
  }

  const fmLines = lines.slice(startIdx + 1, endIdx);
  const body = lines.slice(endIdx + 1).join("\n");

  // Parse the frontmatter YAML subset
  const fm = parseFrontmatterYaml(fmLines, startIdx + 2);
  return [{ ...fm, bodyStartLine: endIdx + 2 }, body];
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

export function parseFrontmatterYaml(
  lines: string[],
  startLineNo = 2,
): Frontmatter {
  // Validate Frontmatter List Termination Rule:
  // A blank line is strictly required after a block list before starting a new top-level
  // section keyword, so raw markdown renders correctly.
  let inBlockList = false;
  let hadBlankLine = true;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
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
      trimmed.startsWith(FM_ENV_PREFIX) ||
      trimmed.startsWith(FM_ALLOW_UNUSED_PREFIX);

    if (startsWithSection) {
      if (inBlockList && !hadBlankLine) {
        throw new TemplateSyntaxError(
          `A blank line is required after a block list before '${trimmed}' so raw markdown renders correctly`,
          startLineNo + i,
          1,
          line,
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
  interface RawItem {
    raw: string;
    loc: { line: number; column: number; snippet: string };
  }
  const rawParams: RawItem[] = [];
  const rawConsts: RawItem[] = [];
  const rawEnv: RawItem[] = [];
  let inlineParamsRaw: RawItem[] | undefined;

  let currentBlock: "none" | "params" | "types" | "consts" | "env" | "imports" =
    "none";

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    const loc = { line: startLineNo + i, column: 1, snippet: line };

    try {
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
          trimmed.slice(FM_ALLOW_UNUSED_PREFIX.length).trim() === LIT_TRUE;
        currentBlock = "none";
        continue;
      }

      // Block starts
      if (trimmed.startsWith(FM_PARAMS_PREFIX)) {
        currentBlock = "params";
        // Inline params: [x = str, y = int]
        const rest = trimmed.slice(FM_PARAMS_PREFIX.length).trim();
        if (rest.startsWith("[")) {
          const items = parseInlineList(rest);
          inlineParamsRaw = items.map((item) => ({ raw: item, loc }));
          currentBlock = "none";
        }
        continue;
      }
      if (trimmed.startsWith(FM_TYPES_PREFIX)) {
        currentBlock = "types";
        const rest = trimmed.slice(FM_TYPES_PREFIX.length).trim();
        if (rest.startsWith("[")) {
          const items = parseInlineList(rest);
          for (const item of items) {
            const [aliasName, aliasType] = parseTypeAlias(item);
            if (typeAliases.has(aliasName)) {
              throw new TemplateSyntaxError(
                `duplicate type alias '${aliasName}'`,
              );
            }
            typeAliases.set(aliasName, aliasType);
          }
          currentBlock = "none";
        }
        continue;
      }
      if (trimmed.startsWith(FM_CONSTS_PREFIX)) {
        currentBlock = "consts";
        const rest = trimmed.slice(FM_CONSTS_PREFIX.length).trim();
        if (rest.startsWith("[")) {
          const items = parseInlineList(rest);
          for (const item of items) {
            rawConsts.push({ raw: item, loc });
          }
          currentBlock = "none";
        }
        continue;
      }
      if (trimmed.startsWith(FM_ENV_PREFIX)) {
        currentBlock = "env";
        const rest = trimmed.slice(FM_ENV_PREFIX.length).trim();
        if (rest.startsWith("[")) {
          const items = parseInlineList(rest);
          for (const item of items) {
            rawEnv.push({ raw: item, loc });
          }
          currentBlock = "none";
        }
        continue;
      }
      if (trimmed.startsWith(FM_IMPORTS_PREFIX)) {
        currentBlock = "imports";
        const rest = trimmed.slice(FM_IMPORTS_PREFIX.length).trim();
        if (rest.startsWith("[")) {
          const items = parseInlineList(rest);
          for (const item of items) {
            imports.push({ ...parseImportDecl(item), loc });
          }
          currentBlock = "none";
        }
        continue;
      }

      // List items
      if (trimmed.startsWith("- ")) {
        const item = trimmed.slice(2).trim();
        switch (currentBlock) {
          case "params":
            rawParams.push({ raw: item, loc });
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
            rawConsts.push({ raw: item, loc });
            break;
          case "env":
            rawEnv.push({ raw: item, loc });
            break;
          case "imports":
            imports.push({ ...parseImportDecl(item), loc });
            break;
          default:
            break;
        }
      }
    } catch (err) {
      if (err instanceof TemplateSyntaxError && err.line === undefined) {
        throw new TemplateSyntaxError(
          err.message,
          loc.line,
          loc.column,
          loc.snippet,
        );
      }
      throw err;
    }
  }

  // Phase 1: Parse consts (they can reference earlier consts)
  const consts: VarDecl[] = [];
  const constValues = new Map<string, Value>();
  for (const item of rawConsts) {
    try {
      const decl = parseConstDecl(item.raw, constValues);
      consts.push({ ...decl, loc: item.loc });
      if (decl.defaultValue !== undefined) {
        constValues.set(decl.name, decl.defaultValue);
      }
    } catch (err) {
      if (err instanceof TemplateSyntaxError && err.line === undefined) {
        throw new TemplateSyntaxError(
          err.message,
          item.loc.line,
          item.loc.column,
          item.loc.snippet,
        );
      }
      throw err;
    }
  }

  // Phase 1b: Parse env declarations (same syntax as consts)
  const env: VarDecl[] = [];
  for (const item of rawEnv) {
    try {
      const decl = parseConstDecl(item.raw, constValues);
      env.push({ ...decl, loc: item.loc });
    } catch (err) {
      if (err instanceof TemplateSyntaxError && err.line === undefined) {
        throw new TemplateSyntaxError(
          err.message,
          item.loc.line,
          item.loc.column,
          item.loc.snippet,
        );
      }
      throw err;
    }
  }

  interpolateImports(imports, constValues);

  // Phase 2: Parse params with const values available for defaults.
  // For imported consts (dotted names like stem.NAME), the default is
  // deferred — stored in unresolvedDefaults for later resolution.
  const params: VarDecl[] = [];
  const unresolvedDefaults = new Map<
    string,
    { text: string; varType: VarType }
  >();
  const allRawParams = inlineParamsRaw ?? rawParams;
  for (const item of allRawParams) {
    try {
      const [decl, unresolved] = parseParamDeclDeferred(item.raw, constValues);
      params.push({ ...decl, loc: item.loc });
      if (unresolved !== undefined) {
        unresolvedDefaults.set(decl.name, unresolved);
      }
    } catch (err) {
      if (err instanceof TemplateSyntaxError && err.line === undefined) {
        throw new TemplateSyntaxError(
          err.message,
          item.loc.line,
          item.loc.column,
          item.loc.snippet,
        );
      }
      throw err;
    }
  }

  return {
    name,
    description,
    params,
    allowUnused,
    typeAliases,
    consts,
    env,
    imports,
    importedConsts: {},
    importedNamespaceTypes: new Map(),
    unresolvedDefaults,
  };
}
