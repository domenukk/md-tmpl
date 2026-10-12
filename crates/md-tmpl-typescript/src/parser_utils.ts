/**
 * Scanning, blockquote preprocessing, and parsing utilities for md-tmpl.
 */

import {
  type LineMapEntry,
  type SourceLocation,
  type Node,
  getLocation,
} from "./ast.js";
import { TemplateSyntaxError, UnknownFilterError } from "./errors.js";
import { parseFilter } from "./filters.js";
import { splitPipes } from "./evaluator.js";
import {
  BLOCKQUOTE_PREFIX,
  BLOCKQUOTE_PREFIX_SPACED,
  COMMENT_END,
  COMMENT_START,
  COMMENT_START_SPACED,
  FM_DELIMITER,
  KW_END_RAW,
  KW_RAW,
  PIPE,
  STMT_END,
  STMT_START,
  TAG_RAW_ASSIGN_PREFIX,
  TRIM_MARKER,
  ERR_BARE_COMMENT,
  ERR_BARE_STMT_TAG,
  ERR_COMMENT_SPACING,
  NODE_TEXT,
  NODE_COMMENT,
  NODE_EXPR,
} from "./consts.js";

export function buildSimpleLineMap(
  body: string,
  startLineNo: number,
): LineMapEntry[] {
  const lines = body.split("\n");
  const lineMap: LineMapEntry[] = [];
  let offset = 0;
  for (const [i, line] of lines.entries()) {
    lineMap.push({
      offset,
      lineNo: startLineNo + i,
      snippet: line,
      colOffset: 0,
    });
    offset += line.length + 1; // +1 for '\n'
  }
  return lineMap;
}

/**
 * Check if a line is a valid neighbor for a standalone tag line.
 *
 * Valid neighbors: blank lines, frontmatter delimiters, or other
 * blockquote tag lines (`> {% ... %}`). Content lines starting with `>`
 * that do NOT contain `{% %}` are NOT valid — they require a blank line.
 * Normal content lines are also invalid neighbors.
 */
export function isValidTagNeighbor(line: string): boolean {
  const trimmed = line.trimStart();
  if (trimmed === "" || trimmed.startsWith(FM_DELIMITER)) return true;
  if (trimmed.startsWith(BLOCKQUOTE_PREFIX)) {
    const afterGt = trimmed.replace(/^>\s*/, "");
    return afterGt.startsWith(STMT_START) || afterGt.startsWith(COMMENT_START);
  }
  return false;
}

/** Maximum display length of a line snippet in blockquote errors (mirrors Rust). */
const SNIPPET_MAX_DISPLAY_LEN = 60;
/** Truncation point that leaves room for the trailing `…` (mirrors Rust). */
const SNIPPET_TRUNCATION_BOUNDARY = SNIPPET_MAX_DISPLAY_LEN - 3;

/** A line truncated to {@link SNIPPET_MAX_DISPLAY_LEN} for error messages. */
function displaySnippet(line: string): string {
  const chars = Array.from(line);
  return chars.length > SNIPPET_MAX_DISPLAY_LEN
    ? `${chars.slice(0, SNIPPET_TRUNCATION_BOUNDARY).join("")}…`
    : line;
}

/**
 * Per-line flags: `true` when the whole line lies inside a raw-block body.
 *
 * Raw-body lines are literal text: they are emitted verbatim, exempt from
 * blockquote rules, and count as valid neighbors of the raw block's own
 * open/close tag lines.
 */
function rawBodyLineFlags(
  body: string,
  lines: readonly string[],
  startLineNo: number,
): boolean[] {
  const ranges = rawBodyRanges(body, startLineNo);
  let start = 0;
  return lines.map((line) => {
    const content = line.endsWith("\r") ? line.slice(0, -1) : line;
    const end = start + content.length;
    const isRaw = ranges.some((r) => r.start <= start && end <= r.end);
    start += line.length + 1;
    return isRaw;
  });
}

/**
 * Strip `> ` prefix from standalone statement lines and consume the
 * trailing newline, matching how the Rust parser treats standalone tags.
 */
