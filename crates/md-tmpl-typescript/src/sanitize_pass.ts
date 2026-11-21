/**
 * Compile-time AST pass for parameter sanitization propagation,
 * boundary notice resolution, and enclosing XML tag / ATX heading detection.
 *
 * @module
 */

import { type IncludeSanitizeContext, type Node, cloneNodes } from "./ast.js";
import {
  BACKSLASH,
  COMMA,
  DEFAULT_QUARANTINE_TAG,
  DOT,
  FILTER_FENCE,
  FILTER_JSON,
  FILTER_QUARANTINE,
  FILTER_SANITIZE,
  FILTER_SANITIZE_ENCLOSED,
  FILTER_SANITIZE_TOKENS,
  FILTER_TOJSON,
  FILTER_TO_JSON,
  FILTER_UNSAFE_TRUST,
  FN_UNSAFE_TRUST,
  NODE_EXPR,
  NODE_FOR,
  NODE_IF,
  NODE_INCLUDE,
  NODE_MATCH,
  NODE_PANIC,
  NODE_RAW,
  NODE_TEXT,
  PIPE_SPACED,
  QUOTE_DOUBLE,
  SANITIZE_KIND_INLINE,
} from "./consts.js";
import { TemplateSyntaxError } from "./errors.js";
import { splitPipes } from "./evaluator.js";
import {
  type DeclaredSanitize,
  PLAIN_PATH_RE,
  checkUntrustedIncludePaths,
  computeIncludeTaint,
  effectiveUntrustedMap,
  resolveCanonicalPath,
  updateIncludeContext,
} from "./include_sanitize.js";
import {
  INLINE_HEADING_SENTINEL,
  parseFilter,
  stripQuotes,
  validateTagSpecForFilter,
} from "./filters.js";
import {
  type SanitizeSpec,
  isAsciiWhitespaceCode,
  isNcnameContinueCode,
  isNcnameStartCode,
  parseSanitizeFilterArgs,
  skipTagWhitespace,
} from "./sanitize_runtime.js";

export * from "./sanitize_runtime.js";
export * from "./sanitize_decl.js";
export {
  checkUntrustedIncludePath,
  checkUntrustedIncludePaths,
} from "./include_sanitize.js";

/** Declared sanitization of an include target without a parameter policy. */
const EMPTY_DECLARED: DeclaredSanitize = new Map();

const CHAR_TAB = 9;
const CHAR_SPACE = 32;
const CHAR_DQUOTE = 34;
const CHAR_HASH = 35;
const CHAR_SQUOTE = 39;
const CHAR_SLASH = 47;
const CHAR_LT = 60;
const CHAR_GT = 62;
const MIN_HEADING_LEVEL = 1;
const MAX_HEADING_LEVEL = 6;
const NEWLINE_CHAR = "\n";
const CR_CHAR = "\r";

/**
 * Parse a `sanitize_notice:` frontmatter value, stripping optional surrounding quotes and
 * decoding the standard string-literal escapes (`\\`, `\"`, `\'`; any other `\X` is kept
 * verbatim), exactly like every other md-tmpl string literal.
 */
export function parseSanitizeNoticeValue(raw: string): string {
  return stripQuotes(raw.trim());
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
  if (spec.kind === SANITIZE_KIND_INLINE) {
    return FILTER_SANITIZE;
  }
  const effectiveNotice = spec.notice ?? fmNotice;
  if (effectiveNotice !== undefined) {
    return `${FILTER_SANITIZE}(${quoteFilterArg(spec.tag)}, ${quoteFilterArg(effectiveNotice)})`;
  }
  return `${FILTER_SANITIZE}(${quoteFilterArg(spec.tag)})`;
}

