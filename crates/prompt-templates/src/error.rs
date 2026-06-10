//! Template engine error types.

use std::fmt;

/// Errors produced by the template engine.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// I/O error while loading a template file.
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
