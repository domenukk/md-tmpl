#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod codegen;
mod compile;
mod struct_gen;
mod type_gen;

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

/// Parsed input for `include_template!("path")` or
/// `include_template!("path" => custom_mod_name)`.
struct IncludeTemplateInput {
    path: LitStr,
    custom_name: Option<Ident>,
}

impl Parse for IncludeTemplateInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;
        let custom_name = if input.peek(Token![=>]) {
            let _arrow: Token![=>] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Self { path, custom_name })
    }
}

/// Parsed input for `template!(r#"source"# => mod_name)`.
///
/// The `=> name` is **required** because inline templates have no file path
/// from which to derive a module name.
struct InlineTemplateInput {
    source: LitStr,
    _arrow: Token![=>],
    name: Ident,
}

impl Parse for InlineTemplateInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            source: input.parse()?,
            _arrow: input.parse()?,
            name: input.parse()?,
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

/// Create a module identifier, using raw syntax (`r#name`) only when `stem`
/// is a Rust keyword.
fn make_module_ident(stem: &str) -> Ident {
    if RUST_KEYWORDS.contains(&stem) {
        format_ident!("r#{}", stem)
    } else {
        Ident::new(stem, proc_macro2::Span::call_site())
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
///   - `render_with(tmpl)` — render with an externally-loaded template
///     (hot-reload).
///   - `validate_template(tmpl)` — check template compatibility.
///   - `to_context()` — convert to a `Context`.
/// * Sub-structs for compound types.
/// * Constants from the `consts:` block.
/// * Type aliases from the `types:` block.
///
/// # Examples
///
/// ```rust
/// prompt_templates_macros::include_template!("prompts/simple_greeting.tmpl.md");
///
/// let output = simple_greeting::Params {
///     name: "World".into(),
/// }
/// .render()
/// .unwrap();
/// assert_eq!(output, "\nHello World!\n");
/// ```
#[proc_macro]
pub fn include_template(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as IncludeTemplateInput);
    let rel_path = parsed.path.value();

    let (full_path, ast) = match load_and_compile(&rel_path) {
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
        None => make_module_ident(&stem_from_path(&rel_path)),
    };

    // Template AST codegen.
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

    // Params struct codegen.
    let struct_name = format_ident!("Params");
    let source = StructGenSource::Module {
        doc_path: &rel_path,
    };
    let struct_tokens = generate_struct_tokens(&fm, &struct_name, &source);

    // Type alias codegen.
    let type_alias_tokens = generate_type_alias_tokens(&fm.type_aliases);

    let expanded = quote! {
        pub mod #mod_ident {
            const _: &str = include_str!(#path_str);

            static __TEMPLATE: ::prompt_templates::__private::Lazy<::prompt_templates::Template> =
                ::prompt_templates::__private::Lazy::new(|| {
                    ::prompt_templates::Template::from_precompiled(
                        &[#(#segments_tokens),*],
                        &[#(#decls_tokens),*],
                        &[#(#inline_templates_tokens),*],
                        #source_hash,
                        &[#(#consts_tokens),*],
                        &[#(#imported_consts_tokens),*],
                    )
                });

            /// Get a reference to the compile-time validated, pre-compiled template.
            pub fn template() -> &'static ::prompt_templates::Template {
                &*__TEMPLATE
            }

            #struct_tokens
            #(#type_alias_tokens)*
        }
    };
    expanded.into()
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
/// prompt_templates_macros::template!(
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
#[proc_macro]
pub fn template(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as InlineTemplateInput);
    let source = parsed.source.value();
    let mod_ident = parsed.name;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base_dir = std::path::Path::new(&manifest_dir);

    let ast = match compile::compile_template_to_ast(&source, base_dir) {
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

    // Template AST codegen.
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

    // Params struct codegen — uses Module so render() calls super::template().
    let struct_name = format_ident!("Params");
    let source = StructGenSource::Module {
        doc_path: "<inline>",
    };
    let struct_tokens = generate_struct_tokens(&fm, &struct_name, &source);

    // Type alias codegen.
    let type_alias_tokens = generate_type_alias_tokens(&fm.type_aliases);

    let expanded = quote! {
        pub mod #mod_ident {
            static __TEMPLATE: ::prompt_templates::__private::Lazy<::prompt_templates::Template> =
                ::prompt_templates::__private::Lazy::new(|| {
                    ::prompt_templates::Template::from_precompiled(
                        &[#(#segments_tokens),*],
                        &[#(#decls_tokens),*],
                        &[#(#inline_templates_tokens),*],
                        #source_hash,
                        &[#(#consts_tokens),*],
                        &[#(#imported_consts_tokens),*],
                    )
                });

            /// Get a reference to the compile-time validated, pre-compiled template.
            pub fn template() -> &'static ::prompt_templates::Template {
                &*__TEMPLATE
            }

            #struct_tokens
            #(#type_alias_tokens)*
        }
    };
    expanded.into()
}