function mergeEnclosingTagsIntoSpec(
  existing: string | undefined,
  openTags: readonly string[],
  inAtxHeading = false,
): string | undefined {
  if (openTags.length === 0 && !inAtxHeading) {
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
  if (inAtxHeading && !combined.includes(INLINE_HEADING_SENTINEL)) {
    combined.push(INLINE_HEADING_SENTINEL);
  }
  return combined.length === 0 ? undefined : combined.join(COMMA);
}

function lookupDirectPathSpec(
  canonical: string,
  paramSanitize: ReadonlyMap<string, SanitizeSpec>,
): SanitizeSpec | undefined {
  const exact = paramSanitize.get(canonical);
  if (exact !== undefined) {
    return exact;
  }
  for (const [key, spec] of paramSanitize) {
    if (canonical.startsWith(`${key}${DOT}`)) {
      return spec;
    }
  }
  return undefined;
}

function pathHasAnyUntrusted(
  canonical: string,
  paramSanitize: ReadonlyMap<string, SanitizeSpec>,
): boolean {
  if (paramSanitize.has(canonical)) {
    return true;
  }
  for (const key of paramSanitize.keys()) {
    if (
      key.startsWith(`${canonical}${DOT}`) ||
      canonical.startsWith(`${key}${DOT}`)
    ) {
      return true;
    }
  }
  return false;
}

function rewriteExprForSanitize(
  expr: string,
  paramSanitize: ReadonlyMap<string, SanitizeSpec> | undefined,
  sanitizeNotice: string | undefined,
  loopAliases: ReadonlyMap<string, string>,
  openTags: readonly string[],
  inAtxHeading = false,
): string {
  const parts = splitPipes(expr);
  const baseExpr = (parts[0] ?? "").trim();
  const filterStrings = parts.slice(1).map((p) => p.trim());

  // 1. Check if baseExpr is a plain variable/field path that matches paramSanitize
  const isUnsafeTrustFn =
    baseExpr.startsWith(`${FN_UNSAFE_TRUST}(`) && baseExpr.endsWith(")");
  if (
    !isUnsafeTrustFn &&
    paramSanitize &&
    paramSanitize.size > 0 &&
    PLAIN_PATH_RE.test(baseExpr)
  ) {
    const canonical = resolveCanonicalPath(baseExpr, loopAliases);
    const hasToJson = filterStrings.some((f) => {
      const [fName] = parseFilter(f);
      return (
        fName === FILTER_TOJSON ||
        fName === FILTER_TO_JSON ||
        fName === FILTER_JSON
      );
    });
    const spec: SanitizeSpec | undefined = hasToJson
      ? pathHasAnyUntrusted(canonical, paramSanitize)
        ? { kind: SANITIZE_KIND_INLINE }
        : undefined
      : lookupDirectPathSpec(canonical, paramSanitize);
    if (spec !== undefined) {
      const hasTokenSanitize = filterStrings.some((f) => {
        const [fName] = parseFilter(f);
        return (
          fName === FILTER_SANITIZE ||
          fName === FILTER_SANITIZE_ENCLOSED ||
          fName === FILTER_SANITIZE_TOKENS ||
          fName === FILTER_UNSAFE_TRUST
        );
      });
      const qIdx = filterStrings.findIndex(
        (f) => parseFilter(f)[0] === FILTER_QUARANTINE,
      );
      if (qIdx !== -1) {
        if (!hasTokenSanitize) {
          filterStrings.splice(qIdx, 0, FILTER_SANITIZE_TOKENS);
        }
      } else if (!hasTokenSanitize) {
        filterStrings.push(formatSanitizeSpecFilter(spec, sanitizeNotice));
      }
    }
  }

  if (filterStrings.length === 0) {
    return expr;
  }

  // 2. Rewrite any `sanitize`, `sanitize_tokens`, or `quarantine` filter for `sanitizeNotice` and `openTags`
  let modified = filterStrings.length !== parts.length - 1;
  const updatedFilters = filterStrings.map((fStr, idx) => {
    const [fName, fArgs] = parseFilter(fStr);
    if (inAtxHeading) {
      if (fName === FILTER_FENCE) {
        throw new TemplateSyntaxError(
          "multi-line filter 'fence' cannot be used inside a Markdown ATX heading line",
        );
      }
      if (fName === FILTER_QUARANTINE) {
        throw new TemplateSyntaxError(
          "multi-line filter 'quarantine' cannot be used inside a Markdown ATX heading line",
        );
      }
      if (fName === FILTER_SANITIZE && fArgs !== undefined) {
        throw new TemplateSyntaxError(
          "multi-line block 'sanitize' cannot be used inside a Markdown ATX heading line",
        );
      }
    }
    if (fName === FILTER_QUARANTINE) {
      if (openTags.length > 0) {
        const raw =
          fArgs === undefined ? DEFAULT_QUARANTINE_TAG : stripQuotes(fArgs);
        const merged = mergeEnclosingTagsIntoSpec(raw, openTags, false);
        if (merged !== undefined && (fArgs === undefined || merged !== raw)) {
          modified = true;
          return `${FILTER_QUARANTINE}(${quoteFilterArg(merged)})`;
        }
      }
      return fStr;
    }
    if (fName === FILTER_SANITIZE_TOKENS) {
      // Keep compiler-inserted `sanitize_tokens | quarantine` intact so they fuse into 1 pass.
      const nextFilter = filterStrings[idx + 1];
      if (
        fArgs === undefined &&
        nextFilter !== undefined &&
        parseFilter(nextFilter)[0] === FILTER_QUARANTINE
      ) {
        return fStr;
      }
      if (openTags.length === 0 && !inAtxHeading) {
        return fStr;
      }
      let existingSpec: string | undefined;
      if (fArgs !== undefined) {
        try {
          [, existingSpec] = validateTagSpecForFilter(
            stripQuotes(fArgs),
            FILTER_SANITIZE_TOKENS,
          );
        } catch {
          return fStr;
        }
      }
      const mergedTags = mergeEnclosingTagsIntoSpec(
        existingSpec,
        openTags,
        inAtxHeading,
      );
      if (mergedTags !== undefined && mergedTags !== existingSpec) {
        modified = true;
        return `${FILTER_SANITIZE_ENCLOSED}(${quoteFilterArg(mergedTags)})`;
      }
      return fStr;
    }
    if (fName === FILTER_SANITIZE_ENCLOSED) {
      const existingTags = fArgs === undefined ? undefined : stripQuotes(fArgs);
      const mergedTags = mergeEnclosingTagsIntoSpec(
        existingTags,
        openTags,
        inAtxHeading,
      );
      if (mergedTags !== undefined && mergedTags !== existingTags) {
        modified = true;
        return `${FILTER_SANITIZE_ENCLOSED}(${quoteFilterArg(mergedTags)})`;
      }
      return fStr;
    }
    if (fName !== FILTER_SANITIZE) {
      return fStr;
    }
    const parsed = parseSanitizeFilterArgs(fArgs);
    if (parsed === undefined) {
      const mergedTags = mergeEnclosingTagsIntoSpec(
        undefined,
        openTags,
        inAtxHeading,
      );
      if (mergedTags !== undefined) {
        modified = true;
        return `${FILTER_SANITIZE_ENCLOSED}(${quoteFilterArg(mergedTags)})`;
      }
      return fStr;
    }
    const mergedTagSpec =
      mergeEnclosingTagsIntoSpec(parsed.tagSpec, openTags, false) ??
      parsed.tagSpec;
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
  return `${baseExpr}${PIPE_SPACED}${updatedFilters.join(PIPE_SPACED)}`;
}

function scanClosingTagsInText(text: string, out: Set<string>): void {
  let i = 0;
  while (i + 3 < text.length) {
    if (text.charCodeAt(i) === CHAR_LT) {
      let pos = skipTagWhitespace(text, i + 1);
      if (pos < text.length && text.charCodeAt(pos) === CHAR_SLASH) {
        pos = skipTagWhitespace(text, pos + 1);
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
          pos = skipTagWhitespace(text, pos);
          if (pos < text.length && text.charCodeAt(pos) === CHAR_GT) {
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
      case NODE_INCLUDE:
        if (node.inlineNodes) {
          collectClosingXmlTags(node.inlineNodes, out);
        }
        break;
      default:
        break;
    }
  }
}

interface EnclosingScanState {
  openTags: string[];
  pendingOpenTag: string | undefined;
  pendingQuote: number | undefined;
  inAtxHeading: boolean;
  atLineStart: boolean;
}

function cloneScanState(state: EnclosingScanState): EnclosingScanState {
  return {
    openTags: [...state.openTags],
    pendingOpenTag: state.pendingOpenTag,
    pendingQuote: state.pendingQuote,
    inAtxHeading: state.inAtxHeading,
    atLineStart: state.atLineStart,
  };
}

function mergeBranchStates(
  target: EnclosingScanState,
  states: readonly EnclosingScanState[],
): void {
  if (states.length === 0) {
    return;
  }
  const unionTags: string[] = [];
  let pendingTag: string | undefined;
  let pendingQuote: number | undefined;
  let anyHeading = false;
  let anyLineStart = false;
  for (const st of states) {
    for (const tag of st.openTags) {
      if (!unionTags.some((t) => t.toLowerCase() === tag.toLowerCase())) {
        unionTags.push(tag);
      }
    }
    if (pendingTag === undefined && st.pendingOpenTag !== undefined) {
      pendingTag = st.pendingOpenTag;
      pendingQuote = st.pendingQuote;
    }
    anyHeading ||= st.inAtxHeading;
    anyLineStart ||= st.atLineStart;
  }
  target.openTags = unionTags;
  target.pendingOpenTag = pendingTag;
  target.pendingQuote = pendingQuote;
  target.inAtxHeading = anyHeading;
  target.atLineStart = anyLineStart;
}

type OpenTagTailOutcome =
  | {
      readonly kind: "closed";
      readonly closeIdx: number;
      readonly selfClosing: boolean;
    }
  | { readonly kind: "pending"; readonly quote: number | undefined }
  | { readonly kind: "aborted"; readonly ltIdx: number };

function scanOpenTagTail(
  text: string,
  startIdx: number,
  initialQuote: number | undefined,
): OpenTagTailOutcome {
  let quote = initialQuote;
  let lastUnquotedNonWs: number | undefined;
  let idx = startIdx;
  while (idx < text.length) {
    const code = text.charCodeAt(idx);
    if (quote !== undefined) {
      if (code === quote) {
        quote = undefined;
        lastUnquotedNonWs = code;
      }
      idx++;
      continue;
    }
    if (code === CHAR_DQUOTE || code === CHAR_SQUOTE) {
      quote = code;
    } else if (code === CHAR_GT) {
      return {
        kind: "closed",
        closeIdx: idx,
        selfClosing: lastUnquotedNonWs === CHAR_SLASH,
      };
    } else if (code === CHAR_LT) {
      return { kind: "aborted", ltIdx: idx };
    } else if (!isAsciiWhitespaceCode(code)) {
      lastUnquotedNonWs = code;
    }
    idx++;
  }
  return { kind: "pending", quote };
}

function updateOpenXmlTagsFromText(
  text: string,
  closingTags: ReadonlySet<string>,
  state: EnclosingScanState,
): void {
  let i = 0;
  if (state.pendingOpenTag !== undefined) {
    const pendingName = state.pendingOpenTag;
    const prevQuote = state.pendingQuote;
    state.pendingOpenTag = undefined;
    state.pendingQuote = undefined;
    const tail = scanOpenTagTail(text, 0, prevQuote);
    if (tail.kind === "closed") {
      if (!tail.selfClosing && closingTags.has(pendingName.toLowerCase())) {
        state.openTags.push(pendingName);
      }
      i = tail.closeIdx + 1;
    } else if (tail.kind === "pending") {
      state.pendingOpenTag = pendingName;
      state.pendingQuote = tail.quote;
      return;
    } else {
      i = tail.ltIdx;
    }
  }

  while (i + 1 < text.length) {
    if (text.charCodeAt(i) !== CHAR_LT) {
      i++;
      continue;
    }
    let pos = skipTagWhitespace(text, i + 1);
    if (pos < text.length && text.charCodeAt(pos) === CHAR_SLASH) {
      pos = skipTagWhitespace(text, pos + 1);
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
        pos = skipTagWhitespace(text, pos);
        if (pos < text.length && text.charCodeAt(pos) === CHAR_GT) {
          const lowerName = name.toLowerCase();
          for (let k = state.openTags.length - 1; k >= 0; k--) {
            if (state.openTags[k]?.toLowerCase() === lowerName) {
              state.openTags.splice(k, 1);
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
      if (end === text.length) {
        if (closingTags.has(name.toLowerCase())) {
          state.pendingOpenTag = name;
          state.pendingQuote = undefined;
        }
        return;
      }
      if (
        text.charCodeAt(end) === CHAR_GT ||
        text.charCodeAt(end) === CHAR_SLASH ||
        skipTagWhitespace(text, end) > end
      ) {
        const tail = scanOpenTagTail(text, end, undefined);
        if (tail.kind === "closed") {
          if (!tail.selfClosing && closingTags.has(name.toLowerCase())) {
            state.openTags.push(name);
          }
          i = tail.closeIdx + 1;
          continue;
        }
        if (tail.kind === "pending") {
          if (closingTags.has(name.toLowerCase())) {
            state.pendingOpenTag = name;
            state.pendingQuote = tail.quote;
          }
          return;
        }
        i = tail.ltIdx;
        continue;
      }
    }
    i++;
  }
}

function stripMarkdownContainerPrefixes(rawLine: string): string {
  let pos = 0;
  const len = rawLine.length;
  for (;;) {
    while (pos < len) {
      const ws = rawLine.charCodeAt(pos);
      if (ws !== 32 /* ' ' */ && ws !== 9 /* '\t' */) break;
      pos++;
    }
    if (pos >= len) {
      return "";
    }
    const ch = rawLine.charCodeAt(pos);
    if (ch === 62 /* '>' */) {
      pos++;
      continue;
    }
    if (
      (ch === 45 /* '-' */ || ch === 42 /* '*' */ || ch === 43) /* '+' */ &&
      pos + 1 < len &&
      (rawLine.charCodeAt(pos + 1) === 32 || rawLine.charCodeAt(pos + 1) === 9)
    ) {
      pos++;
      continue;
    }
    let digits = 0;
    while (
      pos + digits < len &&
      rawLine.charCodeAt(pos + digits) >= 48 &&
      rawLine.charCodeAt(pos + digits) <= 57
    ) {
      digits++;
    }
    if (
      digits >= 1 &&
      digits <= 9 &&
      pos + digits + 1 < len &&
      (rawLine.charCodeAt(pos + digits) === 46 /* '.' */ ||
        rawLine.charCodeAt(pos + digits) === 41) /* ')' */ &&
      (rawLine.charCodeAt(pos + digits + 1) === 32 ||
        rawLine.charCodeAt(pos + digits + 1) === 9)
    ) {
      pos += digits + 1;
      continue;
    }
    return pos === 0 ? rawLine : rawLine.slice(pos);
  }
}

function isAtxHeadingPrefix(line: string): boolean {
  const trimmed = stripMarkdownContainerPrefixes(line);
  let hashes = 0;
  while (hashes < trimmed.length && trimmed.charCodeAt(hashes) === CHAR_HASH) {
    hashes++;
  }
  if (
    hashes < MIN_HEADING_LEVEL ||
    hashes > MAX_HEADING_LEVEL ||
    hashes >= trimmed.length
  ) {
    return false;
  }
  const next = trimmed.charCodeAt(hashes);
  return next === CHAR_SPACE || next === CHAR_TAB;
}

function updateHeadingLineState(text: string, state: EnclosingScanState): void {
  if (text.length === 0) {
    return;
  }
  const lastNl = Math.max(
    text.lastIndexOf(NEWLINE_CHAR),
    text.lastIndexOf(CR_CHAR),
  );
  if (lastNl !== -1) {
    const tail = text.slice(lastNl + 1);
    state.inAtxHeading = isAtxHeadingPrefix(tail);
    state.atLineStart = stripMarkdownContainerPrefixes(tail).length === 0;
  } else if (state.atLineStart) {
    if (isAtxHeadingPrefix(text)) {
      state.inAtxHeading = true;
      state.atLineStart = false;
    } else if (stripMarkdownContainerPrefixes(text).length > 0) {
      state.atLineStart = false;
    }
  }
}

function applySanitizeNodesInner(
  nodes: Node[],
  paramSanitize: ReadonlyMap<string, SanitizeSpec> | undefined,
  sanitizeNotice: string | undefined,
  closingTags: ReadonlySet<string>,
  loopAliases: Map<string, string>,
  state: EnclosingScanState,
): void {
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    if (node === undefined) continue;
    switch (node.kind) {
      case NODE_TEXT:
        if (closingTags.size > 0) {
          updateOpenXmlTagsFromText(node.text, closingTags, state);
        }
        updateHeadingLineState(node.text, state);
        break;
      case NODE_RAW:
        updateHeadingLineState(node.text, state);
        break;
      case NODE_EXPR: {
        const newExpr = rewriteExprForSanitize(
          node.expr,
          paramSanitize,
          sanitizeNotice,
          loopAliases,
          state.openTags,
          state.inAtxHeading,
        );
        if (newExpr !== node.expr) {
          nodes[i] = { ...node, expr: newExpr, _compiled: undefined };
        }
        state.atLineStart = false;
        break;
      }
      case NODE_FOR: {
        const iterBase = (splitPipes(node.iterExpr)[0] ?? "").trim();
        let prevAlias: string | undefined;
        let hadAlias = false;
        if (PLAIN_PATH_RE.test(iterBase)) {
          hadAlias = true;
          prevAlias = loopAliases.get(node.binding);
          loopAliases.set(
            node.binding,
            resolveCanonicalPath(iterBase, loopAliases),
          );
        }
        applySanitizeNodesInner(
          node.body,
          paramSanitize,
          sanitizeNotice,
          closingTags,
          loopAliases,
          cloneScanState(state),
        );
        if (hadAlias) {
          if (prevAlias !== undefined) {
            loopAliases.set(node.binding, prevAlias);
          } else {
            loopAliases.delete(node.binding);
          }
        }
        if (node.elseBody) {
          applySanitizeNodesInner(
            node.elseBody,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            cloneScanState(state),
          );
        }
        break;
      }
      case NODE_IF: {
        const postStates: EnclosingScanState[] = [];
        for (const branch of node.branches) {
          const branchState = cloneScanState(state);
          applySanitizeNodesInner(
            branch.body,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            branchState,
          );
          postStates.push(branchState);
        }
        if (node.elseBody && node.elseBody.length > 0) {
          const elseState = cloneScanState(state);
          applySanitizeNodesInner(
            node.elseBody,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            elseState,
          );
          postStates.push(elseState);
        } else {
          postStates.push(cloneScanState(state));
        }
        mergeBranchStates(state, postStates);
        break;
      }
      case NODE_MATCH: {
        const postStates: EnclosingScanState[] = [];
        for (const arm of node.arms) {
          const armState = cloneScanState(state);
          applySanitizeNodesInner(
            arm.body,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            armState,
          );
          postStates.push(armState);
        }
        if (node.elseArm) {
          const elseState = cloneScanState(state);
          applySanitizeNodesInner(
            node.elseArm,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            elseState,
          );
          postStates.push(elseState);
        }
        if (node.inlineGuard) {
          const guardState = cloneScanState(state);
          applySanitizeNodesInner(
            node.inlineGuard.body,
            paramSanitize,
            sanitizeNotice,
            closingTags,
            loopAliases,
            guardState,
          );
          postStates.push(guardState);
          postStates.push(cloneScanState(state));
        }
        mergeBranchStates(state, postStates);
        break;
      }
      case NODE_PANIC: {
        applySanitizeNodesInner(
          node.body,
          paramSanitize,
          sanitizeNotice,
          closingTags,
          loopAliases,
          cloneScanState(state),
        );
        break;
      }
      case NODE_INCLUDE: {
        const taint =
          paramSanitize !== undefined && paramSanitize.size > 0
            ? computeIncludeTaint(
                node,
                paramSanitize,
                sanitizeNotice,
                loopAliases,
              )
            : undefined;
        if (node.inlineNodes) {
          applySanitizeNodesInner(
            node.inlineNodes,
            taint !== undefined && taint.entries.length > 0
              ? effectiveUntrustedMap(
                  taint.entries,
                  node.inlineDeclared ?? EMPTY_DECLARED,
                )
              : undefined,
            undefined,
            closingTags,
            new Map(),
            cloneScanState(state),
          );
        } else {
          const ctx = updateIncludeContext(
            node.sanitizeCtx,
            taint,
            state.openTags,
            {
              inAtxHeading: state.inAtxHeading,
              atLineStart: state.atLineStart,
            },
            state.pendingOpenTag,
          );
          if (ctx !== node.sanitizeCtx) {
            nodes[i] = { ...node, sanitizeCtx: ctx };
          }
        }
        state.atLineStart = false;
        break;
      }
      default:
        break;
    }
  }
}

/**
 * Returns `true` if `nodes` contain a `sanitize` / `sanitize_tokens` / `quarantine`
 * filter or an include resolved at render time (whose call-site enclosing state must be recorded).
 */
function nodesNeedEnclosingPass(nodes: readonly Node[]): boolean {
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_EXPR:
        if (
          node.expr.includes(FILTER_SANITIZE) ||
          node.expr.includes(FILTER_QUARANTINE) ||
          node.expr.includes(FILTER_FENCE)
        )
          return true;
        break;
      case NODE_FOR:
        if (
          nodesNeedEnclosingPass(node.body) ||
          (node.elseBody !== undefined && nodesNeedEnclosingPass(node.elseBody))
        ) {
          return true;
        }
        break;
      case NODE_IF:
        for (const branch of node.branches) {
          if (nodesNeedEnclosingPass(branch.body)) return true;
        }
        if (
          node.elseBody !== undefined &&
          nodesNeedEnclosingPass(node.elseBody)
        ) {
          return true;
        }
        break;
      case NODE_MATCH:
        for (const arm of node.arms) {
          if (nodesNeedEnclosingPass(arm.body)) return true;
        }
        if (
          node.elseArm !== undefined &&
          nodesNeedEnclosingPass(node.elseArm)
        ) {
          return true;
        }
        if (
          node.inlineGuard !== undefined &&
          nodesNeedEnclosingPass(node.inlineGuard.body)
        ) {
          return true;
        }
        break;
      case NODE_PANIC:
        if (nodesNeedEnclosingPass(node.body)) return true;
        break;
      case NODE_INCLUDE:
        if (
          node.inlineNodes === undefined ||
          nodesNeedEnclosingPass(node.inlineNodes)
        ) {
          return true;
        }
        break;
      default:
        break;
    }
  }
  return false;
}

/**
 * Run the compile-time sanitization AST pass on `nodes`:
 * 1. Propagates declarative `params:` sanitization (`paramSanitize`) onto matching `{{ path }}` expressions.
 * 2. Propagates template-level `sanitize_notice:` (`sanitizeNotice`) onto 1-arg `| sanitize("tag")` filters.
 * 3. Detects enclosing XML tags and ATX headings across static template text and merges them into sanitizing filters.
 */
export function applySanitizeAstPass(
  nodes: Node[],
  paramSanitize?: ReadonlyMap<string, SanitizeSpec>,
  sanitizeNotice?: string,
): void {
  if (
    (paramSanitize === undefined || paramSanitize.size === 0) &&
    sanitizeNotice === undefined &&
    !nodesNeedEnclosingPass(nodes)
  ) {
    return;
  }
  const closingTags = new Set<string>();
  collectClosingXmlTags(nodes, closingTags);
  const loopAliases = new Map<string, string>();
  applySanitizeNodesInner(
    nodes,
    paramSanitize,
    sanitizeNotice,
    closingTags,
    loopAliases,
    {
      openTags: [],
      pendingOpenTag: undefined,
      pendingQuote: undefined,
      inAtxHeading: false,
      atLineStart: true,
    },
  );
}

/**
 * Apply a call-site sanitization context to freshly parsed child `nodes` in place.
 *
 * `nodes` must be parsed without the child's own parameter policy; `declared` is the
 * child's declared sanitization, used to choose the style for untrusted caller values.
 */
export function applyIncludeContext(
  nodes: Node[],
  ctx: IncludeSanitizeContext,
  declared: DeclaredSanitize,
): void {
  const untrusted =
    ctx.untrusted.length > 0
      ? effectiveUntrustedMap(ctx.untrusted, declared)
      : undefined;
  if (
    untrusted === undefined &&
    ctx.enclosingTags.length === 0 &&
    !ctx.inAtxHeading &&
    ctx.pendingOpenTag === undefined &&
    !nodesNeedEnclosingPass(nodes)
  ) {
    return;
  }
  const closingTags = new Set<string>();
  collectClosingXmlTags(nodes, closingTags);
  if (ctx.pendingOpenTag !== undefined) {
    closingTags.add(ctx.pendingOpenTag.toLowerCase());
  }
  applySanitizeNodesInner(nodes, untrusted, undefined, closingTags, new Map(), {
    openTags: [...ctx.enclosingTags],
    pendingOpenTag: ctx.pendingOpenTag,
    pendingQuote: undefined,
    inAtxHeading: ctx.inAtxHeading,
    atLineStart: ctx.atLineStart,
  });
}

/** Per-context memo: call-site context → (child nodes → contextualized clone). */
const contextualizedMemo = new WeakMap<
  IncludeSanitizeContext,
  WeakMap<readonly Node[], readonly Node[]>
>();

/**
 * Return the child nodes to render for an include call site carrying `ctx`.
 */
export function contextualizeIncludeNodes(
  nodes: readonly Node[],
  ctx: IncludeSanitizeContext | undefined,
  declared: DeclaredSanitize,
): readonly Node[] {
  if (ctx === undefined) {
    return nodes;
  }
  let perCtx = contextualizedMemo.get(ctx);
  if (perCtx === undefined) {
    perCtx = new WeakMap();
    contextualizedMemo.set(ctx, perCtx);
  }
  const hit = perCtx.get(nodes);
  if (hit !== undefined) {
    return hit;
  }
  let derived: readonly Node[];
  if (
    ctx.untrusted.length === 0 &&
    ctx.enclosingTags.length === 0 &&
    !ctx.inAtxHeading &&
    ctx.pendingOpenTag === undefined &&
    !nodesNeedEnclosingPass(nodes)
  ) {
    derived = nodes;
  } else {
    const cloned = cloneNodes(nodes);
    applyIncludeContext(cloned, ctx, declared);
    checkUntrustedIncludePaths(cloned);
    derived = cloned;
  }
  perCtx.set(nodes, derived);
  return derived;
}
