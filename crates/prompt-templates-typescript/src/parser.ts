/**
 * Template parser and renderer.
 *
 * Parses the template body into a list of `Node`s, then renders
 * them against a `Context` with type-checked variable resolution.
 *
 * @module
 */

import {
  type Value,
  str,
  int,
  display,
  isTruthy,
  getField,
  typeName,
  ENUM_TAG_KEY,
} from "./value.js";
import { TemplateSyntaxError, UndefinedVariableError } from "./errors.js";
import { applyFilter, parseFilter } from "./filters.js";

// ---------------------------------------------------------------------------
// AST node types
// ---------------------------------------------------------------------------

/** A parsed template node. */
export type Node =
  | { kind: "text"; text: string }
  | { kind: "expr"; expr: string; trimBefore: boolean; trimAfter: boolean }
  | { kind: "comment" }
  | {
      kind: "for";
      binding: string;
      iterExpr: string;
      body: Node[];
      elseBody?: Node[];
    }
  | {
      kind: "if";
      branches: IfBranch[];
      elseBody: Node[] | undefined;
    }
  | {
      kind: "match";
      expr: string;
      arms: MatchArm[];
      defaultArm: Node[] | undefined;
      inlineGuard?: { variant: string; body: Node[] };
    }
  | { kind: "raw"; text: string }
  | {
      kind: "include";
      name: string;
      path?: string;
      withMappings: Map<string, string>;
      forBinding?: string;
      forExpr?: string;
    }
  | {
      kind: "tmpl";
      name: string;
      source: string;
    };

interface IfBranch {
  condition: string;
  body: Node[];
}

interface MatchArm {
  variants: string[];
  body: Node[];
}

// ---------------------------------------------------------------------------
// Scope for rendering
// ---------------------------------------------------------------------------

interface LoopMeta {
  index: number;
}

/**
 * Layered scope for variable resolution during rendering.
 *
 * Constants → loop bindings → context, searched in that order.
 */
export class Scope {
  private readonly ctx: ReadonlyMap<string, Value>;
  private readonly layers: Map<string, Value>[] = [];
  private readonly loopMetas: Map<string, LoopMeta> = new Map();
  private readonly consts: ReadonlyMap<string, Value>;

  constructor(
    ctx: ReadonlyMap<string, Value>,
    consts?: ReadonlyMap<string, Value>,
  ) {
    this.ctx = ctx;
    this.consts = consts ?? new Map();
  }

  pushLayer(): Map<string, Value> {
    const layer = new Map<string, Value>();
    this.layers.push(layer);
    return layer;
  }

  popLayer(): void {
    this.layers.pop();
  }

  setLoopMeta(binding: string, meta: LoopMeta): void {
    this.loopMetas.set(binding, meta);
  }

  getLoopMeta(binding: string): LoopMeta | undefined {
    return this.loopMetas.get(binding);
  }

  resolve(key: string): Value | undefined {
    // 1. Constants
    const constVal = this.consts.get(key);
    if (constVal !== undefined) return constVal;

    // 2. Layers (innermost first)
    for (let i = this.layers.length - 1; i >= 0; i--) {
      const v = this.layers[i]!.get(key);
      if (v !== undefined) return v;
    }

    // 3. Context
    return this.ctx.get(key);
  }

  resolvePath(pathStr: string): Value {
    // Fast path: no dot means simple variable lookup (very common)
    const firstDot = pathStr.indexOf(".");
    if (firstDot === -1) {
      const root = this.resolve(pathStr);
      if (root === undefined) {
        throw new UndefinedVariableError(pathStr);
      }
      return root;
    }

    // Dotted path: scan for dots without allocating an array
    const rootKey = pathStr.slice(0, firstDot);
    const root = this.resolve(rootKey);
    if (root === undefined) {
      throw new UndefinedVariableError(rootKey);
    }
    let current = root;
    let start = firstDot + 1;
    while (start < pathStr.length) {
      const nextDot = pathStr.indexOf(".", start);
      const end = nextDot === -1 ? pathStr.length : nextDot;
      const part = pathStr.slice(start, end);
      const field = getField(current, part);
      if (field === undefined) {
        throw new UndefinedVariableError(
          `field '${part}' not found on ${current.type}`,
        );
      }
      current = field;
      start = end + 1;
    }
    return current;
  }

