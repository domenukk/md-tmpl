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
    const code = input.charCodeAt(i);
    // Skip whitespace
    if (code === 32 /* ' ' */ || code === 9 /* '\t' */) {
      i++;
      continue;
    }

    // Operators starting with &, |, =, !, <, >, (, )
    if (code === 38 /* '&' */) {
      if (input.charCodeAt(i + 1) === 38) {
        tokens.push({ kind: TokKind.And, value: OP_AND });
        i += 2;
        continue;
      }
    } else if (code === 124 /* '|' */) {
      if (input.charCodeAt(i + 1) === 124) {
        tokens.push({ kind: TokKind.Or, value: OP_OR });
        i += 2;
        continue;
      }
      tokens.push({ kind: TokKind.Pipe, value: VARIANT_SEP });
      i++;
      continue;
    } else if (code === 61 /* '=' */) {
      if (input.charCodeAt(i + 1) === 61) {
        tokens.push({ kind: TokKind.Eq, value: OP_EQ });
        i += 2;
        continue;
      }
    } else if (code === 33 /* '!' */) {
      if (input.charCodeAt(i + 1) === 61 /* '=' */) {
        tokens.push({ kind: TokKind.Ne, value: OP_NE });
        i += 2;
        continue;
      }
      tokens.push({ kind: TokKind.Not, value: OP_NOT });
      i++;
      continue;
    } else if (code === 60 /* '<' */) {
      if (input.charCodeAt(i + 1) === 61 /* '=' */) {
        tokens.push({ kind: TokKind.Le, value: OP_LE });
        i += 2;
        continue;
      }
      tokens.push({ kind: TokKind.Lt, value: OP_LT });
      i++;
      continue;
    } else if (code === 62 /* '>' */) {
      if (input.charCodeAt(i + 1) === 61 /* '=' */) {
        tokens.push({ kind: TokKind.Ge, value: OP_GE });
        i += 2;
        continue;
      }
      tokens.push({ kind: TokKind.Gt, value: OP_GT });
      i++;
      continue;
    } else if (code === 40 /* '(' */) {
      tokens.push({ kind: TokKind.LParen, value: PAREN_OPEN });
      i++;
      continue;
    } else if (code === 41 /* ')' */) {
      tokens.push({ kind: TokKind.RParen, value: PAREN_CLOSE });
      i++;
      continue;
    }

    // String literals
    if (code === 34 /* '"' */ || code === 39 /* "'" */) {
      let j = i + 1;
      while (j < len && input.charCodeAt(j) !== code) {
        if (input.charCodeAt(j) === 92 /* '\\' */ && j + 1 < len) j += 2;
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
      (code >= 48 && code <= 57) ||
      (code === 45 /* '-' */ &&
        i + 1 < len &&
        input.charCodeAt(i + 1) >= 48 &&
        input.charCodeAt(i + 1) <= 57 &&
        (prevTok === undefined || isOperatorToken(prevTok)))
    ) {
      let j = i;
      if (code === 45) j++;
      while (j < len) {
        const c = input.charCodeAt(j);
        if ((c >= 48 && c <= 57) || c === 46 /* '.' */) j++;
        else break;
      }
      tokens.push({ kind: TokKind.NumLit, value: input.slice(i, j) });
      i = j;
      continue;
    }

    // Identifiers, keywords, function calls, dotted paths
    if (isIdentStartCode(code)) {
      let j = i + 1;
      while (j < len && isIdentCharCode(input.charCodeAt(j))) j++;

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
      while (j < len && input.charCodeAt(j) === 46 /* '.' */) {
        j++;
        while (j < len && isIdentCharCode(input.charCodeAt(j))) j++;
      }
      // Function call: consume (args)
      if (j < len && input.charCodeAt(j) === 40 /* '(' */) {
        let depth = 1;
        j++;
        while (j < len && depth > 0) {
          const c = input.charCodeAt(j);
          if (c === 40 /* '(' */) depth++;
          else if (c === 41 /* ')' */) depth--;
          j++;
        }
      }
      tokens.push({ kind: TokKind.Ident, value: input.slice(i, j) });
      i = j;
      continue;
    }

    throw new TemplateSyntaxError(
      `unexpected character '${input.charAt(i)}' in condition`,
    );
  }

  return tokens;
}

function isIdentStartCode(code: number): boolean {
  return (
    (code >= 97 && code <= 122) || (code >= 65 && code <= 90) || code === 95
  );
}

function isIdentCharCode(code: number): boolean {
  return isIdentStartCode(code) || (code >= 48 && code <= 57);
}

export function isIdentStart(ch: string): boolean {
  return ch.length > 0 && isIdentStartCode(ch.charCodeAt(0));
}

export function isIdentChar(ch: string): boolean {
  return ch.length > 0 && isIdentCharCode(ch.charCodeAt(0));
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
