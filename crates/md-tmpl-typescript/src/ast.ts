/**
 * AST node types, source location tracking, and line mapping for md-tmpl.
 */

// ---------------------------------------------------------------------------
// Source location and line mapping
// ---------------------------------------------------------------------------

export interface LineMapEntry {
  offset: number; // character offset in processed string
  lineNo: number; // 1-indexed line number in original source
  snippet: string; // original source line text
  colOffset: number; // characters stripped from line start (e.g. 2 for "> ")
}

export interface SourceLocation {
  line: number;
  column: number;
  snippet: string;
}

export function getLocation(
  pos: number,
  lineMap: LineMapEntry[],
): SourceLocation {
  const len = lineMap.length;
  if (len === 0) {
    return { line: 1, column: 1, snippet: "" };
  }
  if (len === 1) {
    const first = lineMap[0];
    if (first === undefined) {
      return { line: 1, column: 1, snippet: "" };
    }
    return {
      line: first.lineNo,
      column: Math.max(1, pos - first.offset + 1 + first.colOffset),
      snippet: first.snippet,
    };
  }
  let low = 0;
  let high = len - 1;
  let best = 0;
  while (low <= high) {
    const mid = (low + high) >> 1;
    const midEntry = lineMap[mid];
    if (midEntry !== undefined && midEntry.offset <= pos) {
      best = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  const entry = lineMap[best];
  if (entry === undefined) {
    return { line: 1, column: 1, snippet: "" };
  }
  const column = Math.max(1, pos - entry.offset + 1 + entry.colOffset);
  return {
    line: entry.lineNo,
    column,
    snippet: entry.snippet,
  };
}

export type LineMapSource = LineMapEntry[] | (() => LineMapEntry[]);

export function getLoc(
  pos?: number,
  lineMap?: LineMapSource,
): SourceLocation | undefined {
  if (pos === undefined || lineMap === undefined) return undefined;
  const entries = typeof lineMap === "function" ? lineMap() : lineMap;
  return getLocation(pos, entries);
}

export function nodeLoc(node: {
  loc?: SourceLocation;
  pos?: number;
  lineMap?: LineMapSource;
}): SourceLocation | undefined {
  return node.loc ?? getLoc(node.pos, node.lineMap);
}

import {
  NODE_TEXT,
  NODE_EXPR,
  NODE_COMMENT,
  NODE_FOR,
  NODE_IF,
  NODE_MATCH,
  NODE_RAW,
  NODE_INCLUDE,
  NODE_TMPL,
  NODE_PANIC,
} from "./consts.js";
import type { CompiledDirectCondition } from "./direct_renderer/condition.js";
import type { CompiledDirectExpr } from "./direct_renderer/expr.js";
import type { SanitizeSpec } from "./sanitize_runtime.js";

/**
 * Sanitization context captured at an `{% include %}` call site that is resolved at
 * render time (file include, `tmpl(...)` parameter, or recursive inline template).
 * Mirrors the Rust `IncludeSanitizeContext`.
 */
export interface IncludeSanitizeContext {
  /** Static XML tags open at the call site (outermost first). */
  readonly enclosingTags: readonly string[];
  /** Whether the call site sits on a Markdown ATX heading line. */
  readonly inAtxHeading: boolean;
  /** Whether the call site is at the start of a line. */
  readonly atLineStart: boolean;
  /** Partially scanned opening tag pending at the call site (`<tag` without `>`). */
  readonly pendingOpenTag?: string;
  /** Child parameter paths receiving untrusted values, sorted by path. */
  readonly untrusted: readonly (readonly [string, SanitizeSpec])[];
  /** Untrusted expression interpolated into the include path, if any. */
  readonly untrustedPathRef?: string;
}

// ---------------------------------------------------------------------------
// AST node types
// ---------------------------------------------------------------------------

/** A parsed template node. */
export type Node =
  | { kind: typeof NODE_TEXT; text: string; loc?: SourceLocation }
  | {
      kind: typeof NODE_EXPR;
      expr: string;
      trimBefore: boolean;
      trimAfter: boolean;
      loc?: SourceLocation;
      pos?: number;
      lineMap?: LineMapSource;
      _compiled?: CompiledDirectExpr;
    }
  | { kind: typeof NODE_COMMENT; loc?: SourceLocation }
  | {
      kind: typeof NODE_FOR;
      binding: string;
      iterExpr: string;
      body: Node[];
      elseBody?: Node[];
      loc?: SourceLocation;
      pos?: number;
      lineMap?: LineMapSource;
      _compiled?: CompiledDirectExpr;
    }
  | {
      kind: typeof NODE_IF;
      branches: IfBranch[];
      elseBody: Node[] | undefined;
      loc?: SourceLocation;
      pos?: number;
      lineMap?: LineMapSource;
    }
  | {
      kind: typeof NODE_MATCH;
      expr: string;
      arms: MatchArm[];
      elseArm: Node[] | undefined;
      inlineGuard?: { variant: string; body: Node[] };
      loc?: SourceLocation;
      pos?: number;
      lineMap?: LineMapSource;
      _compiled?: CompiledDirectExpr;
    }
  | { kind: typeof NODE_RAW; text: string; loc?: SourceLocation }
  | {
      kind: typeof NODE_INCLUDE;
      name: string;
      path?: string;
      withMappings: Map<string, string>;
      forBinding?: string;
      forExpr?: string;
      inlineNodes?: Node[];
      sanitizeCtx?: IncludeSanitizeContext;
      loc?: SourceLocation;
      pos?: number;
      lineMap?: LineMapSource;
    }
  | {
      kind: typeof NODE_TMPL;
      name: string;
      source: string;
      /** Position (in the parsed input) where `source` begins, for error locations. */
      sourcePos?: number;
      loc?: SourceLocation;
      pos?: number;
      lineMap?: LineMapSource;
    }
  | {
      kind: typeof NODE_PANIC;
      body: Node[];
      loc?: SourceLocation;
    };

export interface IfBranch {
  condition: string;
  body: Node[];
  _compiled?: CompiledDirectCondition;
}

export interface MatchArm {
  variants: string[];
  body: Node[];
  guard?: string;
  _compiled?: CompiledDirectCondition;
}

export interface LoopMeta {
  index: number;
  len: number;
  first?: boolean;
  last?: boolean;
}

/**
 * Deep-clone AST nodes so site-specific sanitization can rewrite expressions
 * independently. Cached direct-render compilations (`_compiled`) are dropped so a
 * rewritten clone can never reuse a compilation of the original expression.
 */
export function cloneNodes(nodes: readonly Node[]): Node[] {
  return nodes.map((node): Node => {
    switch (node.kind) {
      case NODE_EXPR:
        return { ...node, _compiled: undefined };
      case NODE_FOR:
        return {
          ...node,
          _compiled: undefined,
          body: cloneNodes(node.body),
          elseBody: node.elseBody ? cloneNodes(node.elseBody) : undefined,
        };
      case NODE_IF:
        return {
          ...node,
          branches: node.branches.map((b) => ({
            ...b,
            _compiled: undefined,
            body: cloneNodes(b.body),
          })),
          elseBody: node.elseBody ? cloneNodes(node.elseBody) : undefined,
        };
      case NODE_MATCH:
        return {
          ...node,
          _compiled: undefined,
          arms: node.arms.map((a) => ({
            ...a,
            _compiled: undefined,
            variants: [...a.variants],
            body: cloneNodes(a.body),
          })),
          elseArm: node.elseArm ? cloneNodes(node.elseArm) : undefined,
          inlineGuard: node.inlineGuard
            ? {
                ...node.inlineGuard,
                body: cloneNodes(node.inlineGuard.body),
              }
            : undefined,
        };
      case NODE_PANIC:
        return {
          ...node,
          body: cloneNodes(node.body),
        };
      case NODE_INCLUDE:
        return {
          ...node,
          withMappings: new Map(node.withMappings),
          inlineNodes: node.inlineNodes
            ? cloneNodes(node.inlineNodes)
            : undefined,
        };
      default:
        return { ...node };
    }
  });
}
