#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[macro_use]
extern crate alloc;

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
#[cfg(feature = "serde")]
mod sanitize_serde;
mod scope;
#[cfg(feature = "serde")]
mod serde_support;
mod template;
mod types;
mod value;

/// Hidden re-exports for use by proc-macro generated code.
///
/// These are not part of the public API — generated code references them
/// via `::md_tmpl::__private::*`.
#[doc(hidden)]
pub mod __private {
    pub use alloc::{borrow::Cow, boxed::Box, format, string::String, sync::Arc, vec, vec::Vec};

    /// Re-export of the `typed_builder` crate and its derive so generated param
    /// structs get a builder without the downstream crate depending on it
    /// directly. Generated structs derive `#crate::__private::TypedBuilder` and
    /// set `#[builder(crate_module_path = #crate::__private::typed_builder)]` so
    /// the derive's internal references resolve through this re-export.
    pub use ::typed_builder;
    pub use ::typed_builder::TypedBuilder;
    pub use hashbrown::HashMap;

    pub use crate::{
        compat::LazyLock,
        compiled::render::{
            cmp_int_float, render_str_filters, write_fixed_float, write_fixed_int, write_float,
            write_int, write_lower, write_upper,
        },
        filter::i64_to_f64,
        template::analysis::inject_enum_type_constants,
    };

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

    /// Copy-able static scalar representation for compile-time constant promotion to `.rodata`.
    #[derive(Clone, Copy)]
    pub enum StaticScalar<'a> {
        Str(&'a str),
        Int(i64),
        Float(u64),
        Bool(bool),
        None,
    }