  /** Return all visible values (context + layers + consts) as a flat Map. */
  allValues(): Map<string, Value> {
    const result = new Map<string, Value>(this.ctx);
    for (const [k, v] of this.consts) {
      result.set(k, v);
    }
    for (const layer of this.layers) {
      for (const [k, v] of layer) {
        result.set(k, v);
      }
    }
    return result;
  }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/**
 * Parse a template body string into a list of AST nodes.
 *
 * Handles `{{ expr }}`, `{% tag %}`, `{# comment #}` delimiters,
 * blockquote stripping for standalone tags, and whitespace control.
 */
export function parseBody(body: string): Node[] {
  // Pre-process: strip blockquote prefix from standalone statement tags
  const processed = preprocessBlockquotes(body);
  return parseNodes(processed, []);
}

/**
 * Check if a line is a valid neighbor for a standalone tag line.
 *
 * Valid neighbors: blank lines, frontmatter delimiters, or other
 * blockquote tag lines (> {% ... %}). Content lines starting with >
 * that do NOT contain {% %} are NOT valid — they require a blank line.
 */
function isValidTagNeighbor(line: string): boolean {
  const trimmed = line.trimStart();
  if (trimmed === "" || trimmed.startsWith("---")) return true;
  // A > line is only valid if it's a blockquote tag line
  if (trimmed.startsWith(">")) {
    return (
      (trimmed.startsWith("> {%") || trimmed.startsWith(">{%")) &&
      trimmed.includes("%}")
    );
  }
  return false;
}

/**
 * Strip `> ` prefix from standalone statement lines and consume the
 * trailing newline, matching how the Rust parser treats standalone tags.
 *
 * Lines like `> {% for ... %}` become just `{% for ... %}` with the
 * surrounding newlines removed, so blockquote-prefixed statement tags
 * don't inject extra whitespace into the output.
 */
function preprocessBlockquotes(body: string): string {
  const lines = body.split("\n");
  const result: string[] = [];
  let skipNextNewline = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const trimmed = line.trimStart();

    // Check for standalone blockquote statement line: `> {% ... %}`
    const isBlockquoteTag =
      (trimmed.startsWith("> {%") || trimmed.startsWith(">{%")) &&
      trimmed.includes("%}");

    let tagLine = line;
    if (isBlockquoteTag) {
      if (trimmed.startsWith("> ")) {
        tagLine = trimmed.slice(2);
      } else if (trimmed.startsWith(">")) {
        tagLine = trimmed.slice(1);
      }
    }

    if (skipNextNewline && tagLine.trim() === "") {
      continue;
    }

    if (isBlockquoteTag) {
      if (i > 0) {
        const prevLine = lines[i - 1]!;
        if (!isValidTagNeighbor(prevLine)) {
          throw new TemplateSyntaxError(
            `Standalone statement tag '${line.trim()}' must be preceded by a blank line or another blockquote tag line (> {%...%})`,
          );
        }
      }

      if (i + 1 < lines.length) {
        const nextLine = lines[i + 1]!;
        if (!isValidTagNeighbor(nextLine)) {
          throw new TemplateSyntaxError(
            `Standalone statement tag '${line.trim()}' must be followed by a blank line or another blockquote tag line (> {%...%})`,
          );
        }
      }

      if (result.length > 0 && result[result.length - 1] === "" && i > 0) {
        result.pop();
      }

      if (tagLine.trim().startsWith("{%") && tagLine.trim().endsWith("%}")) {
        skipNextNewline = true;
      } else {
        skipNextNewline = false;
      }
    } else {
      skipNextNewline = false;
    }

    result.push(tagLine);
  }

  return result.join("\n");
}

/**
 * Recursive descent parser for template nodes.
 *
 * `closingTags` is a list of `{% tag %}` names that terminate the current block.
 */
function parseNodes(input: string, closingTags: string[]): Node[] {
  const nodes: Node[] = [];
  let pos = 0;

  while (pos < input.length) {
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
    const earliest = Math.min(...candidates);

    if (earliest === Infinity) {
      // No more tags — remaining is text
      if (pos < input.length) {
        nodes.push({ kind: "text", text: input.slice(pos) });
      }
      break;
    }

    // Text before the tag
    if (earliest > pos) {
      nodes.push({ kind: "text", text: input.slice(pos, earliest) });
    }

    if (earliest === commentStart) {
      // Comment: {# ... #}
      const endIdx = input.indexOf("#}", earliest + 2);
      if (endIdx === -1) {
        throw new TemplateSyntaxError("unclosed comment {#");
      }
      nodes.push({ kind: "comment" });
      pos = endIdx + 2;
    } else if (earliest === exprStart) {
      // Expression: {{ ... }}
      const [expr, endPos, trimBefore, trimAfter] = parseExpression(
        input,
        earliest,
      );
      nodes.push({ kind: "expr", expr, trimBefore, trimAfter });
      pos = endPos;
    } else {
      // Statement: {% ... %}
      const [tag, endPos, trimBefore, trimAfter] = parseStatement(
        input,
        earliest,
      );
      // Handle {%- trim: strip trailing whitespace from previous text
      if (trimBefore && nodes.length > 0) {
        const last = nodes[nodes.length - 1]!;
        if (last.kind === "text") {
          nodes[nodes.length - 1] = {
            kind: "text",
            text: last.text.replace(/\s+$/, ""),
          };
        }
      }
      const [newNodes, newPos] = handleStatement(tag, input, endPos, trimAfter);
      for (const n of newNodes) {
        nodes.push(n);
      }
      pos = newPos;
    }
  }

  return nodes;
}

/** Parse `{{ expr }}` and return `[expr, endPos, trimBefore, trimAfter]`. */
function parseExpression(
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
  return [expr, endIdx + 2, trimBefore, trimAfter];
}

