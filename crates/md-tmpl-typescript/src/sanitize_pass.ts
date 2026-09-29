/**
 * Compile-time AST pass for parameter sanitization propagation,
 * boundary notice resolution, and enclosing XML tag detection.
 *
 * @module
 */

import type { Node } from "./ast.js";
import {
  BACKSLASH,
  COMMA,
  DOT,
  FILTER_SANITIZE,
  NODE_EXPR,
  NODE_FOR,
  NODE_IF,
  NODE_MATCH,
  NODE_PANIC,
  NODE_TEXT,
  PREFIX_PARAMS_DOT,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
} from "./consts.js";
import { splitPipes } from "./evaluator.js";
import { parseFilter } from "./filters.js";
import {
  type SanitizeSpec,
  parseSanitizeFilterArgs,
  isAsciiWhitespaceCode,
  isNcnameStartCode,
  isNcnameContinueCode,
} from "./sanitize_runtime.js";

export * from "./sanitize_runtime.js";
export * from "./sanitize_decl.js";

/** Parse a `sanitize_notice:` frontmatter value, stripping optional surrounding quotes. */
export function parseSanitizeNoticeValue(raw: string): string {
  const trimmed = raw.trim();
  if (
    trimmed.length >= 2 &&
    ((trimmed.startsWith(QUOTE_DOUBLE) && trimmed.endsWith(QUOTE_DOUBLE)) ||
      (trimmed.startsWith(QUOTE_SINGLE) && trimmed.endsWith(QUOTE_SINGLE)))
  ) {
    const inner = trimmed.slice(1, -1);
    let out = "";
    for (let i = 0; i < inner.length; i++) {
      const ch = inner[i];
      if (ch === BACKSLASH && i + 1 < inner.length) {
        const nextCh = inner[i + 1];
        i++;
        if (nextCh === "n") out += "\n";
        else if (nextCh === "t") out += "\t";
        else if (nextCh !== undefined) out += nextCh;
      } else if (ch !== undefined) {
        out += ch;
      }
    }
    return out;
  }
  return trimmed;
}

// ---------------------------------------------------------------------------
// Compile-time AST pass: parameter sanitization + notice + enclosing XML tags
// ---------------------------------------------------------------------------

function quoteFilterArg(s: string): string {
  const escaped = s
    .replaceAll(BACKSLASH, "\\\\")
    .replaceAll(QUOTE_DOUBLE, '\\"');
  return `${QUOTE_DOUBLE}${escaped}${QUOTE_DOUBLE}`;
}

function formatSanitizeSpecFilter(
  spec: SanitizeSpec,
  fmNotice: string | undefined,
): string {
  if (spec.kind === "inline") {
    return FILTER_SANITIZE;
  }
  const effectiveNotice = spec.notice ?? fmNotice;
  if (effectiveNotice !== undefined) {
    return `${FILTER_SANITIZE}(${quoteFilterArg(spec.tag)}, ${quoteFilterArg(effectiveNotice)})`;
  }
  return `${FILTER_SANITIZE}(${quoteFilterArg(spec.tag)})`;
}

function resolveCanonicalPath(
  rawPath: string,
  loopAliases: ReadonlyMap<string, string>,
): string {
  const trimmed = rawPath.trim();
  const withoutParams = trimmed.startsWith(PREFIX_PARAMS_DOT)
    ? trimmed.slice(PREFIX_PARAMS_DOT.length).trim()
    : trimmed;
  const dotIdx = withoutParams.indexOf(DOT);
  const root = dotIdx === -1 ? withoutParams : withoutParams.slice(0, dotIdx);
  const resolvedRoot = loopAliases.get(root) ?? root;
  if (dotIdx === -1) {
    return resolvedRoot;
  }
  return `${resolvedRoot}${withoutParams.slice(dotIdx)}`;
}

function mergeEnclosingTagsIntoSpec(
  existing: string | undefined,
  openTags: readonly string[],
): string | undefined {
  if (openTags.length === 0) {
    return existing;
  }
  const combined: string[] = [];
  if (existing !== undefined) {
    for (const part of existing.split(COMMA)) {
      const trimmed = part.trim();
      if (
        trimmed.length > 0 &&
        !combined.some((c) => c.toLowerCase() === trimmed.toLowerCase())
      ) {
        combined.push(trimmed);
      }
    }
  }
  for (let i = openTags.length - 1; i >= 0; i--) {
    const tag = openTags[i];
    if (
      tag !== undefined &&
      !combined.some((c) => c.toLowerCase() === tag.toLowerCase())
    ) {
      combined.push(tag);
    }
  }
  return combined.length === 0 ? undefined : combined.join(COMMA);
}

