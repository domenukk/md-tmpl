#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod codegen;
mod compile;
mod struct_gen;
mod type_gen;

use std::cell::RefCell;

use codegen::{codegen_compiled_inline_template, codegen_segment, codegen_value, codegen_var_decl};
use compile::{CompiledTemplateAst, load_and_compile, stem_from_path};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use struct_gen::{StructGenSource, generate_struct_tokens};
use syn::{
    Ident, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};
use type_gen::generate_type_alias_tokens;

thread_local! {
    /// The crate path to use in generated code.
    ///
    /// Defaults to `::md_tmpl` but can be overridden via the
    /// `crate = path` argument in `include_template!` / `template!`.
    /// Using a thread-local avoids threading the value through every
    /// codegen helper.
    static CRATE_PATH: RefCell<proc_macro2::TokenStream> = RefCell::new(quote! { ::md_tmpl });
}

/// Read the current crate path from the thread-local.
pub(crate) fn crate_path() -> proc_macro2::TokenStream {
    CRATE_PATH.with(|cp| cp.borrow().clone())
}

/// Set the crate path for the duration of the closure, then restore it.
fn with_crate_path<F: FnOnce() -> R, R>(path: proc_macro2::TokenStream, f: F) -> R {
    CRATE_PATH.with(|cp| {
        let old = cp.replace(path);
        let result = f();
        cp.replace(old);
        result
    })
}

/// Parsed input for `include_template!("path")`,
/// `include_template!("path" => custom_mod_name)`,
/// `include_template!("path" as StructName)`,
/// `include_template!("path" as StructName => custom_mod_name)`,
/// `include_template!("path", crate = ::my_crate::reexport)`, or
/// `include_template!("path", env = { KEY: "value", KEY2: 42 })`.
struct IncludeTemplateInput {
    path: LitStr,
    struct_name: Option<Ident>,
    custom_name: Option<Ident>,
    crate_path: Option<syn::Path>,
    env: Vec<(String, syn::Expr)>,
}

impl Parse for IncludeTemplateInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;

        // Optional: `as StructName`
        let struct_name = if input.peek(Token![as]) {
            let _as: Token![as] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };

        // Optional: `=> custom_mod_name`
        let custom_name = if input.peek(Token![=>]) {
            let _arrow: Token![=>] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };

        // Optional trailing arguments: `, crate = ...` or `, env = { ... }`
        let mut crate_path = None;
        let mut env = Vec::new();
        while input.peek(Token![,]) {
            let _comma: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            if input.peek(Token![crate]) {
                let _kw: Token![crate] = input.parse()?;
                let _eq: Token![=] = input.parse()?;
                crate_path = Some(input.parse()?);
            } else {
                let kw: Ident = input.parse()?;
                if kw == "env" {
                    let _eq: Token![=] = input.parse()?;
                    env = parse_env_block(input)?;
                } else {
                    return Err(syn::Error::new(
                        kw.span(),
                        format!("unknown option '{kw}', expected 'crate' or 'env'"),
                    ));
                }
            }
        }

        Ok(Self {
            path,
            struct_name,
            custom_name,
            crate_path,
            env,
        })
    }
}

/// Parsed input for `template!(r#"source"# => mod_name)`.
///
/// The `=> name` is **required** because inline templates have no file path
/// from which to derive a module name.
struct InlineTemplateInput {
    source: LitStr,
    struct_name: Option<Ident>,
    name: Ident,
    crate_path: Option<syn::Path>,
    env: Vec<(String, syn::Expr)>,
}

impl Parse for InlineTemplateInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let source: LitStr = input.parse()?;
        let struct_name = if input.peek(Token![as]) {
            let _as: Token![as] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };
        let _: Token![=>] = input.parse()?;
        let name: Ident = input.parse()?;
        let mut crate_path = None;
        let mut env = Vec::new();
        while input.peek(Token![,]) {
            let _comma: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            if input.peek(Token![crate]) {
                let _kw: Token![crate] = input.parse()?;
                let _eq: Token![=] = input.parse()?;
                crate_path = Some(input.parse()?);
            } else {
                let kw: Ident = input.parse()?;
                if kw == "env" {
                    let _eq: Token![=] = input.parse()?;
                    env = parse_env_block(input)?;
                } else {
                    return Err(syn::Error::new(
                        kw.span(),
                        format!("unknown option '{kw}', expected 'crate' or 'env'"),
                    ));
                }
            }
        }
        Ok(Self {
            source,
            struct_name,
            name,
            crate_path,
            env,
        })
    }
}

/// Strict, reserved, and weak keywords in Rust that require `r#` when used
/// as identifiers.  Sourced from the Rust Reference.
const RUST_KEYWORDS: &[&str] = &[
    // Strict keywords
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", // Reserved keywords
    "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized",
    "virtual", "yield", "try", // Weak keyword used in specific contexts
    "union",
];