/** Parse `{% tag %}` and return `[tag, endPos, trimBefore, trimAfter]`. */
function parseStatement(
  input: string,
  start: number,
): [string, number, boolean, boolean] {
  const trimBefore = input[start + 2] === "-";
  const offset = trimBefore ? 3 : 2;
  const endIdx = input.indexOf("%}", start + offset);
  if (endIdx === -1) {
    throw new TemplateSyntaxError("unclosed statement {%");
  }
  let tag = input.slice(start + offset, endIdx).trim();
  let trimAfter = false;
  if (tag.endsWith("-")) {
    trimAfter = true;
    tag = tag.slice(0, -1).trim();
  }
  return [tag, endIdx + 2, trimBefore, trimAfter];
}

/** Try to match a closing tag at the current position. */
function tryMatchClosingTag(
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
    if (tagContent === ct || tagContent.startsWith(ct + " ")) {
      return ct;
    }
  }
  return null;
}

/** Handle a parsed statement tag and return `[nodes, newPos]`. */
function handleStatement(
  tag: string,
  input: string,
  afterTag: number,
  trimAfter: boolean,
): [Node[], number] {
  let bodyStart = afterTag;
  if (trimAfter) {
    // -%} strips all whitespace after the tag (through next newline)
    while (bodyStart < input.length && /\s/.test(input[bodyStart]!)) {
      bodyStart++;
    }
  } else {
    // Default: consume trailing newline after the opening statement tag —
    // matches Rust's parser which strips '\n' after standalone tags.
    if (input[bodyStart] === "\n") {
      bodyStart++;
    } else if (input[bodyStart] === "\r" && input[bodyStart + 1] === "\n") {
      bodyStart += 2;
    }
  }

  // For loop
  if (tag.startsWith("for ")) {
    return handleFor(tag, input, bodyStart);
  }

  // If/elif/else
  if (tag.startsWith("if ")) {
    return handleIf(tag, input, bodyStart);
  }

  // Match
  if (tag.startsWith("match ")) {
    return handleMatch(tag, input, bodyStart);
  }

  // Raw block
  if (tag === "raw" || tag.startsWith("raw=")) {
    return handleRaw(tag, input, bodyStart);
  }

  // Include
  if (tag.startsWith("include ")) {
    return handleInclude(tag, afterTag);
  }

  // Inline template definition
  if (tag.startsWith("tmpl ")) {
    return handleTmpl(tag, input, bodyStart);
  }

  // Unknown tag — possibly a closing tag handled by parent
  return [[], afterTag];
}

// ---------------------------------------------------------------------------
// Statement handlers
// ---------------------------------------------------------------------------

function handleFor(
  tag: string,
  input: string,
  afterTag: number,
): [Node[], number] {
  const match = /^for\s+(\w+)\s+in\s+(.+)$/.exec(tag);
  if (!match || !match[1] || !match[2]) {
    throw new TemplateSyntaxError(`invalid for loop: '${tag}'`);
  }

  const [body, endPos, closingTag] = parseBlockWithClosing(input, afterTag, [
    "/for",
    "else",
  ]);

  let elseBody: Node[] | undefined;
  let finalPos = endPos;

  if (closingTag === "else") {
    const [elBody, elEndPos] = parseBlock(input, endPos, ["/for"]);
    elseBody = elBody;
    finalPos = elEndPos;
  }

  return [
    [
      {
        kind: "for",
        binding: match[1],
        iterExpr: match[2].trim(),
        body,
        elseBody,
      },
    ],
    finalPos,
  ];
}

function handleIf(
  tag: string,
  input: string,
  afterTag: number,
): [Node[], number] {
  const condition = tag.slice(3).trim();
  const branches: IfBranch[] = [];
  let elseBody: Node[] | undefined;

  let pos = afterTag;
  let currentCondition = condition;

  while (true) {
    const [body, endPos, closingTag, closingContent] = parseBlockWithClosing(
      input,
      pos,
      ["/if", "elif", "else"],
    );

    if (closingTag === "elif") {
      branches.push({ condition: currentCondition, body });
      // The condition is in closingContent (everything after "elif ")
      currentCondition = (closingContent ?? "").trim();
      pos = endPos;
    } else if (closingTag === "else") {
      branches.push({ condition: currentCondition, body });
      const [elBody, elEndPos] = parseBlock(input, endPos, ["/if"]);
      elseBody = elBody;
      pos = elEndPos;
      break;
    } else {
      // /if
      branches.push({ condition: currentCondition, body });
      pos = endPos;
      break;
    }
  }

  return [[{ kind: "if", branches, elseBody }], pos];
}

