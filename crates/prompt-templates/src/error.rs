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
}

impl TemplateError {
    /// Create a [`Syntax`](Self::Syntax) error from any string-like value.
    ///
    /// This is the preferred constructor — use it instead of
    /// `TemplateError::Syntax(SyntaxError::new(...))` for brevity.
    pub(crate) fn syntax(msg: impl Into<String>) -> Self {
        Self::Syntax(SyntaxError::new(msg))
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
