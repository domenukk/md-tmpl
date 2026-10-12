/**
 * Single-pass JSON string body escaping shared by the `escape_json` and
 * `json` filters (mirrors Rust `escape_json_str` / `escape_json_into`).
 *
 * @module
 */

const CODE_BACKSLASH = 0x5c;
const CODE_QUOTE = 0x22;
const CODE_SLASH = 0x2f;
const CODE_NEWLINE = 0x0a;
const CODE_CARRIAGE_RETURN = 0x0d;
const CODE_TAB = 0x09;
const CODE_BACKSPACE = 0x08;
const CODE_FORM_FEED = 0x0c;
const CODE_LINE_SEPARATOR = 0x2028;
const CODE_PARAGRAPH_SEPARATOR = 0x2029;
/** First code unit that is not a C0 control character. */
const CODE_FIRST_NON_CONTROL = 0x20;
const HEX_RADIX = 16;
const UNICODE_ESCAPE_WIDTH = 4;
const UNICODE_ESCAPE_PREFIX = "\\u";

function replacementFor(
  code: number,
  escapeSlash: boolean,
): string | undefined {
  switch (code) {
    case CODE_BACKSLASH:
      return "\\\\";
    case CODE_QUOTE:
      return '\\"';
    case CODE_SLASH:
      return escapeSlash ? "\\/" : undefined;
    case CODE_NEWLINE:
      return "\\n";
    case CODE_CARRIAGE_RETURN:
      return "\\r";
    case CODE_TAB:
      return "\\t";
    case CODE_BACKSPACE:
      return "\\b";
    case CODE_FORM_FEED:
      return "\\f";
    case CODE_LINE_SEPARATOR:
      return "\\u2028";
    case CODE_PARAGRAPH_SEPARATOR:
      return "\\u2029";
    default:
      return code < CODE_FIRST_NON_CONTROL
        ? `${UNICODE_ESCAPE_PREFIX}${code.toString(HEX_RADIX).padStart(UNICODE_ESCAPE_WIDTH, "0")}`
        : undefined;
  }
}

/**
 * Escape JSON string body characters: quotes, backslashes, control characters,
 * `U+2028` / `U+2029`, and (when `escapeSlash`) `/` → `\/`. The caller supplies
 * the surrounding quotes.
 */
export function escapeJsonBody(s: string, escapeSlash: boolean): string {
  let out: string | undefined;
  let lastCopied = 0;
  for (let i = 0; i < s.length; i++) {
    const replacement = replacementFor(s.charCodeAt(i), escapeSlash);
    if (replacement !== undefined) {
      out = (out ?? "") + s.slice(lastCopied, i) + replacement;
      lastCopied = i + 1;
    }
  }
  return out !== undefined ? out + s.slice(lastCopied) : s;
}