function handleMatch(
  tag: string,
  input: string,
  afterTag: number,
): [Node[], number] {
  const tagContent = tag.slice(6).trim();

  // Check for inline match: `match expr case Variant`
  const inlineMatch = /^(\S+)\s+case\s+(\w+)$/.exec(tagContent);
  if (inlineMatch && inlineMatch[1] && inlineMatch[2]) {
    const [body, endPos] = parseBlock(input, afterTag, ["/match"]);
    return [
      [
        {
          kind: "match",
          expr: inlineMatch[1],
          arms: [],
          defaultArm: undefined,
          inlineGuard: { variant: inlineMatch[2], body },
        },
      ],
      endPos,
    ];
  }

  const expr = tagContent;
  const arms: MatchArm[] = [];
  let defaultArm: Node[] | undefined;
  let pos = afterTag;

  // Parse case arms
  while (true) {
    const [body, endPos, closingTag, closingContent] = parseBlockWithClosing(
      input,
      pos,
      ["/match", "case", "default"],
    );

    if (closingTag === "case") {
      if (body.length > 0 || arms.length === 0) {
        // This is the body of the previous arm, or we're at the first arm
        if (arms.length > 0) {
          arms[arms.length - 1]!.body = body;
        }
      }
      // Parse the case variants
      const variants = (closingContent ?? "")
        .split("|")
        .map((v) => v.trim())
        .filter((v) => v.length > 0);
      arms.push({ variants, body: [] });
      pos = endPos;
    } else if (closingTag === "default") {
      if (arms.length > 0) {
        arms[arms.length - 1]!.body = body;
      }
      const [defaultBody, defaultEndPos] = parseBlock(input, endPos, [
        "/match",
      ]);
      defaultArm = defaultBody;
      pos = defaultEndPos;
      break;
    } else {
      // /match
      if (arms.length > 0) {
        arms[arms.length - 1]!.body = body;
      }
      pos = endPos;
      break;
    }
  }

  return [[{ kind: "match", expr, arms, defaultArm }], pos];
}

function handleRaw(
  tag: string,
  input: string,
  afterTag: number,
): [Node[], number] {
  let closeTag = "/raw";
  if (tag.startsWith("raw=")) {
    const delim = tag.slice(4).trim();
    closeTag = `/${delim}`;
  }

  // Find the closing raw tag
  const closeStr = `{% ${closeTag} %}`;
  const closeStrTrimmed = `{%${closeTag}%}`;
  let endIdx = input.indexOf(closeStr, afterTag);
  if (endIdx === -1) {
    endIdx = input.indexOf(closeStrTrimmed, afterTag);
  }
  // Also try with blockquote prefix stripped
  if (endIdx === -1) {
    const lines = input.slice(afterTag).split("\n");
    let offset = afterTag;
    for (const line of lines) {
      const trimmed = line.trim();
      if (
        trimmed === `> {% ${closeTag} %}` ||
        trimmed === `{% ${closeTag} %}`
      ) {
        endIdx = offset;
        break;
      }
      offset += line.length + 1;
    }
  }

  if (endIdx === -1) {
    throw new TemplateSyntaxError(
      `unclosed raw block (expected {% ${closeTag} %})`,
    );
  }

  const rawText = input.slice(afterTag, endIdx);
  // Find end of closing tag
  const closeEnd = input.indexOf("%}", endIdx);
  const finalEnd = closeEnd === -1 ? endIdx : closeEnd + 2;

  return [[{ kind: "raw", text: rawText }], finalEnd];
}

function handleInclude(tag: string, afterTag: number): [Node[], number] {
  const rest = tag.slice(8).trim();

  // Parse: [name](path) with ... / for ...
  const linkMatch = /^\[([^\]]+)\]\(([^)]+)\)(.*)$/.exec(rest);
  let name: string;
  let path: string | undefined;
  let remaining: string;

  if (linkMatch && linkMatch[1] && linkMatch[2]) {
    name = linkMatch[1];
    path = linkMatch[2];
    remaining = (linkMatch[3] ?? "").trim();
  } else {
    // Bare name include (inline template)
    const parts = rest.split(/\s+/);
    name = parts[0] ?? rest;
    remaining = parts.slice(1).join(" ").trim();
  }

  const withMappings = new Map<string, string>();
  let forBinding: string | undefined;
  let forExpr: string | undefined;

  // Parse `for binding in expr`
  const forMatch = /^for\s+(\w+)\s+in\s+(\S+)(.*)$/.exec(remaining);
  if (forMatch && forMatch[1] && forMatch[2]) {
    forBinding = forMatch[1];
    forExpr = forMatch[2];
    remaining = (forMatch[3] ?? "").trim();
  }

  // Parse `with key=val, key=val`
  if (remaining.startsWith("with ")) {
    remaining = remaining.slice(5).trim();
  }
  if (remaining.length > 0) {
    const pairs = remaining.split(",");
    for (const pair of pairs) {
      const eqIdx = pair.indexOf("=");
      if (eqIdx !== -1) {
        const key = pair.slice(0, eqIdx).trim();
        const val = pair.slice(eqIdx + 1).trim();
        withMappings.set(key, val);
      }
    }
  }

  return [
    [{ kind: "include", name, path, withMappings, forBinding, forExpr }],
    afterTag,
  ];
}

function handleTmpl(
  tag: string,
  input: string,
  afterTag: number,
): [Node[], number] {
  const name = tag.slice(5).trim();
  const [, endPos] = parseBlock(input, afterTag, ["/tmpl"]);
  const source = input.slice(afterTag, findBlockEnd(input, afterTag, "/tmpl"));
  return [[{ kind: "tmpl", name, source }], endPos];
}

// ---------------------------------------------------------------------------
// Block parsing helpers
// ---------------------------------------------------------------------------