export function preprocessBlockquotes(
  body: string,
  startLineNo = 1,
): [string, LineMapEntry[]] {
  const lines = body.split("\n");
  const rawLines = rawBodyLineFlags(body, lines, startLineNo);
  let result = "";
  const lineMap: LineMapEntry[] = [];
  let afterStandalone = false;
  let pendingBlanks = 0;

  for (const [i, line] of lines.entries()) {
    const trimmed = line.trimStart();

    if (rawLines[i] === true) {
      // Inside raw: blank lines after a standalone tag still need
      // the pendingBlanks mechanism to match Rust's behaviour.
      if (afterStandalone && line.trim() === "") {
        pendingBlanks++;
        continue;
      }
      if (afterStandalone) {
        // tag-to-raw-content: consume 1 mandatory blank, preserve extras.
        if (pendingBlanks > 0) {
          pendingBlanks--;
        }
        for (let b = 0; b < pendingBlanks; b++) {
          result += "\n";
        }
        pendingBlanks = 0;
      }
      if (i > 0 && !afterStandalone) {
        result += "\n";
      }
      afterStandalone = false;
      lineMap.push({
        offset: result.length,
        lineNo: startLineNo + i,
        snippet: line,
        colOffset: 0,
      });
      result += line;
      continue;
    }

    // Enforce bare statement and comment rules
    const bareError = trimmed.startsWith(STMT_START)
      ? ERR_BARE_STMT_TAG
      : trimmed.startsWith(COMMENT_START)
        ? ERR_BARE_COMMENT
        : undefined;
    if (bareError !== undefined) {
      const snippet = displaySnippet(trimmed.replace(/\r$/, ""));
      throw new TemplateSyntaxError(
        `${bareError}: write '> ${snippet}' instead of '${snippet}'`,
        startLineNo + i,
        1,
        line,
      );
    }

    if (trimmed.startsWith(BLOCKQUOTE_PREFIX)) {
      const afterGt = trimmed.replace(/^>\s*/, "");
      if (afterGt.startsWith(COMMENT_START)) {
        const closeIdx = afterGt.indexOf(COMMENT_END);
        if (
          closeIdx !== -1 &&
          ((!afterGt.startsWith(COMMENT_START_SPACED) &&
            !afterGt.startsWith("{#\t")) ||
            (closeIdx > 0 &&
              afterGt[closeIdx - 1] !== " " &&
              afterGt[closeIdx - 1] !== "\t"))
        ) {
          throw new TemplateSyntaxError(
            ERR_COMMENT_SPACING,
            startLineNo + i,
            1,
            line,
          );
        }
      }
    }

    // Check for standalone blockquote statement line: `> {% ... %}` or `> {# ... #}`
    const afterGt = trimmed.replace(/^>\s*/, "");
    const shouldStrip =
      trimmed.startsWith(">") &&
      (afterGt.startsWith("{%") || afterGt.startsWith("{#"));

    let tagLine = line;
    if (shouldStrip) {
      tagLine = afterGt;
    }

    let isStandaloneTag = false;
    if (shouldStrip) {
      if (afterGt.startsWith("{%")) {
        const closePos = afterGt.indexOf("%}", 2);
        if (closePos !== -1 && closePos + 2 === afterGt.length) {
          isStandaloneTag = true;
        }
      } else if (afterGt.startsWith("{#")) {
        const closePos = afterGt.indexOf("#}", 2);
        if (closePos !== -1 && closePos + 2 === afterGt.length) {
          isStandaloneTag = true;
        }
      }
    }

    // Skip blank line immediately after a standalone tag.
    // Track pending blanks: between tags, consume all; before content,
    // consume 1 (mandatory) and preserve extras as output newlines.
    if (afterStandalone && tagLine.trim() === "") {
      pendingBlanks++;
      continue;
    }

    if (afterStandalone && isStandaloneTag) {
      // Tag-to-tag: consume all pending blanks (structural whitespace).
      pendingBlanks = 0;
    } else if (afterStandalone) {
      // Tag-to-content: consume 1 mandatory blank, preserve the rest.
      if (pendingBlanks > 0) {
        pendingBlanks--;
      }
      for (let b = 0; b < pendingBlanks; b++) {
        result += "\n";
      }
      pendingBlanks = 0;
    }

    // Add newline separator before this line (except for the first line
    // and when the previous line was a standalone tag).
    if (i > 0 && !afterStandalone) {
      result += "\n";
    }
    afterStandalone = false;

    if (isStandaloneTag) {
      if (i > 0) {
        const prevLine = lines[i - 1];
        if (
          prevLine !== undefined &&
          rawLines[i - 1] !== true &&
          !isValidTagNeighbor(prevLine)
        ) {
          throw new TemplateSyntaxError(
            `Standalone statement tag '${line.trim()}' must be preceded by a blank line or another blockquote tag line (> {%...%})`,
            startLineNo + i,
            1,
            line,
          );
        }
      }

      if (i + 1 < lines.length) {
        const nextLine = lines[i + 1];
        if (
          nextLine !== undefined &&
          rawLines[i + 1] !== true &&
          !isValidTagNeighbor(nextLine)
        ) {
          throw new TemplateSyntaxError(
            `Standalone statement tag '${line.trim()}' must be followed by a blank line or another blockquote tag line (> {%...%})`,
            startLineNo + i,
            1,
            line,
          );
        }
      }

      // Pop the preceding blank line (standalone tags consume surrounding
      // blank lines, matching Rust's strip_blockquote_tags).
      if (result.endsWith("\n\n") || result === "\n") {
        result = result.slice(0, -1);
      }

      afterStandalone = true;
    }

    lineMap.push({
      offset: result.length,
      lineNo: startLineNo + i,
      snippet: line,
      colOffset: line.length - tagLine.length,
    });
    // Strip trim markers from standalone blockquote tags — they are
    // redundant because blockquote preprocessing already manages all
    // surrounding whitespace.  This matches Rust's behaviour where
    // find_closing_block ignores trim markers.
    if (isStandaloneTag) {
      let cleaned = tagLine;
      // {%- → {% and -%} → %}
      cleaned = cleaned.replace(/^\{%-/, "{%").replace(/-%\}$/, "%}");
      // {#- → {# and -#} → #}
      cleaned = cleaned.replace(/^\{#-/, "{#").replace(/-#\}$/, "#}");
      result += cleaned;
    } else {
      result += tagLine;
    }
  }

  return [result, lineMap];
}

export function getLoc(
  pos?: number,
  lineMap?: LineMapEntry[],
): SourceLocation | undefined {
  return pos !== undefined && lineMap ? getLocation(pos, lineMap) : undefined;
}

import { parseSanitizeFilterArgs } from "./sanitize_pass.js";

export const VALID_FILTERS = new Set([
  "upper",
  "lower",
  "trim",
  "fixed",
  "join",
  "limit",
  "add",
  "sub",
  "escape_xml",
  "xml",
  "escape_json",
  "tojson",
  "to_json",
  "json",
  "sanitize_tokens",
  "fence",
  "quarantine",
  "sanitize",
  "truncate",
  "truncate_middle",
]);

export function validateFilters(expr: string): void {
  if (expr.includes(PIPE)) {
    const parts = splitPipes(expr);
    for (let i = 1; i < parts.length; i++) {
      const part = parts[i];
      if (part === undefined) continue;
      const filterStr = part.trim();
      if (!filterStr) continue;
      const [filterName, rawArg] = parseFilter(filterStr);
      if (!VALID_FILTERS.has(filterName)) {
        throw new UnknownFilterError(filterName);
      }
      if (filterName === "sanitize") {
        parseSanitizeFilterArgs(rawArg);
      }
    }
  }
}

/** Parse `{{ expr }}` and return `[expr, endPos, trimBefore, trimAfter]`. */
export function parseExpression(
  input: string,
  start: number,
): [string, number, boolean, boolean] {
  const trimBefore = input[start + 2] === "-";
  const offset = trimBefore ? 3 : 2;
  const endIdx = input.indexOf("}}", start + offset);
  if (endIdx === -1) {
    throw new TemplateSyntaxError("unclosed expression {{");
  }
  let expr = input.slice(start + offset, endIdx).trim();
  let trimAfter = false;
  if (expr.endsWith("-")) {
    trimAfter = true;
    expr = expr.slice(0, -1).trim();
  }
  validateFilters(expr);
  return [expr, endIdx + 2, trimBefore, trimAfter];
}

/** Parse `{% tag %}` and return `[tag, endPos, trimBefore, trimAfter]`. */
export function parseStatement(
  input: string,
  start: number,
): [string, number, boolean, boolean] {
  const trimBefore = input[start + 2] === "-";
  const offset = trimBefore ? 3 : 2;
  const endIdx = input.indexOf("%}", start + offset);
  if (endIdx === -1) {
    throw new TemplateSyntaxError("unclosed statement {%");
  }
  const rawInner = input.slice(start + offset, endIdx);
  const trimAfter = rawInner.endsWith("-");
  const content = trimAfter ? rawInner.slice(0, -1) : rawInner;

  if (content !== "" && (!/^\s/.test(content) || !/\s$/.test(content))) {
    throw new TemplateSyntaxError(
      "Statement tags must have spaces around the content (e.g. `{% if x %}` or `{%- if x -%}`)",
    );
  }
  const tag = content.trim();
  return [tag, endIdx + 2, trimBefore, trimAfter];
}

export function tryMatchClosingTag(
  input: string,
  pos: number,
  closingTags: string[],
): string | null {
  // Look ahead for {%
  const trimmedFromPos = input.slice(pos).trimStart();
  if (!trimmedFromPos.startsWith("{%")) return null;

  const endIdx = trimmedFromPos.indexOf("%}");
  if (endIdx === -1) return null;

  const tagContent = trimmedFromPos
    .slice(2, endIdx)
    .trim()
    .replace(/^-/, "")
    .replace(/-$/, "")
    .trim();

  for (const ct of closingTags) {
    if (tagContent === ct || tagContent.startsWith(`${ct} `)) {
      return ct;
    }
  }
  return null;
}

export function findNextTag(input: string, from: number): number {
  let pos = from;
  while (pos < input.length - 1) {
    if (input[pos] === "{") {
      if (input[pos + 1] === "%") return pos;
      if (input[pos + 1] === "{") {
        // Skip past }}
        const end = input.indexOf("}}", pos + 2);
        pos = end === -1 ? pos + 2 : end + 2;
        continue;
      }
      if (input[pos + 1] === "#") {
        // Skip past #}
        const end = input.indexOf("#}", pos + 2);
        pos = end === -1 ? pos + 2 : end + 2;
        continue;
      }
    }
    pos++;
  }
  return -1;
}

/** Extract the content of a `{% ... %}` tag at the given position. */
export function extractTagContent(
  input: string,
  start: number,
): [string, number] {
  const endIdx = input.indexOf("%}", start + 2);
  if (endIdx === -1) {
    throw new TemplateSyntaxError("unclosed statement {%");
  }
  return [input.slice(start + 2, endIdx).trim(), endIdx + 2];
}

/** Find the start position of a tag (accounting for blockquote prefix). */
export function findTagStart(input: string, tagIdx: number): number {
  // Walk backwards to find the start of the line
  let lineStart = tagIdx;
  while (lineStart > 0 && input[lineStart - 1] !== "\n") {
    lineStart--;
  }
  // Check if the line only contains the tag (standalone tag)
  const lineContent = input.slice(lineStart, tagIdx).trim();
  if (lineContent === "" || lineContent === ">") {
    return lineStart;
  }
  return tagIdx;
}

/** Find the end of a block for inline template extraction. */
export function findBlockEnd(
  input: string,
  start: number,
  closeTag: string,
): number {
  let depth = 1;
  let pos = start;
  const openTag = closeTag.slice(1); // e.g., "tmpl" from "/tmpl"

  while (pos < input.length && depth > 0) {
    const nextOpen = input.indexOf(`{% ${openTag} `, pos);
    const nextClose = input.indexOf(`{% ${closeTag} %}`, pos);
    const nextCloseAlt = input.indexOf(`{%${closeTag}%}`, pos);
    const actualClose = Math.min(
      nextClose >= 0 ? nextClose : Infinity,
      nextCloseAlt >= 0 ? nextCloseAlt : Infinity,
    );

    if (actualClose === Infinity) break;

    if (nextOpen >= 0 && nextOpen < actualClose) {
      depth++;
      pos = nextOpen + 1;
    } else {
      depth--;
      if (depth === 0) return actualClose;
      pos = actualClose === nextClose ? nextClose + 1 : nextCloseAlt + 1;
    }
  }
  return input.length;
}

export function parseInlineContent(
  input: string,
  from: number,
  to: number,
  nodes: Node[],
  lineMap?: LineMapEntry[],
): void {
  let pos = from;
  while (pos < to) {
    const exprIdx = input.indexOf("{{", pos);
    const commentIdx = input.indexOf("{#", pos);
    const earliest = Math.min(
      exprIdx >= 0 && exprIdx < to ? exprIdx : Infinity,
      commentIdx >= 0 && commentIdx < to ? commentIdx : Infinity,
    );

    if (earliest === Infinity) {
      if (pos < to) {
        nodes.push({
          kind: NODE_TEXT,
          text: input.slice(pos, to),
          loc: getLoc(pos, lineMap),
        });
      }
      break;
    }

    if (earliest > pos) {
      nodes.push({
        kind: NODE_TEXT,
        text: input.slice(pos, earliest),
        loc: getLoc(pos, lineMap),
      });
    }

    if (earliest === commentIdx) {
      const endComment = input.indexOf("#}", commentIdx + 2);
      if (endComment === -1 || endComment >= to) {
        throw new TemplateSyntaxError("unclosed comment {#");
      }
      nodes.push({ kind: NODE_COMMENT, loc: getLoc(commentIdx, lineMap) });
      pos = endComment + 2;
    } else {
      const [expr, endPos, trimBefore, trimAfter] = parseExpression(
        input,
        exprIdx,
      );
      nodes.push({
        kind: NODE_EXPR,
        expr,
        trimBefore,
        trimAfter,
        loc: getLoc(exprIdx, lineMap),
      });
      pos = endPos;
    }
  }
}

// ---------------------------------------------------------------------------
// Raw blocks (`{% raw %}` / `{% raw=DELIM %}`)
// ---------------------------------------------------------------------------

/** Half-open byte range `[start, end)` of a raw-block body. */
export interface RawBodyRange {
  readonly start: number;
  readonly end: number;
}

/**
 * Closing keyword for a raw-block opener (`/raw` or `/DELIM`), or
 * `undefined` when `tag` does not open a raw block.
 */
export function rawCloseKeyword(tag: string): string | undefined {
  if (tag === KW_RAW) return KW_END_RAW;
  if (tag.startsWith(TAG_RAW_ASSIGN_PREFIX)) {
    return `/${tag.slice(TAG_RAW_ASSIGN_PREFIX.length).trim()}`;
  }
  return undefined;
}

/**
 * Offset of the `{%` that closes a raw block opened before `afterTag`, or
 * -1 when the block is unclosed.
 */
export function findRawClose(
  input: string,
  afterTag: number,
  closeKeyword: string,
): number {
  for (const closeTag of [
    `${STMT_START} ${closeKeyword} ${STMT_END}`,
    `${STMT_START}${closeKeyword}${STMT_END}`,
  ]) {
    const idx = input.indexOf(closeTag, afterTag);
    if (idx !== -1) return idx;
  }
  // Fall back to a close tag on its own (optionally blockquoted) line.
  const standaloneClose = `${STMT_START} ${closeKeyword} ${STMT_END}`;
  let offset = afterTag;
  for (const line of input.slice(afterTag).split("\n")) {
    const trimmed = line.trim();
    if (
      trimmed === standaloneClose ||
      trimmed === `${BLOCKQUOTE_PREFIX_SPACED}${standaloneClose}`
    ) {
      return offset;
    }
    offset += line.length + 1;
  }
  return -1;
}

/** Statement content of a `{% ... %}` tag body, without trim markers. */
function cleanStatement(tagContent: string): string {
  let stmt = tagContent;
  if (stmt.startsWith(TRIM_MARKER)) stmt = stmt.slice(TRIM_MARKER.length);
  if (stmt.endsWith(TRIM_MARKER)) stmt = stmt.slice(0, -TRIM_MARKER.length);
  return stmt.trim();
}

/** Number of lines before `offset` in `input`. */
function lineIndexAt(input: string, offset: number): number {
  return input.slice(0, offset).split("\n").length - 1;
}

/**
 * Byte ranges of every top-level raw-block body in `input`, located with
 * the same tag scanner and close-tag search the parser uses.
 *
 * @throws TemplateSyntaxError when a raw block is unclosed.
 */
export function rawBodyRanges(
  input: string,
  startLineNo: number,
): RawBodyRange[] {
  const ranges: RawBodyRange[] = [];
  let pos = 0;
  for (;;) {
    const tagStart = findNextTag(input, pos);
    if (tagStart === -1) return ranges;
    const [tagContent, tagEnd] = extractTagContent(input, tagStart);
    const stmt = cleanStatement(tagContent);
    const closeKeyword = rawCloseKeyword(stmt);
    if (closeKeyword === undefined) {
      pos = tagEnd;
      continue;
    }
    const closeStart = findRawClose(input, tagEnd, closeKeyword);
    if (closeStart === -1) {
      const line = input.split("\n")[lineIndexAt(input, tagStart)];
      throw new TemplateSyntaxError(
        `unclosed '{% ${stmt} %}' block`,
        startLineNo + lineIndexAt(input, tagStart),
        1,
        line,
      );
    }
    ranges.push({ start: tagEnd, end: closeStart });
    const closeEnd = input.indexOf(STMT_END, closeStart);
    pos = closeEnd === -1 ? closeStart : closeEnd + STMT_END.length;
  }
}
