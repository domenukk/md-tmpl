//! Shared string constants for the template engine.
//!
//! Every string that appears as a magic literal in more than one module
//! or defines core syntax/grammar is defined here once.

// -- Built-in function names --------------------------------------------------

/// Name of the loop-index function: `idx(binding)`.
pub(crate) const FN_IDX: &str = "idx";

/// Name of the length function: `len(expr)`.
pub(crate) const FN_LEN: &str = "len";

/// Name of the explicit kind/variant-name function: `kind(expr)`.
pub(crate) const FN_KIND: &str = "kind";

/// Name of the option-presence function: `has(expr)`.
pub(crate) const FN_HAS: &str = "has";

/// All built-in function names.
///
/// Used by static analysis to avoid treating function names as variable
/// references (e.g. `idx` in `idx(item)` is not a variable).
pub(crate) const BUILTIN_FUNCTIONS: &[&str] = &[FN_IDX, FN_LEN, FN_KIND, FN_HAS];

// -- Filter names -------------------------------------------------------------

/// Name of the `upper` filter.
pub(crate) const FILTER_UPPER: &str = "upper";
/// Name of the `lower` filter.
pub(crate) const FILTER_LOWER: &str = "lower";
/// Name of the `trim` filter.
pub(crate) const FILTER_TRIM: &str = "trim";
/// Name of the `fixed` filter.
pub(crate) const FILTER_FIXED: &str = "fixed";
/// Name of the `join` filter.
pub(crate) const FILTER_JOIN: &str = "join";
/// Name of the `limit` filter.
pub(crate) const FILTER_LIMIT: &str = "limit";
/// Name of the `add` filter.
pub(crate) const FILTER_ADD: &str = "add";
/// Name of the `sub` filter.
pub(crate) const FILTER_SUB: &str = "sub";

// -- Enum tag key -------------------------------------------------------------

/// Struct key used for internally-tagged enum variants:
/// `{"__kind__": "VariantName", ...}`.
///
/// Uses a dunder prefix to avoid collisions with user-defined field names.
pub const ENUM_TAG_KEY: &str = "__kind__";

/// Pseudo-field suffix for legacy `.length` access.
pub(crate) const PSEUDO_FIELD_LENGTH: &str = ".length";

// -- Expression syntax chars --------------------------------------------------

/// Opening parenthesis for function calls: `idx(item)`, `len(items)`.
pub const PAREN_OPEN: char = '(';
/// Closing parenthesis for function calls.
pub const PAREN_CLOSE: char = ')';
/// Dot separator for dotted path expressions: `item.label`.
pub const PATH_SEP: char = '.';
/// Pipe separator for filter chains: `{{ name | upper }}`.
pub const PIPE: char = '|';
/// Opening angle bracket for embed literals: `<file.txt>`.
pub const ANGLE_OPEN: char = '<';
/// Double-quote character for string literal delimiters.
pub const QUOTE_DOUBLE: char = '"';
/// Single-quote character for string literal delimiters.
pub const QUOTE_SINGLE: char = '\'';

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
/// Default (catch-all) arm in match blocks: `default`.
pub(crate) const KW_DEFAULT: &str = "default";

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
/// Frontmatter key for local type aliases: `types:`.
pub(crate) const FM_TYPES_PREFIX: &str = "types:";
/// Frontmatter key for cross-template imports: `imports:`.
pub(crate) const FM_IMPORTS_PREFIX: &str = "imports:";
/// Frontmatter key for global constants: `consts:`.
pub(crate) const FM_CONSTS_PREFIX: &str = "consts:";

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
/// Type name for structs: `struct`.
pub(crate) const TYPE_STRUCT: &str = "struct";
/// Type name for enums: `enum`.
pub(crate) const TYPE_ENUM: &str = "enum";
/// Type name for templates: `tmpl`.
pub(crate) const TYPE_TMPL: &str = "tmpl";

/// Type prefix for lists with angle brackets: `list<`.
pub(crate) const TYPE_LIST_PREFIX: &str = "list<";
/// Type prefix for structs with angle brackets: `struct<`.
pub(crate) const TYPE_STRUCT_PREFIX: &str = "struct<";
/// Type prefix for enums with angle brackets: `enum<`.
pub(crate) const TYPE_ENUM_PREFIX: &str = "enum<";
/// Type prefix for templates with angle brackets: `tmpl<`.
pub(crate) const TYPE_TMPL_PREFIX: &str = "tmpl<";
/// Type name for options: `option`.
pub(crate) const TYPE_OPTION: &str = "option";
/// Type prefix for options with angle brackets: `option<`.
#[allow(dead_code)]
pub(crate) const TYPE_OPTION_PREFIX: &str = "option<";

