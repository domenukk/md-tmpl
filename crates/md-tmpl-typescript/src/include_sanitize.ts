/**
 * Include call-site sanitization context: untrusted taint through `with` / `for`,
 * enclosing-tag / heading state at the call site, and untrusted include paths.
 * Mirrors the Rust `compiled/include_sanitize.rs`.
 *
 * @module
 */

import type { IncludeSanitizeContext, Node } from "./ast.js";
import {
  DOT,
  NODE_FOR,
  NODE_IF,
  NODE_INCLUDE,
  NODE_MATCH,
  NODE_PANIC,
  PREFIX_PARAMS_DOT,
  SANITIZE_KIND_BLOCK,
  SANITIZE_KIND_INLINE,
} from "./consts.js";
import { TemplateSyntaxError } from "./errors.js";
import { splitPipes } from "./evaluator.js";
import type { SanitizeSpec } from "./sanitize_runtime.js";

/** Canonical (loop-alias resolved, `params.`-stripped) dot path for `rawPath`. */
export function resolveCanonicalPath(
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

const PLAIN_PATH_RE = /^[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*$/;

export interface IncludeTaint {
  /** Child parameter paths receiving untrusted values, sorted and deduplicated. */
  readonly entries: readonly (readonly [string, SanitizeSpec])[];
  /** Untrusted expression interpolated into the include path, if any. */
  readonly pathRef?: string;
}

/** Return the inner text of a `"..."` / `'...'` string literal, or `undefined`. */
function stripStringLiteral(s: string): string | undefined {
  if (s.length >= 2) {
    const q = s[0];
    if ((q === '"' || q === "'") && s.endsWith(q)) {
      return s.slice(1, -1);
    }
  }
  return undefined;
}

/**
 * Resolve a declarative spec for transfer to a child template: a block spec without an
 * explicit notice inherits the host template's `sanitize_notice`.
 */
function transferSpec(
  spec: SanitizeSpec,
  hostNotice: string | undefined,
): SanitizeSpec {
  if (
    spec.kind === SANITIZE_KIND_BLOCK &&
    spec.notice === undefined &&
    hostNotice !== undefined
  ) {
    return { kind: SANITIZE_KIND_BLOCK, tag: spec.tag, notice: hostNotice };
  }
  return spec;
}

/**
 * Map an untrusted host path expression onto child path `target`:
 * exact match → `target`; nested field `K == C.rest` → `target.rest`;
 * sub-path of an untrusted value `C == K.rest` → `target`.
 */
function mapUntrustedPath(
  target: string,
  pathExpr: string,
  paramSanitize: ReadonlyMap<string, SanitizeSpec>,
  hostNotice: string | undefined,
  loopAliases: ReadonlyMap<string, string>,
  out: [string, SanitizeSpec][],
): void {
  const canonical = resolveCanonicalPath(pathExpr, loopAliases);
  const keys = [...paramSanitize.keys()].sort();
  for (const key of keys) {
    const spec = paramSanitize.get(key);
    if (spec === undefined) continue;
    if (key === canonical) {
      out.push([target, transferSpec(spec, hostNotice)]);
    } else if (key.startsWith(canonical + DOT)) {
      out.push([
        target + key.slice(canonical.length),
        transferSpec(spec, hostNotice),
      ]);
    } else if (canonical.startsWith(key + DOT)) {
      out.push([target, transferSpec(spec, hostNotice)]);
    }
  }
}

/** Return the first `{{ expr }}` interpolation in `text` whose base path is untrusted. */
function firstUntrustedInterpolation(
  text: string,
  paramSanitize: ReadonlyMap<string, SanitizeSpec>,
  loopAliases: ReadonlyMap<string, string>,
): string | undefined {
  let search = text;
  for (;;) {
    const start = search.indexOf("{{");
    if (start === -1) return undefined;
    const rest = search.slice(start + 2);
    const end = rest.indexOf("}}");
    if (end === -1) return undefined;
    const inner = rest.slice(0, end).trim();
    const base = (splitPipes(inner)[0] ?? "").trim();
    if (PLAIN_PATH_RE.test(base)) {
      const hits: [string, SanitizeSpec][] = [];
      mapUntrustedPath("", base, paramSanitize, undefined, loopAliases, hits);
      if (hits.length > 0) return base;
    }
    search = rest.slice(end + 2);
  }
}

/**
 * Compute the child untrusted parameter map for an include call site from the host
 * template's declarative `params:` policy, and detect untrusted include paths.
 */
export function computeIncludeTaint(
  node: Extract<Node, { kind: typeof NODE_INCLUDE }>,
  paramSanitize: ReadonlyMap<string, SanitizeSpec>,
  hostNotice: string | undefined,
  loopAliases: ReadonlyMap<string, string>,
): IncludeTaint {
  const pathRef =
    node.path !== undefined
      ? firstUntrustedInterpolation(node.path, paramSanitize, loopAliases)
      : undefined;
  const entries: [string, SanitizeSpec][] = [];
  for (const [key, rawExpr] of node.withMappings) {
    const expr = rawExpr.trim();
    const lit = stripStringLiteral(expr);
    if (lit !== undefined) {
      if (
        firstUntrustedInterpolation(lit, paramSanitize, loopAliases) !==
        undefined
      ) {
        entries.push([key, { kind: SANITIZE_KIND_INLINE }]);
      }
      continue;
    }
    const base = (splitPipes(expr)[0] ?? "").trim();
    if (PLAIN_PATH_RE.test(base)) {
      mapUntrustedPath(
        key,
        base,
        paramSanitize,
        hostNotice,
        loopAliases,
        entries,
      );
    }
  }
  if (node.forBinding !== undefined && node.forExpr !== undefined) {
    const base = (splitPipes(node.forExpr.trim())[0] ?? "").trim();
    if (PLAIN_PATH_RE.test(base)) {
      mapUntrustedPath(
        node.forBinding,
        base,
        paramSanitize,
        hostNotice,
        loopAliases,
        entries,
      );
    }
  }
  entries.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  const deduped = entries.filter(
    (e, idx) => idx === 0 || entries[idx - 1]?.[0] !== e[0],
  );
  return pathRef !== undefined
    ? { entries: deduped, pathRef }
    : { entries: deduped };
}

/**
 * Merge taint and enclosing-tag / heading state into an include call site's context.
 * Returns the existing object when nothing changes.
 */
export function updateIncludeContext(
  existing: IncludeSanitizeContext | undefined,
  taint: IncludeTaint | undefined,
  openTags: readonly string[],
  headingState: {
    readonly inAtxHeading: boolean;
    readonly atLineStart: boolean;
  },
  pendingTag: string | undefined,
): IncludeSanitizeContext | undefined {
  const hasTaint =
    taint !== undefined &&
    (taint.entries.length > 0 || taint.pathRef !== undefined);
  const hasEnclosing = openTags.length > 0 || headingState.inAtxHeading;
  if (!hasTaint && !hasEnclosing) {
    return existing;
  }
  const base: IncludeSanitizeContext = existing ?? {
    enclosingTags: [],
    inAtxHeading: false,
    atLineStart: false,
    untrusted: [],
  };
  let untrusted = base.untrusted;
  let untrustedPathRef = base.untrustedPathRef;
  if (taint !== undefined) {
    if (taint.entries.length > 0) {
      const merged = [...base.untrusted];
      for (const entry of taint.entries) {
        if (!merged.some(([p]) => p === entry[0])) merged.push(entry);
      }
      merged.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
      untrusted = merged;
    }
    untrustedPathRef ??= taint.pathRef;
  }
  const next: IncludeSanitizeContext = {
    enclosingTags: hasEnclosing ? [...openTags] : base.enclosingTags,
    inAtxHeading: hasEnclosing ? headingState.inAtxHeading : base.inAtxHeading,
    atLineStart: hasEnclosing ? headingState.atLineStart : base.atLineStart,
    ...((hasEnclosing ? pendingTag : base.pendingOpenTag) !== undefined
      ? { pendingOpenTag: hasEnclosing ? pendingTag : base.pendingOpenTag }
      : {}),
    untrusted,
    ...(untrustedPathRef !== undefined ? { untrustedPathRef } : {}),
  };
  return next;
}

/** Error for an include path that interpolates an untrusted value (Rust parity). */
function untrustedIncludePathError(
  path: string,
  expr: string,
): TemplateSyntaxError {
  return new TemplateSyntaxError(
    `include path '${path}' interpolates untrusted value '${expr}': untrusted input must not select which template is included`,
  );
}

/**
 * Reject an include call site whose path interpolates an untrusted value.
 *
 * @throws {TemplateSyntaxError} naming the offending expression.
 */
export function checkUntrustedIncludePath(
  node: Extract<Node, { kind: typeof NODE_INCLUDE }>,
): void {
  const ref = node.sanitizeCtx?.untrustedPathRef;
  if (ref !== undefined) {
    throw untrustedIncludePathError(node.path ?? node.name, ref);
  }
}

/**
 * Walk `nodes` (including compile-time bound inline includes) and reject any include
 * call site whose path interpolates an untrusted value.
 *
 * @throws {TemplateSyntaxError} for the first offending include.
 */
export function checkUntrustedIncludePaths(nodes: readonly Node[]): void {
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_INCLUDE:
        checkUntrustedIncludePath(node);
        if (node.inlineNodes) checkUntrustedIncludePaths(node.inlineNodes);
        break;
      case NODE_FOR:
        checkUntrustedIncludePaths(node.body);
        if (node.elseBody) checkUntrustedIncludePaths(node.elseBody);
        break;
      case NODE_IF:
        for (const branch of node.branches) {
          checkUntrustedIncludePaths(branch.body);
        }
        if (node.elseBody) checkUntrustedIncludePaths(node.elseBody);
        break;
      case NODE_MATCH:
        for (const arm of node.arms) checkUntrustedIncludePaths(arm.body);
        if (node.elseArm) checkUntrustedIncludePaths(node.elseArm);
        if (node.inlineGuard) checkUntrustedIncludePaths(node.inlineGuard.body);
        break;
      case NODE_PANIC:
        checkUntrustedIncludePaths(node.body);
        break;
      default:
        break;
    }
  }
}