function rewriteExprForSanitize(
  expr: string,
  paramSanitize: ReadonlyMap<string, SanitizeSpec> | undefined,
  sanitizeNotice: string | undefined,
  loopAliases: ReadonlyMap<string, string>,
  openTags: readonly string[],
): string {
  const parts = splitPipes(expr);
  const baseExpr = (parts[0] ?? "").trim();
  const filterStrings = parts.slice(1).map((p) => p.trim());

  // 1. Check if baseExpr is a plain variable/field path that matches paramSanitize
  if (
    paramSanitize &&
    paramSanitize.size > 0 &&
    /^[a-zA-Z_]\w*(?:\.[a-zA-Z_]\w*)*$/.test(baseExpr)
  ) {
    const canonical = resolveCanonicalPath(baseExpr, loopAliases);
    const spec = paramSanitize.get(canonical);
    if (spec !== undefined) {
      const alreadySanitized = filterStrings.some((f) => {
        const [fName] = parseFilter(f);
        return (
          fName === FILTER_SANITIZE ||
          fName === "quarantine" ||
          fName === "sanitize_tokens"
        );
      });
      if (!alreadySanitized) {
        filterStrings.push(formatSanitizeSpecFilter(spec, sanitizeNotice));
      }
    }
  }

  if (filterStrings.length === 0) {
    return expr;
  }

  // 2. Rewrite any `sanitize` filter for template-level `sanitizeNotice` and `openTags`
  let modified = filterStrings.length !== parts.length - 1;
  const updatedFilters = filterStrings.map((fStr) => {
    const [fName, fArgs] = parseFilter(fStr);
    if (fName !== FILTER_SANITIZE) {
      return fStr;
    }
    const parsed = parseSanitizeFilterArgs(fArgs);
    if (parsed === undefined) {
      // 0-arg `| sanitize`: if inside enclosing XML tags, rewrite to `sanitize_tokens("tags")`
      const mergedTags = mergeEnclosingTagsIntoSpec(undefined, openTags);
      if (mergedTags !== undefined) {
        modified = true;
        return `sanitize_tokens(${quoteFilterArg(mergedTags)})`;
      }
      return fStr;
    }
    // 1-arg or 2-arg `| sanitize("tag", ...)`: merge enclosing tags + template sanitizeNotice
    const mergedTagSpec =
      mergeEnclosingTagsIntoSpec(parsed.tagSpec, openTags) ?? parsed.tagSpec;
    const effectiveNotice = parsed.notice ?? sanitizeNotice;
    if (mergedTagSpec !== parsed.tagSpec || effectiveNotice !== parsed.notice) {
      modified = true;
      if (effectiveNotice !== undefined) {
        return `${FILTER_SANITIZE}(${quoteFilterArg(mergedTagSpec)}, ${quoteFilterArg(effectiveNotice)})`;
      }
      return `${FILTER_SANITIZE}(${quoteFilterArg(mergedTagSpec)})`;
    }
    return fStr;
  });

  if (!modified) {
    return expr;
  }
  return `${baseExpr} | ${updatedFilters.join(" | ")}`;
}

function scanClosingTagsInText(text: string, out: Set<string>): void {
  let i = 0;
  while (i + 3 < text.length) {
    if (text.charCodeAt(i) === 60 /* '<' */) {
      let pos = i + 1;
      while (pos < text.length && isAsciiWhitespaceCode(text.charCodeAt(pos))) {
        pos++;
      }
      if (pos < text.length && text.charCodeAt(pos) === 47 /* '/' */) {
        pos++;
        while (
          pos < text.length &&
          isAsciiWhitespaceCode(text.charCodeAt(pos))
        ) {
          pos++;
        }
        if (pos < text.length && isNcnameStartCode(text.charCodeAt(pos))) {
          const start = pos;
          pos++;
          while (
            pos < text.length &&
            isNcnameContinueCode(text.charCodeAt(pos))
          ) {
            pos++;
          }
          const name = text.slice(start, pos);
          while (
            pos < text.length &&
            isAsciiWhitespaceCode(text.charCodeAt(pos))
          ) {
            pos++;
          }
          if (pos < text.length && text.charCodeAt(pos) === 62 /* '>' */) {
            out.add(name.toLowerCase());
            i = pos + 1;
            continue;
          }
        }
      }
    }
    i++;
  }
}

