//! Native Rust code generation for `{% for %}` loops and `{% include %}` blocks.

use hashbrown::HashMap;
use md_tmpl_core::{
    Value, VarType,
    compiled::{CompiledExpr, CompiledInclude, CompiledPath, Segment},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{
    Binding, NativeScope, ResolvedValue, codegen_segments, is_copy_scalar, normalize_var_type,
    resolve_static_kinds, sanitize_ident,
};
use crate::crate_path;

fn codegen_for_loop_kinds(
    binding: &str,
    path: &CompiledPath,
    body: &[Segment],
    scope: &NativeScope,
    counter: &mut usize,
    id: usize,
) -> Option<TokenStream> {
    let clean_name = sanitize_ident(binding);
    let item_ident = format_ident!("_md_item_{id}_{clean_name}");
    let idx_ident = format_ident!("_md_idx_{id}_{clean_name}");
    let len_ident = format_ident!("_md_len_{id}_{clean_name}");
    let names = resolve_static_kinds(path, scope)?;
    let mut sub_scope = scope.clone();
    remove_shadowed_bindings(&mut sub_scope.bindings, binding);
    sub_scope.bindings.insert(
        binding.to_string(),
        Binding {
            expr: quote! { (*#item_ident) },
            var_type: VarType::Str,
            type_prefix: String::new(),
        },
    );
    sub_scope
        .loop_meta
        .insert(binding.to_string(), (idx_ident.clone(), len_ident.clone()));
    sub_scope.loop_meta.insert(
        "__active_loop__".to_string(),
        (idx_ident.clone(), len_ident.clone()),
    );
    let body_tokens = codegen_segments(body, &sub_scope, counter)?;
    let len_val = i64::try_from(names.len()).expect("variant count fits i64");
    Some(quote! {
        {
            let #len_ident: i64 = #len_val;
            for (__raw_idx, #item_ident) in [#(#names),*].iter().enumerate() {
                let #idx_ident: i64 = ::core::convert::TryFrom::try_from(__raw_idx).expect("loop idx fits i64");
                #body_tokens
            }
        }
    })
}

pub(super) fn codegen_for_loop(
    binding: &str,
    list_expr: &CompiledExpr,
    body: &[Segment],
    else_body: &[Segment],
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let id = *counter;
    *counter += 1;

    // Case 1: `{% for v in kinds(EnumType) %}`
    if let CompiledExpr::Kinds(path) = list_expr {
        return codegen_for_loop_kinds(binding, path, body, scope, counter, id);
    }

    let clean_name = sanitize_ident(binding);
    let item_ident = format_ident!("_md_item_{id}_{clean_name}");
    let idx_ident = format_ident!("_md_idx_{id}_{clean_name}");
    let len_ident = format_ident!("_md_len_{id}_{clean_name}");

    // Case 2: `{% for item in list_path %}`
    let CompiledExpr::Path(path) = list_expr else {
        return None;
    };
    let ResolvedValue::Runtime(list_binding) = scope.resolve_path(path)? else {
        return None;
    };
    let VarType::List(fields) = &list_binding.var_type else {
        return None;
    };
    if fields.is_empty() {
        return None;
    }

    let (elem_vt, elem_prefix) = if fields.len() == 1 && fields[0].name.is_empty() {
        (fields[0].var_type.clone(), list_binding.type_prefix.clone())
    } else {
        (
            VarType::Struct(fields.clone()),
            format!("{}Item", list_binding.type_prefix),
        )
    };

    let item_expr = if is_copy_scalar(&elem_vt) {
        quote! { (*#item_ident) }
    } else {
        quote! { #item_ident }
    };

    let mut sub_scope = scope.clone();
    remove_shadowed_bindings(&mut sub_scope.bindings, binding);
    sub_scope.bindings.insert(
        binding.to_string(),
        Binding {
            expr: item_expr,
            var_type: elem_vt,
            type_prefix: elem_prefix,
        },
    );
    sub_scope
        .loop_meta
        .insert(binding.to_string(), (idx_ident.clone(), len_ident.clone()));
    sub_scope.loop_meta.insert(
        "__active_loop__".to_string(),
        (idx_ident.clone(), len_ident.clone()),
    );

    let body_tokens = codegen_segments(body, &sub_scope, counter)?;
    let list_tokens = &list_binding.expr;

    if else_body.is_empty() {
        Some(quote! {
            {
                let __list_ref = &(#list_tokens);
                let #len_ident: i64 = ::core::convert::TryFrom::try_from(__list_ref.len()).expect("list len fits i64");
                for (__raw_idx, #item_ident) in __list_ref.iter().enumerate() {
                    let #idx_ident: i64 = ::core::convert::TryFrom::try_from(__raw_idx).expect("loop idx fits i64");
                    #body_tokens
                }
            }
        })
    } else {
        let else_tokens = codegen_segments(else_body, scope, counter)?;
        Some(quote! {
            {
                let __list_ref = &(#list_tokens);
                if __list_ref.is_empty() {
                    #else_tokens
                } else {
                    let #len_ident: i64 = ::core::convert::TryFrom::try_from(__list_ref.len()).expect("list len fits i64");
                    for (__raw_idx, #item_ident) in __list_ref.iter().enumerate() {
                        let #idx_ident: i64 = ::core::convert::TryFrom::try_from(__raw_idx).expect("loop idx fits i64");
                        #body_tokens
                    }
                }
            }
        })
    }
}

fn remove_shadowed_bindings(bindings: &mut HashMap<String, Binding>, root_name: &str) {
    let prefix = format!("{root_name}.");
    bindings.retain(|k, _| k != root_name && !k.starts_with(&prefix));
}

fn bind_include_with_var(
    key: &str,
    val_expr: &str,
    expected_vt: &VarType,
    scope: &NativeScope,
    child_scope: &mut NativeScope,
    setup_stmts: &mut Vec<TokenStream>,
    counter: &mut usize,
) -> Option<()> {
    let cp = crate_path();
    if let Some(inner) = md_tmpl_core::consts::strip_string_literal(val_expr) {
        if *expected_vt != VarType::Str {
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
            let id = *counter;
            *counter += 1;
            let arg_ident = format_ident!("_md_inc_arg_{id}_{}", sanitize_ident(key));
            setup_stmts.push(quote! {
                let mut #arg_ident = #cp::__private::String::new();
                {
                    let __out = &mut #arg_ident;
                    #seg_tokens
                }
            });
            child_scope.bindings.insert(
                key.to_string(),
                Binding {
                    expr: quote! { #arg_ident },
                    var_type: VarType::Str,
                    type_prefix: String::new(),
                },
            );
        } else {
            child_scope
                .static_values
                .insert(key.to_string(), Value::Str(unescaped));
        }
        return Some(());
    }

    if val_expr.contains(md_tmpl_core::consts::PIPE) {
        return None;
    }

    let path = CompiledPath::compile(val_expr);
    if let Some(resolved) = scope.resolve_path(&path) {
        match resolved {
            ResolvedValue::Runtime(b) => {
                child_scope.bindings.insert(key.to_string(), b);
            }
            ResolvedValue::Static(v) => {
                child_scope.static_values.insert(key.to_string(), v);
            }
        }
    } else if val_expr == "true" && *expected_vt == VarType::Bool {
        child_scope
            .static_values
            .insert(key.to_string(), Value::Bool(true));
    } else if val_expr == "false" && *expected_vt == VarType::Bool {
        child_scope
            .static_values
            .insert(key.to_string(), Value::Bool(false));
    } else if let Ok(n) = val_expr.parse::<i64>()
        && *expected_vt == VarType::Int
    {
        child_scope
            .static_values
            .insert(key.to_string(), Value::Int(n));
    } else if let Ok(f) = val_expr.parse::<f64>()
        && *expected_vt == VarType::Float
    {
        child_scope
            .static_values
            .insert(key.to_string(), Value::Float(f));
    } else {
        return None;
    }
    Some(())
}

fn codegen_include_for_each(
    for_binding: &str,
    list_expr_str: &str,
    segments: &[Segment],
    scope: &NativeScope,
    mut child_scope: NativeScope,
    setup_stmts: &[TokenStream],
    counter: &mut usize,
) -> Option<TokenStream> {
    let list_path = CompiledPath::compile(list_expr_str.trim());
    let ResolvedValue::Runtime(list_binding) = scope.resolve_path(&list_path)? else {
        return None;
    };
    let VarType::List(fields) = &list_binding.var_type else {
        return None;
    };
    if fields.is_empty() {
        return None;
    }
    let (elem_vt, elem_prefix) = if fields.len() == 1 && fields[0].name.is_empty() {
        (fields[0].var_type.clone(), list_binding.type_prefix.clone())
    } else {
        (
            VarType::Struct(fields.clone()),
            format!("{}Item", list_binding.type_prefix),
        )
    };
    let id = *counter;
    *counter += 1;
    let clean_name = sanitize_ident(for_binding);
    let item_ident = format_ident!("_md_inc_item_{id}_{clean_name}");
    let idx_ident = format_ident!("_md_inc_idx_{id}_{clean_name}");
    let len_ident = format_ident!("_md_inc_len_{id}_{clean_name}");
    let item_expr = if is_copy_scalar(&elem_vt) {
        quote! { (*#item_ident) }
    } else {
        quote! { #item_ident }
    };
    child_scope.bindings.insert(
        for_binding.to_string(),
        Binding {
            expr: item_expr,
            var_type: elem_vt,
            type_prefix: elem_prefix,
        },
    );
    child_scope.loop_meta.insert(
        for_binding.to_string(),
        (idx_ident.clone(), len_ident.clone()),
    );
    child_scope.loop_meta.insert(
        "__active_loop__".to_string(),
        (idx_ident.clone(), len_ident.clone()),
    );
    let child_body = codegen_segments(segments, &child_scope, counter)?;
    let list_tokens = &list_binding.expr;
    Some(quote! {
        {
            #(#setup_stmts)*
            let __list_ref = &(#list_tokens);
            let #len_ident: i64 = ::core::convert::TryFrom::try_from(__list_ref.len()).expect("list len fits i64");
            for (__raw_idx, #item_ident) in __list_ref.iter().enumerate() {
                let #idx_ident: i64 = ::core::convert::TryFrom::try_from(__raw_idx).expect("loop idx fits i64");
                #child_body
            }
        }
    })
}

pub(super) fn codegen_include(
    inc: &CompiledInclude,
    scope: &NativeScope,
    counter: &mut usize,
) -> Option<TokenStream> {
    let compiled = inc.inline_compiled.as_ref()?;

    let mut child_scope = NativeScope {
        bindings: HashMap::new(),
        loop_meta: scope.loop_meta.clone(),
        static_values: HashMap::new(),
    };
    for (k, v) in compiled.consts.iter() {
        child_scope.static_values.insert(k.clone(), v.clone());
    }
    for (k, v) in compiled.imported_consts.iter() {
        child_scope.static_values.insert(k.clone(), v.clone());
    }

    let mut setup_stmts = Vec::new();
    let mut provided_keys = std::collections::HashSet::new();

    for (key_cow, val_expr_cow) in &inc.with_vars {
        let key = key_cow.as_ref();
        let val_expr = val_expr_cow.as_ref().trim();
        provided_keys.insert(key.to_string());

        let decl = compiled.declarations.iter().find(|d| d.name == key)?;
        let expected_vt = normalize_var_type(&decl.var_type);
        bind_include_with_var(
            key,
            val_expr,
            &expected_vt,
            scope,
            &mut child_scope,
            &mut setup_stmts,
            counter,
        )?;
    }

    if let Some((for_binding, _)) = &inc.for_each {
        provided_keys.insert(for_binding.as_ref().to_string());
    }

    for decl in compiled.declarations.iter() {
        if !provided_keys.contains(&decl.name) {
            if let Some(ref def_val) = decl.default_value {
                child_scope
                    .static_values
                    .insert(decl.name.clone(), def_val.clone());
            } else {
                return None;
            }
        }
    }

    if let Some((for_binding, list_expr_str)) = &inc.for_each {
        codegen_include_for_each(
            for_binding.as_ref(),
            list_expr_str.as_ref(),
            &compiled.segments,
            scope,
            child_scope,
            &setup_stmts,
            counter,
        )
    } else {
        let child_body = codegen_segments(&compiled.segments, &child_scope, counter)?;
        Some(quote! {
            {
                #(#setup_stmts)*
                #child_body
            }
        })
    }
}
