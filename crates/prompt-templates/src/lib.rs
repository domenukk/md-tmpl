#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(feature = "std", doc = include_str!("../README.md"))]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[doc = include_str!("../SPEC.md")]
#[doc(hidden)]
pub mod spec {}

#[cfg(feature = "std")]
mod cache;
pub(crate) mod compat;
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
#[cfg(feature = "std")]
mod include;
mod include_core;
mod parser;
mod scope;
#[cfg(feature = "serde")]
mod serde_support;
mod template;
mod types;
mod value;

#[cfg(all(test, feature = "std"))]
mod inline_template_tests;

/// Hidden re-exports for use by proc-macro generated code.
///
/// These are not part of the public API — generated code references them
/// via `::prompt_templates::__private::*`.
#[doc(hidden)]
pub mod __private {
    pub use alloc::{borrow::Cow, boxed::Box, format, string::String, sync::Arc, vec, vec::Vec};

    pub use crate::compat::LazyLock;

    /// FNV-1a hash over raw bytes.
    ///
    /// Deterministic and stable across Rust versions (unlike
    /// `DefaultHasher`).  Not suitable for cryptographic use.
    #[must_use]
    pub fn fnv1a_hash(data: &[u8]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0100_0000_01b3;
        let mut hash = FNV_OFFSET;
        for &byte in data {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }
}

#[cfg(feature = "std")]
pub use cache::TemplateCache;
pub use context::Context;
pub use error::{SyntaxError, TemplateError};
#[doc(hidden)]
#[cfg(feature = "std")]
pub use frontmatter::parse_frontmatter_with_base_dir;
#[cfg(feature = "std")]
pub use frontmatter::resolve_imports;
pub use frontmatter::{
    Frontmatter, Import, ImportedNamespace, extract_template_stem, parse_frontmatter,
    parse_type_annotation, strip_frontmatter,
};
#[cfg(feature = "serde")]
pub use serde_support::{DeError, SerError, from_value, to_value};
#[cfg(feature = "std")]
pub use template::load_template;
pub use template::{CompileOptions, Template};
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
/// - `{ key: val, ... }` → `Value::Struct`
/// - `(expr)` → any expression via `Into<Value>`
///
/// # Examples
///
/// Simple values:
/// ```
/// use prompt_templates::{Template, ctx};
///
/// let tmpl = Template::from_source(
///     "\
/// ---
/// params: [greeting = str, name = str]
/// ---
/// {{ greeting }}, {{ name }}!",
/// )
/// .unwrap();
/// let output = tmpl
///     .render_ctx(&ctx! {
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
///     "\
/// ---
/// params: [items = list(label = str)]
/// ---
/// > {% for item in items %}
///
/// {{ item.label }}
///
/// > {% /for %}",
/// )
/// .unwrap();
/// let output = tmpl
///     .render_ctx(&ctx! {
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
        let mut ctx = $crate::Context::with_capacity($crate::__count!($($key)*));
        $(
            ctx.set(stringify!($key), $crate::__value!($val));
        )*
        ctx
    }};
}

/// Internal token-counting helper — not part of the public API.
#[macro_export]
#[doc(hidden)]
macro_rules! __count {
    () => { 0_usize };
    ($head:tt $($rest:tt)*) => { 1_usize + $crate::__count!($($rest)*) };
}

/// Internal recursive value builder — not part of the public API.
///
/// Converts token trees into [`Value`] instances:
/// - `[...]` → `Value::List(...)`
/// - `{...}` → `Value::Struct(...)`
/// - `(expr)` → `Value::from(expr)` (for runtime expressions)
/// - literal → `Value::from(literal)`
#[macro_export]
#[doc(hidden)]
macro_rules! __value {
    // Array → List
    ([ $($item:tt),* $(,)? ]) => {
        $crate::Value::List($crate::__private::Arc::new($crate::__private::vec![ $( $crate::__value!($item) ),* ]))
    };
    // Object → Struct
    ({ $($key:ident : $val:tt),* $(,)? }) => {
        $crate::Value::new_struct([
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
