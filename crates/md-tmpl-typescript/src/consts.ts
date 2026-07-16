/**
 * Shared string and character constants for the TypeScript template engine.
 * @module
 */

// Type names
export const TYPE_STR = "str";
export const TYPE_BOOL = "bool";
export const TYPE_INT = "int";
export const TYPE_FLOAT = "float";
export const TYPE_LIST = "list";
export const TYPE_STRUCT = "struct";
export const TYPE_ENUM = "enum";
export const TYPE_TMPL = "tmpl";
export const TYPE_OPTION = "option";
export const TYPE_NONE = "none";
export const TYPE_ALIAS = "alias";
export const TYPE_SCALAR_LIST = "scalar_list";
export const TYPE_UNTYPED_LIST = "untyped_list";

// Node kinds
export const NODE_TEXT = "text";
export const NODE_EXPR = "expr";
export const NODE_COMMENT = "comment";
export const NODE_FOR = "for";
export const NODE_IF = "if";
export const NODE_MATCH = "match";
export const NODE_RAW = "raw";
export const NODE_INCLUDE = "include";
export const NODE_TMPL = "tmpl";
export const NODE_PANIC = "panic";
export const NODE_CASE = "case";
export const NODE_WITH = "with";
export const NODE_ELIF = "elif";

// Delimiters
export const PAREN_OPEN = "(";
export const PAREN_CLOSE = ")";
export const ANGLE_OPEN = "<";
export const ANGLE_CLOSE = ">";
export const BRACKET_OPEN = "[";
export const BRACKET_CLOSE = "]";
export const BRACE_OPEN = "{";
export const BRACE_CLOSE = "}";
export const EXPR_START = "{{";
export const EXPR_END = "}}";
export const COMMA = ",";
export const COLON = ":";
export const EQUALS = "=";
export const SLASH = "/";
export const PATH_PREFIX_CUR = "./";
export const PATH_PREFIX_PARENT = "../";
export const PATH_PREFIX_CUR_WIN = ".\\";
export const PATH_PREFIX_PARENT_WIN = "..\\";

export function isValidResolvedPath(path: string): boolean {
  return (
    path.startsWith(PATH_PREFIX_CUR) ||
    path.startsWith(PATH_PREFIX_PARENT) ||
    path.startsWith(PATH_PREFIX_CUR_WIN) ||
    path.startsWith(PATH_PREFIX_PARENT_WIN) ||
    path.startsWith(SLASH)
  );
}

export const PIPE = "|";
export const PATH_SEP = ".";
export const DOT = ".";
export const QUOTE_DOUBLE = '"';
export const QUOTE_SINGLE = "'";
export const BACKSLASH = "\\";
export const OPTION_SOME = "Some";
export const OPTION_NONE = "None";
export const MATCH_DEFAULT = "_";
export const ENUM_TAG_KEY = "__kind__";
export const ENUM_VARIANTS_KEY = "__variants__";
export const OPTION_VAL_FIELD = "val";

// Frontmatter prefixes
export const FM_NAME_PREFIX = "name:";
export const FM_DESC_PREFIX = "description:";
export const FM_PARAMS_PREFIX = "params:";
export const FM_TYPES_PREFIX = "types:";
export const FM_IMPORTS_PREFIX = "imports:";
export const FM_CONSTS_PREFIX = "consts:";
export const FM_ENV_PREFIX = "env:";
export const FM_ALLOW_UNUSED_PREFIX = "allow_unused:";
export const FM_DELIMITER = "---";
export const BLOCKQUOTE_PREFIX = ">";
export const BLOCKQUOTE_PREFIX_SPACED = "> ";
export const COMMENT_START = "{#";
export const COMMENT_START_SPACED = "{# ";
export const COMMENT_END = "#}";
export const STMT_START = "{%";
export const STMT_END = "%}";
export const TRIM_MARKER = "-";
// Builtin function names
export const FN_IDX = "idx";
export const FN_LEN = "len";
export const FN_KIND = "kind";
export const FN_KINDS = "kinds";
export const FN_HAS = "has";

// Literals
export const LIT_TRUE = "true";
export const LIT_FALSE = "false";

// Keywords & Tags
export const KW_FOR = "for";
export const KW_IF = "if";
export const KW_MATCH = "match";
export const KW_INCLUDE = "include";
export const KW_TMPL = "tmpl";
export const KW_PANIC = "panic";
export const KW_ELSE = "else";
export const KW_CASE = "case";
export const KW_IN = "in";
export const KW_WITH = "with";
export const KW_ELIF = "elif";
export const KW_END_FOR = "/for";
export const KW_END_IF = "/if";
export const KW_END_MATCH = "/match";
export const KW_END_TMPL = "/tmpl";

