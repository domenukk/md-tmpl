/**
 * Compile-time condition syntax validation for md-tmpl templates.
 *
 * These functions validate the _syntax_ of `{% if %}` / guard conditions
 * without evaluating them.  They are a faithful port of the Rust core's
 * `parse_condition` (analysis.rs) and are deliberately substring-based:
 * they split only on the logical operators `||` / `&&`, parentheses,
 * comparison operators, and the `match … case …` form.
 *
 * Extracted from evaluator.ts to keep file size manageable.
 */

import { TemplateSyntaxError } from "./errors.js";
import {
  OP_AND,
  OP_OR,
  OP_NOT,
  PAREN_OPEN,
  PAREN_CLOSE,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  BACKSLASH,
  KW_CASE_SPACED,
  TAG_MATCH_PREFIX,
} from "./consts.js";

/**
 * Validate the SYNTAX of a `{% if %}` / guard condition without evaluating it.
 *
 * This is a faithful port of the Rust core's `parse_condition`
 * (analysis.rs). It is deliberately substring-based: it splits only on the
 * logical operators `||` / `&&`, parentheses, comparison operators, and the
 * `match … case …` form. It does NOT tokenize on whitespace, so operands such
 * as `not flag` or `x not in items` are treated as (invalid) paths that pass
 * the syntax gate and are later reported as undeclared variables — exactly as
 * the Rust backend does. Only genuinely malformed structures — dangling
 * operators (`a &&`), empty operands (`&& a`), empty parentheses (`()`),
 * unclosed parentheses (`(a > 0`), and malformed match-as-condition forms —
 * are rejected here. Because it never touches a scope, undeclared variables do
 * not mask a structural syntax error, matching the Rust backend's
 * compile-time behavior.
 *
 * @throws {TemplateSyntaxError} If the condition is syntactically invalid.
 */
export function validateConditionSyntax(condition: string): void {
  const trimmed = condition.trim();
  if (trimmed.length === 0) {
    throw new TemplateSyntaxError("empty condition");
  }
  validateOrExpr(trimmed);
}

/** or_expr := and_expr ( "||" and_expr )* */
function validateOrExpr(s: string): void {
  if (!s.includes(OP_OR)) {
    validateAndExpr(s);
    return;
  }
  for (const part of condSplitTopLevel(s, OP_OR)) {
    validateAndExpr(part);
  }
}

/** and_expr := unary_expr ( "&&" unary_expr )* */
function validateAndExpr(s: string): void {
  if (!s.includes(OP_AND)) {
    validateUnaryExpr(s);
    return;
  }
  for (const part of condSplitTopLevel(s, OP_AND)) {
    validateUnaryExpr(part);
  }
}

/** unary_expr := "!" unary_expr | "(" or_expr ")" | primary */
function validateUnaryExpr(s: string): void {
  const t = s.trim();
  if (t.startsWith(OP_NOT)) {
    validateUnaryExpr(t.slice(OP_NOT.length));
    return;
  }
  if (t.startsWith(PAREN_OPEN)) {
    const inner = condStripBalancedParens(t);
    if (inner !== undefined) {
      validateOrExpr(inner);
      return;
    }
    throw new TemplateSyntaxError(
      `unclosed '(' in condition: '${t}' — missing matching ')'`,
    );
  }
  validatePrimary(t);
}

/** primary := match-as-condition | comparison | truthiness */
function validatePrimary(s: string): void {
  const t = s.trim();
  if (t.startsWith(TAG_MATCH_PREFIX)) {
    validateMatchAsCondition(t.slice(TAG_MATCH_PREFIX.length).trim());
    return;
  }
  const cmp = condFindFirstTopLevelComparison(t);
  if (cmp !== undefined) {
    validateOperand(t.slice(0, cmp.idx).trim());
    validateOperand(t.slice(cmp.idx + cmp.len).trim());
    return;
  }
  validateOperand(t);
}

/** `expr case Variant [| Variant]*` — mirrors Rust's `parse_match_as_condition`. */
function validateMatchAsCondition(s: string): void {
  const caseIdx = s.indexOf(KW_CASE_SPACED);
  if (caseIdx < 0) {
    throw new TemplateSyntaxError(
      "match-as-condition: expected 'case' keyword after expression",
    );
  }
  const exprStr = s.slice(0, caseIdx).trim();
  const variantStr = s.slice(caseIdx + KW_CASE_SPACED.length).trim();
  if (exprStr.length === 0) {
    throw new TemplateSyntaxError("match-as-condition: empty expression");
  }
  if (variantStr.length === 0) {
    throw new TemplateSyntaxError(
      "match-as-condition: empty variant name after 'case'",
    );
  }
}

