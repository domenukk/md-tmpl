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
  parseSanitizeFilterArgs,
} from "./sanitize_runtime.js";
import {
  type EnclosingScanState,
  cloneScanState,
  collectClosingXmlTags,
  joinPendingOpenTags,
  mergeBranchStates,
  parsePendingOpenTagsSpec,
  updateHeadingLineState,
  updateOpenXmlTagsFromText,
} from "./sanitize_enclosing.js";

export * from "./sanitize_runtime.js";
export * from "./sanitize_decl.js";
export {
  checkUntrustedIncludePath,
  checkUntrustedIncludePaths,
} from "./include_sanitize.js";

/** Declared sanitization of an include target without a parameter policy. */
const EMPTY_DECLARED: DeclaredSanitize = new Map();

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
        const postStates = collectRepeatedBodyStates(
          node.body,
          paramSanitize,
          sanitizeNotice,
          closingTags,
          loopAliases,
          state,
        );
        if (hadAlias) {
          if (prevAlias !== undefined) {
            loopAliases.set(node.binding, prevAlias);
          } else {
            loopAliases.delete(node.binding);
          }
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
          const childUntrusted =
            taint !== undefined && taint.entries.length > 0
              ? effectiveUntrustedMap(
                  taint.entries,
                  node.inlineDeclared ?? EMPTY_DECLARED,
                )
              : undefined;
          if (node.forExpr !== undefined) {
            const postStates = collectRepeatedBodyStates(
              node.inlineNodes,
              childUntrusted,
              undefined,
              closingTags,
              new Map(),
              state,
            );
            postStates.push(cloneScanState(state));
            mergeBranchStates(state, postStates);
          } else {
            applySanitizeNodesInner(
              node.inlineNodes,
              childUntrusted,
              undefined,
              closingTags,
              new Map(),
              state,
            );
          }
        } else {
          const ctx = updateIncludeContext(
            node.sanitizeCtx,
            taint,
            state.openTags,
            {
              inAtxHeading: state.inAtxHeading,
              atLineStart: state.atLineStart,
            },
            joinPendingOpenTags(state.pendingOpenTags),
          );
          if (ctx !== node.sanitizeCtx) {
            nodes[i] = { ...node, sanitizeCtx: ctx };
          }
          state.atLineStart = false;
        }
        break;
      }
      default:
        break;
    }
  }
}

function collectRepeatedBodyStates(
  body: Node[],
  paramSanitize: ReadonlyMap<string, SanitizeSpec> | undefined,
  sanitizeNotice: string | undefined,
  closingTags: ReadonlySet<string>,
  loopAliases: Map<string, string>,
  state: EnclosingScanState,
): EnclosingScanState[] {
  const postStates: EnclosingScanState[] = [];
  const iterState = cloneScanState(state);
  for (;;) {
    const bodyState = cloneScanState(iterState);
    applySanitizeNodesInner(
      body,
      paramSanitize,
      sanitizeNotice,
      closingTags,
      loopAliases,
      bodyState,
    );
    let grew =
      (bodyState.inAtxHeading && !iterState.inAtxHeading) ||
      (bodyState.atLineStart && !iterState.atLineStart);
    iterState.inAtxHeading ||= bodyState.inAtxHeading;
    iterState.atLineStart ||= bodyState.atLineStart;
    for (const tag of bodyState.openTags) {
      if (
        !iterState.openTags.some((t) => t.toLowerCase() === tag.toLowerCase())
      ) {
        iterState.openTags.push(tag);
        grew = true;
      }
    }
    for (const entry of bodyState.pendingOpenTags) {
      if (
        !iterState.pendingOpenTags.some(
          (e) =>
            e.name.toLowerCase() === entry.name.toLowerCase() &&
            e.quote === entry.quote,
        )
      ) {
        iterState.pendingOpenTags.push(entry);
        grew = true;
      }
    }
    postStates.push(bodyState);
    if (!grew) break;
  }
  return postStates;
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
      pendingOpenTags: [],
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
  const pendingOpenTags = parsePendingOpenTagsSpec(ctx.pendingOpenTag);
  for (const entry of pendingOpenTags) {
    closingTags.add(entry.name.toLowerCase());
  }
  applySanitizeNodesInner(nodes, untrusted, undefined, closingTags, new Map(), {
    openTags: [...ctx.enclosingTags],
    pendingOpenTags,
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