function collectClosingXmlTags(nodes: readonly Node[], out: Set<string>): void {
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_TEXT:
        scanClosingTagsInText(node.text, out);
        break;
      case NODE_FOR:
        collectClosingXmlTags(node.body, out);
        if (node.elseBody) collectClosingXmlTags(node.elseBody, out);
        break;
      case NODE_IF:
        for (const branch of node.branches) {
          collectClosingXmlTags(branch.body, out);
        }
        if (node.elseBody) collectClosingXmlTags(node.elseBody, out);
        break;
      case NODE_MATCH:
        for (const arm of node.arms) {
          collectClosingXmlTags(arm.body, out);
        }
        if (node.elseArm) collectClosingXmlTags(node.elseArm, out);
        if (node.inlineGuard) {
          collectClosingXmlTags(node.inlineGuard.body, out);
        }
        break;
      case NODE_PANIC:
        collectClosingXmlTags(node.body, out);
        break;
      default:
        break;
    }
  }
}

function updateOpenXmlTagsFromText(
  text: string,
  closingTags: ReadonlySet<string>,
  openTags: string[],
  pendingOpenTag?: string,
): string | undefined {
  let i = 0;
  if (pendingOpenTag !== undefined) {
    let closeIdx = 0;
    while (
      closeIdx < text.length &&
      text.charCodeAt(closeIdx) !== 62 /* '>' */ &&
      text.charCodeAt(closeIdx) !== 60 /* '<' */
    ) {
      closeIdx++;
    }
    if (closeIdx < text.length && text.charCodeAt(closeIdx) === 62) {
      let beforeGt = closeIdx;
      while (
        beforeGt > 0 &&
        isAsciiWhitespaceCode(text.charCodeAt(beforeGt - 1))
      ) {
        beforeGt--;
      }
      const selfClosing =
        beforeGt > 0 && text.charCodeAt(beforeGt - 1) === 47; /* '/' */
      if (!selfClosing && closingTags.has(pendingOpenTag.toLowerCase())) {
        openTags.push(pendingOpenTag);
      }
      i = closeIdx + 1;
    } else if (closeIdx === text.length) {
      return pendingOpenTag;
    } else {
      i = closeIdx;
    }
  }

  while (i + 2 < text.length) {
    if (text.charCodeAt(i) !== 60 /* '<' */) {
      i++;
      continue;
    }
    let pos = i + 1;
    while (pos < text.length && isAsciiWhitespaceCode(text.charCodeAt(pos))) {
      pos++;
    }
    if (pos < text.length && text.charCodeAt(pos) === 47 /* '/' */) {
      pos++;
      while (pos < text.length && isAsciiWhitespaceCode(text.charCodeAt(pos))) {
        pos++;
      }
      if (pos < text.length && isNcnameStartCode(text.charCodeAt(pos))) {
        const start = pos;
        pos++;
        while (
          pos < text.length &&
          isNcnameContinueCode(text.charCodeAt(pos))
        ) {
          pos++;
        }
        const name = text.slice(start, pos);
        while (
          pos < text.length &&
          isAsciiWhitespaceCode(text.charCodeAt(pos))
        ) {
          pos++;
        }
        if (pos < text.length && text.charCodeAt(pos) === 62 /* '>' */) {
          const lowerName = name.toLowerCase();
          for (let k = openTags.length - 1; k >= 0; k--) {
            if (openTags[k]?.toLowerCase() === lowerName) {
              openTags.splice(k, 1);
              break;
            }
          }
          i = pos + 1;
          continue;
        }
      }
      i++;
      continue;
    }
    if (i + 1 < text.length && isNcnameStartCode(text.charCodeAt(i + 1))) {
      const start = i + 1;
      let end = start + 1;
      while (end < text.length && isNcnameContinueCode(text.charCodeAt(end))) {
        end++;
      }
      const name = text.slice(start, end);
      if (
        end < text.length &&
        (text.charCodeAt(end) === 62 /* '>' */ ||
          text.charCodeAt(end) === 47 /* '/' */ ||
          isAsciiWhitespaceCode(text.charCodeAt(end)))
      ) {
        let closeIdx = end;
        while (
          closeIdx < text.length &&
          text.charCodeAt(closeIdx) !== 62 /* '>' */ &&
          text.charCodeAt(closeIdx) !== 60 /* '<' */
        ) {
          closeIdx++;
        }
        if (closeIdx < text.length && text.charCodeAt(closeIdx) === 62) {
          let beforeGt = closeIdx;
          while (
            beforeGt > end &&
            isAsciiWhitespaceCode(text.charCodeAt(beforeGt - 1))
          ) {
            beforeGt--;
          }
          const selfClosing =
            beforeGt > end && text.charCodeAt(beforeGt - 1) === 47; /* '/' */
          if (!selfClosing && closingTags.has(name.toLowerCase())) {
            openTags.push(name);
          }
          i = closeIdx + 1;
          continue;
        } else if (
          closeIdx === text.length &&
          closingTags.has(name.toLowerCase())
        ) {
          return name;
        }
      }
    }
    i++;
  }
  return undefined;
}