/**
 * Validate a single condition operand. Mirrors the error surface of the Rust
 * core's `ConditionOperand::compile`: an empty token is the only structural
 * syntax error. Paths themselves compile infallibly and are validated later by
 * the undeclared-variable and type-checking passes.
 */
function validateOperand(token: string): void {
  if (token.trim().length === 0) {
    throw new TemplateSyntaxError("empty token in expression");
  }
}

/**
 * Split `s` at top-level occurrences of `delim`, respecting parenthesis
 * nesting and string literals. Mirrors the Rust core's `split_top_level`,
 * including its error cases: a trailing delimiter with nothing after it is a
 * dangling operator, and a wholly empty result is an empty expression.
 *
 * @throws {TemplateSyntaxError} On a dangling operator or empty expression.
 */
function condSplitTopLevel(s: string, delim: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  let i = 0;
  while (i < s.length) {
    const ch = s[i];
    if (ch === PAREN_OPEN) {
      depth++;
      i++;
    } else if (ch === PAREN_CLOSE) {
      depth = Math.max(0, depth - 1);
      i++;
    } else if (ch === QUOTE_SINGLE || ch === QUOTE_DOUBLE) {
      const quote = ch;
      i++;
      while (i < s.length && s[i] !== quote) {
        if (s[i] === BACKSLASH && i + 1 < s.length) i += 2;
        else i++;
      }
      if (i < s.length) i++; // skip the closing quote
    } else if (
      depth === 0 &&
      i + delim.length <= s.length &&
      s.slice(i, i + delim.length) === delim
    ) {
      parts.push(s.slice(start, i).trim());
      i += delim.length;
      start = i;
    } else {
      i++;
    }
  }
  const last = s.slice(start).trim();
  if (last.length > 0) {
    parts.push(last);
  } else if (parts.length > 0) {
    throw new TemplateSyntaxError(
      `dangling '${delim}' operator: missing right-hand expression`,
    );
  }
  if (parts.length === 0) {
    throw new TemplateSyntaxError("empty expression in condition");
  }
  return parts;
}

/**
 * Strip balanced outer parentheses from `s`. Returns the inner slice when `s`
 * starts with `(` and the matching `)` is the last character, otherwise
 * `undefined`. Mirrors the Rust core's `strip_balanced_parens`.
 */
function condStripBalancedParens(s: string): string | undefined {
  if (!s.startsWith(PAREN_OPEN)) return undefined;
  let depth = 0;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === PAREN_OPEN) {
      depth++;
    } else if (ch === PAREN_CLOSE) {
      depth--;
      if (depth === 0) {
        if (i === s.length - 1) return s.slice(1, i);
        return undefined; // ')' is not at the end
      }
    }
  }
  return undefined;
}

/**
 * Find the first top-level occurrence of `op` in `s` (ignoring parens and
 * string literals). Returns the byte index or -1. Mirrors the Rust core's
 * `find_top_level_op`.
 */
/**
 * Find the first top-level comparison operator in `s` (respecting Rust's
 * precedence order where `==`, `!=`, `<=`, `>=` win over `<`, `>`, ` in `).
 */
function condFindFirstTopLevelComparison(
  s: string,
): { idx: number; len: number } | undefined {
  let depth = 0;
  let ltGtIdx = -1;
  let inOpIdx = -1;
  const len = s.length;
  for (let i = 0; i < len; i++) {
    const code = s.charCodeAt(i);
    if (code === 40 /* '(' */) {
      depth++;
    } else if (code === 41 /* ')' */) {
      depth--;
    } else if (code === 34 /* '"' */ || code === 39 /* "'" */) {
      i++;
      while (i < len && s.charCodeAt(i) !== code) {
        if (s.charCodeAt(i) === 92 /* '\\' */ && i + 1 < len) i += 2;
        else i++;
      }
    } else if (depth === 0) {
      const next = s.charCodeAt(i + 1);
      if (
        (code === 61 /* '=' */ ||
          code === 33 /* '!' */ ||
          code === 60 /* '<' */ ||
          code === 62) /* '>' */ &&
        next === 61 /* '=' */
      ) {
        return { idx: i, len: 2 };
      }
      if ((code === 60 /* '<' */ || code === 62) /* '>' */ && ltGtIdx === -1) {
        ltGtIdx = i;
      } else if (
        code === 32 /* ' ' */ &&
        inOpIdx === -1 &&
        next === 105 /* 'i' */ &&
        s.charCodeAt(i + 2) === 110 /* 'n' */ &&
        s.charCodeAt(i + 3) === 32 /* ' ' */
      ) {
        inOpIdx = i;
      }
    }
  }
  if (ltGtIdx !== -1) return { idx: ltGtIdx, len: 1 };
  if (inOpIdx !== -1) return { idx: inOpIdx, len: 4 };
  return undefined;
}
