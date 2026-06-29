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

// Delimiters
export const PAREN_OPEN = "(";
export const PAREN_CLOSE = ")";
export const ANGLE_OPEN = "<";
export const ANGLE_CLOSE = ">";
export const BRACKET_OPEN = "[";
export const BRACKET_CLOSE = "]";
export const BRACE_OPEN = "{";
export const BRACE_CLOSE = "}";
export const COMMA = ",";
export const COLON = ":";
export const EQUALS = "=";
export const SLASH = "/";
export const PATH_PREFIX_CUR = "./";
export const PATH_PREFIX_PARENT = "../";
export const PATH_PREFIX_CUR_WIN = ".\\";
export const PATH_PREFIX_PARENT_WIN = "..\\";
export const PIPE = "|";
export const PATH_SEP = ".";
export const DOT = ".";
export const QUOTE_DOUBLE = '"';
export const QUOTE_SINGLE = "'";
export const OPTION_SOME = "Some";
export const OPTION_NONE = "None";

// Frontmatter prefixes
export const FM_NAME_PREFIX = "name:";
export const FM_DESC_PREFIX = "description:";
export const FM_PARAMS_PREFIX = "params:";
export const FM_TYPES_PREFIX = "types:";
export const FM_IMPORTS_PREFIX = "imports:";
export const FM_CONSTS_PREFIX = "consts:";
export const FM_ALLOW_UNUSED_PREFIX = "allow_unused:";

// Errors
export const ERR_COMPOUND_BRACKETS_PROHIBITED =
  "must use parentheses (...); angle brackets <...> and square brackets [...] are prohibited";
