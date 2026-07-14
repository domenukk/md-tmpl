//! Template engine error types.

use alloc::{string::String, vec::Vec};
use core::fmt;

/// Errors produced by the template engine.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// I/O error while loading a template file.
    #[cfg(feature = "std")]
    #[error("failed to load template: {0}")]
    Io(#[from] std::io::Error),

    /// A referenced variable was not found in the context.
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),

    /// Syntax error in the template.
    #[error("template syntax error: {0}")]
    Syntax(SyntaxError),

    /// Missing required parameters from frontmatter.
    #[error("missing required parameters: {}", .0.join(", "))]
    MissingParams(Vec<String>),

    /// Type mismatch between declaration and value.
    #[error("type mismatch for '{name}': expected {expected}, got {actual} ({actual_value})")]
    TypeMismatch {
        /// Variable name.
        name: String,
        /// The type declared in frontmatter.
        expected: String,
        /// The type found in the context.
        actual: String,
        /// Preview of the actual value for debugging.
        actual_value: String,
    },

    /// Unknown filter name.
    #[error("unknown filter: {0}")]
    UnknownFilter(String),

    /// Include file not found.
    #[error("include not found: {0}")]
    IncludeNotFound(String),

    /// Parameter declarations were mutated at runtime.
    ///
    /// Template frontmatter `params:` declarations are fixed at compile
    /// time. If a runtime-reloaded template has different declarations, this
    /// error is returned — the template body may be changed freely, but the
    /// parameter contract must remain stable.
    #[error(
        "template parameter declarations were modified at runtime: {details}. \
         The frontmatter `params:` block is part of the compile-time \
         contract and must not be changed"
    )]
    DeclarationsMutated {
        /// Human-readable description of what changed.
        details: String,
    },

    /// Extra (undeclared) parameters were passed in the context.
    ///
    /// The template engine is strict by default: only parameters declared
    /// in frontmatter may be passed. Use `allow_extra_params` on the
    /// render call to suppress this check.
    #[error("extra undeclared parameters: {}", .0.join(", "))]
    ExtraParams(Vec<String>),

    /// Template rendering halted by an explicit `{% panic(...) %}` statement.
    #[error("template panic: {0}")]
    Panic(String),
}

impl TemplateError {
    /// Create a [`Panic`](Self::Panic) error from any string-like value.
    pub(crate) fn panic(msg: impl Into<String>) -> Self {
        Self::Panic(msg.into())
    }

    /// Create a [`Syntax`](Self::Syntax) error from any string-like value.
    ///
    /// This is the preferred constructor — use it instead of
    /// `TemplateError::Syntax(SyntaxError::new(...))` for brevity.
    pub(crate) fn syntax(msg: impl Into<String>) -> Self {
        Self::Syntax(SyntaxError::new(msg))
    }

    /// Return the stable, data-independent [`ErrorKind`] of this error.
    ///
    /// Unlike matching on the enum variants (which also carry payloads), this
    /// gives a lightweight, `Copy` discriminant that language bindings can map
    /// onto their own typed-error hierarchies without parsing the display
    /// message.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            #[cfg(feature = "std")]
            Self::Io(_) => ErrorKind::Io,
            Self::UndefinedVariable(_) => ErrorKind::UndefinedVariable,
            Self::Syntax(_) => ErrorKind::Syntax,
            Self::MissingParams(_) => ErrorKind::MissingParams,
            Self::TypeMismatch { .. } => ErrorKind::TypeMismatch,
            Self::UnknownFilter(_) => ErrorKind::UnknownFilter,
            Self::IncludeNotFound(_) => ErrorKind::IncludeNotFound,
            Self::DeclarationsMutated { .. } => ErrorKind::DeclarationsMutated,
            Self::ExtraParams(_) => ErrorKind::ExtraParams,
            Self::Panic(_) => ErrorKind::Panic,
        }
    }
}

