//! Direct native Rust code generation for `Params::render` and `Params::render_into`.
//!
//! Compiles validated template [`Segment`]s directly into native Rust statements
//! that append to `__out: &mut String` without constructing a [`Context`], cloning
//! [`Value`]s, allocating [`HashMap`]s, or walking the runtime AST interpreter.

mod control;
mod loops_and_includes;
mod match_codegen;

use hashbrown::HashMap;
use md_tmpl_core::{
    Frontmatter, Value, VarDecl, VarType, VariantDecl,
    compiled::{CompiledExpr, CompiledPath, FilterKind, ParsedFilter, Segment},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use self::{
    control::codegen_if,
    loops_and_includes::{codegen_for_loop, codegen_include},
    match_codegen::codegen_match,
};
use crate::{
    codegen::{codegen_parsed_filter, is_scalar},
    crate_path,
    type_gen::deduplicate_variant_idents,
};

/// A typed Rust expression bound in the current lexical scope.
#[derive(Clone)]
pub(super) struct Binding {
    /// Rust expression evaluating to the value (by value for `Int`/`Float`/`Bool`,
    /// or a place/reference for `Str`, `Struct`, `List`, `Enum`, `Option`).
    pub(super) expr: TokenStream,
    /// Normalized template variable type (`VarType::Option` always normalized).
    pub(super) var_type: VarType,
    /// Prefix used to construct generated nominal Rust struct/enum identifiers
    /// (e.g. `"ParamsTeamsItem"` or `"ParamsStatus"`).
    pub(super) type_prefix: String,
}

/// Result of resolving a [`CompiledPath`] in [`NativeScope`].
#[derive(Clone)]
pub(super) enum ResolvedValue {
    Runtime(Binding),
    Static(Value),
}

/// Typed numeric expression used when folding `add`/`sub` filter chains.
pub(super) enum NumExpr {
    Int(TokenStream),
    Float(TokenStream),
}

/// Lexical scope tracking active Rust bindings, loop metadata, and compile-time constants.
#[derive(Clone)]
pub(super) struct NativeScope {
    pub(super) bindings: HashMap<String, Binding>,
    pub(super) loop_meta: HashMap<String, (syn::Ident, syn::Ident)>,
    pub(super) static_values: HashMap<String, Value>,
}

/// Normalize legacy `Enum([Some, None])` option encodings into `VarType::Option`.
pub(super) fn normalize_var_type(vt: &VarType) -> VarType {
    if vt.is_option() {
        let inner = vt
            .option_inner_type()
            .expect("is_option() guaranteed inner type");
        return VarType::Option(Box::new(normalize_var_type(inner)));
    }
    match vt {
        VarType::Struct(fields) => VarType::Struct(normalize_decls(fields)),
        VarType::List(fields) => VarType::List(normalize_decls(fields)),
        VarType::Enum(variants) => VarType::Enum(
            variants
                .iter()
                .map(|v| VariantDecl {
                    name: v.name.clone(),
                    fields: normalize_decls(&v.fields),
                })
                .collect(),
        ),
        VarType::Option(inner) => VarType::Option(Box::new(normalize_var_type(inner))),
        other => other.clone(),
    }
}

fn normalize_decls(decls: &[VarDecl]) -> Vec<VarDecl> {
    decls
        .iter()
        .map(|d| VarDecl {
            name: d.name.clone(),
            var_type: normalize_var_type(&d.var_type),
            default_value: d.default_value.clone(),
        })
        .collect()
}

pub(super) fn is_copy_scalar(vt: &VarType) -> bool {
    matches!(vt, VarType::Int | VarType::Float | VarType::Bool)
}

pub(super) fn sanitize_ident(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

impl NativeScope {
    fn from_frontmatter(fm: &Frontmatter, struct_name: &syn::Ident) -> Self {
        let struct_name_str = struct_name.to_string();
        let mut bindings = HashMap::new();
        let mut static_values = HashMap::new();

        for decl in &fm.declarations {
            let ident = crate::make_ident(&decl.name);
            let type_prefix = format!(
                "{struct_name_str}{}",
                md_tmpl_core::to_pascal_case(&decl.name)
            );
            bindings.insert(
                decl.name.clone(),
                Binding {
                    expr: quote! { self.#ident },
                    var_type: normalize_var_type(&decl.var_type),
                    type_prefix,
                },
            );
        }

        for decl in &fm.consts {
            if decl.default_value.is_some() {
                let const_ident = crate::make_ident(&decl.name.to_uppercase());
                let type_prefix = format!(
                    "{struct_name_str}{}",
                    md_tmpl_core::to_pascal_case(&decl.name)
                );
                let expr = if is_scalar(&decl.var_type) {
                    quote! { #const_ident }
                } else {
                    quote! { (*#const_ident) }
                };
                bindings.insert(
                    decl.name.clone(),
                    Binding {
                        expr,
                        var_type: normalize_var_type(&decl.var_type),
                        type_prefix,
                    },
                );
            }
        }

        for decl in &fm.env {
            if let Some(ref val) = decl.default_value {
                static_values.insert(decl.name.clone(), val.clone());
            }
        }

        for (k, v) in &fm.imported_consts {
            static_values.insert(k.clone(), v.clone());
        }

        Self {
            bindings,
            loop_meta: HashMap::new(),
            static_values,
        }
    }

    pub(super) fn resolve_path(&self, path: &CompiledPath) -> Option<ResolvedValue> {
        let parts = path.parts();
        if parts.is_empty() {
            return None;
        }

        // Built-in `loop.*` properties inside an active `{% for %}` loop.
        if parts.len() == 2 && parts[0] == "loop" && !self.bindings.contains_key("loop") {
            let (idx_ident, len_ident) = self.loop_meta.get("__active_loop__")?;
            let binding = match parts[1].as_str() {
                "first" => Binding {
                    expr: quote! { (#idx_ident == 0) },
                    var_type: VarType::Bool,
                    type_prefix: String::new(),
                },
                "last" => Binding {
                    expr: quote! { (#idx_ident + 1 == #len_ident) },
                    var_type: VarType::Bool,
                    type_prefix: String::new(),
                },
                "index0" => Binding {
                    expr: quote! { #idx_ident },
                    var_type: VarType::Int,
                    type_prefix: String::new(),
                },
                "index" => Binding {
                    expr: quote! { (#idx_ident + 1) },
                    var_type: VarType::Int,
                    type_prefix: String::new(),
                },
                "len" | "length" => Binding {
                    expr: quote! { #len_ident },
                    var_type: VarType::Int,
                    type_prefix: String::new(),
                },
                _ => return None,
            };
            return Some(ResolvedValue::Runtime(binding));
        }

        // 1. Check runtime bindings by longest prefix match (supports narrowed
        // dotted paths like `user.nickname` or destructured enum variant fields
        // like `outcome.code`).
        for k in (1..=parts.len()).rev() {
            let prefix_key = if k == 1 {
                parts[0].clone()
            } else {
                parts[..k].join(".")
            };
            if let Some(base_binding) = self.bindings.get(&prefix_key) {
                let mut curr = base_binding.clone();
                for part in &parts[k..] {
                    curr = resolve_sub_field(&curr, part)?;
                }
                return Some(ResolvedValue::Runtime(curr));
            }
        }

        // 2. Check static values (compile-time `env`, `imported_consts`, and
        // injected enum type alias namespaces like `Stage.Design`).
        for k in (1..=parts.len()).rev() {
            let prefix_key = if k == 1 {
                parts[0].clone()
            } else {
                parts[..k].join(".")
            };
            if let Some(mut val) = self.static_values.get(&prefix_key) {
                for part in &parts[k..] {
                    if let Value::Struct(map) = val {
                        val = map.get(part.as_str())?;
                    } else {
                        return None;
                    }
                }
                return Some(ResolvedValue::Static(val.clone()));
            }
        }

        None
    }
}

/// Resolve `.part` on a runtime `Binding`.
fn resolve_sub_field(curr: &Binding, part: &str) -> Option<Binding> {
    match &curr.var_type {
        VarType::Struct(fields) => {
            let f_decl = fields.iter().find(|f| f.name == part)?;
            let f_ident = crate::make_ident(part);
            let base = &curr.expr;
            let next_prefix = format!("{}{}", curr.type_prefix, md_tmpl_core::to_pascal_case(part));
            Some(Binding {
                expr: quote! { (#base).#f_ident },
                var_type: f_decl.var_type.clone(),
                type_prefix: next_prefix,
            })
        }
        VarType::Enum(variants) => {
            let first_field = variants
                .iter()
                .find_map(|v| v.fields.iter().find(|f| f.name == part))?;
            if !is_scalar(&first_field.var_type) {
                return None;
            }
            let enum_ident = format_ident!("{}", curr.type_prefix);
            let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
            let deduped = deduplicate_variant_idents(&variant_names);
            let f_ident = crate::make_ident(part);
            let mut match_arms = Vec::new();
            let mut all_have_field = true;
            for (var, (var_ident, _)) in variants.iter().zip(deduped) {
                if var.fields.iter().any(|f| f.name == part) {
                    if is_copy_scalar(&first_field.var_type) {
                        match_arms.push(quote! {
                            #enum_ident::#var_ident { #f_ident, .. } => *#f_ident
                        });
                    } else {
                        match_arms.push(quote! {
                            #enum_ident::#var_ident { #f_ident, .. } => #f_ident.as_str()
                        });
                    }
                } else {
                    all_have_field = false;
                }
            }
            if !all_have_field {
                match_arms.push(quote! {
                    _ => unreachable!("enum variant verified by enclosing match arm")
                });
            }
            let base = &curr.expr;
            Some(Binding {
                expr: quote! {
                    (match &(#base) {
                        #(#match_arms),*
                    })
                },
                var_type: first_field.var_type.clone(),
                type_prefix: String::new(),
            })
        }
        _ => None,
    }
}

/// Try to compile `segments` into native Rust statements writing into `__out: &mut String`.
///
/// Returns `None` if the template uses dynamic features that require the runtime
/// interpreter (such as runtime `tmpl()` parameter includes).
pub(crate) fn try_codegen_native_render(
    fm: &Frontmatter,
    segments: &[Segment],
    struct_name: &syn::Ident,
) -> Option<TokenStream> {
    if fm
        .declarations
        .iter()
        .any(|d| matches!(d.var_type, VarType::Tmpl(_)))
    {
        return None;
    }
    let scope = NativeScope::from_frontmatter(fm, struct_name);
    let mut counter = 0usize;
    codegen_segments(segments, &scope, &mut counter)
}

pub(super) fn codegen_segments(
    segments: &[Segment],
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let mut stmts = Vec::with_capacity(segments.len());
    for seg in segments {
        stmts.push(codegen_segment(seg, scope, counter)?);
    }
    Some(quote! { #(#stmts)* })
}

fn codegen_segment(seg: &Segment, scope: &NativeScope, counter: &mut usize) -> Option<TokenStream> {
    let cp = crate_path();
    match seg {
        Segment::Static(s) | Segment::Raw(s) => {
            if s.is_empty() {
                Some(quote! {})
            } else if s.len() == 1 {
                let ch = s.chars().next().expect("non-empty string");
                Some(quote! { __out.push(#ch); })
            } else {
                let lit = s.as_ref();
                Some(quote! { __out.push_str(#lit); })
            }
        }
        Segment::Comment(_) => Some(quote! {}),
        Segment::Panic(segs) => {
            let mut sub_stmts = Vec::with_capacity(segs.len());
            for s in segs {
                sub_stmts.push(codegen_segment(s, scope, counter)?);
            }
            Some(quote! {
                {
                    let mut __panic_msg = #cp::__private::String::new();
                    {
                        let __out = &mut __panic_msg;
                        #(#sub_stmts)*
                    }
                    return ::core::result::Result::Err(#cp::TemplateError::Panic(__panic_msg));
                }
            })
        }
        Segment::Expr { expr, filters } => codegen_expr(expr, filters, scope),
        Segment::ForLoop {
            binding,
            list_expr,
            filters,
            body,
            else_body,
        } => {
            if !filters.is_empty() {
                return None;
            }
            codegen_for_loop(binding, list_expr, body, else_body, scope, counter)
        }
        Segment::If {
            branches,
            else_body,
        } => codegen_if(branches, else_body, scope, counter),
        Segment::Match {
            expr,
            arms,
            is_option,
        } => codegen_match(expr, arms, *is_option, scope, counter),
        Segment::Include(inc) => codegen_include(inc, scope, counter),
    }
}

fn codegen_render_static_value(val: &Value) -> Option<TokenStream> {
    let cp = crate_path();
    match val {
        Value::Str(s) => {
            if s.is_empty() {
                Some(quote! {})
            } else {
                Some(quote! { __out.push_str(#s); })
            }
        }
        Value::Int(i) => {
            let s = i.to_string();
            Some(quote! { __out.push_str(#s); })
        }
        Value::Float(f) => {
            let bits = f.to_bits();
            Some(quote! {
                #cp::__private::write_float(::core::primitive::f64::from_bits(#bits), __out);
            })
        }
        Value::Bool(true) => Some(quote! { __out.push_str("true"); }),
        Value::Bool(false) => Some(quote! { __out.push_str("false"); }),
        Value::None => Some(quote! {}),
        Value::List(_) | Value::Struct(_) | Value::Tmpl(_) => None,
    }
}

/// Produce an expression of type `&str` for `kind(path)`.
pub(super) fn codegen_kind_expr_str(
    path: &CompiledPath,
    scope: &NativeScope,
) -> Option<TokenStream> {
    match scope.resolve_path(path)? {
        ResolvedValue::Static(val) => match val {
            Value::Str(s) => Some(quote! { #s }),
            Value::Struct(d) => {
                if let Some(Value::Str(k)) = d.get(md_tmpl_core::consts::ENUM_TAG_KEY) {
                    Some(quote! { #k })
                } else {
                    None
                }
            }
            Value::None => Some(quote! { "None" }),
            _ => None,
        },
        ResolvedValue::Runtime(b) => match &b.var_type {
            VarType::Option(_) => {
                let e = &b.expr;
                Some(quote! {
                    (if (#e).is_some() { "Some" } else { "None" })
                })
            }
            VarType::Enum(variants) => {
                let enum_ident = format_ident!("{}", b.type_prefix);
                let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                let deduped = deduplicate_variant_idents(&variant_names);
                let arms: Vec<TokenStream> = variants
                    .iter()
                    .zip(deduped)
                    .map(|(v, (v_ident, _))| {
                        let name_lit = &v.name;
                        if v.fields.is_empty() {
                            quote! { #enum_ident::#v_ident => #name_lit }
                        } else {
                            quote! { #enum_ident::#v_ident { .. } => #name_lit }
                        }
                    })
                    .collect();
                let e = &b.expr;
                Some(quote! {
                    (match &(#e) {
                        #(#arms),*
                    })
                })
            }
            _ => None,
        },
    }
}

/// Try to extract a numeric (`Int` or `Float`) expression from `CompiledExpr`.
pub(super) fn try_extract_num_expr(expr: &CompiledExpr, scope: &NativeScope) -> Option<NumExpr> {
    match expr {
        CompiledExpr::Literal(Value::Int(i)) => Some(NumExpr::Int(quote! { #i })),
        CompiledExpr::Literal(Value::Float(f)) => {
            let bits = f.to_bits();
            Some(NumExpr::Float(
                quote! { ::core::primitive::f64::from_bits(#bits) },
            ))
        }
        CompiledExpr::Idx(binding) => {
            let (idx_ident, _) = scope.loop_meta.get(binding.as_ref())?;
            Some(NumExpr::Int(quote! { #idx_ident }))
        }
        CompiledExpr::Len(path) => match scope.resolve_path(path)? {
            ResolvedValue::Runtime(b) => match b.var_type {
                VarType::List(_) | VarType::Str => {
                    let e = &b.expr;
                    Some(NumExpr::Int(quote! {
                        i64::try_from((#e).len()).expect("len fits i64")
                    }))
                }
                _ => None,
            },
            ResolvedValue::Static(Value::List(l)) => {
                let n = i64::try_from(l.len()).expect("collection length fits i64");
                Some(NumExpr::Int(quote! { #n }))
            }
            ResolvedValue::Static(Value::Str(s)) => {
                let n = i64::try_from(s.len()).expect("string length fits i64");
                Some(NumExpr::Int(quote! { #n }))
            }
            ResolvedValue::Static(_) => None,
        },
        CompiledExpr::Path(path) => match scope.resolve_path(path)? {
            ResolvedValue::Runtime(b) => match b.var_type {
                VarType::Int => Some(NumExpr::Int(b.expr)),
                VarType::Float => Some(NumExpr::Float(b.expr)),
                _ => None,
            },
            ResolvedValue::Static(Value::Int(i)) => Some(NumExpr::Int(quote! { #i })),
            ResolvedValue::Static(Value::Float(f)) => {
                let bits = f.to_bits();
                Some(NumExpr::Float(
                    quote! { ::core::primitive::f64::from_bits(#bits) },
                ))
            }
            ResolvedValue::Static(_) => None,
        },
        _ => None,
    }
}

/// Try to extract a `&str` expression from `CompiledExpr`.
fn try_extract_str_expr(expr: &CompiledExpr, scope: &NativeScope) -> Option<TokenStream> {
    match expr {
        CompiledExpr::Literal(Value::Str(s)) => Some(quote! { #s }),
        CompiledExpr::Kind(path) => codegen_kind_expr_str(path, scope),
        CompiledExpr::Path(path) => match scope.resolve_path(path)? {
            ResolvedValue::Runtime(b) if b.var_type == VarType::Str => {
                let e = &b.expr;
                Some(quote! { ::core::convert::AsRef::<str>::as_ref(&(#e)) })
            }
            ResolvedValue::Static(Value::Str(s)) => Some(quote! { #s }),
            _ => None,
        },
        _ => None,
    }
}

/// Fold a slice of `Add` / `Sub` filters onto a `NumExpr`.
pub(super) fn fold_numeric_filters(mut num: NumExpr, filters: &[ParsedFilter]) -> Option<NumExpr> {
    let cp = crate_path();
    for f in filters {
        let is_add = match f.kind {
            FilterKind::Add => true,
            FilterKind::Sub => false,
            _ => return None,
        };
        let raw = f.args.as_deref()?.trim();
        if let Ok(n) = raw.parse::<i64>() {
            num = match num {
                NumExpr::Int(e) => {
                    if is_add {
                        NumExpr::Int(quote! { (#e).saturating_add(#n) })
                    } else {
                        NumExpr::Int(quote! { (#e).saturating_sub(#n) })
                    }
                }
                NumExpr::Float(e) => {
                    let nf_bits = md_tmpl_core::__private::i64_to_f64(n).to_bits();
                    let nf = quote! { ::core::primitive::f64::from_bits(#nf_bits) };
                    if is_add {
                        NumExpr::Float(quote! { (#e + #nf) })
                    } else {
                        NumExpr::Float(quote! { (#e - #nf) })
                    }
                }
            };
        } else if let Ok(nf_val) = raw.parse::<f64>() {
            let nf_bits = nf_val.to_bits();
            let nf = quote! { ::core::primitive::f64::from_bits(#nf_bits) };
            num = match num {
                NumExpr::Int(e) => {
                    if is_add {
                        NumExpr::Float(quote! { (#cp::__private::i64_to_f64(#e) + #nf) })
                    } else {
                        NumExpr::Float(quote! { (#cp::__private::i64_to_f64(#e) - #nf) })
                    }
                }
                NumExpr::Float(e) => {
                    if is_add {
                        NumExpr::Float(quote! { (#e + #nf) })
                    } else {
                        NumExpr::Float(quote! { (#e - #nf) })
                    }
                }
            };
        } else {
            return None;
        }
    }
    Some(num)
}

pub(super) fn codegen_expr(
    expr: &CompiledExpr,
    filters: &[ParsedFilter],
    scope: &NativeScope,
) -> Option<TokenStream> {
    let cp = crate_path();

    // 1. Fast path: no filters.
    if filters.is_empty() {
        if let Some(num) = try_extract_num_expr(expr, scope) {
            return Some(match num {
                NumExpr::Int(e) => quote! { #cp::__private::write_int(#e, __out); },
                NumExpr::Float(e) => quote! { #cp::__private::write_float(#e, __out); },
            });
        }
        if let Some(str_expr) = try_extract_str_expr(expr, scope) {
            return Some(quote! { __out.push_str(#str_expr); });
        }
        match expr {
            CompiledExpr::Literal(val) => return codegen_render_static_value(val),
            CompiledExpr::Has(path) => {
                if let ResolvedValue::Runtime(b) = scope.resolve_path(path)?
                    && matches!(b.var_type, VarType::Option(_))
                {
                    let e = &b.expr;
                    return Some(quote! {
                        __out.push_str(if (#e).is_some() { "true" } else { "false" });
                    });
                }
                return None;
            }
            CompiledExpr::Path(path) => match scope.resolve_path(path)? {
                ResolvedValue::Static(val) => return codegen_render_static_value(&val),
                ResolvedValue::Runtime(b) if b.var_type == VarType::Bool => {
                    let e = &b.expr;
                    return Some(quote! {
                        __out.push_str(if #e { "true" } else { "false" });
                    });
                }
                ResolvedValue::Runtime(_) => return None,
            },
            _ => return None,
        }
    }

    // 2. Numeric expression with `add`/`sub` and/or trailing `fixed(n)`.
    if let Some(base_num) = try_extract_num_expr(expr, scope) {
        let (arith_slice, trailing_fixed) = match filters.split_last() {
            Some((last, prefix)) if last.kind == FilterKind::Fixed => {
                (prefix, Some(last.parsed_num?))
            }
            _ => (filters, None),
        };
        if let Some(folded) = fold_numeric_filters(base_num, arith_slice) {
            return Some(match (folded, trailing_fixed) {
                (NumExpr::Int(e), None) => quote! { #cp::__private::write_int(#e, __out); },
                (NumExpr::Float(e), None) => quote! { #cp::__private::write_float(#e, __out); },
                (NumExpr::Int(e), Some(prec)) => {
                    quote! { #cp::__private::write_fixed_int(#e, #prec, __out); }
                }
                (NumExpr::Float(e), Some(prec)) => {
                    quote! { #cp::__private::write_fixed_float(#e, #prec, __out); }
                }
            });
        }
    }

    // 3. String expression with filters.
    if let Some(str_expr) = try_extract_str_expr(expr, scope) {
        if filters.len() == 1 {
            match filters[0].kind {
                FilterKind::Upper => {
                    return Some(quote! { #cp::__private::write_upper(#str_expr, __out); });
                }
                FilterKind::Lower => {
                    return Some(quote! { #cp::__private::write_lower(#str_expr, __out); });
                }
                FilterKind::Trim => {
                    return Some(quote! { __out.push_str((#str_expr).trim()); });
                }
                FilterKind::EscapeXml => {
                    return Some(quote! { __out.push_str(&#cp::escape_xml_str(#str_expr)); });
                }
                FilterKind::EscapeJson => {
                    return Some(quote! { __out.push_str(&#cp::escape_json_str(#str_expr)); });
                }
                _ => {}
            }
        }
        let filter_tokens: Vec<TokenStream> = filters.iter().map(codegen_parsed_filter).collect();
        return Some(quote! {
            #cp::__private::render_str_filters(#str_expr, &[#(#filter_tokens),*], __out)?;
        });
    }

    // 4. List expression with optional `limit(n)` followed by `join(sep)`.
    codegen_list_join_expr(expr, filters, scope)
}

/// Unquote and unescape a filter string argument at compile time.
fn unquote_filter_arg(raw: &str) -> String {
    match md_tmpl_core::consts::strip_string_literal(raw) {
        Some(inner) => md_tmpl_core::consts::unescape_string_literal(inner),
        None => raw.to_string(),
    }
}

fn codegen_list_join_expr(
    expr: &CompiledExpr,
    filters: &[ParsedFilter],
    scope: &NativeScope,
) -> Option<TokenStream> {
    let cp = crate_path();
    let (limit_n, join_filter) = match filters {
        [j] if j.kind == FilterKind::Join => (None, j),
        [l, j] if l.kind == FilterKind::Limit && j.kind == FilterKind::Join => {
            (Some(l.parsed_num?), j)
        }
        _ => return None,
    };
    let sep = unquote_filter_arg(join_filter.args.as_deref().unwrap_or(""));

    match expr {
        CompiledExpr::Path(path) => {
            let ResolvedValue::Runtime(b) = scope.resolve_path(path)? else {
                return None;
            };
            let VarType::List(fields) = &b.var_type else {
                return None;
            };
            if fields.len() != 1 || !fields[0].name.is_empty() {
                return None;
            }
            let elem_vt = &fields[0].var_type;
            let write_item = match elem_vt {
                VarType::Str => quote! {
                    __out.push_str(::core::convert::AsRef::<str>::as_ref(__join_item));
                },
                VarType::Int => quote! { #cp::__private::write_int(*__join_item, __out); },
                VarType::Float => quote! { #cp::__private::write_float(*__join_item, __out); },
                VarType::Bool => quote! {
                    __out.push_str(if *__join_item { "true" } else { "false" });
                },
                _ => return None,
            };
            let list_e = &b.expr;
            let iter_expr = if let Some(n) = limit_n {
                quote! { (#list_e).iter().take(#n) }
            } else {
                quote! { (#list_e).iter() }
            };
            Some(quote! {
                for (__join_idx, __join_item) in #iter_expr.enumerate() {
                    if __join_idx > 0 {
                        __out.push_str(#sep);
                    }
                    #write_item
                }
            })
        }
        CompiledExpr::Kinds(path) => {
            let names = resolve_static_kinds(path, scope)?;
            let slice = if let Some(n) = limit_n {
                &names[..names.len().min(n)]
            } else {
                &names[..]
            };
            let joined = slice.join(&sep);
            Some(quote! { __out.push_str(#joined); })
        }
        _ => None,
    }
}

/// Resolve `kinds(path)` to its list of variant names at compile time.
pub(super) fn resolve_static_kinds(
    path: &CompiledPath,
    scope: &NativeScope,
) -> Option<Vec<String>> {
    match scope.resolve_path(path)? {
        ResolvedValue::Static(Value::Struct(d)) => {
            let Value::List(items) = d.get(md_tmpl_core::consts::ENUM_VARIANTS_KEY)? else {
                return None;
            };
            let mut out = Vec::with_capacity(items.len());
            for item in items.iter() {
                if let Value::Str(s) = item {
                    out.push(s.clone());
                } else {
                    return None;
                }
            }
            Some(out)
        }
        ResolvedValue::Runtime(b) => {
            if let VarType::Enum(variants) = b.var_type {
                Some(variants.into_iter().map(|v| v.name).collect())
            } else {
                None
            }
        }
        ResolvedValue::Static(_) => None,
    }
}