function applySanitizeNodesInner(
  nodes: Node[],
  paramSanitize: ReadonlyMap<string, SanitizeSpec> | undefined,
  sanitizeNotice: string | undefined,
  closingTags: ReadonlySet<string>,
  loopAliases: Map<string, string>,
  openTags: string[],
  initialPending?: string,
): string | undefined {
  let pendingTag = initialPending;
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    if (node === undefined) continue;
    switch (node.kind) {
      case NODE_TEXT:
        if (closingTags.size > 0) {
          pendingTag = updateOpenXmlTagsFromText(
            node.text,
            closingTags,
            openTags,
            pendingTag,
          );
        }
        break;
      case NODE_EXPR: {
        const newExpr = rewriteExprForSanitize(
          node.expr,
          paramSanitize,
          sanitizeNotice,
          loopAliases,
          openTags,
        );
        if (newExpr !== node.expr) {
          nodes[i] = { ...node, expr: newExpr };
        }
        break;
      }
      case NODE_FOR: {
        const iterBase = (splitPipes(node.iterExpr)[0] ?? "").trim();
        let prevAlias: string | undefined;
        let hadAlias = false;
        if (/^[a-zA-Z_]\w*(?:\.[a-zA-Z_]\w*)*$/.test(iterBase)) {
          hadAlias = true;
          prevAlias = loopAliases.get(node.binding);
          loopAliases.set(
            node.binding,
            resolveCanonicalPath(iterBase, loopAliases),
          );
        }
        const bodyTags = [...openTags];
        applySanitizeNodesInner(
          node.body,
          paramSanitize,
          sanitizeNotice,
          closingTags,
          loopAliases,
          bodyTags,
          pendingTag,
        );
        if (hadAlias) {
          if (prevAlias !== undefined) {
            loopAliases.set(node.binding, prevAlias);
          } else {
            loopAliases.delete(node.binding);
          }
        }
        if (node.elseBody) {
          const elseTags = [...openTags];
          applySanitizeNodesInner(
            node.elseBody,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            elseTags,
            pendingTag,
          );
        }
        break;
      }
      case NODE_IF: {
        for (const branch of node.branches) {
          const branchTags = [...openTags];
          applySanitizeNodesInner(
            branch.body,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            branchTags,
            pendingTag,
          );
        }
        if (node.elseBody) {
          const elseTags = [...openTags];
          applySanitizeNodesInner(
            node.elseBody,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            elseTags,
            pendingTag,
          );
        }
        break;
      }
      case NODE_MATCH: {
        for (const arm of node.arms) {
          const armTags = [...openTags];
          applySanitizeNodesInner(
            arm.body,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            armTags,
            pendingTag,
          );
        }
        if (node.elseArm) {
          const elseTags = [...openTags];
          applySanitizeNodesInner(
            node.elseArm,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            elseTags,
            pendingTag,
          );
        }
        if (node.inlineGuard) {
          const guardTags = [...openTags];
          applySanitizeNodesInner(
            node.inlineGuard.body,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            guardTags,
            pendingTag,
          );
        }
        break;
      }
      case NODE_PANIC: {
        const panicTags = [...openTags];
        applySanitizeNodesInner(
          node.body,
          paramSanitize,
          sanitizeNotice,
          closingTags,
          loopAliases,
          panicTags,
          pendingTag,
        );
        break;
      }
      default:
        break;
    }
  }
  return pendingTag;
}

/**
 * Run the compile-time sanitization AST pass on `nodes`:
 * 1. Propagates declarative `params:` sanitization (`paramSanitize`) onto matching `{{ path }}` expressions.
 * 2. Propagates template-level `sanitize_notice:` (`sanitizeNotice`) onto 1-arg `| sanitize("tag")` filters.
 * 3. Detects enclosing XML tags across static template text and merges them into `| sanitize` filters.
 */
export function applySanitizeAstPass(
  nodes: Node[],
  paramSanitize?: ReadonlyMap<string, SanitizeSpec>,
  sanitizeNotice?: string,
): void {
  const closingTags = new Set<string>();
  collectClosingXmlTags(nodes, closingTags);
  const loopAliases = new Map<string, string>();
  const openTags: string[] = [];
  applySanitizeNodesInner(
    nodes,
    paramSanitize,
    sanitizeNotice,
    closingTags,
    loopAliases,
    openTags,
  );
}
