/**
 * Include call-site sanitization context: untrusted taint through `with` / `for`,
 * enclosing-tag / heading state at the call site, and untrusted include paths.
 * Mirrors the Rust `compiled/include_sanitize.rs`.
 *
 * Trust follows the value, not the declaration: include targets (inline `{% tmpl %}`,
 * included files, `tmpl(...)` handles) render without their own parameter policy, so a
 * trusted caller value stays verbatim and an untrusted one is sanitized once, at the
 * emission point. The target's declared specs only choose the style for untrusted
 * values: a declared block quarantine wins over the caller's style.
 *
 * @module
 */

import type { IncludeSanitizeContext, Node } from "./ast.js";
import {
  DOT,
  EXPR_START,
  FILTER_UNSAFE_TRUST,
  FN_HAS,
  FN_IDX,
  FN_KIND,
  FN_KINDS,
  FN_LEN,
  FN_UNSAFE_TRUST,
  LIT_FALSE,
  LIT_TRUE,
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
import { parseFilter } from "./filters.js";
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

export const PLAIN_PATH_RE =
  /^[A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*$/;

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
 * Declared parameter sanitization of an include target: canonical parameter path → spec,
 * with block notices resolved against the target's own `sanitize_notice:`. Never applied
 * to trusted values; it only chooses the style for untrusted caller values.
 */
export type DeclaredSanitize = ReadonlyMap<string, SanitizeSpec>;

/** Declared sanitization of an include target without a parameter policy. */
export const EMPTY_DECLARED_SANITIZE: DeclaredSanitize = new Map();

/**
 * Build the {@link DeclaredSanitize} of an include target from its frontmatter policy,
 * giving block specs without their own notice the target's `sanitize_notice`.
 */
export function declaredSanitize(
  paramSanitize: ReadonlyMap<string, SanitizeSpec> | undefined,
  sanitizeNotice: string | undefined,
): DeclaredSanitize {
  if (paramSanitize === undefined || paramSanitize.size === 0) {
    return EMPTY_DECLARED_SANITIZE;
  }
  const out = new Map<string, SanitizeSpec>();
  for (const [path, spec] of paramSanitize) {
    out.set(path, transferSpec(spec, sanitizeNotice));
  }
  return out;
}

/**
 * Style for an untrusted caller value arriving at child path `path`: the child's declared
 * block quarantine wins (it shapes the child's own output), otherwise the caller's style.
 */
function resolveIncludeSpec(
  path: string,
  callerSpec: SanitizeSpec,
  declared: DeclaredSanitize,
): SanitizeSpec {
  const own = declared.get(path);
  return own?.kind === SANITIZE_KIND_BLOCK ? own : callerSpec;
}

/** Resolve every untrusted call-site entry against the child's declared specs. */
export function effectiveUntrustedMap(
  entries: readonly (readonly [string, SanitizeSpec])[],
  declared: DeclaredSanitize,
): Map<string, SanitizeSpec> {
  const map = new Map<string, SanitizeSpec>();
  for (const [path, spec] of entries) {
    map.set(path, resolveIncludeSpec(path, spec, declared));
  }
  return map;
}

/** Static trust class of the base of an include argument or interpolation. */
const BaseTrust = {
  /** A plain variable path whose trust is looked up in the declarative policy. */
  Path: "path",
  /** A literal or a built-in function returning a non-injectable value. */
  Trusted: "trusted",
  /** Anything else: untrusted, so no unrecognized form can launder taint. */
  Unknown: "unknown",
} as const;
type BaseTrust = (typeof BaseTrust)[keyof typeof BaseTrust];

/** Built-in functions whose results cannot carry injected text. */
const TRUSTED_FUNCTIONS: ReadonlySet<string> = new Set([
  FN_LEN,
  FN_IDX,
  FN_KIND,
  FN_KINDS,
  FN_HAS,
]);
const NUMBER_LITERAL_RE = /^-?[0-9]+(?:\.[0-9]+)?$/;
const FUNCTION_CALL_RE = /^([A-Za-z0-9_]+)\s*\((.+)\)$/s;

/** Classify `base` for trust propagation (mirrors Rust `classify_base`). */
function classifyBase(base: string): BaseTrust {
  if (PLAIN_PATH_RE.test(base)) {
    return BaseTrust.Path;
  }
  const lit = stripStringLiteral(base);
  if (lit !== undefined) {
    return lit.includes(EXPR_START) ? BaseTrust.Unknown : BaseTrust.Trusted;
  }
  if (NUMBER_LITERAL_RE.test(base) || base === LIT_TRUE || base === LIT_FALSE) {
    return BaseTrust.Trusted;
  }
  const call = FUNCTION_CALL_RE.exec(base);
  const fnName = call?.[1];
  if (fnName !== undefined && TRUSTED_FUNCTIONS.has(fnName)) {
    return BaseTrust.Trusted;
  }
  return BaseTrust.Unknown;
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

/** Return `true` when `expr` explicitly opts out of untrusted taint via `unsafe_trust(x)` or `x | unsafe_trust`. */
function hasUnsafeTrustOptOut(expr: string): boolean {
  const parts = splitPipes(expr.trim());
  const base = (parts[0] ?? "").trim();
  if (base.startsWith(`${FN_UNSAFE_TRUST}(`) && base.endsWith(")")) {
    return true;
  }
  for (let i = 1; i < parts.length; i++) {
    const [fName] = parseFilter((parts[i] ?? "").trim());
    if (fName === FILTER_UNSAFE_TRUST) {
      return true;
    }
  }
  return false;
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
    if (!hasUnsafeTrustOptOut(inner)) {
      const base = (splitPipes(inner)[0] ?? "").trim();
      switch (classifyBase(base)) {
        case BaseTrust.Path: {
          const hits: [string, SanitizeSpec][] = [];
          mapUntrustedPath(
            "",
            base,
            paramSanitize,
            undefined,
            loopAliases,
            hits,
          );
          if (hits.length > 0) return base;
          break;
        }
        case BaseTrust.Trusted:
          break;
        case BaseTrust.Unknown:
          return base;
      }
    }
    search = rest.slice(end + 2);
  }
}

/** Append the untrusted child entries for argument `rawExpr` passed to child path `target`. */
function pushArgumentTaint(
  target: string,
  rawExpr: string,
  paramSanitize: ReadonlyMap<string, SanitizeSpec>,
  hostNotice: string | undefined,
  loopAliases: ReadonlyMap<string, string>,
  entries: [string, SanitizeSpec][],
): void {
  const expr = rawExpr.trim();
  if (hasUnsafeTrustOptOut(expr)) {
    return;
  }
  const base = (splitPipes(expr)[0] ?? "").trim();
  const lit = stripStringLiteral(base);
  if (lit !== undefined) {
    if (
      firstUntrustedInterpolation(lit, paramSanitize, loopAliases) !== undefined
    ) {
      entries.push([target, { kind: SANITIZE_KIND_INLINE }]);
    }
    return;
  }
  switch (classifyBase(base)) {
    case BaseTrust.Path:
      mapUntrustedPath(
        target,
        base,
        paramSanitize,
        hostNotice,
        loopAliases,
        entries,
      );
      break;
    case BaseTrust.Trusted:
      break;
    case BaseTrust.Unknown:
      entries.push([target, { kind: SANITIZE_KIND_INLINE }]);
      break;
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
    pushArgumentTaint(
      key,
      rawExpr,
      paramSanitize,
      hostNotice,
      loopAliases,
      entries,
    );
  }
  if (node.forBinding !== undefined && node.forExpr !== undefined) {
    pushArgumentTaint(
      node.forBinding,
      node.forExpr,
      paramSanitize,
      hostNotice,
      loopAliases,
      entries,
    );
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
