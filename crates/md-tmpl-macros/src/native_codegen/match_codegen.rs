//! Native Rust code generation for `{% match %}` blocks over `Option`, enums, and scalars.

use md_tmpl_core::{
    Value, VarType, VariantDecl,
    compiled::{CompiledPath, ConditionOperand, MatchArm},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{
    Binding, NativeScope, ResolvedValue, codegen_segments,
    control::{TypedOperand, codegen_condition, codegen_operand_typed},
    is_copy_scalar, sanitize_ident,
};
use crate::{crate_path, type_gen::deduplicate_variant_idents};

pub(super) fn codegen_match(
    expr: &CompiledPath,
    arms: &[MatchArm],
    is_option: bool,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let resolved = scope.resolve_path(expr)?;
    match resolved {
        ResolvedValue::Runtime(b) => match &b.var_type {
            VarType::Option(inner) => codegen_match_option(expr, &b, inner, arms, scope, counter),
            VarType::Enum(declared) if !is_option => {
                codegen_match_enum(expr, &b, declared, arms, scope, counter)
            }
            VarType::Str | VarType::Int | VarType::Float | VarType::Bool => {
                codegen_match_scalar(&b, arms, scope, counter)
            }
            _ => None,
        },
        ResolvedValue::Static(val) => {
            let synthetic = match val {
                Value::Str(s) => Binding {
                    expr: quote! { #s },
                    var_type: VarType::Str,
                    type_prefix: String::new(),
                },
                Value::Int(i) => Binding {
                    expr: quote! { #i },
                    var_type: VarType::Int,
                    type_prefix: String::new(),
                },
                Value::Float(f) => {
                    let bits = f.to_bits();
                    Binding {
                        expr: quote! { ::core::primitive::f64::from_bits(#bits) },
                        var_type: VarType::Float,
                        type_prefix: String::new(),
                    }
                }
                Value::Bool(b) => Binding {
                    expr: quote! { #b },
                    var_type: VarType::Bool,
                    type_prefix: String::new(),
                },
                _ => return None,
            };
            codegen_match_scalar(&synthetic, arms, scope, counter)
        }
    }
}

fn codegen_match_option(
    expr: &CompiledPath,
    opt_binding: &Binding,
    inner_vt: &VarType,
    arms: &[MatchArm],
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let mut rust_arms = Vec::with_capacity(arms.len() + 1);
    let mut has_unconditional_some = false;
    let mut has_unconditional_none = false;
    let mut has_unconditional_default = false;

    for arm in arms {
        let id = *counter;
        *counter += 1;
        let has_default = arm
            .variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::MATCH_DEFAULT);
        let has_some = arm
            .variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::OPTION_SOME);
        let has_none = arm
            .variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::OPTION_NONE);

        let mut arm_scope = scope.clone();
        let pat = if has_default || (has_some && has_none) {
            if arm.guard.is_none() {
                has_unconditional_default = true;
            }
            quote! { _ }
        } else if has_some {
            if arm.guard.is_none() {
                has_unconditional_some = true;
            }
            let val_ident = format_ident!("_md_opt_{id}_{}", sanitize_ident(expr.as_str()));
            let bound_expr = if is_copy_scalar(inner_vt) {
                quote! { (*#val_ident) }
            } else {
                quote! { #val_ident }
            };
            arm_scope.bindings.insert(
                expr.as_str().to_string(),
                Binding {
                    expr: bound_expr,
                    var_type: inner_vt.clone(),
                    type_prefix: opt_binding.type_prefix.clone(),
                },
            );
            quote! { ::core::option::Option::Some(#val_ident) }
        } else if has_none {
            if arm.guard.is_none() {
                has_unconditional_none = true;
            }
            quote! { ::core::option::Option::None }
        } else {
            return None;
        };

        let guard_tok = if let Some(ref g) = arm.guard {
            let g_expr = codegen_condition(g, &arm_scope, counter)?;
            quote! { if #g_expr }
        } else {
            quote! {}
        };

        let body_tok = codegen_segments(&arm.body, &arm_scope, counter)?;
        rust_arms.push(quote! {
            #pat #guard_tok => {
                #body_tok
            }
        });
    }

    if !(has_unconditional_default || has_unconditional_some && has_unconditional_none) {
        rust_arms.push(quote! { _ => {} });
    }

    let opt_e = &opt_binding.expr;
    Some(quote! {
        match &(#opt_e) {
            #(#rust_arms),*
        }
    })
}

fn codegen_single_enum_arm_pat(
    expr: &CompiledPath,
    enum_binding: &Binding,
    enum_ident: &syn::Ident,
    decl: &VariantDecl,
    v_ident: &syn::Ident,
    id: usize,
    arm_scope: &mut NativeScope,
) -> TokenStream {
    if decl.fields.is_empty() {
        return quote! { #enum_ident::#v_ident };
    }
    let mut pat_fields = Vec::with_capacity(decl.fields.len());
    for f_decl in &decl.fields {
        let f_ident = crate::make_ident(&f_decl.name);
        let bind_ident = format_ident!("_md_vfield_{id}_{}", sanitize_ident(&f_decl.name));
        pat_fields.push(quote! { #f_ident: #bind_ident });

        let field_expr = if is_copy_scalar(&f_decl.var_type) {
            quote! { (*#bind_ident) }
        } else {
            quote! { #bind_ident }
        };
        let field_prefix = format!(
            "{}{v_ident}{}",
            enum_binding.type_prefix,
            md_tmpl_core::to_pascal_case(&f_decl.name)
        );
        arm_scope.bindings.insert(
            format!("{}.{}", expr.as_str(), f_decl.name),
            Binding {
                expr: field_expr,
                var_type: f_decl.var_type.clone(),
                type_prefix: field_prefix,
            },
        );
    }
    quote! { #enum_ident::#v_ident { #(#pat_fields),* } }
}

fn codegen_match_enum(
    expr: &CompiledPath,
    enum_binding: &Binding,
    declared: &[VariantDecl],
    arms: &[MatchArm],
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let enum_ident = format_ident!("{}", enum_binding.type_prefix);
    let decl_names: Vec<String> = declared.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&decl_names);

    let mut rust_arms = Vec::with_capacity(arms.len() + 1);
    let mut unconditional_covered: Vec<&str> = Vec::new();
    let mut has_unconditional_default = false;

    for arm in arms {
        let id = *counter;
        *counter += 1;
        let is_default = arm
            .variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::MATCH_DEFAULT);

        let mut arm_scope = scope.clone();

        let pat = if is_default {
            if arm.guard.is_none() {
                has_unconditional_default = true;
            }
            quote! { _ }
        } else if arm.variants.len() == 1 {
            let v_name = arm.variants[0].as_ref();
            let (decl, (v_ident, _)) = declared
                .iter()
                .zip(deduped.iter())
                .find(|(d, _)| d.name == v_name)?;
            if arm.guard.is_none() {
                unconditional_covered.push(decl.name.as_str());
            }
            codegen_single_enum_arm_pat(
                expr,
                enum_binding,
                &enum_ident,
                decl,
                v_ident,
                id,
                &mut arm_scope,
            )
        } else {
            let mut sub_pats = Vec::with_capacity(arm.variants.len());
            for v_name in &arm.variants {
                let (decl, (v_ident, _)) = declared
                    .iter()
                    .zip(deduped.iter())
                    .find(|(d, _)| d.name == v_name.as_ref())?;
                if arm.guard.is_none() {
                    unconditional_covered.push(decl.name.as_str());
                }
                if decl.fields.is_empty() {
                    sub_pats.push(quote! { #enum_ident::#v_ident });
                } else {
                    sub_pats.push(quote! { #enum_ident::#v_ident { .. } });
                }
            }
            quote! { #(#sub_pats)|* }
        };

        let guard_tok = if let Some(ref g) = arm.guard {
            let g_expr = codegen_condition(g, &arm_scope, counter)?;
            quote! { if #g_expr }
        } else {
            quote! {}
        };

        let body_tok = codegen_segments(&arm.body, &arm_scope, counter)?;
        rust_arms.push(quote! {
            #pat #guard_tok => {
                #body_tok
            }
        });
    }

    let all_covered = has_unconditional_default
        || declared
            .iter()
            .all(|d| unconditional_covered.contains(&d.name.as_str()));
    if !all_covered {
        rust_arms.push(quote! { _ => {} });
    }

    let enum_e = &enum_binding.expr;
    Some(quote! {
        match &(#enum_e) {
            #(#rust_arms),*
        }
    })
}

fn codegen_match_scalar(
    scalar_binding: &Binding,
    arms: &[MatchArm],
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let val_ident = format_ident!("_md_match_val_{}", *counter);
    *counter += 1;

    let base_e = &scalar_binding.expr;
    let init_stmt = match scalar_binding.var_type {
        VarType::Str => quote! {
            let #val_ident: &str = ::core::convert::AsRef::<str>::as_ref(&(#base_e));
        },
        VarType::Int => quote! { let #val_ident: i64 = #base_e; },
        VarType::Float => quote! { let #val_ident: f64 = #base_e; },
        VarType::Bool => quote! { let #val_ident: bool = #base_e; },
        _ => return None,
    };

    let mut branches = Vec::with_capacity(arms.len());
    let mut fallback_body = None;

    for arm in arms {
        let is_default = arm
            .variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::MATCH_DEFAULT);
        let body_tok = codegen_segments(&arm.body, scope, counter)?;

        if is_default && arm.guard.is_none() {
            fallback_body = Some(body_tok);
            break;
        }

        let mut label_checks = Vec::with_capacity(arm.variants.len());
        if !is_default {
            for label_cow in &arm.variants {
                let check = codegen_scalar_case_check(
                    &val_ident,
                    &scalar_binding.var_type,
                    label_cow.as_ref(),
                    scope,
                    counter,
                )?;
                label_checks.push(check);
            }
        }

        let label_cond = if is_default {
            quote! { true }
        } else {
            quote! { (#(#label_checks)||*) }
        };

        let full_cond = if let Some(ref g) = arm.guard {
            let g_tok = codegen_condition(g, scope, counter)?;
            if is_default {
                g_tok
            } else {
                quote! { (#label_cond && (#g_tok)) }
            }
        } else {
            label_cond
        };

        if branches.is_empty() {
            branches.push(quote! {
                if #full_cond {
                    #body_tok
                }
            });
        } else {
            branches.push(quote! {
                else if #full_cond {
                    #body_tok
                }
            });
        }
    }

    let else_part = match fallback_body {
        Some(fb) if branches.is_empty() => return Some(fb),
        Some(fb) => quote! {
            else {
                #fb
            }
        },
        None => quote! {},
    };

    Some(quote! {
        {
            #init_stmt
            #(#branches)*
            #else_part
        }
    })
}

fn codegen_scalar_case_check(
    val_ident: &syn::Ident,
    var_type: &VarType,
    label: &str,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let cp = crate_path();
    // 1. Quoted string literal (possibly interpolated `"...{{ x }}..."`).
    if let Some(inner) = md_tmpl_core::consts::strip_string_literal(label) {
        if *var_type != VarType::Str {
            return None;
        }
        let unescaped = md_tmpl_core::consts::unescape_string_literal(inner);
        if unescaped.contains(md_tmpl_core::consts::EXPR_START) {
            let segs = match md_tmpl_core::compiled::compile_body(&unescaped) {
                Ok(s) => s,
                Err(err) => {
                    drop(err);
                    return None;
                }
            };
            let seg_tokens = codegen_segments(&segs, scope, counter)?;
            return Some(quote! {
                (#val_ident == ({
                    let mut __case_str = #cp::__private::String::new();
                    {
                        let __out = &mut __case_str;
                        #seg_tokens
                    }
                    __case_str
                }).as_str())
            });
        }
        return Some(quote! { (#val_ident == #unescaped) });
    }

    // 2. Literal scalar according to `var_type`.
    match var_type {
        VarType::Bool => {
            if label == "true" {
                return Some(quote! { #val_ident });
            } else if label == "false" {
                return Some(quote! { !#val_ident });
            }
        }
        VarType::Int => {
            if let Ok(n) = label.parse::<i64>() {
                return Some(quote! { (#val_ident == #n) });
            }
        }
        VarType::Float => {
            if let Ok(f) = label.parse::<f64>() {
                let bits = f.to_bits();
                return Some(quote! {
                    (#val_ident).partial_cmp(&::core::primitive::f64::from_bits(#bits)).is_some_and(::core::cmp::Ordering::is_eq)
                });
            }
        }
        VarType::Str => {}
        _ => return None,
    }

    // 3. Variable or constant reference (or unquoted string fallback on `VarType::Str`).
    let path = CompiledPath::compile(label);
    if scope.resolve_path(&path).is_some() {
        let op = ConditionOperand::Path {
            path,
            filters: Vec::new(),
        };
        let rhs = codegen_operand_typed(&op, scope, counter)?;
        return match (var_type, rhs) {
            (VarType::Float, TypedOperand::Int(r)) => Some(quote! {
                #cp::__private::cmp_int_float(#r, #val_ident).is_some_and(::core::cmp::Ordering::is_eq)
            }),
            (VarType::Float, TypedOperand::Float(r)) => Some(quote! {
                (#val_ident).partial_cmp(&(#r)).is_some_and(::core::cmp::Ordering::is_eq)
            }),
            (VarType::Str, TypedOperand::Str(r))
            | (VarType::Int, TypedOperand::Int(r))
            | (VarType::Bool, TypedOperand::Bool(r)) => Some(quote! { (#val_ident == (#r)) }),
            _ => None,
        };
    }

    if *var_type == VarType::Str {
        Some(quote! { (#val_ident == #label) })
    } else {
        None
    }
}
