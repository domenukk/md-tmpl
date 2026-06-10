//! Shared string constants for the template engine.
//!
//! Every string that appears as a magic literal in more than one module
//! or defines core syntax/grammar is defined here once.

// -- Built-in function names --------------------------------------------------

/// Name of the loop-index function: `idx(binding)`.
pub(crate) const FN_IDX: &str = "idx";

/// Name of the length function: `len(expr)`.
pub(crate) const FN_LEN: &str = "len";

/// Name of the explicit string conversion function: `str(expr)`.
pub(crate) const FN_STR: &str = "str";

/// All built-in function names.
///
/// Used by static analysis to avoid treating function names as variable
/// references (e.g. `idx` in `idx(bug)` is not a variable).
pub(crate) const BUILTIN_FUNCTIONS: &[&str] = &[FN_IDX, FN_LEN, FN_STR];

// -- Filter names -------------------------------------------------------------

/// Name of the `upper` filter.
pub(crate) const FILTER_UPPER: &str = "upper";
/// Name of the `lower` filter.
pub(crate) const FILTER_LOWER: &str = "lower";
/// Name of the `trim` filter.
pub(crate) const FILTER_TRIM: &str = "trim";
/// Name of the `fixed` filter.
pub(crate) const FILTER_FIXED: &str = "fixed";
/// Name of the `default` filter.
pub(crate) const FILTER_DEFAULT: &str = "default";
/// Name of the `length` filter.
pub(crate) const FILTER_LENGTH: &str = "length";
/// Name of the `join` filter.
pub(crate) const FILTER_JOIN: &str = "join";
/// Name of the `limit` filter.
pub(crate) const FILTER_LIMIT: &str = "limit";
/// Name of the `gt` filter.
pub(crate) const FILTER_GT: &str = "gt";

// -- Enum tag key -------------------------------------------------------------

/// Dict key used for internally-tagged enum variants:
/// `{"tag": "VariantName", ...}`.
pub(crate) const ENUM_TAG_KEY: &str = "tag";

// -- Template tag delimiters -------------------------------------------------

/// Delimiter indicating the start of an expression: `{{`.
pub(crate) const EXPR_START: &str = "{{";
/// Delimiter indicating the end of an expression: `}}`.
pub(crate) const EXPR_END: &str = "}}";

/// Delimiter indicating the start of a statement: `{%`.
pub(crate) const STMT_START: &str = "{%";
/// Delimiter indicating the end of a statement: `%}`.
pub(crate) const STMT_END: &str = "%}";

/// Delimiter indicating the start of a comment: `{#`.
pub(crate) const COMMENT_START: &str = "{#";
/// Delimiter indicating the end of a comment: `#}`.
pub(crate) const COMMENT_END: &str = "#}";

/// Whitespace control trim marker: `-`.
pub(crate) const TRIM_MARKER: char = '-';

// -- Grammar keywords and tags ------------------------------------------------

/// Spaced for loop tag prefix: `for `.
pub(crate) const TAG_FOR_PREFIX: &str = "for ";
/// Spaced in keyword for loops: ` in `.
pub(crate) const KW_IN_SPACED: &str = " in ";

/// Spaced if statement tag prefix: `if `.
pub(crate) const TAG_IF_PREFIX: &str = "if ";
/// Spaced elif statement tag prefix: `elif `.
pub(crate) const TAG_ELIF_PREFIX: &str = "elif ";
/// Else keyword: `else`.
pub(crate) const KW_ELSE: &str = "else";

/// Raw literal block keyword: `raw`.
pub(crate) const KW_RAW: &str = "raw";
/// Raw custom delimiter assignment prefix: `raw=`.
pub(crate) const KW_RAW_ASSIGN: &str = "raw=";

/// Include statement prefix: `include `.
pub(crate) const TAG_INCLUDE_PREFIX: &str = "include ";
/// Include `with` override statement prefix: `with `.
pub(crate) const TAG_WITH_PREFIX: &str = "with ";
/// Spaced include `with` override: ` with `.
pub(crate) const TAG_WITH_SPACED: &str = " with ";

/// Inline template tag name: `tmpl `.
pub(crate) const TAG_TMPL_PREFIX: &str = "tmpl ";

/// Match statement tag prefix: `match `.
pub(crate) const TAG_MATCH_PREFIX: &str = "match ";
/// Case arm tag prefix: `case `.
pub(crate) const TAG_CASE_PREFIX: &str = "case ";

// -- Closing block tags -------------------------------------------------------

