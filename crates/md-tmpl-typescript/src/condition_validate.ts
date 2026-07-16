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
  OP_EQ,
  OP_NE,
  OP_LT,
  OP_GT,
  OP_LE,
  OP_GE,
  OP_AND,
  OP_OR,
  OP_NOT,
  PAREN_OPEN,
  PAREN_CLOSE,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  BACKSLASH,
  KW_IN,
  KW_CASE_SPACED,
  TAG_MATCH_PREFIX,
} from "./consts.js";

/** Comparison operators recognised in conditions, in Rust's precedence order
 * (longer operators first so `<=`/`>=` win over `<`/`>`). Mirrors the Rust
 * core's `COMPARISON_OPS`. */
const CONDITION_COMPARISON_OPS: readonly string[] = [
  OP_EQ,
  OP_NE,
  OP_LE,
  OP_GE,
  OP_LT,
  OP_GT,
  ` ${KW_IN} `,
];

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
  for (const part of condSplitTopLevel(s, OP_OR)) {
    validateAndExpr(part);
  }
}

/** and_expr := unary_expr ( "&&" unary_expr )* */
function validateAndExpr(s: string): void {
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
  for (const op of CONDITION_COMPARISON_OPS) {
    const idx = condFindTopLevelOp(t, op);
    if (idx >= 0) {
      validateOperand(t.slice(0, idx).trim());
      validateOperand(t.slice(idx + op.length).trim());
      return;
    }
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
function condFindTopLevelOp(s: string, op: string): number {
  let depth = 0;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === PAREN_OPEN) {
      depth++;
    } else if (ch === PAREN_CLOSE) {
      depth--;
    } else if (ch === QUOTE_SINGLE || ch === QUOTE_DOUBLE) {
      const quote = ch;
      i++;
      while (i < s.length && s[i] !== quote) {
        if (s[i] === BACKSLASH && i + 1 < s.length) i += 2;
        else i++;
      }
    } else if (
      depth === 0 &&
      i + op.length <= s.length &&
      s.slice(i, i + op.length) === op
    ) {
      return i;
    }
  }
  return -1;
}
