/**
 * The lightweight YAML-subset frontmatter parser.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import type { Value } from "../value.js";
import {
  BACKSLASH,
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
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
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
  for (const [i, line] of lines.entries()) {
    if (line.trim() === FM_DELIMITER) {
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
  for (const [i, line] of lines.entries()) {
    if (i <= startIdx) continue;
    if (line.trim() === FM_DELIMITER) {
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

/**
 * Join YAML continuation lines so a single logical declaration may span
 * multiple physical lines — e.g. a multi-line `:=` default value or a
 * multi-line compound type declaration.
 *
 * Mirrors the Rust reference implementation's `join_continuation_lines`: a
 * physical line that does not begin a new top-level section and is not a new
 * `- ` block-list item is appended (space-joined) to the preceding logical
 * line. Blank lines and full-line `#` comments are layout/documentation only:
 * they are preserved in place (so block-list termination validation still
 * sees them) and do NOT break an in-progress continuation.
 *
 * Note: md-tmpl values use flow style (`[...]`, `{...}`), so continuation
 * lines never begin with `- `; a `- ` prefix therefore unambiguously marks a
 * new block-list item rather than a value continuation.
 */
export function joinContinuationLines(lines: string[]): string[] {
  const out: string[] = [];
  // Index in `out` of the logical line that continuations attach to, or -1.
  let currentIdx = -1;
  for (const raw of lines) {
    const trimmed = raw.trim();
    if (trimmed === "" || trimmed.startsWith("#")) {
      // Layout/comment line: keep it, but do not break an in-progress
      // continuation and do not become a continuation target.
      out.push(raw);
      continue;
    }
    const isSection =
      trimmed.startsWith(FM_NAME_PREFIX) ||
      trimmed.startsWith(FM_DESC_PREFIX) ||
      trimmed.startsWith(FM_TYPES_PREFIX) ||
      trimmed.startsWith(FM_IMPORTS_PREFIX) ||
      trimmed.startsWith(FM_PARAMS_PREFIX) ||
      trimmed.startsWith(FM_CONSTS_PREFIX) ||
      trimmed.startsWith(FM_ENV_PREFIX) ||
      trimmed.startsWith(FM_ALLOW_UNUSED_PREFIX);
    const isNewItem = trimmed.startsWith("- ");
    if (!isSection && !isNewItem && currentIdx !== -1) {
      // Continuation of the current logical line.
      const currentLine = out[currentIdx] ?? "";
      out[currentIdx] = `${currentLine} ${trimmed}`;
      continue;
    }
    if (isNewItem) {
      // Strip a YAML-consistent inline `#` comment from the block list-item
      // scalar before joining (mirrors the Rust core's
      // `strip_list_item_comment`). Leading indentation is preserved so
      // downstream trimming and error snippets are unaffected.
      const scalar = trimmed.slice(2).trimStart();
      const kept = stripListItemComment(scalar);
      if (kept === "") {
        // `- # comment` → empty list item; skip like a full-line comment and
        // do not break an in-progress continuation.
        continue;
      }
      const indent = raw.slice(0, raw.length - raw.trimStart().length);
      out.push(`${indent}- ${kept}`);
      currentIdx = out.length - 1;
      continue;
    }
    out.push(raw);
    currentIdx = out.length - 1;
  }
  return out;
}

/**
 * Strip a YAML-consistent inline `#` comment from a block list-item scalar.
 *
 * `scalar` is the text following the `- ` block-sequence marker. Mirrors real
 * YAML plain-scalar comment semantics (and the Rust core's
 * `strip_list_item_comment`): a `#` that begins the scalar or is preceded by
 * whitespace starts a comment running to end of line.
 *
 * A scalar wholly wrapped in a YAML quote (`"..."` / `'...'`) protects any `#`
 * inside the quotes — only a `#` after the closing quote is treated as a
 * comment. This is intentionally NOT md-tmpl-string-aware: the `"` inside an
 * unquoted (plain) scalar such as `x = str := "a # b"` are ordinary
 * characters, so ` #` still starts a comment, mirroring real YAML. To keep a
 * literal ` #`, wrap the whole declaration in an outer YAML quote.
 *
 * Trailing whitespace is trimmed when a comment is removed.
 */
export function stripListItemComment(scalar: string): string {
  const first = scalar[0];
  if (first === QUOTE_DOUBLE) {
    const end = closingQuoteEnd(scalar, QUOTE_DOUBLE);
    if (end === undefined) return scalar; // unterminated — reported downstream
    const pos = findYamlComment(scalar.slice(end), false);
    return pos === undefined ? scalar : scalar.slice(0, end + pos).trimEnd();
  }
  if (first === QUOTE_SINGLE) {
    const end = closingQuoteEnd(scalar, QUOTE_SINGLE);
    if (end === undefined) return scalar;
    const pos = findYamlComment(scalar.slice(end), false);
    return pos === undefined ? scalar : scalar.slice(0, end + pos).trimEnd();
  }
  const pos = findYamlComment(scalar, true);
  return pos === undefined ? scalar : scalar.slice(0, pos).trimEnd();
}

/**
 * Index of the `#` that begins a YAML comment in `s`, or `undefined`.
 *
 * A `#` starts a comment when preceded by ASCII whitespace, or — when
 * `startIsComment` is true — when it is the first character of the string.
 * Mirrors the Rust core's `find_yaml_comment`.
 */
function findYamlComment(
  s: string,
  startIsComment: boolean,
): number | undefined {
  for (let i = 0; i < s.length; i++) {
    if (s[i] !== "#") continue;
    const isComment =
      i === 0 ? startIsComment : s[i - 1] === " " || s[i - 1] === "\t";
    if (isComment) return i;
  }
  return undefined;
}

/**
 * Return the index just past the closing quote of a YAML quoted scalar that
 * starts at index 0, or `undefined` if it is never closed.
 *
 * Double-quoted scalars honor `\`-escapes; single-quoted scalars use the YAML
 * `''` escape for a literal quote.
 */
function closingQuoteEnd(s: string, quote: string): number | undefined {
  if (quote === QUOTE_DOUBLE) {
    let escaped = false;
    for (let i = 1; i < s.length; i++) {
      if (escaped) {
        escaped = false;
      } else if (s[i] === BACKSLASH) {
        escaped = true;
      } else if (s[i] === QUOTE_DOUBLE) {
        return i + 1;
      }
    }
    return undefined;
  }
  for (let i = 1; i < s.length; i++) {
    if (s[i] === QUOTE_SINGLE) {
      if (s[i + 1] === QUOTE_SINGLE) {
        i++; // consume the second quote of an escaped `''`
        continue;
      }
      return i + 1;
    }
  }
  return undefined;
}

export function parseFrontmatterYaml(
  rawLines: string[],
  startLineNo = 2,
): Frontmatter {
  // Fold multi-line declarations onto single logical lines before parsing.
  const lines = joinContinuationLines(rawLines);

  // Validate Frontmatter List Termination Rule:
  // A blank line is strictly required after a block list before starting a new top-level
  // section keyword, so raw markdown renders correctly.
  let inBlockList = false;
  let hadBlankLine = true;
  for (const [i, line] of lines.entries()) {
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

  for (const [i, line] of lines.entries()) {
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
