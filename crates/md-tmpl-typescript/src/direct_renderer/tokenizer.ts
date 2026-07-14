/**
 * Tokenizer for direct-renderer condition expressions.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
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
  VARIANT_SEP,
} from "../consts.js";

// ---------------------------------------------------------------------------
// Direct condition tokenizer (same grammar as evaluator.ts)
// ---------------------------------------------------------------------------

export const enum DirectTokKind {
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

export interface DirectToken {
  kind: DirectTokKind;
  value: string;
}

export function tokenizeDirectCondition(input: string): DirectToken[] {
  const tokens: DirectToken[] = [];
  let i = 0;
  const len = input.length;

  while (i < len) {
    if (input.charCodeAt(i) === 32 || input.charCodeAt(i) === 9) {
      i++;
      continue;
    }

    if (i + 1 < len) {
      const two = input.slice(i, i + 2);
      if (two === OP_AND) {
        tokens.push({ kind: DirectTokKind.And, value: two });
        i += 2;
        continue;
      }
      if (two === OP_OR) {
        tokens.push({ kind: DirectTokKind.Or, value: two });
        i += 2;
        continue;
      }
      if (two === OP_EQ) {
        tokens.push({ kind: DirectTokKind.Eq, value: two });
        i += 2;
        continue;
      }
      if (two === OP_NE) {
        tokens.push({ kind: DirectTokKind.Ne, value: two });
        i += 2;
        continue;
      }
      if (two === OP_LE) {
        tokens.push({ kind: DirectTokKind.Le, value: two });
        i += 2;
        continue;
      }
      if (two === OP_GE) {
        tokens.push({ kind: DirectTokKind.Ge, value: two });
        i += 2;
        continue;
      }
    }

    const ch = input[i]!;
    if (ch === OP_NOT) {
      tokens.push({ kind: DirectTokKind.Not, value: ch });
      i++;
      continue;
    }
    if (ch === PAREN_OPEN) {
      tokens.push({ kind: DirectTokKind.LParen, value: ch });
      i++;
      continue;
    }
    if (ch === PAREN_CLOSE) {
      tokens.push({ kind: DirectTokKind.RParen, value: ch });
      i++;
      continue;
    }
    if (ch === OP_LT) {
      tokens.push({ kind: DirectTokKind.Lt, value: ch });
      i++;
      continue;
    }
    if (ch === OP_GT) {
      tokens.push({ kind: DirectTokKind.Gt, value: ch });
      i++;
      continue;
    }
    if (ch === VARIANT_SEP) {
      tokens.push({ kind: DirectTokKind.Pipe, value: ch });
      i++;
      continue;
    }

    if (ch === QUOTE_DOUBLE || ch === QUOTE_SINGLE) {
      const quote = ch;
      let j = i + 1;
      while (j < len && input[j] !== quote) j++;
      tokens.push({ kind: DirectTokKind.StrLit, value: input.slice(i, j + 1) });
      i = j + 1;
      continue;
    }

    if (
      (ch >= "0" && ch <= "9") ||
      (ch === "-" &&
        i + 1 < len &&
        input[i + 1]! >= "0" &&
        input[i + 1]! <= "9" &&
        (tokens.length === 0 ||
          isDirectOperatorToken(tokens[tokens.length - 1]!)))
    ) {
      let j = i;
      if (ch === "-") j++;
      while (
        j < len &&
        ((input[j]! >= "0" && input[j]! <= "9") || input[j] === DOT)
      )
        j++;
      tokens.push({ kind: DirectTokKind.NumLit, value: input.slice(i, j) });
      i = j;
      continue;
    }

    if (isDirectIdentStart(ch)) {
      let j = i;
      while (j < len && isDirectIdentChar(input[j]!)) j++;
      const word = input.slice(i, j);

      if (word === LIT_TRUE || word === LIT_FALSE) {
        tokens.push({ kind: DirectTokKind.BoolLit, value: word });
        i = j;
        continue;
      }
      if (word === KW_IN) {
        tokens.push({ kind: DirectTokKind.In, value: word });
        i = j;
        continue;
      }
      if (word === KW_MATCH) {
        tokens.push({ kind: DirectTokKind.Match, value: word });
        i = j;
        continue;
      }
      if (word === KW_CASE) {
        tokens.push({ kind: DirectTokKind.Case, value: word });
        i = j;
        continue;
      }

      while (j < len && input[j] === DOT) {
        j++;
        while (j < len && isDirectIdentChar(input[j]!)) j++;
      }
      if (j < len && input[j] === PAREN_OPEN) {
        let depth = 1;
        j++;
        while (j < len && depth > 0) {
          if (input[j] === PAREN_OPEN) depth++;
          else if (input[j] === PAREN_CLOSE) depth--;
          j++;
        }
      }
      tokens.push({ kind: DirectTokKind.Ident, value: input.slice(i, j) });
      i = j;
      continue;
    }

    throw new TemplateSyntaxError(`unexpected character '${ch}' in condition`);
  }

  return tokens;
}

export function isDirectIdentStart(ch: string): boolean {
  return (ch >= "a" && ch <= "z") || (ch >= "A" && ch <= "Z") || ch === "_";
}
export function isDirectIdentChar(ch: string): boolean {
  return isDirectIdentStart(ch) || (ch >= "0" && ch <= "9");
}
export function isDirectOperatorToken(t: DirectToken): boolean {
  return (
    t.kind !== DirectTokKind.Ident &&
    t.kind !== DirectTokKind.StrLit &&
    t.kind !== DirectTokKind.NumLit &&
    t.kind !== DirectTokKind.BoolLit &&
    t.kind !== DirectTokKind.RParen
  );
}
