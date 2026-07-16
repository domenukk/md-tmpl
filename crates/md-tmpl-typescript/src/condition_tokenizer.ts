/**
 * Shared condition tokenizer for md-tmpl template expressions.
 *
 * This module provides a single tokenizer implementation used by both
 * the evaluator (Value-based) and the direct renderer (plain JS values).
 *
 * @module
 */

import { TemplateSyntaxError } from "./errors.js";
import {
  DOT,
  KW_CASE,
  KW_IN,
  KW_MATCH,
  LIT_FALSE,
  LIT_TRUE,
  OP_AND,
  OP_EQ,
  OP_GE,
  OP_GT,
  OP_LE,
  OP_LT,
  OP_NE,
  OP_NOT,
  OP_OR,
  PAREN_CLOSE,
  PAREN_OPEN,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  BACKSLASH,
  VARIANT_SEP,
} from "./consts.js";

// ---------------------------------------------------------------------------
// Condition tokenizer
// ---------------------------------------------------------------------------

// Using a regular enum (not const enum) so values can be re-exported
// across module boundaries without issues.
export enum TokKind {
  And = "&&",
  Or = "||",
  Not = "!",
  LParen = "(",
  RParen = ")",
  Eq = "==",
  Ne = "!=",
  Le = "<=",
  Ge = ">=",
  Lt = "<",
  Gt = ">",
  In = "in",
  Pipe = "|",
  Match = "match",
  Case = "case",

  Ident = "IDENT",
  StrLit = "STR",
  NumLit = "NUM",
  BoolLit = "BOOL",
}

export interface Token {
  kind: TokKind;
  value: string;
}

export function tokenizeCondition(input: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  const len = input.length;

  while (i < len) {
    // Skip whitespace
    if (input.charCodeAt(i) === 32 || input.charCodeAt(i) === 9) {
      i++;
      continue;
    }

    // Two-char operators
    if (i + 1 < len) {
      const two = input.slice(i, i + 2);
      if (two === OP_AND) {
        tokens.push({ kind: TokKind.And, value: two });
        i += 2;
        continue;
      }
      if (two === OP_OR) {
        tokens.push({ kind: TokKind.Or, value: two });
        i += 2;
        continue;
      }
      if (two === OP_EQ) {
        tokens.push({ kind: TokKind.Eq, value: two });
        i += 2;
        continue;
      }
      if (two === OP_NE) {
        tokens.push({ kind: TokKind.Ne, value: two });
        i += 2;
        continue;
      }
      if (two === OP_LE) {
        tokens.push({ kind: TokKind.Le, value: two });
        i += 2;
        continue;
      }
      if (two === OP_GE) {
        tokens.push({ kind: TokKind.Ge, value: two });
        i += 2;
        continue;
      }
    }

    const ch = input.charAt(i);

    // Single-char operators
    if (ch === OP_NOT) {
      tokens.push({ kind: TokKind.Not, value: ch });
      i++;
      continue;
    }
    if (ch === PAREN_OPEN) {
      tokens.push({ kind: TokKind.LParen, value: ch });
      i++;
      continue;
    }
    if (ch === PAREN_CLOSE) {
      tokens.push({ kind: TokKind.RParen, value: ch });
      i++;
      continue;
    }
    if (ch === OP_LT) {
      tokens.push({ kind: TokKind.Lt, value: ch });
      i++;
      continue;
    }
    if (ch === OP_GT) {
      tokens.push({ kind: TokKind.Gt, value: ch });
      i++;
      continue;
    }
    // Pipe operator — only used inside match-case for multi-variant
    if (ch === VARIANT_SEP) {
      tokens.push({ kind: TokKind.Pipe, value: ch });
      i++;
      continue;
    }

    // String literals
    if (ch === QUOTE_DOUBLE || ch === QUOTE_SINGLE) {
      const quote = ch;
      let j = i + 1;
      while (j < len && input[j] !== quote) {
        // A backslash escapes the next char, so an escaped quote does not
        // close the literal (mirrors the Rust tokenizer / split_at_depth_zero).
        if (input[j] === BACKSLASH && j + 1 < len) j += 2;
        else j++;
      }
      if (j >= len) {
        throw new TemplateSyntaxError(
          `unclosed string literal in condition: ${input.slice(i)}`,
        );
      }
      tokens.push({ kind: TokKind.StrLit, value: input.slice(i, j + 1) });
      i = j + 1;
      continue;
    }

    // Number literals (including negative: only if preceded by an operator or start)
    const prevTok = tokens[tokens.length - 1];
    if (
      (ch >= "0" && ch <= "9") ||
      (ch === "-" &&
        i + 1 < len &&
        input.charAt(i + 1) >= "0" &&
        input.charAt(i + 1) <= "9" &&
        (prevTok === undefined || isOperatorToken(prevTok)))
    ) {
      let j = i;
      if (ch === "-") j++;
      while (
        j < len &&
        ((input.charAt(j) >= "0" && input.charAt(j) <= "9") || input[j] === DOT)
      )
        j++;
      tokens.push({ kind: TokKind.NumLit, value: input.slice(i, j) });
      i = j;
      continue;
    }

    // Identifiers, keywords, function calls, dotted paths
    if (isIdentStart(ch)) {
      let j = i;
      while (j < len && isIdentChar(input.charAt(j))) j++;

      const word = input.slice(i, j);

      // Check for keywords
      if (word === LIT_TRUE || word === LIT_FALSE) {
        tokens.push({ kind: TokKind.BoolLit, value: word });
        i = j;
        continue;
      }
      if (word === KW_IN) {
        tokens.push({ kind: TokKind.In, value: word });
        i = j;
        continue;
      }
      if (word === KW_MATCH) {
        tokens.push({ kind: TokKind.Match, value: word });
        i = j;
        continue;
      }
      if (word === KW_CASE) {
        tokens.push({ kind: TokKind.Case, value: word });
        i = j;
        continue;
      }

      // Check for dotted path or function call
      while (j < len && input[j] === DOT) {
        j++;
        while (j < len && isIdentChar(input.charAt(j))) j++;
      }
      // Function call: consume (args)
      if (j < len && input[j] === PAREN_OPEN) {
        let depth = 1;
        j++;
        while (j < len && depth > 0) {
          if (input[j] === PAREN_OPEN) depth++;
          else if (input[j] === PAREN_CLOSE) depth--;
          j++;
        }
      }
      tokens.push({ kind: TokKind.Ident, value: input.slice(i, j) });
      i = j;
      continue;
    }

    throw new TemplateSyntaxError(`unexpected character '${ch}' in condition`);
  }

  return tokens;
}

export function isIdentStart(ch: string): boolean {
  return (ch >= "a" && ch <= "z") || (ch >= "A" && ch <= "Z") || ch === "_";
}

export function isIdentChar(ch: string): boolean {
  return isIdentStart(ch) || (ch >= "0" && ch <= "9");
}

export function isOperatorToken(t: Token): boolean {
  return (
    t.kind !== TokKind.Ident &&
    t.kind !== TokKind.StrLit &&
    t.kind !== TokKind.NumLit &&
    t.kind !== TokKind.BoolLit &&
    t.kind !== TokKind.RParen
  );
}
