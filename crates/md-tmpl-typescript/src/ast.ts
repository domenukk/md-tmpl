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
  if (lineMap.length === 0) {
    return { line: 1, column: 1, snippet: "" };
  }
  let low = 0;
  let high = lineMap.length - 1;
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

export function getLoc(
  pos?: number,
  lineMap?: LineMapEntry[],
): SourceLocation | undefined {
  return pos !== undefined && lineMap ? getLocation(pos, lineMap) : undefined;
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
    }
  | { kind: typeof NODE_COMMENT; loc?: SourceLocation }
  | {
      kind: typeof NODE_FOR;
      binding: string;
      iterExpr: string;
      body: Node[];
      elseBody?: Node[];
      loc?: SourceLocation;
    }
  | {
      kind: typeof NODE_IF;
      branches: IfBranch[];
      elseBody: Node[] | undefined;
      loc?: SourceLocation;
    }
  | {
      kind: typeof NODE_MATCH;
      expr: string;
      arms: MatchArm[];
      elseArm: Node[] | undefined;
      inlineGuard?: { variant: string; body: Node[] };
      loc?: SourceLocation;
    }
  | { kind: typeof NODE_RAW; text: string; loc?: SourceLocation }
  | {
      kind: typeof NODE_INCLUDE;
      name: string;
      path?: string;
      withMappings: Map<string, string>;
      forBinding?: string;
      forExpr?: string;
      loc?: SourceLocation;
    }
  | {
      kind: typeof NODE_TMPL;
      name: string;
      source: string;
      loc?: SourceLocation;
    }
  | {
      kind: typeof NODE_PANIC;
      body: Node[];
      loc?: SourceLocation;
    };

export interface IfBranch {
  condition: string;
  body: Node[];
}

export interface MatchArm {
  variants: string[];
  body: Node[];
  guard?: string;
}

export interface LoopMeta {
  index: number;
  first?: boolean;
  last?: boolean;
}