    impl StaticScalar<'_> {
        #[must_use]
        pub fn into_value(self) -> crate::Value {
            match self {
                Self::Str(s) => crate::Value::Str(String::from(s)),
                Self::Int(i) => crate::Value::Int(i),
                Self::Float(bits) => crate::Value::Float(f64::from_bits(bits)),
                Self::Bool(b) => crate::Value::Bool(b),
                Self::None => crate::Value::None,
            }
        }
    }

    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn make_str_value(s: &str) -> crate::Value {
        crate::Value::Str(String::from(s))
    }

    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn make_scalar_list_value(items: &[StaticScalar<'_>]) -> crate::Value {
        crate::Value::List(Arc::new(
            items
                .iter()
                .copied()
                .map(StaticScalar::into_value)
                .collect(),
        ))
    }

    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn make_scalar_struct_value(fields: &[(&str, StaticScalar<'_>)]) -> crate::Value {
        let mut map = HashMap::with_capacity(fields.len());
        for &(k, v) in fields {
            map.insert(String::from(k), v.into_value());
        }
        crate::Value::Struct(Arc::new(map))
    }

    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn make_scalar_struct_list_value(items: &[&[(&str, StaticScalar<'_>)]]) -> crate::Value {
        crate::Value::List(Arc::new(
            items
                .iter()
                .map(|fields| make_scalar_struct_value(fields))
                .collect(),
        ))
    }

    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn make_enum_namespace_value(
        unit_variants: &[&str],
        data_variants: &[&str],
        all_variants: &[&str],
    ) -> crate::Value {
        let mut map = HashMap::with_capacity(unit_variants.len() + data_variants.len() + 1);
        for &v in unit_variants {
            map.insert(String::from(v), crate::Value::Str(String::from(v)));
        }
        for &v in data_variants {
            let mut partial = HashMap::with_capacity(1);
            partial.insert(
                String::from(crate::consts::ENUM_TAG_KEY),
                crate::Value::Str(String::from(v)),
            );
            map.insert(String::from(v), crate::Value::Struct(Arc::new(partial)));
        }
        let variant_list = all_variants
            .iter()
            .map(|&v| crate::Value::Str(String::from(v)))
            .collect();
        map.insert(
            String::from(crate::consts::ENUM_VARIANTS_KEY),
            crate::Value::List(Arc::new(variant_list)),
        );
        crate::Value::Struct(Arc::new(map))
    }
}

#[cfg(feature = "std")]
pub use cache::TemplateCache;
pub use context::Context;
pub use error::{ErrorDetails, ErrorKind, SyntaxError, TemplateError};
pub use filter::{
    DEFAULT_LIST_TRUNCATE_MARKER, DEFAULT_QUARANTINE_TAG, DEFAULT_SANITIZE_NOTICE,
    DEFAULT_SANITIZE_TAG, DEFAULT_TRUNCATE_MARKER, DEFAULT_TRUNCATE_MARKER_COMPACT,
    ROLE_TOKEN_DELIMITERS, SANITIZE_NOTICE_TAG_PLACEHOLDER, TOKEN_DELIMITERS,
    TOOL_OUTPUT_QUARANTINE_CLOSE_TAG, TOOL_OUTPUT_QUARANTINE_OPEN_TAG, TOOL_OUTPUT_QUARANTINE_TAG,
    TRUNCATE_PLACEHOLDER_COUNT, TRUNCATE_PLACEHOLDER_SKIPPED, UNTRUSTED_QUARANTINE_CLOSE_TAG,
    UNTRUSTED_QUARANTINE_OPEN_TAG, UNTRUSTED_TOOL_OUTPUT_TAG, XML_DECODE_PAIRS, ceil_char_boundary,
    decode_xml_str, escape_json_str, escape_xml_attr_str, escape_xml_body_str, escape_xml_str,
    fence_str, floor_char_boundary, format_sanitize_notice, has_control_tokens,
    has_quarantine_tag_breakout, has_role_control_tokens, has_untrusted_breakout,
    is_quarantined_str, is_sanitized_block_str, next_char_boundary, prev_char_boundary,
    quarantine_str, quarantine_untrusted_idempotent_str, quarantine_untrusted_str,
    sanitize_block_idempotent_str, sanitize_block_into, sanitize_block_str,
    sanitize_quarantine_payload, sanitize_role_tokens_str, sanitize_str, sanitize_tokens_str,
    sanitize_untrusted_str, strip_outer_quarantine_tag, truncate_middle_list, truncate_middle_str,
    unquarantine_str, unsanitize_block_str,
};
#[doc(hidden)]
#[cfg(feature = "std")]
pub use frontmatter::parse_frontmatter_with_base_dir;
pub use frontmatter::{
    Frontmatter, Import, ImportedNamespace, extract_template_stem, parse_frontmatter,
    parse_frontmatter_with_env, parse_type_annotation, strip_frontmatter,
};
#[cfg(feature = "std")]
pub use frontmatter::{resolve_imports, resolve_imports_with_consts};
#[cfg(feature = "serde")]
pub use sanitize_serde::{
    StringSanitizeMode, TransformingCompound, TransformingSerializer, serialize_with_sanitization,
    to_sanitized_value,
};
#[cfg(feature = "serde")]
pub use serde_support::{DeError, SerError, from_value, to_value};
#[cfg(feature = "std")]
pub use template::load_template;
pub use template::{CompileOptions, PrecompiledTemplateData, Template};
pub use types::{
    BUILTIN_TYPE_NAMES, SanitizeSpec, TypeCheckError, VarDecl, VarType, VariantDecl, to_pascal_case,
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
/// use md_tmpl_core::{Template, ctx};
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
/// use md_tmpl_core::{Template, ctx};
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

#[cfg(test)]
mod private_tests {
    use alloc::{string::String, sync::Arc};

    use super::{
        __private::{
            StaticScalar, make_enum_namespace_value, make_scalar_list_value,
            make_scalar_struct_list_value, make_scalar_struct_value, make_str_value,
        },
        Context, Value,
        compat::HashMap,
        scope::Scope,
    };

    #[test]
    fn static_scalar_into_value_covers_all_variants() {
        assert_eq!(
            StaticScalar::Str("hello").into_value(),
            Value::Str(String::from("hello"))
        );
        assert_eq!(StaticScalar::Int(-42).into_value(), Value::Int(-42));
        assert_eq!(
            StaticScalar::Float(2.5_f64.to_bits()).into_value(),
            Value::Float(2.5)
        );
        assert_eq!(StaticScalar::Bool(true).into_value(), Value::Bool(true));
        assert_eq!(StaticScalar::None.into_value(), Value::None);
    }

    #[test]
    fn make_value_helpers_construct_expected_values() {
        assert_eq!(make_str_value("abc"), Value::Str(String::from("abc")));

        let list_val = make_scalar_list_value(&[
            StaticScalar::Str("a"),
            StaticScalar::Int(1),
            StaticScalar::Bool(false),
        ]);
        assert_eq!(
            list_val,
            Value::List(Arc::new(alloc::vec![
                Value::Str(String::from("a")),
                Value::Int(1),
                Value::Bool(false),
            ]))
        );

        let struct_val = make_scalar_struct_value(&[
            ("title", StaticScalar::Str("task")),
            ("priority", StaticScalar::Int(3)),
        ]);
        let Value::Struct(ref fields) = struct_val else {
            panic!("expected Value::Struct");
        };
        assert_eq!(fields.get("title"), Some(&Value::Str(String::from("task"))));
        assert_eq!(fields.get("priority"), Some(&Value::Int(3)));

        let struct_list = make_scalar_struct_list_value(&[
            &[("k", StaticScalar::Str("v1"))],
            &[("k", StaticScalar::Str("v2"))],
        ]);
        let Value::List(ref items) = struct_list else {
            panic!("expected Value::List");
        };
        assert_eq!(items.len(), 2);

        let enum_ns = make_enum_namespace_value(
            &["Open", "Closed"],
            &["Failed"],
            &["Open", "Closed", "Failed"],
        );
        let Value::Struct(ref ns_fields) = enum_ns else {
            panic!("expected Value::Struct for enum namespace");
        };
        assert_eq!(
            ns_fields.get("Open"),
            Some(&Value::Str(String::from("Open")))
        );
        let Some(Value::Struct(failed_fields)) = ns_fields.get("Failed") else {
            panic!("expected Value::Struct for data variant");
        };
        assert_eq!(
            failed_fields.get(crate::consts::ENUM_TAG_KEY),
            Some(&Value::Str(String::from("Failed")))
        );
        let Some(Value::List(variants)) = ns_fields.get(crate::consts::ENUM_VARIANTS_KEY) else {
            panic!("expected variants list");
        };
        assert_eq!(variants.len(), 3);
    }

    #[test]
    fn scope_push_consts_arc_shares_arcs_and_resolves() {
        let ctx = Context::new();
        let mut scope = Scope::new(&ctx);

        let mut c_map = HashMap::new();
        c_map.insert(String::from("LOCAL_C"), Value::Int(7));
        let c_arc = Arc::new(c_map);

        let mut imp_map = HashMap::new();
        imp_map.insert(String::from("ns.REMOTE_C"), Value::Str(String::from("ok")));
        let imp_arc = Arc::new(imp_map);

        scope.push_consts_arc(&c_arc, &imp_arc);
        assert_eq!(Arc::strong_count(&c_arc), 2);
        assert_eq!(Arc::strong_count(&imp_arc), 2);
        assert_eq!(scope.resolve("LOCAL_C"), Some(&Value::Int(7)));
        assert_eq!(
            scope.resolve_path_str("ns.REMOTE_C").unwrap(),
            &Value::Str(String::from("ok"))
        );

        scope.pop_consts();
        assert_eq!(Arc::strong_count(&c_arc), 1);
        assert_eq!(Arc::strong_count(&imp_arc), 1);
        assert_eq!(scope.resolve("LOCAL_C"), None);
    }
}