/// Variant name for the `Some` variant of `option<T>`.
pub const OPTION_SOME: &str = "Some";
/// Variant name for the `None` variant of `option<T>`.
pub const OPTION_NONE: &str = "None";
/// Field name for the inner value of `option<T>`'s `Some` variant.
pub const OPTION_VAL_FIELD: &str = "val";

// -- Literals -----------------------------------------------------------------

/// Boolean true literal: `true`.
pub(crate) const LIT_TRUE: &str = "true";
/// Boolean false literal: `false`.
pub(crate) const LIT_FALSE: &str = "false";

/// Try to strip balanced quotes from a string literal token.
///
/// Returns `Some(inner)` if `token` is a valid quoted string literal
/// (`"..."` or `'...'`), otherwise `None`.
#[must_use]
pub fn strip_string_literal(token: &str) -> Option<&str> {
    if token.len() >= 2
        && ((token.starts_with(QUOTE_DOUBLE) && token.ends_with(QUOTE_DOUBLE))
            || (token.starts_with(QUOTE_SINGLE) && token.ends_with(QUOTE_SINGLE)))
    {
        return Some(&token[1..token.len() - 1]);
    }
    None
}

// -- Error messages -----------------------------------------------------------

/// Error when frontmatter block is missing.
pub(crate) const ERR_MISSING_FM: &str =
    "missing mandatory YAML frontmatter block (starts with ---)";
/// Error when frontmatter block is unclosed.
pub(crate) const ERR_UNCLOSED_FM: &str = "unclosed YAML frontmatter block";
/// Prefix for undeclared variable references error.
pub(crate) const ERR_UNDECLARED_PREFIX: &str = "undeclared variable(s) referenced in body: ";

/// Error when a param is named after a reserved keyword.
pub(crate) const ERR_RESERVED_KEYWORD: &str = "reserved keyword used as name";
/// Error when two params have the same name.
pub(crate) const ERR_DUPLICATE_PARAM: &str = "duplicate parameter name";
/// Error when a `types:` entry has a duplicate name.
pub(crate) const ERR_DUPLICATE_TYPE_ALIAS: &str = "duplicate type alias";
/// Error when a `types:` entry shadows a built-in type name.
pub(crate) const ERR_BUILTIN_SHADOW: &str = "type alias shadows built-in type name";
/// Error when a type alias and param name collide in `PascalCase`.
pub(crate) const ERR_TYPE_PARAM_CONFLICT: &str =
    "type alias name conflicts with parameter name (PascalCase collision)";
/// Error for circular import chains.
#[cfg(feature = "std")]
pub(crate) const ERR_CIRCULAR_IMPORT: &str = "circular import detected";
/// Error when a type alias name shadows an import alias (stem).
pub(crate) const ERR_TYPE_SHADOWS_IMPORT: &str = "type alias shadows import alias";
/// Error when a param's `PascalCase` name shadows an import alias.
pub(crate) const ERR_PARAM_SHADOWS_IMPORT: &str =
    "parameter name (PascalCase) shadows import alias";
/// Error when a `types:` entry is declared but never referenced.
pub(crate) const ERR_UNUSED_TYPE_ALIAS: &str = "unused type alias";
/// Error when a constant name is duplicated.
pub(crate) const ERR_DUPLICATE_CONST: &str = "duplicate constant name";
/// Error when a param and a const share the same name.
pub(crate) const ERR_PARAM_CONST_CONFLICT: &str = "parameter name conflicts with constant name";
/// Error when a for-loop binding shadows a declared name.
pub(crate) const ERR_FOR_BINDING_SHADOWS: &str = "for loop binding shadows";
/// Error when a `{% %}` tag starts a line without a blockquote `>` prefix.
pub(crate) const ERR_BARE_STMT_TAG: &str =
    "statement tag at line start must be blockquote-prefixed with '> '";

/// Built-in type names and keywords that cannot be used as user-defined names.
pub(crate) const RESERVED_NAMES: &[&str] = &[
    TYPE_LIST,
    TYPE_STRUCT,
    TYPE_ENUM,
    TYPE_TMPL,
    TYPE_OPTION,
    "params",
    TYPE_STR,
    TYPE_INT,
    TYPE_FLOAT,
    TYPE_BOOL,
];

