/**
 * Template parser and renderer for md-tmpl.
 */

import {
  type LineMapEntry,
  type LineMapSource,
  type SourceLocation,
  getLocation,
  type Node,
} from "./ast.js";
import { Scope } from "./scope.js";
import { type RenderOptions, renderNodes } from "./renderer.js";
import { evaluateExpression } from "./evaluator.js";
import { TemplateSyntaxError } from "./errors.js";
import {
  buildSimpleLineMap,
  extractTagContent,
  findNextTag,
  findTagStart,
  getLoc,
  parseExpression,
  parseInlineContent,
  parseStatement,
  preprocessBlockquotes,
  tryMatchClosingTag,
  VALID_FILTERS,
  validateFilters,
  withParseLocation,
} from "./parser_utils.js";
import { handleStatement } from "./statement_handlers.js";
import {
  ERR_COMMENT_SPACING,
  NODE_TEXT,
  NODE_COMMENT,
  NODE_EXPR,
} from "./consts.js";

// Re-export core types and functions for external consumers
export {
  type LineMapEntry,
  type SourceLocation,
  getLocation,
  type Node,
  Scope,
  type RenderOptions,
  renderNodes,
  evaluateExpression,
  VALID_FILTERS,
  validateFilters,
};

/**
 * Parse a template body string into a list of AST nodes.
 *
 * Handles `{{ expr }}`, `{% tag %}`, `{# comment #}` delimiters,
 * blockquote stripping for standalone tags, and whitespace control.
 */
export function parseBody(
  body: string,
  skipPreprocess = false,
  startLineNo = 1,
): Node[] {
  const processed = skipPreprocess
    ? body
    : preprocessBlockquotes(body, startLineNo, false)[0];
  let cachedLineMap: LineMapEntry[] | undefined;
  const lineMap: LineMapSource = () => {
    cachedLineMap ??= skipPreprocess
      ? buildSimpleLineMap(body, startLineNo)
      : preprocessBlockquotes(body, startLineNo, true)[1];
    return cachedLineMap;
  };
  return parseNodes(processed, [], lineMap);
}

/**
 * Recursive descent parser for template nodes.
 *
 * `closingTags` is a list of `{% tag %}` names that terminate the current block.
 */
