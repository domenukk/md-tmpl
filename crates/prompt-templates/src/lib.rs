#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

// Run doc-tests from the full language specification.
#[doc = include_str!("../SPEC.md")]
#[doc(hidden)]
pub mod spec {}

mod cache;
#[doc(hidden)]
pub mod compiled;
/// Template grammar constants, syntax characters, and utility functions.
///
/// Contains the canonical definitions of expression delimiters, tag markers,
/// type names, and other tokens used by the template engine.
pub mod consts;
mod context;
mod error;
mod filter;
mod frontmatter;
mod include;
mod parser;
mod scope;
#[cfg(feature = "serde")]
mod serde_support;
mod template;
mod types;
mod value;

#[cfg(test)]
mod inline_template_tests;

pub use cache::TemplateCache;
pub use context::Context;
pub use error::{SyntaxError, TemplateError};
#[doc(hidden)]
pub use frontmatter::parse_frontmatter_with_base_dir;
pub use frontmatter::{
    Frontmatter, Import, ImportedNamespace, extract_template_stem, parse_frontmatter,
    resolve_imports, strip_frontmatter,
};
#[cfg(feature = "serde")]
pub use serde_support::{DeError, SerError, from_value, to_value};
pub use template::{Template, load_template};
pub use types::{
    BUILTIN_TYPE_NAMES, TypeCheckError, VarDecl, VarType, VariantDecl, to_pascal_case,
};
pub use value::{Value, ValueTypeError};

/// Construct a [`Context`] with JSON-like syntax.
///
/// Values are recursively converted:
/// - `"string"` → `Value::Str`
/// - `42_i64` → `Value::Int`
/// - `true` / `false` → `Value::Bool`
/// - `[a, b, c]` → `Value::List`
/// - `{ key: val, ... }` → `Value::Dict`
/// - `(expr)` → any expression via `Into<Value>`
///
/// # Examples
///
/// Simple values:
/// ```
/// use prompt_templates::{Template, ctx};
///
/// let tmpl = Template::from_source(
///     "---\n\
///      params: [greeting = str, name = str]\n\
///      ---\n\
///      {{ greeting }}, {{ name }}!",
/// )
/// .unwrap();
/// let output = tmpl
///     .render(&ctx! {
///         greeting: "Hello",
///         name: "world",
///     })
///     .unwrap();
/// assert_eq!(output, "Hello, world!");
/// ```
///
/// Nested dicts and lists:
/// ```
/// use prompt_templates::{Template, ctx};
///
/// let tmpl = Template::from_source(
///     "---\n\
///      params: [items = list<label = str>]\n\
///      ---\n\
///      > {% for item in items %}\n\
///      {{ item.label }}\n\
///      > {% /for %}",
/// )
/// .unwrap();
/// let output = tmpl
///     .render(&ctx! {
///         items: [
///             { label: "alpha" },
///             { label: "beta" },
///         ]
///     })
///     .unwrap();
/// assert_eq!(output, "alpha\nbeta\n");
/// ```
#[macro_export]
macro_rules! ctx {
    ($($key:ident : $val:tt),* $(,)?) => {{
        let mut ctx = $crate::Context::new();
        $(
            ctx.set(stringify!($key), $crate::__value!($val));
        )*
        ctx
    }};
}

/// Construct a [`Value::Dict`] with JSON-like syntax.
///
/// # Examples
///
/// ```
/// use prompt_templates::{Value, dict};
///
/// let item = dict! { label: "alpha", score: 42_i64 };
/// assert_eq!(item.get_field("label").unwrap().to_string(), "alpha");
/// ```
#[macro_export]
macro_rules! dict {
    ($($key:ident : $val:tt),* $(,)?) => {
        $crate::__value!({ $($key : $val),* })
    };
}

/// Internal recursive value builder — not part of the public API.
///
/// Converts token trees into [`Value`] instances:
/// - `[...]` → `Value::List(...)`
/// - `{...}` → `Value::Dict(...)`
/// - `(expr)` → `Value::from(expr)` (for runtime expressions)
/// - literal → `Value::from(literal)`
#[macro_export]
#[doc(hidden)]
macro_rules! __value {
    // Array → List
    ([ $($item:tt),* $(,)? ]) => {
        $crate::Value::List(vec![ $( $crate::__value!($item) ),* ])
    };
    // Object → Dict
    ({ $($key:ident : $val:tt),* $(,)? }) => {
        $crate::Value::dict([
            $( (stringify!($key), $crate::__value!($val)) ),*
        ])
    };
    // Parenthesized expression → runtime value
    (( $e:expr )) => {
        $crate::Value::from($e)
    };
    // Any single literal or ident (strings, numbers, bools, parameter names)
    ($other:expr) => {
        $crate::Value::from($other)
    };
}