export const TAG_FOR_PREFIX = "for ";
export const TAG_IF_PREFIX = "if ";
export const TAG_MATCH_PREFIX = "match ";
export const TAG_INCLUDE_PREFIX = "include ";
export const TAG_TMPL_PREFIX = "tmpl ";
export const TAG_PANIC_PREFIX = "panic ";
export const TAG_PANIC_PAREN_PREFIX = "panic(";
export const TAG_RAW_ASSIGN_PREFIX = "raw=";
export const TAG_ELIF_PREFIX = "elif ";
export const TAG_CASE_PREFIX = "case ";
export const TAG_WITH_PREFIX = "with ";

export const KW_RAW = "raw";
export const KW_RAW_SPACED = " raw ";
export const KW_RAW_ASSIGN_SPACED = " raw=";
export const KW_RAW_CLOSE_SPACED = " raw%";
export const KW_END_RAW = "/raw";
export const KW_END_RAW_TRIM = "- /raw";
export const TRIM_SPACED = "{%-";

// Operators
export const OP_IN_SPACED = " in ";
export const OP_EQ = "==";
export const OP_NE = "!=";
export const OP_LT = "<";
export const OP_GT = ">";
export const OP_LE = "<=";
export const OP_GE = ">=";
export const OP_AND = "&&";
export const OP_OR = "||";
export const OP_NOT = "!";
export const KW_CASE_SPACED = " case ";
export const VARIANT_SEP = "|";

// Errors
export const ERR_COMPOUND_BRACKETS_PROHIBITED =
  "must use parentheses (...); angle brackets <...> and square brackets [...] are prohibited";

/**
 * Prefix for the compile-time undeclared-variable error. Mirrors the Rust
 * core's `ERR_UNDECLARED_PREFIX` so both backends emit the identical message
 * (and the shared conformance suite can assert on the `undeclared variable`
 * substring).
 */
export const ERR_UNDECLARED_PREFIX =
  "undeclared variable(s) referenced in body: ";

/**
 * Hint substring emitted when a parameter default uses the qualified
 * `Type.Variant` form (e.g. `Stage.Build`) instead of the canonical bare
 * variant name (e.g. `Build`). Both backends must include this exact phrase.
 */
export const ERR_BARE_VARIANT_HINT = "use the bare variant name";

// Variable prefixes
export const PREFIX_CONSTS_DOT = "consts.";
export const PREFIX_OPTS_DOT = "opts.";
export const PREFIX_OPTIONS_DOT = "options.";
export const PREFIX_PARAMS_DOT = "params.";

/**
 * Unescape the inner content of a string literal (surrounding quotes already
 * stripped) using md-tmpl's uniform escape rules. Mirrors the Rust core's
 * `unescape_string_literal` so all backends agree.
 *
 * Recognized escapes:
 * - `\\` -> `\`
 * - `\"` -> `"`
 * - `\'` -> `'`
 *
 * Any other backslash sequence `\X` is preserved verbatim (both the backslash
 * and `X`), so strings containing literal backslashes (e.g. `\n`, `c:\path`)
 * are unaffected. A trailing lone backslash is kept.
 */
export function unescapeStringLiteral(inner: string): string {
  // Fast path: no backslash means nothing to unescape.
  if (!inner.includes(BACKSLASH)) return inner;
  let out = "";
  for (let i = 0; i < inner.length; i++) {
    const c = inner.charAt(i);
    if (c === BACKSLASH) {
      const next = inner.charAt(i + 1);
      if (
        next === QUOTE_DOUBLE ||
        next === QUOTE_SINGLE ||
        next === BACKSLASH
      ) {
        out += next;
        i++;
      } else {
        // Unknown escape or trailing backslash: keep the backslash verbatim.
        out += BACKSLASH;
      }
    } else {
      out += c;
    }
  }
  return out;
}

/** Returns true if the match node uses option-style variant names. */
export function isOptionMatchNode(node: {
  arms: { variants: string[] }[];
  inlineGuard?: { variant: string };
}): boolean {
  if (node.inlineGuard) {
    return (
      node.inlineGuard.variant === OPTION_SOME ||
      node.inlineGuard.variant === OPTION_NONE
    );
  }
  return node.arms.some((arm) =>
    arm.variants.some((v) => v === OPTION_SOME || v === OPTION_NONE),
  );
}