/// A stable, `Copy` categorisation of a [`TemplateError`].
///
/// This mirrors the [`TemplateError`] variants but drops their payloads, giving
/// a discriminant that is cheap to pass around and stable across releases. It is
/// primarily intended for FFI and language bindings that expose a typed-error
/// hierarchy of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// I/O error while loading a template file. See [`TemplateError::Io`].
    Io,
    /// A referenced variable was not found. See [`TemplateError::UndefinedVariable`].
    UndefinedVariable,
    /// Syntax error in the template. See [`TemplateError::Syntax`].
    Syntax,
    /// Missing required parameters. See [`TemplateError::MissingParams`].
    MissingParams,
    /// Type mismatch. See [`TemplateError::TypeMismatch`].
    TypeMismatch,
    /// Unknown filter name. See [`TemplateError::UnknownFilter`].
    UnknownFilter,
    /// Include file not found. See [`TemplateError::IncludeNotFound`].
    IncludeNotFound,
    /// Declarations mutated at runtime. See [`TemplateError::DeclarationsMutated`].
    DeclarationsMutated,
    /// Extra undeclared parameters. See [`TemplateError::ExtraParams`].
    ExtraParams,
    /// Explicit `{% panic(...) %}`. See [`TemplateError::Panic`].
    Panic,
}

