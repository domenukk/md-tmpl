#![doc = include_str!("../README.md")]

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Ident, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

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

/// Pre-parse and validate a `.tmpl.md` template at compile time.
///
/// The file path is resolved relative to the calling crate's `CARGO_MANIFEST_DIR`.
///
/// At compile time this macro:
/// 1. Reads the template file
/// 2. Parses frontmatter (template name, description, typed variable declarations)
/// 3. Validates that all `{{ var }}` expressions in the body reference declared
///    variables (when frontmatter declares a `params:` list)
/// 4. Checks type annotations are syntactically valid
///
/// If any check fails, a **compile error** is emitted with a descriptive message.
///
/// At runtime, the returned [`prompt_templates::Template`] has **zero parsing overhead** —
/// all parsing was done at compile time. Only `.render()` runs at runtime.
///
/// The returned `Template` is cloned from a `LazyLock<Template>` static.
/// If you plan to render in a hot loop, bind the result once:
///
/// ```text
/// let tmpl = include_template!("prompts/greeting.tmpl.md");
/// for item in items {
///     tmpl.render(&ctx).unwrap();
/// }
/// ```
///
/// # Panics
///
/// This is a proc macro — it does not panic at runtime. Invalid templates
/// cause compile-time errors.
#[proc_macro]
pub fn include_template(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let rel_path = path_lit.value();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let full_path = std::path::Path::new(&manifest_dir).join(&rel_path);

    let source = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("failed to read template '{}': {e}", full_path.display());
            return syn::Error::new(path_lit.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let base_dir = full_path.parent().unwrap_or(std::path::Path::new("."));
    let CompiledTemplateAst {
        frontmatter: fm,
        segments,
        inline_templates,
        source_hash,
    } = match compile_template_to_ast(&source, base_dir) {
        Ok(res) => res,
        Err(e) => {
            let msg = format!("template '{rel_path}' syntax error: {e}");
            return syn::Error::new(path_lit.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let path_str = full_path.to_string_lossy().to_string();

    let segments_tokens = segments.iter().map(codegen_segment);
    let decls_tokens = fm.declarations.iter().map(codegen_var_decl);
    let inline_templates_tokens = inline_templates.iter().map(|(k, v)| {
        let v_tokens = codegen_compiled_inline_template(v);
        quote! { (#k, #v_tokens) }
    });

    let expanded = quote! {
        {
            // include_str! establishes a file dependency so cargo rebuilds
            // when the template changes.
            const _: &str = include_str!(#path_str);
            static _TEMPL: ::std::sync::LazyLock<::prompt_templates::Template> = ::std::sync::LazyLock::new(|| {
                ::prompt_templates::Template::from_precompiled(
                    &[#(#segments_tokens),*],
                    &[#(#decls_tokens),*],
                    &[#(#inline_templates_tokens),*],
                    #source_hash,
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

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let full_path = std::path::Path::new(&manifest_dir).join(&rel_path);

    let source = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("failed to read template '{}': {e}", full_path.display());
            return syn::Error::new(path_lit.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let base_dir = full_path.parent().unwrap_or(std::path::Path::new("."));
    if let Err(msg) = compile_template_to_ast(&source, base_dir) {
        let err_msg = format!("template '{rel_path}' syntax error: {msg}");
        return syn::Error::new(path_lit.span(), err_msg)
            .to_compile_error()
            .into();
    }

    let path_str = full_path.to_string_lossy().to_string();

    // Just validate — emit a no-op with include_str for dependency tracking.
    let expanded = quote! {
        {
            const _: &str = include_str!(#path_str);
        }
    };

    expanded.into()
}

/// Generate a typed parameter struct from a `.tmpl.md` template's frontmatter.
///
/// Reads the template at compile time, inspects variable declarations, and
/// generates a struct with correctly-typed fields. The struct gets:
///
/// - **`render(&self, tmpl: &Template)`** — convert fields to a Context and render
/// - **`validate_template(tmpl: &Template)`** — check that a reloaded template's
///   variable names still match (for hot-reload safety)
///
/// # Type Mapping
///
/// | Frontmatter type | Rust type |
/// |-----------------|-----------|
/// | `str` | `String` |
/// | `int` | `i64` |
/// | `float` | `f64` |
/// | `bool` | `bool` |
/// | `list<field = type, ...>` | `Vec<{StructName}{Field}Item>` (sub-struct) |
/// | `list` (untyped) | `Vec<::prompt_templates::Value>` |
/// | `dict<field = type, ...>` | `{StructName}{Field}` (sub-struct) |
/// | untyped | `::prompt_templates::Value` |
///
/// # Examples
///
/// ```rust
/// // Given greeting.tmpl.md with frontmatter:
/// //   params: [name = str, count = int, items = list<label = str>]
///
/// prompt_templates_macros::template_params_struct!("prompts/greeting.tmpl.md" => GreetingParams);
///
/// // Generated:
/// //   struct GreetingParams { pub name: String, pub count: i64, pub items: Vec<GreetingParamsItemsItem> }
/// //   struct GreetingParamsItemsItem { pub label: String }
///
/// let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
/// let output = GreetingParams {
///     name: "Alice".into(),
///     count: 42,
///     items: vec![GreetingParamsItemsItem { label: "hello".into() }],
/// }.render(&tmpl).unwrap();
///
/// // Hot-reload — struct works with disk-loaded template if vars match:
/// let tmpl = prompt_templates::Template::from_file(std::path::Path::new("prompts/greeting.tmpl.md")).unwrap();
/// GreetingParams::validate_template(&tmpl).unwrap();
/// let output = GreetingParams {
///     name: "Bob".into(),
///     count: 1,
///     items: vec![],
/// }.render(&tmpl).unwrap();
/// ```
#[proc_macro]
pub fn template_params_struct(input: TokenStream) -> TokenStream {
    // Parse: "path/to/template.tmpl.md" => StructName
    let parsed = parse_macro_input!(input as ParamsStructInput);
    let path_raw = parsed.path.value();
    let struct_name = parsed.name;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let full_path = std::path::Path::new(&manifest_dir).join(&path_raw);

    let source = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("failed to read template '{}': {e}", full_path.display());
            return syn::Error::new(parsed.path.span(), msg)
                .to_compile_error()
                .into();
        }
    };

    let base_dir = full_path.parent().unwrap_or(std::path::Path::new("."));
    let CompiledTemplateAst { frontmatter, .. } = match compile_template_to_ast(&source, base_dir) {
        Ok(res) => res,
        Err(e) => {
            let msg = format!("template '{path_raw}' syntax error: {e}");
            return syn::Error::new(parsed.path.span(), msg)
                .to_compile_error()
                .into();
        }
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

/// Generate the struct definition and impl block from frontmatter declarations.
fn generate_struct_tokens(
    frontmatter: &prompt_templates::Frontmatter,
    struct_name: &syn::Ident,
    struct_name_str: &str,
    path_raw: &str,
    path_str: &str,
) -> proc_macro2::TokenStream {
    let mut sub_structs = Vec::new();
    let mut fields = Vec::new();
    let mut set_stmts = Vec::new();
    let mut expected_vars = Vec::new();

    for decl in &frontmatter.declarations {
        let field_name = format_ident!("{}", decl.name);
        let var_name_str = &decl.name;
        let (field_type, field_set) = var_type_to_rust(
            &decl.var_type,
            struct_name_str,
            &decl.name,
            &mut sub_structs,
        );

        fields.push(quote! { pub #field_name: #field_type });
        set_stmts.push(field_set(quote! { self.#field_name }, var_name_str));
        expected_vars.push(var_name_str.clone());
    }

    let expected_var_lits: Vec<_> = expected_vars.iter().map(|s| quote! { #s }).collect();
    let expected_count = expected_vars.len();
    let doc_attrs = build_struct_docs(frontmatter, path_raw);

    let serde_derives = if cfg!(feature = "serde") {
        quote! { #[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)] }
    } else {
        quote! { #[derive(Debug, Clone)] }
    };

    quote! {
        // Dependency tracking.
        const _: &str = include_str!(#path_str);

        #(#sub_structs)*

        #(#doc_attrs)*
        #serde_derives
        pub struct #struct_name {
            #(#fields),*
        }

        impl #struct_name {
            /// Expected variable names this struct was generated from.
            const EXPECTED_VARS: [&'static str; #expected_count] = [#(#expected_var_lits),*];

            /// Validate that a template's variable declarations match this struct.
            ///
            /// Use this when hot-reloading a template from disk to ensure the
            /// reloaded version is still compatible with this compiled struct.
            ///
            /// The template body can be edited freely, but the `params:`
            /// block in the frontmatter **must not be changed** — it is part
            /// of the compile-time contract.
            ///
            /// # Errors
            ///
            /// Returns [`TemplateError::DeclarationsMutated`] if the
            /// template's declared variable names don't match the expected
            /// set.
            pub fn validate_template(tmpl: &::prompt_templates::Template) -> ::std::result::Result<(), ::prompt_templates::TemplateError> {
                let decl_names: ::std::collections::HashSet<&str> = tmpl
                    .declarations()
                    .iter()
                    .map(|d| d.name.as_str())
                    .collect();
                let expected: ::std::collections::HashSet<&str> = Self::EXPECTED_VARS
                    .iter()
                    .copied()
                    .collect();

                if decl_names != expected {
                    let missing: Vec<_> = expected.difference(&decl_names).collect();
                    let extra: Vec<_> = decl_names.difference(&expected).collect();
                    return Err(::prompt_templates::TemplateError::DeclarationsMutated {
                        details: format!(
                            "removed {:?}, added {:?}. \
                             You may edit the template body, but the \
                             frontmatter `params:` block must stay \
                             unchanged",
                            missing, extra
                        ),
                    });
                }
                Ok(())
            }

            /// Convert this struct into a [`Context`](::prompt_templates::Context).
            #[must_use]
            pub fn to_context(&self) -> ::prompt_templates::Context {
                let mut ctx = ::prompt_templates::Context::new();
                #(#set_stmts)*
                ctx
            }

            /// Validate the template and render with this struct's fields.
            ///
            /// # Errors
            ///
            /// Returns [`TemplateError`] if validation or rendering fails.
            pub fn render(&self, tmpl: &::prompt_templates::Template) -> ::std::result::Result<String, ::prompt_templates::TemplateError> {
                Self::validate_template(tmpl)?;
                let ctx = self.to_context();
                tmpl.render(&ctx)
            }
        }
    }
}

/// Build doc-comment attributes from frontmatter metadata.
fn build_struct_docs(
    frontmatter: &prompt_templates::Frontmatter,
    path_raw: &str,
) -> Vec<proc_macro2::TokenStream> {
    let tmpl_name = if frontmatter.name.is_empty() {
        path_raw.to_string()
    } else {
        frontmatter.name.clone()
    };
    let description = if frontmatter.description.is_empty() {
        String::from("(no description)")
    } else {
        frontmatter.description.clone()
    };

    let mut doc_lines = vec![
        format!("Parameters for the **{tmpl_name}** template."),
        String::new(),
        description,
        String::new(),
        format!("Source: `{path_raw}`"),
        String::new(),
    ];

    if !frontmatter.declarations.is_empty() {
        doc_lines.push("# Variables".to_string());
        doc_lines.push(String::new());
        doc_lines.push("| Name | Type |".to_string());
        doc_lines.push("|------|------|".to_string());
        for decl in &frontmatter.declarations {
            doc_lines.push(format!("| `{}` | `{}` |", decl.name, decl.var_type));
        }
        doc_lines.push(String::new());
    }

    doc_lines.push("Fill in the fields and call [`render()`](Self::render) to produce".to_string());
    doc_lines.push("the rendered template output. Supports hot-reload via".to_string());
    doc_lines.push("[`validate_template()`](Self::validate_template).".to_string());

    doc_lines
        .iter()
        .map(|line| quote! { #[doc = #line] })
        .collect()
}

/// A closure that generates a context-setter statement from a value expression.
type SetterFn = Box<dyn Fn(proc_macro2::TokenStream, &str) -> proc_macro2::TokenStream>;

/// Map a `VarType` to a Rust type token and a setter closure.
fn var_type_to_rust(
    var_type: &prompt_templates::VarType,
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    use prompt_templates::VarType;

    match var_type {
        VarType::Str => (quote! { String }, simple_setter("as_str()")),
        VarType::Int => (quote! { i64 }, simple_setter("")),
        VarType::Float => (quote! { f64 }, simple_setter("")),
        VarType::Bool => (quote! { bool }, simple_setter("")),
        VarType::List(fields) if !fields.is_empty() => {
            typed_list_codegen(fields, parent_struct, field_name, sub_structs)
        }
        VarType::List(_) => (
            quote! { Vec<::prompt_templates::Value> },
            Box::new(|val, name| {
                let name_lit = name.to_string();
                quote! { ctx.set(#name_lit, ::prompt_templates::Value::List(#val.clone())); }
            }),
        ),
        VarType::Dict(fields) if !fields.is_empty() => {
            typed_dict_codegen(fields, parent_struct, field_name, sub_structs)
        }
        VarType::Dict(_) => (
            quote! { ::prompt_templates::Value },
            Box::new(|val, name| {
                let name_lit = name.to_string();
                quote! { ctx.set(#name_lit, #val.clone()); }
            }),
        ),
        VarType::Enum(variants) => {
            typed_enum_codegen(variants, parent_struct, field_name, sub_structs)
        }
    }
}

/// Create a setter that calls `ctx.set(name, val)` or `ctx.set(name, val.as_str())`.
fn simple_setter(suffix: &str) -> SetterFn {
    let suffix = suffix.to_string();
    Box::new(move |val, name| {
        let name_lit = name.to_string();
        if suffix.is_empty() {
            quote! { ctx.set(#name_lit, #val); }
        } else {
            // Only case is .as_str() for String fields.
            quote! { ctx.set(#name_lit, #val.as_str()); }
        }
    })
}

/// Generate a sub-struct and setter for a typed list (`list<field = type, ...>`).
fn typed_list_codegen(
    inner_fields: &[prompt_templates::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = capitalize_first(field_name);
    let item_struct_name = format_ident!("{parent_struct}{capitalized}Item");

    let mut item_fields = Vec::new();
    let mut item_set_stmts = Vec::new();

    for inner_decl in inner_fields {
        let inner_field = format_ident!("{}", inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}Item"),
            &inner_decl.name,
            sub_structs,
        );
        item_fields.push(quote! { pub #inner_field: #inner_type });
        item_set_stmts.push(inner_set(quote! { item.#inner_field }, inner_name_str));
    }

    let serde_derives = if cfg!(feature = "serde") {
        quote! { #[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)] }
    } else {
        quote! { #[derive(Debug, Clone)] }
    };

    sub_structs.push(quote! {
        /// Auto-generated sub-struct for list items.
        #serde_derives
        pub struct #item_struct_name {
            #(#item_fields),*
        }
    });

    let item_struct = item_struct_name.clone();
    (
        quote! { Vec<#item_struct> },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            let stmts = &item_set_stmts;
            quote! {
                ctx.set(#name_lit, ::prompt_templates::Value::List(
                    #val.iter().map(|item| {
                        let mut ctx = ::prompt_templates::Context::new();
                        #(#stmts)*
                        ::prompt_templates::Value::Dict(ctx.into_inner())
                    }).collect()
                ));
            }
        }),
    )
}

/// Generate a sub-struct and setter for a typed dict (`dict<field = type, ...>`).
fn typed_dict_codegen(
    inner_fields: &[prompt_templates::VarDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = capitalize_first(field_name);
    let dict_struct_name = format_ident!("{parent_struct}{capitalized}");

    let mut dict_fields = Vec::new();
    let mut dict_set_stmts = Vec::new();

    for inner_decl in inner_fields {
        let inner_field = format_ident!("{}", inner_decl.name);
        let inner_name_str = &inner_decl.name;
        let (inner_type, inner_set) = var_type_to_rust(
            &inner_decl.var_type,
            &format!("{parent_struct}{capitalized}"),
            &inner_decl.name,
            sub_structs,
        );
        dict_fields.push(quote! { pub #inner_field: #inner_type });
        dict_set_stmts.push(inner_set(quote! { val.#inner_field }, inner_name_str));
    }

    let serde_derives = if cfg!(feature = "serde") {
        quote! { #[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)] }
    } else {
        quote! { #[derive(Debug, Clone)] }
    };

    sub_structs.push(quote! {
        /// Auto-generated sub-struct for dict fields.
        #serde_derives
        pub struct #dict_struct_name {
            #(#dict_fields),*
        }
    });

    let dict_struct = dict_struct_name.clone();
    (
        quote! { #dict_struct },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            let stmts = &dict_set_stmts;
            quote! {
                {
                    let val = &#val;
                    let mut inner_ctx = ::prompt_templates::Context::new();
                    #(#stmts)*
                    ctx.set(#name_lit, ::prompt_templates::Value::Dict(inner_ctx.into_inner()));
                }
            }
        }),
    )
}

/// Generate variant name identifier and its raw string if they differ (for serde rename).
fn string_to_variant_ident(s: &str) -> (syn::Ident, Option<String>) {
    let mut clean = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if capitalize_next {
                clean.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                clean.push(c);
            }
        } else {
            capitalize_next = true;
        }
    }
    if clean.is_empty() {
        clean = "Variant".to_string();
    }
    let ident = quote::format_ident!("{}", clean);
    if clean == s {
        (ident, None)
    } else {
        (ident, Some(s.to_string()))
    }
}

/// Generate a sub-enum and setter for enum variables (`enum[Variant1(fields...), Variant2]`).
fn typed_enum_codegen(
    variants: &[prompt_templates::VariantDecl],
    parent_struct: &str,
    field_name: &str,
    sub_structs: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, SetterFn) {
    let capitalized = capitalize_first(field_name);
    let enum_name = quote::format_ident!("{parent_struct}{capitalized}");

    let mut variant_tokens = Vec::new();

    for var in variants {
        let (var_ident, rename) = string_to_variant_ident(&var.name);
        let serde_rename = rename.map(|r| quote! { #[serde(rename = #r)] });

        if var.fields.is_empty() {
            variant_tokens.push(quote! {
                #serde_rename
                #var_ident
            });
        } else {
            let mut struct_fields = Vec::new();
            for inner_decl in &var.fields {
                let inner_field = quote::format_ident!("{}", inner_decl.name);
                let (inner_type, _) = var_type_to_rust(
                    &inner_decl.var_type,
                    &format!("{parent_struct}{capitalized}{var_ident}"),
                    &inner_decl.name,
                    sub_structs,
                );
                struct_fields.push(quote! { #inner_field: #inner_type });
            }

            variant_tokens.push(quote! {
                #serde_rename
                #var_ident {
                    #(#struct_fields),*
                }
            });
        }
    }

    let serde_derives = if cfg!(feature = "serde") {
        quote! {
            #[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
            #[serde(tag = "tag")]
        }
    } else {
        quote! { #[derive(Debug, Clone)] }
    };

    sub_structs.push(quote! {
        /// Auto-generated enum for parameter constraints.
        #serde_derives
        pub enum #enum_name {
            #(#variant_tokens),*
        }
    });

    let enum_type = enum_name.clone();
    (
        quote! { #enum_type },
        Box::new(move |val, name| {
            let name_lit = name.to_string();
            quote! {
                ctx.set(
                    #name_lit,
                    ::prompt_templates::to_value(&#val)
                        .expect("infallible: generated enum type is always serializable"),
                );
            }
        }),
    )
}

/// Capitalize the first character of a string.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

// ---------------------------------------------------------------------------
// Compile-time AST Compilation and Codegen helpers
// ---------------------------------------------------------------------------

fn hash_source(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

/// Result of compiling a template at macro expansion time.
struct CompiledTemplateAst {
    frontmatter: prompt_templates::Frontmatter,
    segments: Vec<prompt_templates::compiled::Segment>,
    inline_templates: HashMap<String, prompt_templates::compiled::CompiledInlineTemplate>,
    source_hash: u64,
}

fn compile_template_to_ast(
    source: &str,
    base_dir: &std::path::Path,
) -> Result<CompiledTemplateAst, String> {
    let source_hash = hash_source(source);
    let (fm, body) = prompt_templates::parse_frontmatter(source).map_err(|e| e.to_string())?;

    let (mut segments, inline_templates) =
        prompt_templates::compiled::compile(body).map_err(|e| e.to_string())?;

    // Static analysis: Enforce that all parameters referenced in the body are declared.
    let referenced = prompt_templates::compiled::collect_referenced_params(&segments);
    let declared: std::collections::HashSet<&str> = fm.params.iter().map(String::as_str).collect();
    let undeclared: Vec<&String> = referenced
        .iter()
        .filter(|v| !declared.contains(v.as_str()))
        .collect();
    if !undeclared.is_empty() {
        let mut names: Vec<&str> = undeclared.iter().map(|s| s.as_str()).collect();
        names.sort_unstable();
        return Err(format!(
            "undeclared variable(s) referenced in body: {}",
            names.join(", ")
        ));
    }

    // Recursively resolve includes at compile time
    resolve_includes_recursive(&mut segments, &fm.declarations, base_dir, 0)?;

    // Flow-sensitive type check: validate variant names and field access.
    let type_errors =
        prompt_templates::compiled::validate_field_accesses(&segments, &fm.declarations);
    if !type_errors.is_empty() {
        return Err(type_errors.join("\n"));
    }

    Ok(CompiledTemplateAst {
        frontmatter: fm,
        segments,
        inline_templates,
        source_hash,
    })
}

fn resolve_includes_recursive(
    segments: &mut [prompt_templates::compiled::Segment],
    parent_declarations: &[prompt_templates::VarDecl],
    base_dir: &std::path::Path,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 {
        return Err("maximum include depth exceeded".to_string());
    }
    for seg in segments {
        match seg {
            prompt_templates::compiled::Segment::Include(inc) => {
                resolve_single_include(inc, parent_declarations, base_dir, depth)?;
            }
            prompt_templates::compiled::Segment::ForLoop { body, .. } => {
                resolve_includes_recursive(body, parent_declarations, base_dir, depth)?;
            }
            prompt_templates::compiled::Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    resolve_includes_recursive(branch_body, parent_declarations, base_dir, depth)?;
                }
                resolve_includes_recursive(else_body, parent_declarations, base_dir, depth)?;
            }
            prompt_templates::compiled::Segment::Match { arms, .. } => {
                for (_, arm_body) in arms {
                    resolve_includes_recursive(arm_body, parent_declarations, base_dir, depth)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Process a single include directive: load, validate params, type-check, compile, and recurse.
fn resolve_single_include(
    inc: &mut prompt_templates::compiled::CompiledInclude,
    parent_declarations: &[prompt_templates::VarDecl],
    base_dir: &std::path::Path,
    depth: usize,
) -> Result<(), String> {
    let include_path = base_dir.join(inc.path.as_ref());
    let included_source = std::fs::read_to_string(&include_path)
        .map_err(|e| format!("cannot read include {}: {e}", include_path.display()))?;

    let (included_fm, included_body) = prompt_templates::parse_frontmatter(&included_source)
        .map_err(|e| format!("syntax error in include {}: {e}", include_path.display()))?;

    validate_include_params(inc, &included_fm)?;
    typecheck_include_vars(inc, &included_fm, parent_declarations)?;

    let (mut included_segments, _included_inline_templates) =
        prompt_templates::compiled::compile(included_body).map_err(|e| {
            format!(
                "compilation error in include {}: {e}",
                include_path.display()
            )
        })?;

    let child_base_dir = include_path.parent().unwrap_or(base_dir);
    resolve_includes_recursive(
        &mut included_segments,
        &included_fm.declarations,
        child_base_dir,
        depth + 1,
    )?;

    inc.inline_compiled = Some(prompt_templates::compiled::CompiledInlineTemplate {
        segments: std::sync::Arc::from(included_segments),
        declarations: std::sync::Arc::from(included_fm.declarations),
    });
    Ok(())
}

/// Validate that all declared parameters in the included template are provided.
fn validate_include_params(
    inc: &prompt_templates::compiled::CompiledInclude,
    included_fm: &prompt_templates::Frontmatter,
) -> Result<(), String> {
    let mut provided: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (key, _) in &inc.with_vars {
        provided.insert(key.as_ref());
    }
    if let Some((binding, _)) = &inc.for_each {
        provided.insert(binding.as_ref());
    }

    let missing: Vec<&prompt_templates::VarDecl> = included_fm
        .declarations
        .iter()
        .filter(|d| !provided.contains(&*d.name))
        .collect();

    if !missing.is_empty() {
        let missing_desc: Vec<String> = missing
            .iter()
            .map(|d| format!("{}: {}", d.name, d.var_type))
            .collect();
        let fix_hint: Vec<String> = missing
            .iter()
            .map(|d| format!("{}={}", d.name, d.name))
            .collect();
        return Err(format!(
            "include '{}' requires explicit params: {}. Use 'with {}' to pass them",
            inc.path,
            missing_desc.join(", "),
            fix_hint.join(", "),
        ));
    }
    Ok(())
}

/// Type-check that provided variables have compatible types with the included template.
fn typecheck_include_vars(
    inc: &prompt_templates::compiled::CompiledInclude,
    included_fm: &prompt_templates::Frontmatter,
    parent_declarations: &[prompt_templates::VarDecl],
) -> Result<(), String> {
    for (key, val_expr) in &inc.with_vars {
        let Some(included_decl) = included_fm
            .declarations
            .iter()
            .find(|d| d.name == key.as_ref())
        else {
            continue;
        };

        let is_literal =
            val_expr.starts_with('"') || val_expr.starts_with('\'') || val_expr.starts_with('<');
        if is_literal {
            continue;
        }

        let root_var = val_expr
            .split('.')
            .next()
            .unwrap_or(val_expr.as_ref())
            .trim();
        if let Some(parent_decl) = parent_declarations.iter().find(|d| d.name == root_var)
            && root_var == val_expr.trim()
            && parent_decl.var_type != included_decl.var_type
        {
            return Err(format!(
                "include '{}': type mismatch for '{}': parent declares '{}' but included template expects '{}'",
                inc.path, key, parent_decl.var_type, included_decl.var_type,
            ));
        }
    }
    Ok(())
}

fn codegen_segment(seg: &prompt_templates::compiled::Segment) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::Segment;
    match seg {
        Segment::Static(s) => quote! {
            ::prompt_templates::compiled::Segment::Static(::std::borrow::Cow::Borrowed(#s))
        },
        Segment::Expr { path, filters } => {
            let filters_tokens = filters.iter().map(codegen_parsed_filter);
            quote! {
                ::prompt_templates::compiled::Segment::Expr {
                    path: ::std::borrow::Cow::Borrowed(#path),
                    filters: ::std::vec![#(#filters_tokens),*],
                }
            }
        }
        Segment::ForLoop {
            binding,
            list_path,
            body,
        } => {
            let body_tokens = body.iter().map(codegen_segment);
            quote! {
                ::prompt_templates::compiled::Segment::ForLoop {
                    binding: ::std::borrow::Cow::Borrowed(#binding),
                    list_path: ::std::borrow::Cow::Borrowed(#list_path),
                    body: ::std::vec![#(#body_tokens),*],
                }
            }
        }
        Segment::If {
            branches,
            else_body,
        } => {
            let branch_tokens = branches.iter().map(|(cond, body)| {
                let cond_tokens = codegen_condition(cond);
                let body_tokens = body.iter().map(codegen_segment);
                quote! {
                    (#cond_tokens, ::std::vec![#(#body_tokens),*])
                }
            });
            let else_tokens = else_body.iter().map(codegen_segment);
            quote! {
                ::prompt_templates::compiled::Segment::If {
                    branches: ::std::vec![#(#branch_tokens),*],
                    else_body: ::std::vec![#(#else_tokens),*],
                }
            }
        }
        Segment::Raw(s) => quote! {
            ::prompt_templates::compiled::Segment::Raw(::std::borrow::Cow::Borrowed(#s))
        },
        Segment::Comment(refs) => {
            quote! {
                ::prompt_templates::compiled::Segment::Comment(::std::vec![#(::std::borrow::Cow::Borrowed(#refs)),*])
            }
        }
        Segment::Include(inc) => {
            let path = &inc.path;
            let with_vars = inc.with_vars.iter().map(|(k, v)| {
                quote! { (::std::borrow::Cow::Borrowed(#k), ::std::borrow::Cow::Borrowed(#v)) }
            });
            let for_each = inc.for_each.as_ref().map_or_else(
                || quote! { ::std::option::Option::None },
                |(b, l)| quote! { ::std::option::Option::Some((::std::borrow::Cow::Borrowed(#b), ::std::borrow::Cow::Borrowed(#l))) },
            );
            let inline_compiled = inc.inline_compiled.as_ref().map_or_else(
                || quote! { ::std::option::Option::None },
                |ic| {
                    let ic_tokens = codegen_compiled_inline_template(ic);
                    quote! { ::std::option::Option::Some(#ic_tokens) }
                },
            );
            quote! {
                ::prompt_templates::compiled::Segment::Include(
                    ::prompt_templates::compiled::CompiledInclude {
                        path: ::std::borrow::Cow::Borrowed(#path),
                        with_vars: ::std::vec![#(#with_vars),*],
                        for_each: #for_each,
                        inline_compiled: #inline_compiled,
                    }
                )
            }
        }
        Segment::Match { expr, arms } => {
            let arm_tokens = arms.iter().map(|(variants, body)| {
                let body_tokens = body.iter().map(codegen_segment);
                let variant_tokens = variants.iter().map(|v| {
                    quote! { ::std::borrow::Cow::Borrowed(#v) }
                });
                quote! {
                    (::std::vec![#(#variant_tokens),*], ::std::vec![#(#body_tokens),*])
                }
            });
            quote! {
                ::prompt_templates::compiled::Segment::Match {
                    expr: ::std::borrow::Cow::Borrowed(#expr),
                    arms: ::std::vec![#(#arm_tokens),*],
                }
            }
        }
    }
}

fn codegen_parsed_filter(f: &prompt_templates::compiled::ParsedFilter) -> proc_macro2::TokenStream {
    let kind = codegen_filter_kind(f.kind);
    let args = f.args.as_ref().map_or_else(
        || quote! { ::std::option::Option::None },
        |a| quote! { ::std::option::Option::Some(::std::borrow::Cow::Borrowed(#a)) },
    );
    quote! {
        ::prompt_templates::compiled::ParsedFilter {
            kind: #kind,
            args: #args,
        }
    }
}

fn codegen_filter_kind(k: prompt_templates::compiled::FilterKind) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::FilterKind;
    match k {
        FilterKind::Upper => quote! { ::prompt_templates::compiled::FilterKind::Upper },
        FilterKind::Lower => quote! { ::prompt_templates::compiled::FilterKind::Lower },
        FilterKind::Trim => quote! { ::prompt_templates::compiled::FilterKind::Trim },
        FilterKind::Fixed => quote! { ::prompt_templates::compiled::FilterKind::Fixed },
        FilterKind::Default => quote! { ::prompt_templates::compiled::FilterKind::Default },
        FilterKind::Length => quote! { ::prompt_templates::compiled::FilterKind::Length },
        FilterKind::Join => quote! { ::prompt_templates::compiled::FilterKind::Join },
        FilterKind::Limit => quote! { ::prompt_templates::compiled::FilterKind::Limit },
        FilterKind::Gt => quote! { ::prompt_templates::compiled::FilterKind::Gt },
    }
}

fn codegen_condition(c: &prompt_templates::compiled::Condition) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::Condition;
    match c {
        Condition::Truthy(v) => quote! {
            ::prompt_templates::compiled::Condition::Truthy(::std::borrow::Cow::Borrowed(#v))
        },
        Condition::Comparison { left, op, right } => {
            let op_tokens = codegen_comparison_op(*op);
            quote! {
                ::prompt_templates::compiled::Condition::Comparison {
                    left: ::std::borrow::Cow::Borrowed(#left),
                    op: #op_tokens,
                    right: ::std::borrow::Cow::Borrowed(#right),
                }
            }
        }
    }
}

fn codegen_comparison_op(op: prompt_templates::compiled::ComparisonOp) -> proc_macro2::TokenStream {
    use prompt_templates::compiled::ComparisonOp;
    match op {
        ComparisonOp::Eq => quote! { ::prompt_templates::compiled::ComparisonOp::Eq },
        ComparisonOp::Ne => quote! { ::prompt_templates::compiled::ComparisonOp::Ne },
        ComparisonOp::Le => quote! { ::prompt_templates::compiled::ComparisonOp::Le },
        ComparisonOp::Ge => quote! { ::prompt_templates::compiled::ComparisonOp::Ge },
        ComparisonOp::Lt => quote! { ::prompt_templates::compiled::ComparisonOp::Lt },
        ComparisonOp::Gt => quote! { ::prompt_templates::compiled::ComparisonOp::Gt },
    }
}

fn codegen_compiled_inline_template(
    t: &prompt_templates::compiled::CompiledInlineTemplate,
) -> proc_macro2::TokenStream {
    let segments_tokens = t.segments.iter().map(codegen_segment);
    let decls_tokens = t.declarations.iter().map(codegen_var_decl);
    quote! {
        ::prompt_templates::compiled::CompiledInlineTemplate {
            segments: ::std::sync::Arc::from([#(#segments_tokens),*]),
            declarations: ::std::sync::Arc::from([#(#decls_tokens),*]),
        }
    }
}

fn codegen_var_decl(d: &prompt_templates::VarDecl) -> proc_macro2::TokenStream {
    let name = &d.name;
    let type_tokens = codegen_var_type(&d.var_type);
    quote! {
        ::prompt_templates::VarDecl {
            name: ::std::string::String::from(#name),
            var_type: #type_tokens,
            default_value: ::std::option::Option::None,
        }
    }
}

fn codegen_var_type(t: &prompt_templates::VarType) -> proc_macro2::TokenStream {
    use prompt_templates::VarType;
    match t {
        VarType::Str => quote! { ::prompt_templates::VarType::Str },
        VarType::Bool => quote! { ::prompt_templates::VarType::Bool },
        VarType::Int => quote! { ::prompt_templates::VarType::Int },
        VarType::Float => quote! { ::prompt_templates::VarType::Float },
        VarType::List(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { ::prompt_templates::VarType::List(::std::vec![#(#fields_tokens),*]) }
        }
        VarType::Dict(fields) => {
            let fields_tokens = fields.iter().map(codegen_var_decl);
            quote! { ::prompt_templates::VarType::Dict(::std::vec![#(#fields_tokens),*]) }
        }
        VarType::Enum(variants) => {
            let variants_tokens = variants.iter().map(codegen_variant_decl);
            quote! { ::prompt_templates::VarType::Enum(::std::vec![#(#variants_tokens),*]) }
        }
    }
}

fn codegen_variant_decl(v: &prompt_templates::VariantDecl) -> proc_macro2::TokenStream {
    let name = &v.name;
    let fields_tokens = v.fields.iter().map(codegen_var_decl);
    quote! {
        ::prompt_templates::VariantDecl {
            name: ::std::string::String::from(#name),
            fields: ::std::vec![#(#fields_tokens),*],
        }
    }
}