export function parseNodes(
  input: string,
  closingTags: string[],
  lineMap?: LineMapSource,
): Node[] {
  const nodes: Node[] = [];
  let pos = 0;

  while (pos < input.length) {
    let earliest = Infinity;
    try {
      // Check for closing tags
      if (closingTags.length > 0) {
        const closeMatch = tryMatchClosingTag(input, pos, closingTags);
        if (closeMatch !== null) {
          break;
        }
      }

      // Find next tag
      const exprStart = input.indexOf("{{", pos);
      const stmtStart = input.indexOf("{%", pos);
      const commentStart = input.indexOf("{#", pos);

      // Find earliest delimiter
      const candidates = [
        exprStart >= 0 ? exprStart : Infinity,
        stmtStart >= 0 ? stmtStart : Infinity,
        commentStart >= 0 ? commentStart : Infinity,
      ];
      earliest = Math.min(...candidates);

      if (earliest === Infinity) {
        // No more tags — remaining is text
        if (pos < input.length) {
          nodes.push({
            kind: NODE_TEXT,
            text: input.slice(pos),
          });
        }
        break;
      }

      // Text before the tag
      if (earliest > pos) {
        nodes.push({
          kind: NODE_TEXT,
          text: input.slice(pos, earliest),
        });
      }

      if (earliest === commentStart) {
        // Comment: {# ... #} or {#- ... -#}
        const trimBefore = input[earliest + 2] === "-";
        const offset = trimBefore ? 3 : 2;
        const endIdx = input.indexOf("#}", earliest + offset);
        if (endIdx === -1) {
          throw new TemplateSyntaxError("unclosed comment {#");
        }
        let rawInner = input.slice(earliest + offset, endIdx);
        const trimAfter = rawInner.endsWith("-");
        if (trimAfter) rawInner = rawInner.slice(0, -1);

        if (
          rawInner !== "" &&
          (!/^\s/.test(rawInner) || !/\s$/.test(rawInner))
        ) {
          throw new TemplateSyntaxError(ERR_COMMENT_SPACING);
        }
        // Apply {#- trim: strip trailing whitespace from previous text node
        if (trimBefore && nodes.length > 0) {
          const last = nodes[nodes.length - 1];
          if (last?.kind === "text") {
            nodes[nodes.length - 1] = {
              ...last,
              kind: "text",
              text: last.text.replace(/\s+$/, ""),
            };
          }
        }
        nodes.push({ kind: NODE_COMMENT });
        pos = endIdx + 2;
        // Apply -#} trim: strip leading whitespace from following text
        if (trimAfter) {
          while (pos < input.length && /\s/.test(input.charAt(pos))) {
            pos++;
          }
        }
      } else if (earliest === exprStart) {
        // Expression: {{ ... }}
        const [expr, endPos, trimBefore, trimAfter] = parseExpression(
          input,
          earliest,
        );
        nodes.push({
          kind: NODE_EXPR,
          expr,
          trimBefore,
          trimAfter,
          pos: earliest,
          lineMap,
        });
        pos = endPos;
      } else {
        // Statement: {% ... %}
        const [tag, endPos, trimBefore, trimAfter] = parseStatement(
          input,
          earliest,
        );
        // Handle {%- trim: strip trailing whitespace from previous text
        if (trimBefore && nodes.length > 0) {
          const last = nodes[nodes.length - 1];
          if (last?.kind === "text") {
            nodes[nodes.length - 1] = {
              ...last,
              kind: "text",
              text: last.text.replace(/\s+$/, ""),
            };
          }
        }
        const [newNodes, newPos] = handleStatement(
          tag,
          input,
          endPos,
          trimAfter,
          lineMap,
          earliest,
        );
        for (const n of newNodes) {
          nodes.push(n);
        }
        pos = newPos;
      }
    } catch (err) {
      const errPos = earliest !== Infinity ? earliest : pos;
      throw withParseLocation(err, errPos, lineMap);
    }
  }

  return nodes;
}

export function parseBlock(
  input: string,
  start: number,
  closingTags: string[],
  lineMap?: LineMapSource,
  openTag?: { keyword: string; tagStart?: number },
): [Node[], number] {
  const [body, endPos] = parseBlockWithClosing(
    input,
    start,
    closingTags,
    lineMap,
    openTag,
  );
  return [body, endPos];
}

/**
 * Parse until any of the closing tags, returning body, end position,
 * which tag was matched, and the content after the tag keyword.
 */