impl ErrorKind {
    /// Return the stable, machine-readable identifier for this kind.
    ///
    /// These identifiers are part of the public contract (they cross the FFI
    /// boundary), so they must not change between releases.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::UndefinedVariable => "undefined_variable",
            Self::Syntax => "syntax",
            Self::MissingParams => "missing_params",
            Self::TypeMismatch => "type_mismatch",
            Self::UnknownFilter => "unknown_filter",
            Self::IncludeNotFound => "include_not_found",
            Self::DeclarationsMutated => "declarations_mutated",
            Self::ExtraParams => "extra_params",
            Self::Panic => "panic",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A structured syntax error with optional line number and source context.
///
/// Callers that match on [`TemplateError::Syntax`] can inspect `line` and
/// `snippet` programmatically instead of parsing the error message string.
#[derive(Debug, Clone)]
pub struct SyntaxError {
    /// The error message (without location prefix).
    pub message: String,
    /// 1-based line number where the error occurred (if known).
    pub line: Option<usize>,
    /// Snippet of the offending source line (if available).
    pub snippet: Option<String>,
}

impl SyntaxError {
    /// Create a syntax error with just a message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            snippet: None,
        }
    }

    /// Attach a line number and source snippet.
    #[must_use]
    pub fn at_line(mut self, line: usize, snippet: impl Into<String>) -> Self {
        self.line = Some(line);
        self.snippet = Some(snippet.into());
        self
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.snippet.as_deref()) {
            (Some(line), Some(snippet)) => {
                write!(f, "line {line}: {}\n  --> {snippet}", self.message)
            }
            (Some(line), None) => write!(f, "line {line}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

/// Allow `TemplateError::Syntax("message".to_string().into())` etc.
impl From<String> for SyntaxError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

/// Compute the Levenshtein edit distance between two strings.
///
/// Returns the minimum number of single-character edits (insertions,
/// deletions, or substitutions) required to transform `a` into `b`.
pub(crate) fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        core::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    // ── SyntaxError::new ────────────────────────────────────────────

    #[test]
    fn syntax_error_new_sets_message() {
        let err = SyntaxError::new("unexpected token");
        assert_eq!(err.message, "unexpected token");
    }

    #[test]
    fn syntax_error_new_defaults_line_to_none() {
        let err = SyntaxError::new("oops");
        assert!(err.line.is_none());
    }

    #[test]
    fn syntax_error_new_defaults_snippet_to_none() {
        let err = SyntaxError::new("oops");
        assert!(err.snippet.is_none());
    }

    // ── SyntaxError::at_line ────────────────────────────────────────

    #[test]
    fn syntax_error_at_line_sets_line_and_snippet() {
        let err = SyntaxError::new("bad token").at_line(42, "{{ bad }}");
        assert_eq!(err.line, Some(42));
        assert_eq!(err.snippet.as_deref(), Some("{{ bad }}"));
        assert_eq!(err.message, "bad token");
    }

    #[test]
    fn syntax_error_at_line_line_one() {
        let err = SyntaxError::new("err").at_line(1, "line1");
        assert_eq!(err.line, Some(1));
    }

    // ── SyntaxError Display ─────────────────────────────────────────

    #[test]
    fn syntax_error_display_with_line_and_snippet() {
        let err = SyntaxError::new("unexpected end").at_line(7, "{{ if");
        let formatted = err.to_string();
        assert_eq!(formatted, "line 7: unexpected end\n  --> {{ if");
    }

    #[test]
    fn syntax_error_display_with_line_only() {
        let mut err = SyntaxError::new("missing bracket");
        err.line = Some(3);
        // snippet is None
        let formatted = err.to_string();
        assert_eq!(formatted, "line 3: missing bracket");
    }

    #[test]
    fn syntax_error_display_message_only() {
        let err = SyntaxError::new("generic problem");
        assert_eq!(err.to_string(), "generic problem");
    }

    #[test]
    fn syntax_error_display_message_only_when_snippet_without_line() {
        // Edge case: snippet set but line is None → falls into the catch-all
        let mut err = SyntaxError::new("edge case");
        err.snippet = Some("some snippet".into());
        // line is None, so the `_` branch fires
        assert_eq!(err.to_string(), "edge case");
    }

    // ── From<String> for SyntaxError ────────────────────────────────

    #[test]
    fn syntax_error_from_string() {
        let s = String::from("converted message");
        let err: SyntaxError = s.into();
        assert_eq!(err.message, "converted message");
        assert!(err.line.is_none());
        assert!(err.snippet.is_none());
    }

    // ── TemplateError::syntax() convenience ─────────────────────────

    #[test]
    fn template_error_syntax_constructor() {
        let err = TemplateError::syntax("bad template");
        match &err {
            TemplateError::Syntax(inner) => {
                assert_eq!(inner.message, "bad template");
                assert!(inner.line.is_none());
            }
            other => panic!("expected Syntax variant, got: {other}"),
        }
    }

    #[test]
    fn template_error_syntax_accepts_string() {
        let err = TemplateError::syntax(String::from("owned msg"));
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // ── TemplateError Display for every non-Io variant ──────────────

    #[test]
    fn template_error_display_undefined_variable() {
        let err = TemplateError::UndefinedVariable("user_name".into());
        assert_eq!(err.to_string(), "undefined variable: user_name");
    }

    #[test]
    fn template_error_display_syntax() {
        let err = TemplateError::Syntax(SyntaxError::new("unexpected end of input"));
        assert_eq!(
            err.to_string(),
            "template syntax error: unexpected end of input"
        );
    }

    #[test]
    fn template_error_display_missing_params() {
        let err = TemplateError::MissingParams(vec!["alpha".into(), "beta".into()]);
        assert_eq!(err.to_string(), "missing required parameters: alpha, beta");
    }

    #[test]
    fn template_error_display_missing_params_single() {
        let err = TemplateError::MissingParams(vec!["only".into()]);
        assert_eq!(err.to_string(), "missing required parameters: only");
    }

    #[test]
    fn template_error_display_type_mismatch() {
        let err = TemplateError::TypeMismatch {
            name: "count".into(),
            expected: "int".into(),
            actual: "string".into(),
            actual_value: "\"hello\"".into(),
        };
        assert_eq!(
            err.to_string(),
            "type mismatch for 'count': expected int, got string (\"hello\")"
        );
    }

    #[test]
    fn template_error_display_unknown_filter() {
        let err = TemplateError::UnknownFilter("capitalize".into());
        assert_eq!(err.to_string(), "unknown filter: capitalize");
    }

    #[test]
    fn template_error_display_include_not_found() {
        let err = TemplateError::IncludeNotFound("header.tmpl".into());
        assert_eq!(err.to_string(), "include not found: header.tmpl");
    }

    #[test]
    fn template_error_display_declarations_mutated() {
        let err = TemplateError::DeclarationsMutated {
            details: "added parameter 'foo'".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("declarations were modified at runtime"));
        assert!(msg.contains("added parameter 'foo'"));
    }

    #[test]
    fn template_error_display_extra_params() {
        let err = TemplateError::ExtraParams(vec!["x".into(), "y".into()]);
        assert_eq!(err.to_string(), "extra undeclared parameters: x, y");
    }

    #[test]
    fn template_error_display_panic() {
        let err = TemplateError::panic("custom panic message");
        assert_eq!(err.to_string(), "template panic: custom panic message");
    }

    // ── ErrorKind ───────────────────────────────────────────────────

    #[test]
    fn error_kind_maps_every_variant() {
        assert_eq!(
            TemplateError::UndefinedVariable("x".into()).kind(),
            ErrorKind::UndefinedVariable
        );
        assert_eq!(TemplateError::syntax("bad").kind(), ErrorKind::Syntax);
        assert_eq!(
            TemplateError::MissingParams(vec!["a".into()]).kind(),
            ErrorKind::MissingParams
        );
        assert_eq!(
            TemplateError::TypeMismatch {
                name: "n".into(),
                expected: "int".into(),
                actual: "str".into(),
                actual_value: "\"x\"".into(),
            }
            .kind(),
            ErrorKind::TypeMismatch
        );
        assert_eq!(
            TemplateError::UnknownFilter("f".into()).kind(),
            ErrorKind::UnknownFilter
        );
        assert_eq!(
            TemplateError::IncludeNotFound("i".into()).kind(),
            ErrorKind::IncludeNotFound
        );
        assert_eq!(
            TemplateError::DeclarationsMutated {
                details: "d".into()
            }
            .kind(),
            ErrorKind::DeclarationsMutated
        );
        assert_eq!(
            TemplateError::ExtraParams(vec!["e".into()]).kind(),
            ErrorKind::ExtraParams
        );
        assert_eq!(TemplateError::panic("p").kind(), ErrorKind::Panic);
    }

    #[test]
    fn error_kind_as_str_is_stable() {
        // These identifiers cross the FFI boundary and are part of the public
        // contract — pin them so an accidental rename is caught.
        assert_eq!(ErrorKind::Io.as_str(), "io");
        assert_eq!(ErrorKind::UndefinedVariable.as_str(), "undefined_variable");
        assert_eq!(ErrorKind::Syntax.as_str(), "syntax");
        assert_eq!(ErrorKind::MissingParams.as_str(), "missing_params");
        assert_eq!(ErrorKind::TypeMismatch.as_str(), "type_mismatch");
        assert_eq!(ErrorKind::UnknownFilter.as_str(), "unknown_filter");
        assert_eq!(ErrorKind::IncludeNotFound.as_str(), "include_not_found");
        assert_eq!(
            ErrorKind::DeclarationsMutated.as_str(),
            "declarations_mutated"
        );
        assert_eq!(ErrorKind::ExtraParams.as_str(), "extra_params");
        assert_eq!(ErrorKind::Panic.as_str(), "panic");
    }

    #[test]
    fn error_kind_display_matches_as_str() {
        assert_eq!(ErrorKind::MissingParams.to_string(), "missing_params");
    }

    // ── levenshtein_distance ────────────────────────────────────────

    #[test]
    fn levenshtein_identical_strings() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn levenshtein_one_empty() {
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "xyz"), 3);
    }

    #[test]
    fn levenshtein_single_char_diff() {
        assert_eq!(levenshtein_distance("cat", "bat"), 1);
    }

    #[test]
    fn levenshtein_kitten_sitting() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert_eq!(levenshtein_distance("abc", "xyz"), 3);
    }

    #[test]
    fn levenshtein_prefix() {
        assert_eq!(levenshtein_distance("abc", "abcdef"), 3);
    }

    #[test]
    fn levenshtein_single_insertion() {
        assert_eq!(levenshtein_distance("ac", "abc"), 1);
    }

    #[test]
    fn levenshtein_single_deletion() {
        assert_eq!(levenshtein_distance("abc", "ac"), 1);
    }

    #[test]
    fn levenshtein_symmetric() {
        assert_eq!(
            levenshtein_distance("foo", "bar"),
            levenshtein_distance("bar", "foo")
        );
    }
}