/// Closing tag for `if` statement block: `/if`.
pub(crate) const CLOSE_IF: &str = "/if";
/// Closing tag for `for` statement block: `/for`.
pub(crate) const CLOSE_FOR: &str = "/for";
/// Closing tag for `raw` statement block: `/raw`.
pub(crate) const CLOSE_RAW: &str = "/raw";
/// Closing tag for inline template definition: `/tmpl`.
pub(crate) const CLOSE_TMPL: &str = "/tmpl";
/// Closing tag for match block: `/match`.
pub(crate) const CLOSE_MATCH: &str = "/match";

// -- Legacy block tags (for warnings/errors) ----------------------------------

/// Legacy closing tag for `if` statement block: `endif`.
pub(crate) const LEGACY_ENDIF: &str = "endif";
/// Legacy closing tag for `for` statement block: `endfor`.
pub(crate) const LEGACY_ENDFOR: &str = "endfor";
/// Legacy closing tag for `raw` statement block: `endraw`.
pub(crate) const LEGACY_ENDRAW: &str = "endraw";

// -- Markdown Blockquote delimiters -------------------------------------------

/// Blockquote character used to prefix template directives: `>`.
pub(crate) const BLOCKQUOTE_PREFIX: char = '>';
/// Spaced blockquote prefix: `> `.
pub(crate) const BLOCKQUOTE_PREFIX_SPACED: &str = "> ";
/// Compact statement blockquote start: `>{`.
pub(crate) const BLOCKQUOTE_COMPACT_OPEN: &str = ">{";
/// Spaced statement blockquote start: `> {%`.
pub(crate) const BLOCKQUOTE_SPACED_OPEN: &str = "> {%";

// -- Frontmatter YAML delimiters & keys ---------------------------------------

/// YAML frontmatter block delimiter: `---`.
pub(crate) const FM_DELIMITER: &str = "---";
/// YAML frontmatter block delimiter ending line: `\n---`.
pub(crate) const FM_DELIMITER_NEWLINE: &str = "\n---";

/// Frontmatter key for template name: `name:`.
pub(crate) const FM_NAME_PREFIX: &str = "name:";
/// Frontmatter key for template description: `description:`.
pub(crate) const FM_DESC_PREFIX: &str = "description:";
/// Frontmatter key for template parameters: `params:`.
pub(crate) const FM_PARAMS_PREFIX: &str = "params:";
/// Frontmatter key to allow unused declared parameters: `allow_unused:`.
pub(crate) const FM_ALLOW_UNUSED_PREFIX: &str = "allow_unused:";

// -- Type annotations ---------------------------------------------------------

/// Type name for strings: `str`.
pub(crate) const TYPE_STR: &str = "str";
/// Type name for booleans: `bool`.
pub(crate) const TYPE_BOOL: &str = "bool";
/// Type name for integers: `int`.
pub(crate) const TYPE_INT: &str = "int";
/// Type name for floating point numbers: `float`.
pub(crate) const TYPE_FLOAT: &str = "float";
/// Type name for lists: `list`.
pub(crate) const TYPE_LIST: &str = "list";
/// Type name for dictionaries: `dict`.
pub(crate) const TYPE_DICT: &str = "dict";
/// Type name for enums: `enum`.
pub(crate) const TYPE_ENUM: &str = "enum";

/// Type prefix for lists with angle brackets: `list<`.
pub(crate) const TYPE_LIST_PREFIX: &str = "list<";
/// Type prefix for dicts with angle brackets: `dict<`.
pub(crate) const TYPE_DICT_PREFIX: &str = "dict<";
/// Type prefix for enums with angle brackets: `enum<`.
pub(crate) const TYPE_ENUM_PREFIX: &str = "enum<";

// -- Literals -----------------------------------------------------------------

/// Boolean true literal: `true`.
pub(crate) const LIT_TRUE: &str = "true";
/// Boolean false literal: `false`.
pub(crate) const LIT_FALSE: &str = "false";

// -- Error messages -----------------------------------------------------------

/// Error when frontmatter block is missing.
pub(crate) const ERR_MISSING_FM: &str =
    "missing mandatory YAML frontmatter block (starts with ---)";
/// Error when frontmatter block is unclosed.
pub(crate) const ERR_UNCLOSED_FM: &str = "unclosed YAML frontmatter block";
/// Error when `params:` block is missing in frontmatter.
pub(crate) const ERR_MISSING_PARAMS: &str = "missing mandatory `params:` block in frontmatter";
/// Prefix for undeclared variable references error.
pub(crate) const ERR_UNDECLARED_PREFIX: &str = "undeclared variable(s) referenced in body: ";
