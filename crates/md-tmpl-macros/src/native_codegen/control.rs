//! Native Rust code generation for `{% if %}` conditionals, boolean conditions,
//! and comparisons.

use md_tmpl_core::{
    Value, VarType,
    compiled::{ComparisonOp, CompiledExpr, CompiledPath, Condition, ConditionOperand, Segment},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{
    Binding, NativeScope, NumExpr, ResolvedValue, codegen_expr, codegen_kind_expr_str,
    codegen_segments, fold_numeric_filters, is_copy_scalar, resolve_static_kinds, sanitize_ident,
    try_extract_num_expr,
};
use crate::{crate_path, type_gen::deduplicate_variant_idents};

/// Information needed to narrow an `Option` path inside a branch or condition.
#[derive(Clone)]
struct OptionNarrowing {
    path_str: String,
    opt_expr: TokenStream,
    inner_vt: VarType,
    type_prefix: String,
}

fn extract_has_narrowing(condition: &Condition, scope: &NativeScope) -> Option<OptionNarrowing> {
    let Condition::Truthy(ConditionOperand::Has(path) | ConditionOperand::Path { path, .. }) =
        condition
    else {
        return None;
    };
    let ResolvedValue::Runtime(b) = scope.resolve_path(path)? else {
        return None;
    };
    let VarType::Option(inner) = b.var_type else {
        return None;
    };
    Some(OptionNarrowing {
        path_str: path.as_str().to_string(),
        opt_expr: b.expr,
        inner_vt: (*inner).clone(),
        type_prefix: b.type_prefix,
    })
}

fn collect_and_narrowings(
    condition: &Condition,
    scope: &NativeScope,
    out: &mut Vec<OptionNarrowing>,
) {
    match condition {
        Condition::And(left, right) => {
            collect_and_narrowings(left, scope, out);
            let mut right_scope = scope.clone();
            for n in out.iter() {
                apply_option_narrowing_inline(&mut right_scope, n);
            }
            collect_and_narrowings(right, &right_scope, out);
        }
        other => {
            if let Some(n) = extract_has_narrowing(other, scope) {
                out.push(n);
            }
        }
    }
}

fn extract_not_has_narrowing(
    condition: &Condition,
    scope: &NativeScope,
) -> Option<OptionNarrowing> {
    let Condition::Not(inner) = condition else {
        return None;
    };
    extract_has_narrowing(inner, scope)
}

/// Apply an `OptionNarrowing` directly as an inline `.as_ref().expect(...)` expression
/// (used when evaluating the right-hand side of `has(x) && ...` where short-circuiting
/// guarantees `x.is_some()`).
fn apply_option_narrowing_inline(scope: &mut NativeScope, n: &OptionNarrowing) {
    let opt_e = &n.opt_expr;
    let expr = if is_copy_scalar(&n.inner_vt) {
        quote! { (*(#opt_e).as_ref().expect("option verified by short-circuiting has()")) }
    } else {
        quote! { (#opt_e).as_ref().expect("option verified by short-circuiting has()") }
    };
    scope.bindings.insert(
        n.path_str.clone(),
        Binding {
            expr,
            var_type: n.inner_vt.clone(),
            type_prefix: n.type_prefix.clone(),
        },
    );
}

/// Bind `OptionNarrowing`s as local variables at the top of a branch block and insert
/// them into `scope`.
fn bind_option_narrowings_in_block(
    scope: &mut NativeScope,
    narrowings: &[OptionNarrowing],
    counter: &mut usize,
) -> Vec<TokenStream> {
    let mut let_stmts = Vec::with_capacity(narrowings.len());
    for n in narrowings {
        let id = *counter;
        *counter += 1;
        let var_ident = format_ident!("_md_narrow_{id}_{}", sanitize_ident(&n.path_str));
        let opt_e = &n.opt_expr;
        let_stmts.push(quote! {
            let #var_ident = (#opt_e).as_ref().expect("option verified present by branch condition");
        });
        let bound_expr = if is_copy_scalar(&n.inner_vt) {
            quote! { (*#var_ident) }
        } else {
            quote! { #var_ident }
        };
        scope.bindings.insert(
            n.path_str.clone(),
            Binding {
                expr: bound_expr,
                var_type: n.inner_vt.clone(),
                type_prefix: n.type_prefix.clone(),
            },
        );
    }
    let_stmts
}

pub(super) fn codegen_if(
    branches: &[(Condition, Vec<Segment>)],
    else_body: &[Segment],
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    if branches.is_empty() {
        return Some(quote! {});
    }

    let mut carry: Vec<OptionNarrowing> = Vec::new();
    let mut branch_tokens = Vec::with_capacity(branches.len());

    for (idx, (condition, body)) in branches.iter().enumerate() {
        let mut cond_scope = scope.clone();
        for n in &carry {
            apply_option_narrowing_inline(&mut cond_scope, n);
        }
        let cond_expr = codegen_condition(condition, &cond_scope, counter)?;

        let mut positive = Vec::new();
        collect_and_narrowings(condition, &cond_scope, &mut positive);

        let mut body_scope = scope.clone();
        let all_narrowings: Vec<OptionNarrowing> = carry.iter().cloned().chain(positive).collect();
        let narrow_lets =
            bind_option_narrowings_in_block(&mut body_scope, &all_narrowings, counter);
        let body_stmts = codegen_segments(body, &body_scope, counter)?;

        if idx == 0 {
            branch_tokens.push(quote! {
                if #cond_expr {
                    #(#narrow_lets)*
                    #body_stmts
                }
            });
        } else {
            branch_tokens.push(quote! {
                else if #cond_expr {
                    #(#narrow_lets)*
                    #body_stmts
                }
            });
        }

        if let Some(neg) = extract_not_has_narrowing(condition, &cond_scope) {
            carry.push(neg);
        }
    }

    let else_part = if else_body.is_empty() {
        quote! {}
    } else {
        let mut else_scope = scope.clone();
        let narrow_lets = bind_option_narrowings_in_block(&mut else_scope, &carry, counter);
        let else_stmts = codegen_segments(else_body, &else_scope, counter)?;
        quote! {
            else {
                #(#narrow_lets)*
                #else_stmts
            }
        }
    };

    Some(quote! {
        #(#branch_tokens)*
        #else_part
    })
}

pub(super) fn codegen_condition(
    condition: &Condition,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    match condition {
        Condition::Truthy(operand) => codegen_truthy(operand, scope, counter),
        Condition::Not(inner) => {
            let inner_tok = codegen_condition(inner, scope, counter)?;
            Some(quote! { !(#inner_tok) })
        }
        Condition::And(left, right) => {
            let left_tok = codegen_condition(left, scope, counter)?;
            let mut right_scope = scope.clone();
            let mut narrowings = Vec::new();
            collect_and_narrowings(left, scope, &mut narrowings);
            for n in &narrowings {
                apply_option_narrowing_inline(&mut right_scope, n);
            }
            let right_tok = codegen_condition(right, &right_scope, counter)?;
            Some(quote! { ((#left_tok) && (#right_tok)) })
        }
        Condition::Or(left, right) => {
            let left_tok = codegen_condition(left, scope, counter)?;
            let right_tok = codegen_condition(right, scope, counter)?;
            Some(quote! { ((#left_tok) || (#right_tok)) })
        }
        Condition::Comparison { left, op, right } => {
            codegen_comparison(left, *op, right, scope, counter)
        }
        Condition::MatchVariant {
            expr,
            variants,
            is_option,
        } => codegen_match_variant_cond(expr, variants, *is_option, scope),
    }
}

fn codegen_truthy(
    operand: &ConditionOperand,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    match operand {
        ConditionOperand::Literal(val) => {
            let b = val.is_truthy();
            Some(quote! { #b })
        }
        ConditionOperand::Has(path) => {
            let ResolvedValue::Runtime(b) = scope.resolve_path(path)? else {
                return None;
            };
            if matches!(b.var_type, VarType::Option(_)) {
                let e = &b.expr;
                Some(quote! { (#e).is_some() })
            } else {
                None
            }
        }
        ConditionOperand::Idx(binding) => {
            let (idx_ident, _) = scope.loop_meta.get(binding.as_ref())?;
            Some(quote! { (#idx_ident != 0) })
        }
        ConditionOperand::Len(path) => match scope.resolve_path(path)? {
            ResolvedValue::Runtime(b) if matches!(b.var_type, VarType::List(_) | VarType::Str) => {
                let e = &b.expr;
                Some(quote! { !(#e).is_empty() })
            }
            ResolvedValue::Static(Value::List(l)) => {
                let non_empty = !l.is_empty();
                Some(quote! { #non_empty })
            }
            ResolvedValue::Static(Value::Str(s)) => {
                let non_empty = !s.is_empty();
                Some(quote! { #non_empty })
            }
            _ => None,
        },
        ConditionOperand::Kind(path) => {
            let k_expr = codegen_kind_expr_str(path, scope)?;
            Some(quote! { !(#k_expr).is_empty() })
        }
        ConditionOperand::Kinds(path) => {
            let names = resolve_static_kinds(path, scope)?;
            let non_empty = !names.is_empty();
            Some(quote! { #non_empty })
        }
        ConditionOperand::InterpolatedStr(segs) => {
            let seg_tokens = codegen_segments(segs, scope, counter)?;
            let cp = crate_path();
            Some(quote! {
                !({
                    let mut __interp = #cp::__private::String::new();
                    {
                        let __out = &mut __interp;
                        #seg_tokens
                    }
                    __interp
                }).is_empty()
            })
        }
        ConditionOperand::Path { path, filters } => {
            if !filters.is_empty() {
                let op_val = codegen_operand_typed(operand, scope, counter)?;
                return match op_val {
                    TypedOperand::Bool(e) => Some(e),
                    TypedOperand::Int(e) => Some(quote! { (#e != 0) }),
                    TypedOperand::Float(e) => Some(quote! { (#e != 0.0) }),
                    TypedOperand::Str(e) => Some(quote! { !(#e).is_empty() }),
                    TypedOperand::List { .. } => None,
                };
            }
            match scope.resolve_path(path)? {
                ResolvedValue::Static(val) => {
                    let b = val.is_truthy();
                    Some(quote! { #b })
                }
                ResolvedValue::Runtime(b) => {
                    let e = &b.expr;
                    match b.var_type {
                        VarType::Bool => Some(quote! { (#e) }),
                        VarType::Int => Some(quote! { (#e != 0) }),
                        VarType::Float => Some(quote! { (#e != 0.0) }),
                        VarType::Str | VarType::List(_) => Some(quote! { !(#e).is_empty() }),
                        VarType::Option(_) => Some(quote! { (#e).is_some() }),
                        _ => None,
                    }
                }
            }
        }
    }
}

/// Strongly-typed representation of a resolved [`ConditionOperand`].
pub(super) enum TypedOperand {
    Str(TokenStream),
    Int(TokenStream),
    Float(TokenStream),
    Bool(TokenStream),
    List { expr: TokenStream, elem_vt: VarType },
}

pub(super) fn codegen_operand_typed(
    operand: &ConditionOperand,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TypedOperand> {
    let cp = crate_path();
    match operand {
        ConditionOperand::Literal(val) => match val {
            Value::Str(s) => Some(TypedOperand::Str(quote! { #s })),
            Value::Int(i) => Some(TypedOperand::Int(quote! { #i })),
            Value::Float(f) => {
                let bits = f.to_bits();
                Some(TypedOperand::Float(quote! {
                    ::core::primitive::f64::from_bits(#bits)
                }))
            }
            Value::Bool(b) => Some(TypedOperand::Bool(quote! { #b })),
            _ => None,
        },
        ConditionOperand::Idx(binding) => {
            let (idx_ident, _) = scope.loop_meta.get(binding.as_ref())?;
            Some(TypedOperand::Int(quote! { #idx_ident }))
        }
        ConditionOperand::Len(path) => match scope.resolve_path(path)? {
            ResolvedValue::Runtime(b) if matches!(b.var_type, VarType::List(_) | VarType::Str) => {
                let e = &b.expr;
                Some(TypedOperand::Int(quote! {
                    i64::try_from((#e).len()).expect("len fits i64")
                }))
            }
            ResolvedValue::Static(Value::List(l)) => {
                let n = i64::try_from(l.len()).expect("collection length fits i64");
                Some(TypedOperand::Int(quote! { #n }))
            }
            ResolvedValue::Static(Value::Str(s)) => {
                let n = i64::try_from(s.len()).expect("string length fits i64");
                Some(TypedOperand::Int(quote! { #n }))
            }
            _ => None,
        },
        ConditionOperand::Has(path) => {
            let ResolvedValue::Runtime(b) = scope.resolve_path(path)? else {
                return None;
            };
            if matches!(b.var_type, VarType::Option(_)) {
                let e = &b.expr;
                Some(TypedOperand::Bool(quote! { (#e).is_some() }))
            } else {
                None
            }
        }
        ConditionOperand::Kind(path) => {
            let k_expr = codegen_kind_expr_str(path, scope)?;
            Some(TypedOperand::Str(k_expr))
        }
        ConditionOperand::Kinds(path) => {
            let names = resolve_static_kinds(path, scope)?;
            Some(TypedOperand::List {
                expr: quote! { [#(#names),*] },
                elem_vt: VarType::Str,
            })
        }
        ConditionOperand::InterpolatedStr(segs) => {
            let seg_tokens = codegen_segments(segs, scope, counter)?;
            Some(TypedOperand::Str(quote! {
                ({
                    let mut __interp = #cp::__private::String::new();
                    {
                        let __out = &mut __interp;
                        #seg_tokens
                    }
                    __interp
                }).as_str()
            }))
        }
        ConditionOperand::Path { path, filters } => {
            codegen_path_operand_typed(path, filters, scope)
        }
    }
}

fn codegen_path_operand_typed(
    path: &CompiledPath,
    filters: &[md_tmpl_core::compiled::ParsedFilter],
    scope: &NativeScope,
) -> Option<TypedOperand> {
    let cp = crate_path();
    let expr = CompiledExpr::Path(path.clone());
    if filters.is_empty() {
        match scope.resolve_path(path)? {
            ResolvedValue::Static(val) => match val {
                Value::Str(s) => Some(TypedOperand::Str(quote! { #s })),
                Value::Int(i) => Some(TypedOperand::Int(quote! { #i })),
                Value::Float(f) => {
                    let bits = f.to_bits();
                    Some(TypedOperand::Float(quote! {
                        ::core::primitive::f64::from_bits(#bits)
                    }))
                }
                Value::Bool(b) => Some(TypedOperand::Bool(quote! { #b })),
                Value::List(items) => {
                    let mut strs = Vec::with_capacity(items.len());
                    for item in items.iter() {
                        if let Value::Str(s) = item {
                            strs.push(s.clone());
                        } else {
                            return None;
                        }
                    }
                    Some(TypedOperand::List {
                        expr: quote! { [#(#strs),*] },
                        elem_vt: VarType::Str,
                    })
                }
                _ => None,
            },
            ResolvedValue::Runtime(b) => {
                let e = b.expr;
                match b.var_type {
                    VarType::Str => Some(TypedOperand::Str(quote! {
                        ::core::convert::AsRef::<str>::as_ref(&(#e))
                    })),
                    VarType::Int => Some(TypedOperand::Int(e)),
                    VarType::Float => Some(TypedOperand::Float(e)),
                    VarType::Bool => Some(TypedOperand::Bool(e)),
                    VarType::List(fields) if fields.len() == 1 && fields[0].name.is_empty() => {
                        Some(TypedOperand::List {
                            expr: e,
                            elem_vt: fields[0].var_type.clone(),
                        })
                    }
                    _ => None,
                }
            }
        }
    } else {
        if let Some(base_num) = try_extract_num_expr(&expr, scope)
            && let Some(folded) = fold_numeric_filters(base_num, filters)
        {
            return Some(match folded {
                NumExpr::Int(e) => TypedOperand::Int(e),
                NumExpr::Float(e) => TypedOperand::Float(e),
            });
        }
        let render_stmt = codegen_expr(&expr, filters, scope)?;
        Some(TypedOperand::Str(quote! {
            ({
                let mut __op_buf = #cp::__private::String::new();
                {
                    let __out = &mut __op_buf;
                    #render_stmt
                }
                __op_buf
            }).as_str()
        }))
    }
}

fn codegen_comparison(
    left: &ConditionOperand,
    op: ComparisonOp,
    right: &ConditionOperand,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let cp = crate_path();
    let l = codegen_operand_typed(left, scope, counter)?;
    let r = codegen_operand_typed(right, scope, counter)?;

    if op == ComparisonOp::In {
        return codegen_in_comparison(l, r);
    }

    match (l, r) {
        (TypedOperand::Int(a), TypedOperand::Int(b)) => Some(match op {
            ComparisonOp::Eq => quote! { ((#a) == (#b)) },
            ComparisonOp::Ne => quote! { ((#a) != (#b)) },
            ComparisonOp::Lt => quote! { ((#a) < (#b)) },
            ComparisonOp::Le => quote! { ((#a) <= (#b)) },
            ComparisonOp::Gt => quote! { ((#a) > (#b)) },
            ComparisonOp::Ge => quote! { ((#a) >= (#b)) },
            ComparisonOp::In => unreachable!(),
        }),
        (TypedOperand::Float(a), TypedOperand::Float(b)) => Some(match op {
            ComparisonOp::Eq => {
                quote! { (#a).partial_cmp(&(#b)).is_some_and(::core::cmp::Ordering::is_eq) }
            }
            ComparisonOp::Ne => {
                quote! { !(#a).partial_cmp(&(#b)).is_some_and(::core::cmp::Ordering::is_eq) }
            }
            ComparisonOp::Lt => quote! { ((#a) < (#b)) },
            ComparisonOp::Le => quote! { ((#a) <= (#b)) },
            ComparisonOp::Gt => quote! { ((#a) > (#b)) },
            ComparisonOp::Ge => quote! { ((#a) >= (#b)) },
            ComparisonOp::In => unreachable!(),
        }),
        (TypedOperand::Int(a), TypedOperand::Float(b)) => {
            let method = match op {
                ComparisonOp::Eq => quote! { is_eq },
                ComparisonOp::Ne => {
                    return Some(quote! {
                        !#cp::__private::cmp_int_float(#a, #b).is_some_and(::core::cmp::Ordering::is_eq)
                    });
                }
                ComparisonOp::Lt => quote! { is_lt },
                ComparisonOp::Le => quote! { is_le },
                ComparisonOp::Gt => quote! { is_gt },
                ComparisonOp::Ge => quote! { is_ge },
                ComparisonOp::In => unreachable!(),
            };
            Some(quote! {
                #cp::__private::cmp_int_float(#a, #b).is_some_and(::core::cmp::Ordering::#method)
            })
        }
        (TypedOperand::Float(a), TypedOperand::Int(b)) => {
            let method = match op {
                ComparisonOp::Eq => quote! { is_eq },
                ComparisonOp::Ne => {
                    return Some(quote! {
                        !#cp::__private::cmp_int_float(#b, #a).is_some_and(::core::cmp::Ordering::is_eq)
                    });
                }
                ComparisonOp::Lt => quote! { is_gt },
                ComparisonOp::Le => quote! { is_ge },
                ComparisonOp::Gt => quote! { is_lt },
                ComparisonOp::Ge => quote! { is_le },
                ComparisonOp::In => unreachable!(),
            };
            Some(quote! {
                #cp::__private::cmp_int_float(#b, #a).is_some_and(::core::cmp::Ordering::#method)
            })
        }
        (TypedOperand::Str(a), TypedOperand::Str(b))
        | (TypedOperand::Bool(a), TypedOperand::Bool(b)) => Some(match op {
            ComparisonOp::Eq => quote! { ((#a) == (#b)) },
            ComparisonOp::Ne => quote! { ((#a) != (#b)) },
            ComparisonOp::Lt | ComparisonOp::Le | ComparisonOp::Gt | ComparisonOp::Ge => {
                quote! { false }
            }
            ComparisonOp::In => unreachable!(),
        }),
        _ => None,
    }
}

fn codegen_in_comparison(left: TypedOperand, right: TypedOperand) -> Option<TokenStream> {
    match (left, right) {
        (TypedOperand::Str(l_str), TypedOperand::Str(r_str)) => {
            Some(quote! { (#r_str).contains(#l_str) })
        }
        (
            TypedOperand::List {
                expr: l_list,
                elem_vt: VarType::Str,
            },
            TypedOperand::Str(r_str),
        ) => Some(quote! {
            match #r_str {
                __r_s => (#l_list).iter().all(|__item| __r_s.contains(::core::convert::AsRef::<str>::as_ref(__item))),
            }
        }),
        (
            TypedOperand::Str(l_str),
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Str,
            },
        ) => Some(quote! {
            match #l_str {
                __l_s => (#r_list).iter().any(|__item| ::core::convert::AsRef::<str>::as_ref(__item) == __l_s),
            }
        }),
        (
            TypedOperand::Int(l_int),
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Int,
            },
        ) => Some(quote! {
            {
                let __l_v: i64 = #l_int;
                (#r_list).iter().any(|__item| *__item == __l_v)
            }
        }),
        (
            TypedOperand::Bool(l_bool),
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Bool,
            },
        ) => Some(quote! {
            {
                let __l_v: bool = #l_bool;
                (#r_list).iter().any(|__item| *__item == __l_v)
            }
        }),
        (
            TypedOperand::List {
                expr: l_list,
                elem_vt: VarType::Str,
            },
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Str,
            },
        ) => Some(quote! {
            (#l_list).iter().all(|__l_item| {
                let __l_s: &str = ::core::convert::AsRef::<str>::as_ref(__l_item);
                (#r_list).iter().any(|__r_item| ::core::convert::AsRef::<str>::as_ref(__r_item) == __l_s)
            })
        }),
        (
            TypedOperand::List {
                expr: l_list,
                elem_vt: VarType::Int,
            },
            TypedOperand::List {
                expr: r_list,
                elem_vt: VarType::Int,
            },
        ) => Some(quote! {
            (#l_list).iter().all(|__l_item| (#r_list).iter().any(|__r_item| *__r_item == *__l_item))
        }),
        _ => None,
    }
}

fn codegen_match_variant_cond(
    expr: &CompiledPath,
    variants: &[std::borrow::Cow<'static, str>],
    is_option: bool,
    scope: &NativeScope,
) -> Option<TokenStream> {
    let ResolvedValue::Runtime(b) = scope.resolve_path(expr)? else {
        return None;
    };
    if variants
        .iter()
        .any(|v| v.as_ref() == md_tmpl_core::consts::MATCH_DEFAULT)
    {
        return Some(quote! { true });
    }
    let e = &b.expr;
    if is_option || matches!(b.var_type, VarType::Option(_)) {
        let has_some = variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::OPTION_SOME);
        let has_none = variants
            .iter()
            .any(|v| v.as_ref() == md_tmpl_core::consts::OPTION_NONE);
        return match (has_some, has_none) {
            (true, true) => Some(quote! { true }),
            (true, false) => Some(quote! { (#e).is_some() }),
            (false, true) => Some(quote! { (#e).is_none() }),
            (false, false) => Some(quote! { false }),
        };
    }
    let VarType::Enum(declared) = &b.var_type else {
        return None;
    };
    let enum_ident = format_ident!("{}", b.type_prefix);
    let decl_names: Vec<String> = declared.iter().map(|v| v.name.clone()).collect();
    let deduped = deduplicate_variant_idents(&decl_names);
    let mut pat_tokens = Vec::new();
    for v_name in variants {
        let target =
            md_tmpl_core::consts::strip_string_literal(v_name.as_ref()).unwrap_or(v_name.as_ref());
        let (decl, (v_ident, _)) = declared
            .iter()
            .zip(deduped.iter())
            .find(|(d, _)| d.name == target)?;
        if decl.fields.is_empty() {
            pat_tokens.push(quote! { #enum_ident::#v_ident });
        } else {
            pat_tokens.push(quote! { #enum_ident::#v_ident { .. } });
        }
    }
    if pat_tokens.is_empty() {
        Some(quote! { false })
    } else {
        Some(quote! { matches!(&(#e), #(#pat_tokens)|*) })
    }
}