#[cfg(test)]
mod tests {
    use super::*;

    // -- strip_string_literal -------------------------------------------------

    #[test]
    fn strip_double_quoted_string() {
        assert_eq!(strip_string_literal("\"hello\""), Some("hello"));
    }

    #[test]
    fn strip_single_quoted_string() {
        assert_eq!(strip_string_literal("'world'"), Some("world"));
    }

    #[test]
    fn strip_empty_double_quoted_string() {
        assert_eq!(strip_string_literal("\"\""), Some(""));
    }

    #[test]
    fn strip_empty_single_quoted_string() {
        assert_eq!(strip_string_literal("''"), Some(""));
    }

    #[test]
    fn strip_unquoted_string_returns_none() {
        assert_eq!(strip_string_literal("hello"), None);
    }

    #[test]
    fn strip_mismatched_quotes_returns_none() {
        assert_eq!(strip_string_literal("\"hello'"), None);
        assert_eq!(strip_string_literal("'hello\""), None);
    }

    #[test]
    fn strip_single_quote_char_returns_none() {
        assert_eq!(strip_string_literal("\""), None);
        assert_eq!(strip_string_literal("'"), None);
    }

    #[test]
    fn strip_empty_input_returns_none() {
        assert_eq!(strip_string_literal(""), None);
    }

    #[test]
    fn strip_quoted_string_with_spaces() {
        assert_eq!(strip_string_literal("\"hello world\""), Some("hello world"));
        assert_eq!(strip_string_literal("'hello world'"), Some("hello world"));
    }

    #[test]
    fn strip_quoted_string_with_inner_quotes() {
        // Inner quotes of the opposite kind are preserved.
        assert_eq!(strip_string_literal("\"it's\""), Some("it's"));
        assert_eq!(strip_string_literal("'say \"hi\"'"), Some("say \"hi\""));
    }

    // -- RESERVED_NAMES -------------------------------------------------------

    #[test]
    fn reserved_names_contains_type_names() {
        for name in &[
            "list", "struct", "enum", "tmpl", "option", "str", "int", "float", "bool",
        ] {
            assert!(
                RESERVED_NAMES.contains(name),
                "{name} should be in RESERVED_NAMES"
            );
        }
    }

    #[test]
    fn reserved_names_contains_params() {
        assert!(RESERVED_NAMES.contains(&"params"));
    }

    // -- BUILTIN_FUNCTIONS ----------------------------------------------------

    #[test]
    fn builtin_functions_contains_expected_entries() {
        assert!(BUILTIN_FUNCTIONS.contains(&"idx"));
        assert!(BUILTIN_FUNCTIONS.contains(&"len"));
        assert!(BUILTIN_FUNCTIONS.contains(&"kind"));
        assert!(BUILTIN_FUNCTIONS.contains(&"has"));
    }

    #[test]
    fn builtin_functions_length() {
        assert_eq!(BUILTIN_FUNCTIONS.len(), 4);
    }

    // -- Delimiter constants --------------------------------------------------

    #[test]
    fn expr_delimiters() {
        assert_eq!(EXPR_START, "{{");
        assert_eq!(EXPR_END, "}}");
    }

    #[test]
    fn stmt_delimiters() {
        assert_eq!(STMT_START, "{%");
        assert_eq!(STMT_END, "%}");
    }

    #[test]
    fn comment_delimiters() {
        assert_eq!(COMMENT_START, "{#");
        assert_eq!(COMMENT_END, "#}");
    }

    #[test]
    fn closing_block_tags() {
        assert_eq!(CLOSE_IF, "/if");
        assert_eq!(CLOSE_FOR, "/for");
        assert_eq!(CLOSE_RAW, "/raw");
        assert_eq!(CLOSE_TMPL, "/tmpl");
        assert_eq!(CLOSE_MATCH, "/match");
    }

    #[test]
    fn frontmatter_delimiter() {
        assert_eq!(FM_DELIMITER, "---");
    }

    #[test]
    fn enum_tag_key_value() {
        assert_eq!(ENUM_TAG_KEY, "__kind__");
    }

    #[test]
    fn syntax_chars() {
        assert_eq!(PAREN_OPEN, '(');
        assert_eq!(PAREN_CLOSE, ')');
        assert_eq!(PATH_SEP, '.');
        assert_eq!(PIPE, '|');
        assert_eq!(QUOTE_DOUBLE, '"');
        assert_eq!(QUOTE_SINGLE, '\'');
    }
}
