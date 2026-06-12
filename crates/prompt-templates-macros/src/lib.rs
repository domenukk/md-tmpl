#![doc = include_str!("../README.md")]

mod codegen;
mod compile;
mod struct_gen;
mod type_gen;

use codegen::{codegen_compiled_inline_template, codegen_segment, codegen_value, codegen_var_decl};
use compile::{CompiledTemplateAst, load_and_compile, stem_from_path};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use struct_gen::generate_struct_tokens;
use syn::{
    Ident, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};
use type_gen::generate_type_alias_tokens;

/// Parsed input for `template_params_struct!("path" => StructName)`.
struct ParamsStructInput {
    path: LitStr,
    _arrow: Token![=>],
    name: Ident,
}

impl Parse for ParamsStructInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            path: input.parse()?,
            _arrow: input.parse()?,
            name: input.parse()?,
        })
    }
}

/// Helper: convert a `load_and_compile` error into a compile error token stream.
fn err_tokens(span: proc_macro2::Span, rel_path: &str, e: &str) -> TokenStream {
    let msg = format!("template '{rel_path}': {e}");
    syn::Error::new(span, msg).to_compile_error().into()
}

/// Pre-parse and validate a `.tmpl.md` template at compile time.
///
/// See the crate-level docs for full details. The file path is resolved
/// relative to `CARGO_MANIFEST_DIR`. Returns a `&'static Template` backed
/// by a `LazyLock`.
#[proc_macro]
pub fn include_template(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let rel_path = path_lit.value();

    let (full_path, ast) = match load_and_compile(&rel_path) {
        Ok(v) => v,
        Err(e) => return err_tokens(path_lit.span(), &rel_path, &e),
    };
    let CompiledTemplateAst {
        frontmatter: fm,
        segments,
        inline_templates,
        source_hash,
    } = ast;
    let path_str = full_path.to_string_lossy().to_string();

    let segments_tokens = segments.iter().map(codegen_segment);
    let decls_tokens = fm.declarations.iter().map(codegen_var_decl);
    let inline_templates_tokens = inline_templates.iter().map(|(k, v)| {
        let v_tokens = codegen_compiled_inline_template(v);
        quote! { (#k, #v_tokens) }
    });
    let consts_tokens = fm.consts.iter().filter_map(|d| {
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

    let expanded = quote! {
        {
            const _: &str = include_str!(#path_str);
            static _TEMPL: ::std::sync::LazyLock<::prompt_templates::Template> = ::std::sync::LazyLock::new(|| {
                ::prompt_templates::Template::from_precompiled(
                    &[#(#segments_tokens),*],
                    &[#(#decls_tokens),*],
                    &[#(#inline_templates_tokens),*],
                    #source_hash,
                    &[#(#consts_tokens),*],
                    &[#(#imported_consts_tokens),*],
                )
            });
            &*_TEMPL
        }
    };
    expanded.into()
}

/// Validate a `.tmpl.md` template at compile time without producing a value.
///
/// Useful for static assertions in test modules or build scripts.
///
/// # Examples
///
/// ```rust
/// prompt_templates_macros::validate_template!("prompts/simple_greeting.tmpl.md");
/// ```
#[proc_macro]
pub fn validate_template(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let rel_path = path_lit.value();

    let (full_path, _) = match load_and_compile(&rel_path) {
        Ok(v) => v,
        Err(e) => return err_tokens(path_lit.span(), &rel_path, &e),
    };
    let path_str = full_path.to_string_lossy().to_string();

    let expanded = quote! { { const _: &str = include_str!(#path_str); } };
    expanded.into()
}

/// Generate a module with typed parameter structs from a `.tmpl.md` template.
///
/// Reads the template at compile time, inspects variable declarations, and
/// generates a module named after the file stem containing a `Params` struct
/// with correctly-typed fields.
///
/// # Syntax
///
/// ```text
/// include_types!("path/to/template.tmpl.md");
/// // Generates: mod template { pub struct Params { ... } ... }
/// ```
///
/// The module name is the file stem (e.g., `greeting` from `greeting.tmpl.md`).
/// The struct is always named `Params`. Sub-structs for compound types use
/// `PascalCase` naming (e.g., `ParamsItemsItem` for a list field called `items`).
///
/// # Type Mapping
///
/// | Frontmatter type | Rust type |
/// |-----------------|-----------|
/// | `str` | `String` |
/// | `int` | `i64` |
/// | `float` | `f64` |
/// | `bool` | `bool` |
/// | `list<field = type, ...>` | `Vec<Params{Field}Item>` (sub-struct) |
/// | `list` (untyped) | `Vec<::prompt_templates::Value>` |
/// | `dict<field = type, ...>` | `Params{Field}` (sub-struct) |
/// | untyped | `::prompt_templates::Value` |
///
/// # Examples
///
/// ```rust
/// // Given greeting.tmpl.md with frontmatter:
/// //   params: [name = str, count = int, items = list<label = str>]
///
/// prompt_templates_macros::include_types!("prompts/greeting.tmpl.md");
///
/// // Generated:
/// //   mod greeting { pub struct Params { pub name: String, ... } ... }
///
/// let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
/// let output = greeting::Params {
///     name: "Alice".into(),
///     count: 42,
///     items: vec![greeting::ParamsItemsItem {
///         label: "hello".into(),
///     }],
/// }
/// .render(&tmpl)
/// .unwrap();
/// ```
#[proc_macro]
pub fn include_types(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let path_raw = path_lit.value();

    let (full_path, CompiledTemplateAst { frontmatter, .. }) = match load_and_compile(&path_raw) {
        Ok(v) => v,
        Err(e) => return err_tokens(path_lit.span(), &path_raw, &e),
    };
    let path_str = full_path.to_string_lossy().to_string();
    let mod_ident = format_ident!("r#{}", stem_from_path(&path_raw));
    let struct_name = format_ident!("Params");
    let struct_name_str = "Params".to_string();

    let inner = generate_struct_tokens(
        &frontmatter,
        &struct_name,
        &struct_name_str,
        &path_raw,
        &path_str,
    );
    let type_alias_tokens = generate_type_alias_tokens(&frontmatter.type_aliases);

    let expanded = quote! {
        pub mod #mod_ident {
            #inner
            #(#type_alias_tokens)*
        }
    };
    expanded.into()
}

/// Generate a standalone typed parameter struct with a caller-chosen name.
///
/// Unlike [`include_types!`] which wraps the struct in a module named after the
/// template stem, this macro emits the struct (and any nested types) directly
/// into the calling scope with a caller-chosen name.
///
/// # Syntax
///
/// ```rust
/// prompt_templates_macros::template_params_struct!("prompts/simple_greeting.tmpl.md" => Greeting);
///
/// let tmpl = prompt_templates_macros::include_template!("prompts/simple_greeting.tmpl.md");
/// let output = Greeting { name: "World".into() }.render(&tmpl).unwrap();
/// assert_eq!(output, "\nHello World!\n");
/// ```
#[proc_macro]
pub fn template_params_struct(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as ParamsStructInput);
    let path_raw = parsed.path.value();
    let struct_name = parsed.name;

    let (full_path, CompiledTemplateAst { frontmatter, .. }) = match load_and_compile(&path_raw) {
        Ok(v) => v,
        Err(e) => return err_tokens(parsed.path.span(), &path_raw, &e),
    };
    let path_str = full_path.to_string_lossy().to_string();
    let struct_name_str = struct_name.to_string();

    generate_struct_tokens(
        &frontmatter,
        &struct_name,
        &struct_name_str,
        &path_raw,
        &path_str,
    )
    .into()
}
