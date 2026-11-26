/**
 * Enclosing XML tag and Markdown ATX heading scan helpers for the compile-time
 * sanitization AST pass.
 *
 * @module
 */

import type { Node } from "./ast.js";
import {
  COMMA,
  NODE_FOR,
  NODE_IF,
  NODE_INCLUDE,
  NODE_MATCH,
  NODE_PANIC,
  NODE_TEXT,
} from "./consts.js";
import {
  isAsciiWhitespaceCode,
  isNcnameContinueCode,
  isNcnameStartCode,
  skipTagWhitespace,
} from "./sanitize_runtime.js";

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

export interface PendingOpenTagEntry {
  readonly name: string;
  readonly quote: number | undefined;
}

export interface EnclosingScanState {
  openTags: string[];
  pendingOpenTags: PendingOpenTagEntry[];
  inAtxHeading: boolean;
  atLineStart: boolean;
}

export function cloneScanState(state: EnclosingScanState): EnclosingScanState {
  return {
    openTags: [...state.openTags],
    pendingOpenTags: [...state.pendingOpenTags],
    inAtxHeading: state.inAtxHeading,
    atLineStart: state.atLineStart,
  };
}

export function joinPendingOpenTags(
  pending: readonly PendingOpenTagEntry[],
): string | undefined {
  if (pending.length === 0) {
    return undefined;
  }
  const names: string[] = [];
  for (const entry of pending) {
    if (!names.some((n) => n.toLowerCase() === entry.name.toLowerCase())) {
      names.push(entry.name);
    }
  }
  return names.join(COMMA);
}

export function parsePendingOpenTagsSpec(
  pendingSpec: string | undefined,
): PendingOpenTagEntry[] {
  if (pendingSpec === undefined || pendingSpec.length === 0) {
    return [];
  }
  const entries: PendingOpenTagEntry[] = [];
  for (const part of pendingSpec.split(COMMA)) {
    const trimmed = part.trim();
    if (trimmed.length > 0) {
      entries.push({ name: trimmed, quote: undefined });
    }
  }
  return entries;
}

export function mergeBranchStates(
  target: EnclosingScanState,
  states: readonly EnclosingScanState[],
): void {
  if (states.length === 0) {
    return;
  }
  const unionTags: string[] = [];
  const unionPending: PendingOpenTagEntry[] = [];
  let anyHeading = false;
  let anyLineStart = false;
  for (const st of states) {
    for (const tag of st.openTags) {
      if (!unionTags.some((t) => t.toLowerCase() === tag.toLowerCase())) {
        unionTags.push(tag);
      }
    }
    for (const entry of st.pendingOpenTags) {
      if (
        !unionPending.some(
          (e) =>
            e.name.toLowerCase() === entry.name.toLowerCase() &&
            e.quote === entry.quote,
        )
      ) {
        unionPending.push(entry);
      }
    }
    anyHeading ||= st.inAtxHeading;
    anyLineStart ||= st.atLineStart;
  }
  target.openTags = unionTags;
  target.pendingOpenTags = unionPending;
  target.inAtxHeading = anyHeading;
  target.atLineStart = anyLineStart;
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

export function collectClosingXmlTags(
  nodes: readonly Node[],
  out: Set<string>,
): void {
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

export function updateOpenXmlTagsFromText(
  text: string,
  closingTags: ReadonlySet<string>,
  state: EnclosingScanState,
): void {
  let i = 0;
  if (state.pendingOpenTags.length > 0) {
    const prevPending = state.pendingOpenTags;
    const stillPending: PendingOpenTagEntry[] = [];
    const pushedBatch: string[] = [];
    let minResume: number | undefined;
    for (const entry of prevPending) {
      const tail = scanOpenTagTail(text, 0, entry.quote);
      if (tail.kind === "closed") {
        const lower = entry.name.toLowerCase();
        if (
          !tail.selfClosing &&
          closingTags.has(lower) &&
          !pushedBatch.includes(lower)
        ) {
          pushedBatch.push(lower);
          state.openTags.push(entry.name);
        }
        const next = tail.closeIdx + 1;
        minResume = minResume === undefined ? next : Math.min(minResume, next);
      } else if (tail.kind === "pending") {
        stillPending.push({ name: entry.name, quote: tail.quote });
      } else {
        minResume =
          minResume === undefined
            ? tail.ltIdx
            : Math.min(minResume, tail.ltIdx);
      }
    }
    state.pendingOpenTags = stillPending;
    if (minResume === undefined) {
      return;
    }
    i = minResume;
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
          state.pendingOpenTags.push({ name, quote: undefined });
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
            state.pendingOpenTags.push({ name, quote: tail.quote });
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

export function updateHeadingLineState(
  text: string,
  state: EnclosingScanState,
): void {
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
