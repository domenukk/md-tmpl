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
  KW_END_RAW_TRIM,
  KW_RAW_ASSIGN_SPACED,
  KW_RAW_CLOSE_SPACED,
  KW_RAW_SPACED,
  PIPE,
  STMT_START,
  TRIM_SPACED,
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
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
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

/**
 * Strip `> ` prefix from standalone statement lines and consume the
 * trailing newline, matching how the Rust parser treats standalone tags.
 */
export function preprocessBlockquotes(
  body: string,
  startLineNo = 1,
): [string, LineMapEntry[]] {
  const lines = body.split("\n");
  let result = "";
  const lineMap: LineMapEntry[] = [];
  let skipNextNewline = false;
  let inRaw = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const trimmed = line.trimStart();

    if (inRaw) {
      if (
        trimmed.includes(STMT_START) &&
        (trimmed.includes(KW_END_RAW) || trimmed.includes(KW_END_RAW_TRIM))
      ) {
        inRaw = false;
      } else {
        if (i > 0 && !skipNextNewline) {
          result += "\n";
        }
        skipNextNewline = false;
        lineMap.push({
          offset: result.length,
          lineNo: startLineNo + i,
          snippet: line,
          colOffset: 0,
        });
        result += line;
        continue;
      }
    } else if (
      trimmed.startsWith(BLOCKQUOTE_PREFIX) &&
      trimmed.includes(STMT_START) &&
      (trimmed.includes(KW_RAW_SPACED) ||
        trimmed.includes(KW_RAW_ASSIGN_SPACED) ||
        trimmed.includes(KW_RAW_CLOSE_SPACED))
    ) {
      inRaw = true;
    }

    // Enforce bare statement and comment rules
    if (trimmed.startsWith(STMT_START) || trimmed.startsWith(TRIM_SPACED)) {
      throw new TemplateSyntaxError(
        `Statement tags starting at the beginning of a line must have a blockquote prefix (> {% ... %}) to ensure proper Markdown rendering: '${line.trim()}'`,
        startLineNo + i,
        1,
        line,
      );
    }
    if (trimmed.startsWith(COMMENT_START)) {
      throw new TemplateSyntaxError(
        `Comments starting at the beginning of a line must have a blockquote prefix (> {# ... #}) to ensure proper Markdown rendering: '${line.trim()}'`,
        startLineNo + i,
        1,
        line,
      );
    }

    if (trimmed.startsWith(BLOCKQUOTE_PREFIX)) {
      if (
        !trimmed.startsWith(BLOCKQUOTE_PREFIX_SPACED) &&
        !trimmed.startsWith(">\t")
      ) {
        const afterGt = trimmed.slice(1).trimStart();
        if (
          afterGt.startsWith(STMT_START) ||
          afterGt.startsWith(COMMENT_START)
        ) {
          throw new TemplateSyntaxError(
            `statement tag at line start must be blockquote-prefixed with '> ': write '> ${afterGt}' instead of '${line.trim()}'`,
            startLineNo + i,
            1,
            line,
          );
        }
      }
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
            `Blockquote comments must have spaces around the content (e.g. '> {# comment #}'): '${line.trim()}'`,
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
    if (skipNextNewline && tagLine.trim() === "") {
      continue;
    }

    // Add newline separator before this line (except for the first line
    // and when the previous line was a standalone tag that consumed its
    // trailing newline).
    if (i > 0 && !skipNextNewline) {
      result += "\n";
    }
    skipNextNewline = false;

    if (isStandaloneTag) {
      const isRawTag = tagLine.includes("raw");
      if (!isRawTag && i > 0) {
        const prevLine = lines[i - 1]!;
        if (!isValidTagNeighbor(prevLine)) {
          throw new TemplateSyntaxError(
            `Standalone statement tag '${line.trim()}' must be preceded by a blank line or another blockquote tag line (> {%...%})`,
            startLineNo + i,
            1,
            line,
          );
        }
      }

      if (!isRawTag && i + 1 < lines.length) {
        const nextLine = lines[i + 1]!;
        if (!isValidTagNeighbor(nextLine)) {
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

      skipNextNewline = true;
    }

    lineMap.push({
      offset: result.length,
      lineNo: startLineNo + i,
      snippet: line,
      colOffset: line.length - tagLine.length,
    });
    result += tagLine;
  }

  return [result, lineMap];
}

export function getLoc(
  pos?: number,
  lineMap?: LineMapEntry[],
): SourceLocation | undefined {
  return pos !== undefined && lineMap ? getLocation(pos, lineMap) : undefined;
}

export const VALID_FILTERS = new Set([
  "upper",
  "lower",
  "trim",
  "fixed",
  "join",
  "limit",
  "add",
  "sub",
]);

export function validateFilters(expr: string): void {
  if (expr.indexOf(PIPE) !== -1) {
    const parts = splitPipes(expr);
    for (let i = 1; i < parts.length; i++) {
      const filterStr = parts[i]!.trim();
      if (!filterStr) continue;
      const [filterName] = parseFilter(filterStr);
      if (!VALID_FILTERS.has(filterName)) {
        throw new UnknownFilterError(filterName);
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