/** Parse until a closing tag, returning `[body, endPos]`. */
function parseBlock(
  input: string,
  start: number,
  closingTags: string[],
): [Node[], number] {
  const [body, endPos] = parseBlockWithClosing(input, start, closingTags);
  return [body, endPos];
}

/**
 * Parse until any of the closing tags, returning body, end position,
 * which tag was matched, and the content after the tag keyword.
 */
function parseBlockWithClosing(
  input: string,
  start: number,
  closingTags: string[],
): [Node[], number, string | undefined, string | undefined] {
  let pos = start;
  const nodes: Node[] = [];

  while (pos < input.length) {
    // Look for next {% tag %}
    const stmtIdx = findNextTag(input, pos);
    if (stmtIdx === -1) {
      if (pos < input.length) {
        parseInlineContent(input, pos, input.length, nodes);
      }
      return [nodes, input.length, undefined, undefined];
    }

    // Check if it's a closing tag or branch tag
    const [tagContent, tagEnd] = extractTagContent(input, stmtIdx);
    const cleanTag = tagContent.replace(/^-/, "").replace(/-$/, "").trim();

    for (const ct of closingTags) {
      if (cleanTag === ct || cleanTag.startsWith(ct + " ")) {
        // Parse any expressions/comments in the text before the closing tag
        const textEnd = findTagStart(input, stmtIdx);
        if (textEnd > pos) {
          parseInlineContent(input, pos, textEnd, nodes);
        }
        // Handle {%- on closing tag: trim trailing whitespace from last text node
        const closingTrimBefore = tagContent.startsWith("-");
        if (closingTrimBefore && nodes.length > 0) {
          const last = nodes[nodes.length - 1]!;
          if (last.kind === "text") {
            nodes[nodes.length - 1] = {
              kind: "text",
              text: last.text.replace(/\s+$/, ""),
            };
          }
        }
        const content = cleanTag.startsWith(ct + " ")
          ? cleanTag.slice(ct.length + 1).trim()
          : undefined;
        // Handle -%} on closing tag: strip all whitespace after
        const closingTrimAfter = tagContent.endsWith("-");
        let adjustedEnd = tagEnd;
        if (closingTrimAfter) {
          while (adjustedEnd < input.length && /\s/.test(input[adjustedEnd]!)) {
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

    // Not a closing tag — parse it as content
    const exprIdx = input.indexOf("{{", pos);
    const commentIdx = input.indexOf("{#", pos);

    // Find earliest between current stmt, expr, comment
    const earliest = Math.min(
      stmtIdx,
      exprIdx >= 0 ? exprIdx : Infinity,
      commentIdx >= 0 ? commentIdx : Infinity,
    );

    if (earliest > pos) {
      nodes.push({ kind: "text", text: input.slice(pos, earliest) });
    }

    if (earliest === commentIdx && commentIdx >= 0) {
      const endComment = input.indexOf("#}", commentIdx + 2);
      pos = endComment === -1 ? commentIdx + 2 : endComment + 2;
      nodes.push({ kind: "comment" });
    } else if (earliest === exprIdx && exprIdx >= 0 && exprIdx < stmtIdx) {
      const [expr, endPos, trimBefore, trimAfter] = parseExpression(
        input,
        exprIdx,
      );
      nodes.push({ kind: "expr", expr, trimBefore, trimAfter });
      pos = endPos;
    } else {
      // It's a nested statement tag — handle recursively
      const [, endPos, _trimBefore, trimAfter] = parseStatement(input, stmtIdx);
      const innerTag = input
        .slice(stmtIdx + 2, endPos - 2)
        .trim()
        .replace(/^-/, "")
        .replace(/-$/, "")
        .trim();
      const [innerNodes, innerEnd] = handleStatement(
        innerTag,
        input,
        endPos,
        trimAfter,
      );
      for (const n of innerNodes) {
        nodes.push(n);
      }
      pos = innerEnd;
    }
  }

  return [nodes, pos, undefined, undefined];
}

/**
 * Parse inline content (expressions and comments) within a text range.
 *
 * Scans `input[start..end)` for `{{ }}` and `{# #}` delimiters and
 * pushes the resulting nodes (text + expr + comment) into `out`.
 */
function parseInlineContent(
  input: string,
  start: number,
  end: number,
  out: Node[],
): void {
  let pos = start;
  while (pos < end) {
    // Find next {{ or {#
    let exprIdx = -1;
    let commentIdx = -1;
    for (let i = pos; i < end - 1; i++) {
      if (input[i] === "{") {
        if (input[i + 1] === "{" && exprIdx === -1) {
          exprIdx = i;
          break;
        }
        if (input[i + 1] === "#" && commentIdx === -1) {
          commentIdx = i;
          break;
        }
      }
    }

    const nextExpr = exprIdx >= 0 ? exprIdx : Infinity;
    const nextComment = commentIdx >= 0 ? commentIdx : Infinity;
    const earliest = Math.min(nextExpr, nextComment);

    if (earliest === Infinity) {
      // No more delimiters — rest is text
      if (pos < end) {
        out.push({ kind: "text", text: input.slice(pos, end) });
      }
      break;
    }

    // Text before the delimiter
    if (earliest > pos) {
      out.push({ kind: "text", text: input.slice(pos, earliest) });
    }

    if (earliest === commentIdx) {
      const closeIdx = input.indexOf("#}", earliest + 2);
      if (closeIdx === -1 || closeIdx >= end) {
        out.push({ kind: "text", text: input.slice(earliest, end) });
        break;
      }
      out.push({ kind: "comment" });
      pos = closeIdx + 2;
    } else {
      // Expression {{ ... }}
      const [expr, endPos, trimBefore, trimAfter] = parseExpression(
        input,
        earliest,
      );
      if (endPos > end) {
        // Expression extends beyond our range — push as text
        out.push({ kind: "text", text: input.slice(earliest, end) });
        break;
      }
      out.push({ kind: "expr", expr, trimBefore, trimAfter });
      pos = endPos;
    }
  }
}

/** Find the next `{%` that's not inside `{{` or `{#`. */
function findNextTag(input: string, from: number): number {
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
function extractTagContent(input: string, start: number): [string, number] {
  const endIdx = input.indexOf("%}", start + 2);
  if (endIdx === -1) {
    throw new TemplateSyntaxError("unclosed statement {%");
  }
  return [input.slice(start + 2, endIdx).trim(), endIdx + 2];
}

/** Find the start position of a tag (accounting for blockquote prefix). */
function findTagStart(input: string, tagIdx: number): number {
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
function findBlockEnd(input: string, start: number, closeTag: string): number {
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

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/** Rendering options. */
export interface RenderOptions {
  /** Inline template definitions available for `{% include %}`. */
  inlineTemplates?: Map<string, { params: Map<string, unknown>; body: string }>;
  /** Template loader for file-based `{% include %}`. */
  templateLoader?: (
    path: string,
    basePath?: string,
  ) => [Node[], Map<string, Value>] | undefined;
  /** Maximum include depth. */
  maxIncludeDepth?: number;
}

/**
 * Render a list of parsed nodes against a scope.
 */
export function renderNodes(
  nodes: readonly Node[],
  scope: Scope,
  options?: RenderOptions,
): string {
  const parts: string[] = [];

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i]!;
    switch (node.kind) {
      case "text":
        parts.push(node.text);
        break;

      case "expr": {
        if (node.trimBefore && parts.length > 0) {
          const last = parts[parts.length - 1]!;
          parts[parts.length - 1] = last.replace(/\s+$/, "");
        }
        const val = evaluateExpression(node.expr, scope);
        const rendered = display(val);
        parts.push(rendered);
        if (node.trimAfter) {
          // Trim leading whitespace from the next text node without
          // mutating the AST (which would corrupt subsequent renders).
          if (i + 1 < nodes.length && nodes[i + 1]!.kind === "text") {
            const next = nodes[i + 1]! as { kind: "text"; text: string };
            parts.push(next.text.replace(/^\s+/, ""));
            i++; // skip the next node — we already handled it
          }
        }
        break;
      }

      case "comment":
        // Comments produce no output
        break;

      case "for": {
        const listVal = evaluateExpression(node.iterExpr, scope);
        if (listVal.type !== "list") {
          throw new TemplateSyntaxError(
            `for loop requires a list, got ${listVal.type}`,
          );
        }
        if (listVal.items.length === 0) {
          if (node.elseBody) {
            parts.push(renderNodes(node.elseBody, scope, options));
          }
        } else {
          for (let idx = 0; idx < listVal.items.length; idx++) {
            const item = listVal.items[idx]!;
            const layer = scope.pushLayer();
            layer.set(node.binding, item);
            scope.setLoopMeta(node.binding, { index: idx });
            parts.push(renderNodes(node.body, scope, options));
            scope.popLayer();
          }
        }
        break;
      }

      case "if": {
        let matched = false;
        for (const branch of node.branches) {
          if (evaluateCondition(branch.condition, scope)) {
            parts.push(renderNodes(branch.body, scope, options));
            matched = true;
            break;
          }
        }
        if (!matched && node.elseBody) {
          parts.push(renderNodes(node.elseBody, scope, options));
        }
        break;
      }

      case "match": {
        if (node.inlineGuard) {
          // Use resolvePath to get raw variant (not unwrapped)
          const val = scope.resolvePath(node.expr.trim());
          const variantName = getVariantName(val);
          if (variantName === node.inlineGuard.variant) {
            parts.push(renderNodes(node.inlineGuard.body, scope, options));
          }
        } else {
          // Use resolvePath to get raw variant (not unwrapped)
          const val = scope.resolvePath(node.expr.trim());
          const variantName = getVariantName(val);

          let matched = false;
          for (const arm of node.arms) {
            if (arm.variants.includes(variantName)) {
              parts.push(renderNodes(arm.body, scope, options));
              matched = true;
              break;
            }
          }
          if (!matched && node.defaultArm) {
            parts.push(renderNodes(node.defaultArm, scope, options));
          }
        }
        break;
      }

      case "raw":
        parts.push(node.text);
        break;

      case "include": {
        const depth = options?.maxIncludeDepth ?? 16;
        if (depth <= 0) {
          throw new TemplateSyntaxError("maximum include depth exceeded");
        }
        const childOpts: RenderOptions = {
          ...options,
          maxIncludeDepth: depth - 1,
        };

        // Try inline templates first
        if (!node.path && options?.inlineTemplates?.has(node.name)) {
          const inline = options.inlineTemplates.get(node.name)!;
          // Create child scope with mapped params
          const childValues = new Map<string, Value>(scope.allValues());
          for (const [targetKey, sourceExpr] of node.withMappings) {
            childValues.set(targetKey, evaluateExpression(sourceExpr, scope));
          }
          const childScope = new Scope(childValues, new Map());
          // Parse inline template body and render
          const inlineNodes = parseBody(inline.body);
          parts.push(renderNodes(inlineNodes, childScope, childOpts));
          break;
        }

        // File-based include
        if (node.path && options?.templateLoader) {
          const loaded = options.templateLoader(node.path, undefined);
          if (!loaded) {
            throw new TemplateSyntaxError(
              `failed to load included template '${node.name}' from '${node.path}'`,
            );
          }
          const [includedNodes, includedConsts] = loaded;

          if (node.forBinding && node.forExpr) {
            // {% include [name](path) for item in items with ... %}
            const listVal = evaluateExpression(node.forExpr, scope);
            if (listVal.type !== "list") {
              throw new TemplateSyntaxError(
                `include for-loop requires a list, got ${listVal.type}`,
              );
            }
            const results: string[] = [];
            for (const item of listVal.items) {
              const childValues = new Map<string, Value>(scope.allValues());
              for (const [k, v] of includedConsts) {
                childValues.set(k, v);
              }
              // Map "with" mappings — evaluate each source expr
              for (const [targetKey, sourceExpr] of node.withMappings) {
                childValues.set(
                  targetKey,
                  evaluateExpression(sourceExpr, scope),
                );
              }
              // The for-binding variable maps to the current item
              childValues.set(node.forBinding, item);
              const childScope = new Scope(childValues, new Map());
              results.push(renderNodes(includedNodes, childScope, childOpts));
            }
            parts.push(results.join(""));
          } else {
            // Simple include: {% include [name](path) with key=val %}
            const childValues = new Map<string, Value>(scope.allValues());
            for (const [k, v] of includedConsts) {
              childValues.set(k, v);
            }
            for (const [targetKey, sourceExpr] of node.withMappings) {
              childValues.set(targetKey, evaluateExpression(sourceExpr, scope));
            }
            const childScope = new Scope(childValues, new Map());
            parts.push(renderNodes(includedNodes, childScope, childOpts));
          }
        }
        break;
      }

      case "tmpl":
        // Template definitions are stored at parse time, not rendered inline
        break;
    }
  }

  return parts.join("");
}

// ---------------------------------------------------------------------------
// Expression evaluation
// ---------------------------------------------------------------------------

/** Evaluate a template expression (possibly with filters). */
export function evaluateExpression(expr: string, scope: Scope): Value {
  // Fast path: no pipe means no filters (the vast majority of expressions)
  const pipeIdx = expr.indexOf("|");
  if (pipeIdx === -1) {
    // No filters — resolve directly, trim only if needed
    const trimmed =
      expr.charCodeAt(0) === 32 || expr.charCodeAt(expr.length - 1) === 32
        ? expr.trim()
        : expr;
    return resolveExpr(trimmed, scope);
  }

  const trimmed = expr.trim();
  // Split by pipe, respecting parentheses
  const parts = splitPipes(trimmed);
  const pathPart = parts[0]!.trim();

  let value = resolveExpr(pathPart, scope);

  // Apply filter chain
  for (let i = 1; i < parts.length; i++) {
    const filterStr = parts[i]!.trim();
    const [filterName, filterArgs] = parseFilter(filterStr);
    value = applyFilter(value, filterName, filterArgs);
  }

  return value;
}

/** Pre-compiled regex for function calls. */
const FUNC_CALL_RE = /^(\w+)\((.+)\)$/;

/** Resolve a single expression (path, function call, or literal). */
function resolveExpr(expr: string, scope: Scope): Value {
  // Fast path: if expr doesn't end with ')' it can't be a function call
  if (expr.charCodeAt(expr.length - 1) !== 41 /* ')' */) {
    return unwrapOption(scope.resolvePath(expr));
  }

  // Function calls: idx(binding), len(expr), kind(expr)
  const funcMatch = FUNC_CALL_RE.exec(expr);
  if (funcMatch && funcMatch[1] && funcMatch[2]) {
    const funcName = funcMatch[1];
    const arg = funcMatch[2].trim();
    switch (funcName) {
      case "idx": {
        const meta = scope.getLoopMeta(arg);
        if (meta === undefined) {
          throw new TemplateSyntaxError(
            `idx() requires active loop binding '${arg}'`,
          );
        }
        return int(meta.index);
      }
      case "len": {
        const val = scope.resolvePath(arg);
        if (val.type === "list") return int(val.items.length);
        if (val.type === "str") return int(val.value.length);
        if (val.type === "dict") return int(val.fields.size);
        throw new TemplateSyntaxError(
          `len() requires a list, string, or struct, got ${typeName(val)}`,
        );
      }
      case "kind": {
        const val = scope.resolvePath(arg);
        const name = getVariantName(val);
        return str(name);
      }
      case "has": {
        const val = scope.resolvePath(arg);
        // Check if the value is a None variant (option is absent)
        if (val.type === "str" && val.value === "None") {
          return { type: "bool", value: false };
        }
        if (val.type === "dict") {
          const tag = val.fields.get(ENUM_TAG_KEY);
          if (tag && tag.type === "str" && tag.value === "None") {
            return { type: "bool", value: false };
          }
          if (tag && tag.type === "str" && tag.value === "Some") {
            return { type: "bool", value: true };
          }
        }
        // Not an option type — always truthy (has a value)
        return { type: "bool", value: true };
      }
      default:
        throw new TemplateSyntaxError(`unknown function '${funcName}'`);
    }
  }

  // Dotted path (or expression that looked like a function but wasn't)
  return unwrapOption(scope.resolvePath(expr));
}

/**
 * Unwrap option Some values for display: Some(val=X) → X.
 * Returns the value unchanged if it's not a Some variant.
 */
function unwrapOption(val: Value): Value {
  if (
    val.type === "dict" &&
    val.fields.get(ENUM_TAG_KEY)?.type === "str" &&
    (val.fields.get(ENUM_TAG_KEY) as { type: "str"; value: string }).value ===
      "Some" &&
    val.fields.has("val") &&
    val.fields.size === 2
  ) {
    return val.fields.get("val")!;
  }
  return val;
}

/** Get the variant name from an enum value. */
function getVariantName(val: Value): string {
  if (val.type === "str") return val.value;
  if (val.type === "dict") {
    const tag = val.fields.get(ENUM_TAG_KEY);
    if (tag && tag.type === "str") return tag.value;
    throw new TemplateSyntaxError(
      "kind() requires an enum value (struct with variant tag)",
    );
  }
  throw new TemplateSyntaxError(
    `kind() requires an enum value, got ${val.type}`,
  );
}

/** Evaluate a condition expression for `{% if %}`. */
function evaluateCondition(condition: string, scope: Scope): boolean {
  const trimmed = condition.trim();

  // Comparison operators
  const ops = ["==", "!=", "<=", ">=", "<", ">"] as const;
  for (const op of ops) {
    const idx = trimmed.indexOf(` ${op} `);
    if (idx !== -1) {
      const left = evaluateExpression(trimmed.slice(0, idx).trim(), scope);
      const right = evaluateConditionOperand(
        trimmed.slice(idx + op.length + 2).trim(),
        scope,
      );
      return compareValues(left, right, op);
    }
  }

  // Truthiness check
  const val = evaluateExpression(trimmed, scope);
  return isTruthy(val);
}

/** Evaluate a condition operand (may be a literal or expression). */
function evaluateConditionOperand(operand: string, scope: Scope): Value {
  const trimmed = operand.trim();

  // String literal
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return str(trimmed.slice(1, -1));
  }

  // Boolean literals
  if (trimmed === "true") return { type: "bool", value: true };
  if (trimmed === "false") return { type: "bool", value: false };

  // Number literals
  const num = Number(trimmed);
  if (!isNaN(num)) {
    return Number.isInteger(num) ? int(num) : { type: "float", value: num };
  }

  // Expression
  return evaluateExpression(trimmed, scope);
}

/** Compare two values with a comparison operator. */
function compareValues(
  left: Value,
  right: Value,
  op: "==" | "!=" | "<" | ">" | "<=" | ">=",
): boolean {
  const l = coerceForComparison(left);
  const r = coerceForComparison(right);

  switch (op) {
    case "==":
      return l === r;
    case "!=":
      return l !== r;
    case "<":
      return l < r;
    case ">":
      return l > r;
    case "<=":
      return l <= r;
    case ">=":
      return l >= r;
  }
}

/** Coerce a value to a primitive for comparison. */
function coerceForComparison(v: Value): string | number | boolean {
  switch (v.type) {
    case "str":
      return v.value;
    case "bool":
      return v.value;
    case "int":
      return v.value;
    case "float":
      return v.value;
    case "list":
      return display(v);
    case "dict":
      return display(v);
  }
}

/** Split expression by pipe, respecting parentheses. Uses slice instead of char-by-char concatenation. */
function splitPipes(expr: string): string[] {
  const result: string[] = [];
  let depth = 0;
  let start = 0;

  for (let i = 0; i < expr.length; i++) {
    const ch = expr.charCodeAt(i);
    if (ch === 40 /* ( */ || ch === 60 /* < */) {
      depth++;
    } else if (ch === 41 /* ) */ || ch === 62 /* > */) {
      depth--;
    } else if (ch === 124 /* | */ && depth === 0) {
      result.push(expr.slice(start, i));
      start = i + 1;
    }
  }

  if (start < expr.length) {
    result.push(expr.slice(start));
  }

  return result;
}