/// Keywords that cannot be used as raw identifiers (`r#self` is a compile
/// error).  For these we prefix with `__` and emit `#[serde(rename = "…")]`.
const UNESCAPABLE_KEYWORDS: &[&str] = &["self", "Self", "super", "crate"];

/// Create an identifier, using raw syntax (`r#name`) when `name` is a Rust
/// keyword.  For the four keywords that cannot be raw identifiers (`self`,
/// `Self`, `super`, `crate`), we prefix with `__` instead (e.g. `__self`).
///
/// Works for module names, field names, and any user-provided identifier that
/// might collide with a keyword.
pub(crate) fn make_ident(name: &str) -> Ident {
    if UNESCAPABLE_KEYWORDS.contains(&name) {
        format_ident!("__{}", name)
    } else if RUST_KEYWORDS.contains(&name) {
        format_ident!("r#{}", name)
    } else {
        Ident::new(name, proc_macro2::Span::call_site())
    }
}

/// Returns a `#[serde(rename = "original")]` attribute token stream when the
/// name was mangled by [`make_ident`] (i.e., one of the un-escapable keywords).
/// Returns an empty token stream otherwise — safe to interpolate unconditionally.
pub(crate) fn serde_rename_attr(name: &str) -> proc_macro2::TokenStream {
    if UNESCAPABLE_KEYWORDS.contains(&name) {
        quote! { #[serde(rename = #name)] }
    } else {
        quote! {}
    }
}

/// Parse an env block: `{ KEY: expr, KEY2: expr, ... }`.
///
/// Each key is an identifier and each value is a literal expression
/// (string, int, float, or bool).
fn parse_env_block(input: ParseStream) -> syn::Result<Vec<(String, syn::Expr)>> {
    let content;
    syn::braced!(content in input);
    let mut entries = Vec::new();
    while !content.is_empty() {
        let key: Ident = content.parse()?;
        let _colon: Token![:] = content.parse()?;
        let expr: syn::Expr = content.parse()?;
        entries.push((key.to_string(), expr));
        if content.peek(Token![,]) {
            let _comma: Token![,] = content.parse()?;
        }
    }
    Ok(entries)
}

/// Evaluate an env expression at proc-macro expansion time.
///
/// Supports:
/// - String literals: `"value"` → `Value::Str("value")`
/// - Integer literals: `42` → `Value::Int(42)`
/// - Float literals: `3.14` → `Value::Float(3.14)`
/// - Bool literals: `true`/`false` → `Value::Bool(true/false)`
fn eval_env_expr(expr: &syn::Expr) -> Result<md_tmpl_core::Value, String> {
    match expr {
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Str(s) => Ok(md_tmpl_core::Value::Str(s.value())),
            syn::Lit::Int(i) => {
                let n: i64 = i
                    .base10_parse()
                    .map_err(|e| format!("invalid integer: {e}"))?;
                Ok(md_tmpl_core::Value::Int(n))
            }
            syn::Lit::Float(f) => {
                let n: f64 = f
                    .base10_parse()
                    .map_err(|e| format!("invalid float: {e}"))?;
                Ok(md_tmpl_core::Value::Float(n))
            }
            syn::Lit::Bool(b) => Ok(md_tmpl_core::Value::Bool(b.value)),
            _ => Err("env value must be a string, int, float, or bool literal".to_string()),
        },
        // Handle `true` and `false` as path expressions (syn parses
        // bare `true`/`false` as Expr::Path, not Expr::Lit, in some contexts).
        syn::Expr::Path(p) => {
            if p.path.is_ident("true") {
                Ok(md_tmpl_core::Value::Bool(true))
            } else if p.path.is_ident("false") {
                Ok(md_tmpl_core::Value::Bool(false))
            } else {
                Err(format!(
                    "env value must be a literal, got path: {}",
                    quote! { #expr }
                ))
            }
        }
        _ => Err(format!(
            "env value must be a literal, got: {}",
            quote! { #expr }
        )),
    }
}

/// Helper: convert a `load_and_compile` error into a compile error token stream.
fn err_tokens(span: proc_macro2::Span, rel_path: &str, e: &str) -> TokenStream {
    let msg = format!("template '{rel_path}': {e}");
    syn::Error::new(span, msg).to_compile_error().into()
}

/// Pre-parse and validate a `.tmpl.md` template at compile time and emit a
/// complete module.
///
/// # Syntax
///
/// ```text
/// include_template!("path/to/template.tmpl.md");
/// include_template!("path/to/template.tmpl.md" => custom_mod);
/// ```
///
/// When no custom name is given the module name is derived from the file stem
/// (e.g. `greeting` from `greeting.tmpl.md`).
///
/// # Generated module contents
///
/// * `pub fn template() -> &'static Template` — the pre-compiled template
///   singleton.
/// * `pub struct Params { … }` — typed parameter struct with:
///   - `render()` — zero-arg render using the embedded template.
///   - `render_reloaded(tmpl)` — render with a hot-reloaded template
///     from disk.
///   - `validate_template(tmpl)` — check template compatibility.
///   - `to_context()` — convert to a `Context`.
/// * Sub-structs for compound types.
/// * Constants from the `consts:` block.
/// * Type aliases from the `types:` block.
///
/// # Examples
///
/// ```rust
/// extern crate md_tmpl_core as md_tmpl;
/// md_tmpl_macros::include_template!("prompts/simple_greeting.tmpl.md");
///
/// let output = simple_greeting::Params {
///     name: "World".into(),
/// }
/// .render()
/// .unwrap();
/// assert_eq!(output, "\nHello World!\n");
/// ```
///
/// # Panics
///
/// Panics if an `env` expression cannot be evaluated at macro expansion time.
#[proc_macro]
pub fn include_template(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as IncludeTemplateInput);
    let rel_path = parsed.path.value();

    // Evaluate env expressions at macro expansion time.
    let env_values: Vec<(String, md_tmpl_core::Value)> = parsed
        .env
        .iter()
        .map(|(k, expr)| {
            let val = eval_env_expr(expr).unwrap_or_else(|e| panic!("env '{k}': {e}"));
            (k.clone(), val)
        })
        .collect();
    let env_refs: Vec<(&str, md_tmpl_core::Value)> = env_values
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let (full_path, ast) = match load_and_compile(&rel_path, &env_refs) {
        Ok(v) => v,
        Err(e) => return err_tokens(parsed.path.span(), &rel_path, &e),
    };
    let CompiledTemplateAst {
        frontmatter: fm,
        segments,
        inline_templates,
        source_hash,
    } = ast;
    let path_str = full_path.to_string_lossy().to_string();

    // Module name: custom or derived from file stem.
    let mod_ident = match parsed.custom_name {
        Some(ident) => ident,
        None => make_ident(&stem_from_path(&rel_path)),
    };

    // Crate path: custom or default `::md_tmpl`.
    let crate_path = parsed
        .crate_path
        .map_or_else(|| quote! { ::md_tmpl }, |p| quote! { #p });

    with_crate_path(crate_path.clone(), || {
        // Template AST codegen.
        let segments_tokens = segments.iter().map(codegen_segment);
        let decls_tokens = fm.declarations.iter().map(codegen_var_decl);
        let inline_templates_tokens = inline_templates.iter().map(|(k, v)| {
            let v_tokens = codegen_compiled_inline_template(v);
            quote! { (#k, #v_tokens) }
        });
        let consts_tokens = fm.consts.iter().chain(fm.env.iter()).filter_map(|d| {
            d.default_value.as_ref().map(|v| {
                let name = &d.name;
                let val_tokens = codegen_value(v);
                quote! { (#name, #val_tokens) }
            })
        });
        let imported_consts_tokens = fm.imported_consts.iter().map(|(k, v)| {
            let val_tokens = codegen_value(v);
            quote! { (#k, #val_tokens) }
        });

        // Params struct codegen.
        let struct_name = parsed
            .struct_name
            .unwrap_or_else(|| format_ident!("Params"));
        let source = StructGenSource::Module {
            doc_path: &rel_path,
        };
        let struct_tokens = generate_struct_tokens(&fm, &struct_name, &source);

        // Type alias codegen.
        let type_alias_tokens = generate_type_alias_tokens(&fm.type_aliases);

        let name_token = if let Some(n) = &fm.name {
            quote! { Some(#n) }
        } else {
            quote! { None }
        };
        let desc_token = if let Some(d) = &fm.description {
            quote! { Some(#d) }
        } else {
            quote! { None }
        };

        let expanded = quote! {
            pub mod #mod_ident {
                const _: &str = include_str!(#path_str);

                fn __init_template() -> #crate_path::Template {
                    #crate_path::Template::from_precompiled(&#crate_path::PrecompiledTemplateData {
                        segments: &[#(#segments_tokens),*],
                        declared_variables: &[#(#decls_tokens),*],
                        inline_templates: &[#(#inline_templates_tokens),*],
                        source_hash: #source_hash,
                        consts: &[#(#consts_tokens),*],
                        imported_consts: &[#(#imported_consts_tokens),*],
                        name: #name_token,
                        description: #desc_token,
                    })
                }
                static __TEMPLATE: #crate_path::__private::LazyLock<#crate_path::Template> =
                    #crate_path::__private::LazyLock::new(__init_template);

                /// Get a reference to the compile-time validated, pre-compiled template.
                pub fn template() -> &'static #crate_path::Template {
                    &*__TEMPLATE
                }

                #struct_tokens
                #(#type_alias_tokens)*
            }
        };
        expanded.into()
    })
}

/// Parse and validate an inline template string at compile time and emit a
/// complete module.
///
/// Unlike [`include_template!`] which reads from a file, this macro takes a
/// string literal containing the full template source (including frontmatter).
/// The `=> module_name` is **required** because there is no file path from
/// which to derive a name.
///
/// The generated module has the same shape as [`include_template!`] — see its
/// docs for details.
///
/// # Examples
///
/// ```rust
/// extern crate md_tmpl_core as md_tmpl;
/// md_tmpl_macros::template!(
///     r#"
/// ---
/// params:
///   - name = str
/// ---
/// Hello {{ name }}!
/// "# => greeting
/// );
///
/// let output = greeting::Params { name: "World".into() }
///     .render()
///     .unwrap();
/// assert_eq!(output, "Hello World!\n");
/// ```
///
/// # Panics
///
/// Panics if an `env` expression cannot be evaluated at macro expansion time.
#[proc_macro]
pub fn template(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as InlineTemplateInput);
    let source = parsed.source.value();
    let mod_ident = parsed.name;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base_dir = std::path::Path::new(&manifest_dir);

    // Evaluate env expressions at macro expansion time.
    let env_values: Vec<(String, md_tmpl_core::Value)> = parsed
        .env
        .iter()
        .map(|(k, expr)| {
            let val = eval_env_expr(expr).unwrap_or_else(|e| panic!("env '{k}': {e}"));
            (k.clone(), val)
        })
        .collect();
    let env_refs: Vec<(&str, md_tmpl_core::Value)> = env_values
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();

    let ast = match compile::compile_template_to_ast(&source, base_dir, &env_refs) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("inline template: {e}");
            return syn::Error::new(parsed.source.span(), msg)
                .to_compile_error()
                .into();
        }
    };
    let CompiledTemplateAst {
        frontmatter: fm,
        segments,
        inline_templates,
        source_hash,
    } = ast;

    // Crate path: custom or default `::md_tmpl`.
    let crate_path = parsed
        .crate_path
        .map_or_else(|| quote! { ::md_tmpl }, |p| quote! { #p });

    with_crate_path(crate_path.clone(), || {
        // Template AST codegen.
        let segments_tokens = segments.iter().map(codegen_segment);
        let decls_tokens = fm.declarations.iter().map(codegen_var_decl);
        let inline_templates_tokens = inline_templates.iter().map(|(k, v)| {
            let v_tokens = codegen_compiled_inline_template(v);
            quote! { (#k, #v_tokens) }
        });
        let consts_tokens = fm.consts.iter().chain(fm.env.iter()).filter_map(|d| {
            d.default_value.as_ref().map(|v| {
                let name = &d.name;
                let val_tokens = codegen_value(v);
                quote! { (#name, #val_tokens) }
            })
        });
        let imported_consts_tokens = fm.imported_consts.iter().map(|(k, v)| {
            let val_tokens = codegen_value(v);
            quote! { (#k, #val_tokens) }
        });

        // Params struct codegen — uses Module so render() calls super::template().
        let struct_name = parsed
            .struct_name
            .clone()
            .unwrap_or_else(|| format_ident!("Params"));
        let source = StructGenSource::Module {
            doc_path: "<inline>",
        };
        let struct_tokens = generate_struct_tokens(&fm, &struct_name, &source);

        // Type alias codegen.
        let type_alias_tokens = generate_type_alias_tokens(&fm.type_aliases);

        let name_token = if let Some(n) = &fm.name {
            quote! { Some(#n) }
        } else {
            quote! { None }
        };
        let desc_token = if let Some(d) = &fm.description {
            quote! { Some(#d) }
        } else {
            quote! { None }
        };

        let expanded = quote! {
            pub mod #mod_ident {
                fn __init_template() -> #crate_path::Template {
                    #crate_path::Template::from_precompiled(&#crate_path::PrecompiledTemplateData {
                        segments: &[#(#segments_tokens),*],
                        declared_variables: &[#(#decls_tokens),*],
                        inline_templates: &[#(#inline_templates_tokens),*],
                        source_hash: #source_hash,
                        consts: &[#(#consts_tokens),*],
                        imported_consts: &[#(#imported_consts_tokens),*],
                        name: #name_token,
                        description: #desc_token,
                    })
                }
                static __TEMPLATE: #crate_path::__private::LazyLock<#crate_path::Template> =
                    #crate_path::__private::LazyLock::new(__init_template);

                /// Get a reference to the compile-time validated, pre-compiled template.
                pub fn template() -> &'static #crate_path::Template {
                    &*__TEMPLATE
                }

                #struct_tokens
                #(#type_alias_tokens)*
            }
        };
        expanded.into()
    })
}