export function parseBlockWithClosing(
  input: string,
  start: number,
  closingTags: string[],
  lineMap?: LineMapSource,
  openTag?: { keyword: string; tagStart?: number },
): [Node[], number, string | undefined, string | undefined] {
  let pos = start;
  const nodes: Node[] = [];

  while (pos < input.length) {
    let earliest = pos;
    try {
      // Look for next {% tag %}
      const stmtIdx = findNextTag(input, pos);
      if (stmtIdx === -1) {
        if (pos < input.length) {
          parseInlineContent(input, pos, input.length, nodes, lineMap);
        }
        if (closingTags.length > 0) {
          const loc = getLoc(openTag?.tagStart ?? earliest, lineMap);
          const keyword = openTag
            ? openTag.keyword
            : (closingTags[0] ?? "").replace(/^\//, "");
          throw new TemplateSyntaxError(
            `unclosed '{% ${keyword} %}' block`,
            loc?.line,
            loc?.column,
            loc?.snippet,
          );
        }
        return [nodes, input.length, undefined, undefined];
      }

      // Check if it's a closing tag or branch tag
      const [tagContent, tagEnd] = extractTagContent(input, stmtIdx);
      const hasLeadingDash = tagContent.charCodeAt(0) === 45; /* '-' */
      const hasTrailingDash =
        tagContent.charCodeAt(tagContent.length - 1) === 45; /* '-' */
      const cleanTag =
        hasLeadingDash || hasTrailingDash
          ? tagContent
              .slice(
                hasLeadingDash ? 1 : 0,
                hasTrailingDash ? -1 : tagContent.length,
              )
              .trim()
          : tagContent;

      for (const ct of closingTags) {
        if (cleanTag === ct || cleanTag.startsWith(`${ct} `)) {
          // Parse any expressions/comments in the text before the closing tag
          const textEnd = findTagStart(input, stmtIdx);
          if (textEnd > pos) {
            parseInlineContent(input, pos, textEnd, nodes, lineMap);
          }
          // Handle {%- on closing tag: trim trailing whitespace from last text node
          if (hasLeadingDash && nodes.length > 0) {
            const last = nodes[nodes.length - 1];
            if (last?.kind === "text") {
              nodes[nodes.length - 1] = {
                ...last,
                kind: "text",
                text: last.text.replace(/\s+$/, ""),
              };
            }
          }
          const content = cleanTag.startsWith(`${ct} `)
            ? cleanTag.slice(ct.length + 1).trim()
            : undefined;
          // Handle -%} on closing tag: strip all whitespace after
          let adjustedEnd = tagEnd;
          if (hasTrailingDash) {
            while (
              adjustedEnd < input.length &&
              /\s/.test(input.charAt(adjustedEnd))
            ) {
              adjustedEnd++;
            }
          } else {
            // Default: consume trailing newline after closing tag
            if (input[adjustedEnd] === "\n") {
              adjustedEnd++;
            } else if (
              input[adjustedEnd] === "\r" &&
              input[adjustedEnd + 1] === "\n"
            ) {
              adjustedEnd += 2;
            }
          }
          return [nodes, adjustedEnd, ct, content];
        }
      }

      // Not a closing tag — parse all inline content up to stmtIdx, then handle stmtIdx
      while (pos < stmtIdx) {
        const exprIdx = input.indexOf("{{", pos);
        const commentIdx = input.indexOf("{#", pos);
        earliest = Math.min(
          stmtIdx,
          exprIdx >= 0 ? exprIdx : Infinity,
          commentIdx >= 0 ? commentIdx : Infinity,
        );
        if (earliest > pos) {
          nodes.push({
            kind: NODE_TEXT,
            text: input.slice(pos, earliest),
          });
        }
        if (earliest === stmtIdx) {
          pos = stmtIdx;
          break;
        }
        if (earliest === commentIdx && commentIdx >= 0) {
          const endComment = input.indexOf("#}", commentIdx + 2);
          pos = endComment === -1 ? commentIdx + 2 : endComment + 2;
          nodes.push({ kind: NODE_COMMENT });
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
            pos: exprIdx,
            lineMap,
          });
          pos = endPos;
        }
      }

      earliest = stmtIdx;
      const [, endPos, , trimAfter] = parseStatement(input, stmtIdx);
      const [innerNodes, innerEnd] = handleStatement(
        cleanTag,
        input,
        endPos,
        trimAfter,
        lineMap,
        stmtIdx,
      );
      for (const n of innerNodes) {
        nodes.push(n);
      }
      pos = innerEnd;
    } catch (err) {
      throw withParseLocation(err, earliest, lineMap);
    }
  }

  if (closingTags.length > 0) {
    const loc = getLoc(openTag?.tagStart ?? pos, lineMap);
    const keyword = openTag
      ? openTag.keyword
      : (closingTags[0]?.replace(/^\//, "") ?? "");
    throw new TemplateSyntaxError(
      `unclosed '{% ${keyword} %}' block`,
      loc?.line,
      loc?.column,
      loc?.snippet,
    );
  }
  return [nodes, pos, undefined, undefined];
}
