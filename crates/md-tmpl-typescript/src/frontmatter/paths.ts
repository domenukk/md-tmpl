/**
 * Path interpolation and validation helpers for frontmatter imports.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import { type Value, display, getField } from "../value.js";
import {
  DOT,
  EXPR_END,
  EXPR_START,
  PATH_PREFIX_CUR,
  PATH_PREFIX_CUR_WIN,
  PATH_PREFIX_PARENT,
  PATH_PREFIX_PARENT_WIN,
  PREFIX_CONSTS_DOT,
  PREFIX_OPTIONS_DOT,
  PREFIX_OPTS_DOT,
  PREFIX_PARAMS_DOT,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  SLASH,
  isValidResolvedPath,
} from "../consts.js";
import { type ImportDecl } from "./types.js";

export function interpolatePathStr(
  path: string,
  availableConsts?: ReadonlyMap<string, Value>,
): string {
  const constsMap = availableConsts ?? new Map<string, Value>();
  let result = "";
  let remaining = path;

  let startIdx = remaining.indexOf(EXPR_START);
  while (startIdx !== -1) {
    result += remaining.slice(0, startIdx);
    const afterStart = remaining.slice(startIdx + EXPR_START.length);

    const endIdx = afterStart.indexOf(EXPR_END);
    if (endIdx === -1) {
      throw new TemplateSyntaxError(
        `unclosed '${EXPR_START}' in import path '${path}'`,
      );
    }

    const expr = afterStart.slice(0, endIdx).trim();
    if (expr === "") {
      throw new TemplateSyntaxError(
        `empty expression '${EXPR_START}${EXPR_END}' in import path '${path}'`,
      );
    }

    let valOpt: Value | undefined;
    const lit = stripStringLiteral(expr);
    if (lit !== expr) {
      valOpt = { type: "str", value: lit };
    } else {
      valOpt = constsMap.get(expr);
      if (valOpt === undefined) {
        let stripped = expr;
        if (stripped.startsWith(PREFIX_CONSTS_DOT))
          stripped = stripped.slice(PREFIX_CONSTS_DOT.length).trim();
        else if (stripped.startsWith(PREFIX_OPTS_DOT))
          stripped = stripped.slice(PREFIX_OPTS_DOT.length).trim();
        else if (stripped.startsWith(PREFIX_OPTIONS_DOT))
          stripped = stripped.slice(PREFIX_OPTIONS_DOT.length).trim();
        else if (stripped.startsWith(PREFIX_PARAMS_DOT))
          stripped = stripped.slice(PREFIX_PARAMS_DOT.length).trim();

        valOpt = constsMap.get(stripped);
        if (valOpt === undefined) {
          const parts = stripped.split(DOT);
          const rootKey = parts[0]?.trim() ?? "";
          let val = constsMap.get(rootKey);
          if (val !== undefined) {
            let ok = true;
            for (const rawPart of parts.slice(1)) {
              const part = rawPart.trim();
              const nextVal = getField(val, part);
              if (nextVal !== undefined) {
                val = nextVal;
              } else {
                ok = false;
                break;
              }
            }
            if (ok) valOpt = val;
          }
        }
      }
    }

    if (valOpt === undefined) {
      throw new TemplateSyntaxError(
        `unresolvable expression '${EXPR_START}${expr}${EXPR_END}' in import path '${path}'`,
      );
    }

    result += display(valOpt);
    remaining = afterStart.slice(endIdx + EXPR_END.length);
    startIdx = remaining.indexOf(EXPR_START);
  }

  result += remaining;
  return result;
}

export function interpolateImports(
  imports: ImportDecl[],
  availableConsts: ReadonlyMap<string, Value>,
): void {
  for (let i = 0; i < imports.length; i++) {
    const imp = imports[i];
    if (imp === undefined) continue;
    if (imp.path.includes(EXPR_START)) {
      const loc = (
        imp as { loc?: { line?: number; column?: number; snippet?: string } }
      ).loc;
      try {
        const interpolated = interpolatePathStr(imp.path, availableConsts);
        if (interpolated.includes(EXPR_START)) {
          // Path still has unresolved {{ }} — skip for now, will be
          // resolved later during resolveImportedConsts (chained resolution).
          continue;
        }
        if (!isValidResolvedPath(interpolated)) {
          throw new TemplateSyntaxError(
            `import path '${interpolated}' must start with './', '../', or '/'`,
          );
        }
        imports[i] = { ...imp, path: interpolated };
      } catch (err) {
        if (
          err instanceof TemplateSyntaxError &&
          err.message.includes("unresolvable")
        ) {
          // Unresolvable vars — skip for now, will be resolved later
          // during resolveImportedConsts (chained import resolution).
          continue;
        }
        if (
          err instanceof TemplateSyntaxError &&
          err.line === undefined &&
          loc
        ) {
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
  }
}

// ---------------------------------------------------------------------------
// Declaration parsers
// ---------------------------------------------------------------------------

/**
 * Strips surrounding double quotes (`"`) or single quotes (`'`) from a string.
 * This is used for parameter/const default string values like
 * `param = str := "default"` or for string literals in references like
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
    path.startsWith(SLASH) ||
    path.startsWith(EXPR_START)
  );
}
