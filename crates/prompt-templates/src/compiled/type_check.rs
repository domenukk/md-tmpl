//! Compile-time flow-sensitive type checker for enum field access.
//!
//! Validates two properties:
//!
//! 1. **Variant names** in `{% case Variant %}` are checked against the
//!    enum's declared variants — typos become compile errors.
//! 2. **Field access** on enum values is flow-sensitive:
//!    - Outside a `{% match %}`, only fields present on **all** variants
//!      are accessible.
//!    - Inside `{% case A %}`, fields of variant `A` are accessible.
//!    - Inside `{% case A | B %}`, only fields present on **both** `A`
//!      and `B` are accessible.
//! 3. **Match exhaustiveness**: multi-arm match must cover all variants.
//! 4. **For-loop type**: `{% for x in y %}` requires `y` to be a list.
//! 5. **Scalar field access**: `x.field` on `str`/`int`/`bool`/`float` is an error.
//! 6. **Undeclared variables**: any reference to an undeclared variable is an error.

use std::{borrow::Cow, collections::HashMap};

use super::{CompiledInclude, Condition, Segment};
use crate::types::{VarDecl, VarType, VariantDecl};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate all field accesses in the compiled segment tree.
///
/// Returns a list of human-readable error messages (empty = valid).
///
/// This runs at **compile time** inside the proc macro — keep it
/// allocation-light on the happy path.
#[must_use]
pub fn validate_field_accesses(segments: &[Segment], declarations: &[VarDecl]) -> Vec<String> {
    let mut type_env = TypeEnv::from_declarations(declarations);
    let mut errors = Vec::new();
    walk_segments(segments, &mut type_env, &mut errors);
    errors
}

// ---------------------------------------------------------------------------
// Type environment
// ---------------------------------------------------------------------------

/// Maps variable names to their declared types.
///
/// Cheaply cloneable: inner map uses `Cow` references where possible.
/// When entering a match arm, the matched variable's type is temporarily
/// narrowed (replaced) and restored when leaving the arm.
#[derive(Clone)]
struct TypeEnv<'a> {
    /// Root variables from frontmatter declarations.
    vars: HashMap<&'a str, &'a VarType>,
    /// Overrides applied inside match arms (narrowed enum types).
    /// Key is the root variable name, value is the narrowed `VarType`.
    narrowed: HashMap<String, VarType>,
}

impl<'a> TypeEnv<'a> {
    fn from_declarations(declarations: &'a [VarDecl]) -> Self {
        let mut vars = HashMap::with_capacity(declarations.len());
        for decl in declarations {
            vars.insert(decl.name.as_str(), &decl.var_type);
        }
        Self {
            vars,
            narrowed: HashMap::new(),
        }
    }

    /// Resolve the type of a root variable, checking narrowed overrides first.
    fn lookup(&self, name: &str) -> Option<&VarType> {
        self.narrowed
            .get(name)
            .or_else(|| self.vars.get(name).copied())
    }

    /// Insert a narrowed type override. Returns the previous value, if any.
    fn narrow(&mut self, name: &str, ty: VarType) -> Option<VarType> {
        self.narrowed.insert(name.to_string(), ty)
    }

    /// Remove a narrowed type override.
    fn unnarrow(&mut self, name: &str) {
        self.narrowed.remove(name);
    }
}

// ---------------------------------------------------------------------------
// Segment walker
// ---------------------------------------------------------------------------

fn walk_segments(segments: &[Segment], env: &mut TypeEnv<'_>, errors: &mut Vec<String>) {
    for seg in segments {
        walk_segment(seg, env, errors);
    }
}

fn walk_segment(seg: &Segment, env: &mut TypeEnv<'_>, errors: &mut Vec<String>) {
    match seg {
        Segment::Static(_) | Segment::Raw(_) | Segment::Comment(_) => {}

        Segment::Expr { path, .. } => {
            validate_path(path, env, errors);
        }

        Segment::ForLoop {
            binding,
            list_path,
            body,
        } => {
            validate_path(list_path, env, errors);
            // Resolve the element type and validate it's a list.
            let resolved = resolve_path_type(list_path, env).cloned();
            match resolved {
                Some(VarType::List(ref fields)) => {
                    // Register the loop binding with element type.
                    let elem_ty = VarType::Dict(fields.clone());
                    let prev = env.narrow(binding, elem_ty);
                    walk_segments(body, env, errors);
                    match prev {
                        Some(t) => {
                            env.narrow(binding, t);
                        }
                        None => {
                            env.unnarrow(binding);
                        }
                    }
                }
                Some(other) => {
                    errors.push(format!(
                        "for loop over '{list_path}': expected list, got {other}"
                    ));
                }
                None => {
                    // Path validation already reported the error.
                    walk_segments(body, env, errors);
                }
            }
        }

        Segment::If {
            branches,
            else_body,
        } => {
            for (condition, branch_body) in branches {
                validate_condition(condition, env, errors);
                walk_segments(branch_body, env, errors);
            }
            walk_segments(else_body, env, errors);
        }

        Segment::Match { expr, arms } => {
            validate_match(expr, arms, env, errors);
        }

        Segment::Include(inc) => {
            validate_include(inc, env, errors);
        }
    }
}

// ---------------------------------------------------------------------------
// Match validation
// ---------------------------------------------------------------------------

fn validate_match(
    expr: &str,
    arms: &[(Vec<Cow<'static, str>>, Vec<Segment>)],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    // A match without any case is always wrong.
    if arms.is_empty() {
        errors.push(format!(
            "match on '{expr}': no case arms — add at least one {{% case %}}"
        ));
        return;
    }

    // Resolve the full path type (e.g. `bug.vt` → enum, not just `bug` → dict).
    let expr_type = resolve_path_type(expr, env).cloned();

    match expr_type {
        Some(VarType::Enum(ref declared)) => {
            // For narrowing, we need to track by the full expr path so that
            // `bug.vt.label` resolves correctly inside the arm body.
            // We narrow by the root variable name and replace its type with
            // one where the matched field is narrowed.
            validate_match_arms_with_narrowing(expr, declared, arms, env, errors);
        }
        Some(other_type) => {
            // Match on a non-enum type is a compile error.
            errors.push(format!(
                "match on '{expr}': expected enum, got {other_type} — \
                 use {{% if %}} with == for non-enum dispatch"
            ));
            // Still walk arm bodies for other errors.
            for (_, arm_body) in arms {
                walk_segments(arm_body, env, errors);
            }
        }
        None => {
            let root = extract_root(expr);
            errors.push(format!("match on '{expr}': undeclared variable '{root}'"));
        }
    }
}

/// Validate arms of a match on a known enum type.
///
/// Narrows the matched expression's type inside each arm so that field
/// accesses are validated against the correct variant(s).
fn validate_match_arms_with_narrowing(
    expr: &str,
    declared: &[VariantDecl],
    arms: &[(Vec<Cow<'static, str>>, Vec<Segment>)],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    let mut covered_variants: Vec<&str> = Vec::new();

    for (case_variants, arm_body) in arms {
        // 1) Check that all case variant names exist in the enum.
        for case_name in case_variants {
            if declared.iter().any(|v| v.name == case_name.as_ref()) {
                covered_variants.push(case_name.as_ref());
            } else {
                let valid: Vec<&str> = declared.iter().map(|v| v.name.as_str()).collect();
                errors.push(format!(
                    "match on '{expr}': unknown variant '{case_name}' \
                     (declared variants: {})",
                    valid.join(", ")
                ));
            }
        }

        // 2) Narrow the matched expression for this arm's body.
        //    Key is the full expr path (e.g. "bug.vt"), not just the root.
        let narrowed_variants: Vec<VariantDecl> = declared
            .iter()
            .filter(|v| case_variants.iter().any(|c| c.as_ref() == v.name))
            .cloned()
            .collect();

        if narrowed_variants.is_empty() {
            // All case names were invalid — still walk for other errors.
            walk_segments(arm_body, env, errors);
        } else {
            let narrowed_type = VarType::Enum(narrowed_variants);
            let prev = env.narrow(expr, narrowed_type);
            walk_segments(arm_body, env, errors);
            match prev {
                Some(t) => {
                    env.narrow(expr, t);
                }
                None => {
                    env.unnarrow(expr);
                }
            }
        }
    }

    // 3) Exhaustiveness: all declared variants must be covered.
    //    Single-arm inline guards ({% match x case Y %}) are exempt.
    if arms.len() > 1 {
        let missing: Vec<&str> = declared
            .iter()
            .filter(|v| !covered_variants.contains(&v.name.as_str()))
            .map(|v| v.name.as_str())
            .collect();
        if !missing.is_empty() {
            errors.push(format!(
                "match on '{expr}': non-exhaustive — missing variant(s): {}",
                missing.join(", ")
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Validate a dotted path like `outcome.evidence` against the type env.
fn validate_path(path: &str, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
    // Skip function calls like `idx(item)`, `len(list)`, `str(x)`.
    if path.contains('(') {
        return;
    }
    // Skip string literals.
    if path.starts_with('\'') || path.starts_with('"') {
        return;
    }
    // Skip numeric literals.
    if path.bytes().next().is_some_and(|b| b.is_ascii_digit()) {
        return;
    }

    let mut parts = path.split('.');
    let Some(root) = parts.next() else {
        return;
    };

    let Some(root_type) = env.lookup(root) else {
        errors.push(format!("'{root}': undeclared variable"));
        return;
    };

    // Walk remaining path segments against the type.
    let mut current_type = root_type;
    let mut traversed = root.to_string();

    for field in parts {
        traversed.push('.');
        traversed.push_str(field);

        // Check narrowed overrides first (e.g. "bug.vt" narrowed by match).
        if let Some(narrowed) = env.narrowed.get(&traversed) {
            current_type = narrowed;
            continue;
        }

        match resolve_field(current_type, field) {
            FieldResult::Ok(ty) => {
                current_type = ty;
            }
            FieldResult::NotAvailable { reason } => {
                errors.push(format!("'{traversed}': {reason}"));
                return; // Stop checking deeper fields.
            }
            FieldResult::Terminal => {
                // Reached a leaf type (e.g. .tag → str). No deeper access possible.
                return;
            }
        }
    }
}

/// Result of resolving a field on a type.
enum FieldResult<'a> {
    /// Field found, here's the resolved type (borrowed from the `VarType` tree).
    Ok(&'a VarType),
    /// Field not available — with a human-readable reason.
    NotAvailable { reason: String },
    /// Reached a terminal/leaf type — stop resolving deeper but no error.
    /// Used for built-in fields like `.tag` that return a known scalar.
    Terminal,
}

/// Resolve a single field access on a type.
fn resolve_field<'a>(ty: &'a VarType, field: &str) -> FieldResult<'a> {
    match ty {
        VarType::Enum(variants) => resolve_enum_field(variants, field),

        VarType::Dict(fields) => {
            if fields.is_empty() {
                // Untyped dict (no declared fields) — allow any field.
                FieldResult::Terminal
            } else if let Some(d) = fields.iter().find(|d| d.name == field) {
                FieldResult::Ok(&d.var_type)
            } else {
                let declared: Vec<&str> = fields.iter().map(|d| d.name.as_str()).collect();
                FieldResult::NotAvailable {
                    reason: format!(
                        "field '{field}' does not exist on dict \
                         (declared fields: {})",
                        declared.join(", ")
                    ),
                }
            }
        }

        VarType::List(fields) => {
            // List itself doesn't support field access — iterate first.
            if fields.is_empty() {
                FieldResult::Terminal
            } else {
                let declared: Vec<&str> = fields.iter().map(|d| d.name.as_str()).collect();
                FieldResult::NotAvailable {
                    reason: format!(
                        "cannot access field '{field}' on list — \
                         use {{% for %}} to iterate (element fields: {})",
                        declared.join(", ")
                    ),
                }
            }
        }

        // Scalars have no fields — always an error.
        VarType::Str | VarType::Int | VarType::Float | VarType::Bool => FieldResult::NotAvailable {
            reason: format!("cannot access field '{field}' on {ty}"),
        },
    }
}

/// Resolve a field on an enum type.
///
/// A field is accessible **only** if it exists on **all** active (narrowed)
/// variants. Use `str(expr)` to get the variant name as a string.
fn resolve_enum_field<'a>(variants: &'a [VariantDecl], field: &str) -> FieldResult<'a> {
    if variants.is_empty() {
        return FieldResult::Terminal;
    }

    // Check that ALL active variants have this field.
    let mut resolved_type: Option<&VarType> = None;
    let mut missing_on: Vec<&str> = Vec::new();

    for variant in variants {
        match variant.fields.iter().find(|d| d.name == field) {
            Some(decl) => {
                resolved_type = Some(&decl.var_type);
            }
            None => {
                missing_on.push(&variant.name);
            }
        }
    }

    if missing_on.is_empty() {
        // All variants have the field.
        match resolved_type {
            Some(ty) => FieldResult::Ok(ty),
            None => FieldResult::Terminal, // shouldn't happen — but be safe.
        }
    } else if missing_on.len() == variants.len() {
        // No variant has this field.
        let variant_names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
        FieldResult::NotAvailable {
            reason: format!(
                "field '{field}' does not exist on any variant ({})",
                variant_names.join(", ")
            ),
        }
    } else {
        // Some variants have it, some don't.
        let hint = if variants.len() > 1 {
            format!(
                "field '{field}' is not available on variant(s) {} — \
                 use {{% match %}} to narrow the type first",
                missing_on.join(", ")
            )
        } else {
            format!(
                "field '{field}' is not available on variant '{}'",
                missing_on[0]
            )
        };
        FieldResult::NotAvailable { reason: hint }
    }
}

// ---------------------------------------------------------------------------
// Condition validation
// ---------------------------------------------------------------------------

fn validate_condition(condition: &Condition, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
    match condition {
        Condition::Truthy(path) => validate_path(path, env, errors),
        Condition::Comparison { left, right, .. } => {
            validate_path(left, env, errors);
            validate_path(right, env, errors);
        }
    }
}

// ---------------------------------------------------------------------------
// Include validation
// ---------------------------------------------------------------------------

fn validate_include(inc: &CompiledInclude, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
    for (_, val_expr) in &inc.with_vars {
        validate_path(val_expr, env, errors);
    }
    if let Some((_, list_expr)) = &inc.for_each {
        validate_path(list_expr, env, errors);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the root variable name from a dotted path.
///
/// `"outcome.evidence"` → `"outcome"`, `"x"` → `"x"`.
fn extract_root(path: &str) -> &str {
    path.split('.').next().unwrap_or(path)
}

/// Resolve a dotted path to its declared type.
///
/// Returns `None` if the root variable is unknown or if any field
/// in the path doesn't resolve to a known type.
///
/// Also checks narrowed overrides at each level, so `bug.vt` can be
/// narrowed inside a match arm and `bug.vt.label` resolves correctly.
fn resolve_path_type<'a>(path: &str, env: &'a TypeEnv<'_>) -> Option<&'a VarType> {
    let mut parts = path.split('.');
    let root = parts.next()?;
    let mut current = env.lookup(root)?;
    let mut traversed = root.to_string();

    for field in parts {
        // Before resolving the field, check if the full path so far + field
        // has a narrowed override (e.g. "bug.vt" narrowed inside a match).
        traversed.push('.');
        traversed.push_str(field);
        if let Some(narrowed) = env.narrowed.get(&traversed) {
            current = narrowed;
            continue;
        }

        match resolve_field(current, field) {
            FieldResult::Ok(ty) => current = ty,
            _ => return None,
        }
    }

    Some(current)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{VarDecl, VarType, VariantDecl};

    /// Helper: build declarations from a shorthand.
    fn enum_decl(name: &str, variants: Vec<VariantDecl>) -> VarDecl {
        VarDecl {
            name: name.to_string(),
            var_type: VarType::Enum(variants),
            default_value: None,
        }
    }

    fn variant(name: &str, fields: Vec<(&str, VarType)>) -> VariantDecl {
        VariantDecl {
            name: name.to_string(),
            fields: fields
                .into_iter()
                .map(|(n, t)| VarDecl {
                    name: n.to_string(),
                    var_type: t,
                    default_value: None,
                })
                .collect(),
        }
    }

    fn unit_variant(name: &str) -> VariantDecl {
        variant(name, vec![])
    }

    fn compile_and_check(template: &str, decls: &[VarDecl]) -> Vec<String> {
        let (_fm, body) = crate::parse_frontmatter(template).expect("parse");
        let (segments, _) = crate::compiled::compile(body).expect("compile");
        validate_field_accesses(&segments, decls)
    }

    // -- Variant name validation -----------------------------------------

    #[test]
    fn match_valid_variant_names() {
        let decls = vec![enum_decl(
            "outcome",
            vec![unit_variant("Confirmed"), unit_variant("NotConfirmed")],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed, NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}yes{% case NotConfirmed %}no{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn match_unknown_variant_name() {
        let decls = vec![enum_decl(
            "outcome",
            vec![unit_variant("Confirmed"), unit_variant("NotConfirmed")],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed, NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confrimed %}yes{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {errors:?}");
        assert!(
            errors[0].contains("unknown variant 'Confrimed'"),
            "got: {}",
            errors[0]
        );
        assert!(
            errors[0].contains("Confirmed, NotConfirmed"),
            "should list valid variants: {}",
            errors[0]
        );
    }

    // -- Field access outside match (must be on ALL variants) ------------

    #[test]
    fn field_on_all_variants_ok() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("Confirmed", vec![("reason", VarType::Str)]),
                variant("Rejected", vec![("reason", VarType::Str)]),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(reason = str), Rejected(reason = str)>\n---\n\
                     {{ outcome.reason }}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "unexpected: {errors:?}");
    }

    #[test]
    fn field_not_on_all_variants_error() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("Confirmed", vec![("evidence", VarType::Str)]),
                unit_variant("NotConfirmed"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     {{ outcome.evidence }}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {errors:?}");
        assert!(
            errors[0].contains("not available on variant") && errors[0].contains("NotConfirmed"),
            "got: {}",
            errors[0]
        );
    }

    #[test]
    fn tag_field_is_error() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("Confirmed", vec![("evidence", VarType::Str)]),
                unit_variant("NotConfirmed"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     {{ outcome.tag }}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, ".tag should be an error: {errors:?}");
        assert!(
            errors[0].contains("tag") && errors[0].contains("does not exist"),
            "got: {}",
            errors[0]
        );
    }

    // -- Field access inside match arm (narrowed) ------------------------

    #[test]
    fn field_in_matching_arm_ok() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("Confirmed", vec![("evidence", VarType::Str)]),
                unit_variant("NotConfirmed"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}{{ outcome.evidence }}{% case NotConfirmed %}none{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "narrowed access should work: {errors:?}");
    }

    #[test]
    fn field_in_wrong_arm_error() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("Confirmed", vec![("evidence", VarType::Str)]),
                unit_variant("NotConfirmed"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case NotConfirmed %}{{ outcome.evidence }}{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("evidence") && errors[0].contains("NotConfirmed"),
            "got: {}",
            errors[0]
        );
    }

    // -- Multi-variant case: intersection of fields ----------------------

    #[test]
    fn multi_variant_shared_field_ok() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant(
                    "Confirmed",
                    vec![("reason", VarType::Str), ("evidence", VarType::Str)],
                ),
                variant("ConfirmedWithCaveats", vec![("reason", VarType::Str)]),
                unit_variant("Rejected"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(reason = str, evidence = str), ConfirmedWithCaveats(reason = str), Rejected>\n---\n\
                     > {% match outcome %}{% case Confirmed | ConfirmedWithCaveats %}{{ outcome.reason }}{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(
            errors.is_empty(),
            "shared field 'reason' should work: {errors:?}"
        );
    }

    #[test]
    fn multi_variant_non_shared_field_error() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant(
                    "Confirmed",
                    vec![("reason", VarType::Str), ("evidence", VarType::Str)],
                ),
                variant("ConfirmedWithCaveats", vec![("reason", VarType::Str)]),
                unit_variant("Rejected"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(reason = str, evidence = str), ConfirmedWithCaveats(reason = str), Rejected>\n---\n\
                     > {% match outcome %}{% case Confirmed | ConfirmedWithCaveats %}{{ outcome.evidence }}{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("evidence") && errors[0].contains("ConfirmedWithCaveats"),
            "got: {}",
            errors[0]
        );
    }

    // -- Inline match case -----------------------------------------------

    #[test]
    fn inline_match_case_field_ok() {
        let decls = vec![enum_decl(
            "vt",
            vec![
                variant("Known", vec![("label", VarType::Str)]),
                unit_variant("Unknown"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - vt = enum<Known(label = str), Unknown>\n---\n\
                     > {% match vt case Known %}{{ vt.label }}{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "inline narrowing: {errors:?}");
    }

    // -- Nested match ----------------------------------------------------

    #[test]
    fn nested_match_narrows_independently() {
        let decls = vec![
            enum_decl(
                "a",
                vec![
                    variant("X", vec![("x_val", VarType::Str)]),
                    unit_variant("Y"),
                ],
            ),
            enum_decl(
                "b",
                vec![
                    variant("P", vec![("p_val", VarType::Str)]),
                    unit_variant("Q"),
                ],
            ),
        ];
        let tmpl = "---\nname: t\nparams:\n  - a = enum<X(x_val = str), Y>\n  - b = enum<P(p_val = str), Q>\n---\n\
                     > {% match a %}{% case X %}\
                     > {% match b %}{% case P %}{{ a.x_val }} {{ b.p_val }}{% /match %}\
                     {% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "nested narrowing: {errors:?}");
    }

    // -- Match on non-enum rejects at compile time ----------------------

    #[test]
    fn match_on_str_is_compile_error() {
        let decls = vec![VarDecl {
            name: "status".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - status = str\n---\n\
                     > {% match status %}{% case Active %}active{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("expected enum") && errors[0].contains("str"),
            "got: {}",
            errors[0]
        );
    }

    #[test]
    fn match_on_int_is_compile_error() {
        let decls = vec![VarDecl {
            name: "count".to_string(),
            var_type: VarType::Int,
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - count = int\n---\n\
                     > {% match count %}{% case One %}one{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("expected enum") && errors[0].contains("int"),
            "got: {}",
            errors[0]
        );
    }

    #[test]
    fn match_on_bool_is_compile_error() {
        let decls = vec![VarDecl {
            name: "flag".to_string(),
            var_type: VarType::Bool,
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - flag = bool\n---\n\
                     > {% match flag %}{% case True %}yes{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("expected enum") && errors[0].contains("bool"),
            "got: {}",
            errors[0]
        );
    }

    // -- Condition paths inside if inside match --------------------------

    #[test]
    fn condition_inside_match_arm_validated() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("Confirmed", vec![("evidence", VarType::Str)]),
                unit_variant("NotConfirmed"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}{% if outcome.evidence %}yes{% /if %}{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "condition in arm: {errors:?}");
    }

    // -- Field doesn't exist on any variant ------------------------------

    #[test]
    fn field_nonexistent_on_all_variants() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("A", vec![("x", VarType::Str)]),
                variant("B", vec![("y", VarType::Str)]),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<A(x = str), B(y = str)>\n---\n\
                     > {% match outcome %}{% case A %}{{ outcome.z }}{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].contains("'z'") && errors[0].contains("does not exist"),
            "got: {}",
            errors[0]
        );
    }

    // -- Loop binding type tracking --------------------------------------

    #[test]
    fn for_loop_binding_typed_from_list() {
        let decls = vec![VarDecl {
            name: "bugs".to_string(),
            var_type: VarType::List(vec![
                VarDecl {
                    name: "title".to_string(),
                    var_type: VarType::Str,
                    default_value: None,
                },
                VarDecl {
                    name: "vt".to_string(),
                    var_type: VarType::Enum(vec![
                        variant("Known", vec![("label", VarType::Str)]),
                        unit_variant("Unknown"),
                    ]),
                    default_value: None,
                },
            ]),
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - bugs = list<title = str, vt = enum<Known(label = str), Unknown>>\n---\n\
                     > {% for bug in bugs %}{% match bug.vt case Known %}{{ bug.vt.label }}{% /match %}{% /for %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(
            errors.is_empty(),
            "loop binding should be typed: {errors:?}"
        );
    }

    #[test]
    fn for_loop_binding_field_access_validated() {
        let decls = vec![VarDecl {
            name: "bugs".to_string(),
            var_type: VarType::List(vec![
                VarDecl {
                    name: "title".to_string(),
                    var_type: VarType::Str,
                    default_value: None,
                },
                VarDecl {
                    name: "vt".to_string(),
                    var_type: VarType::Enum(vec![
                        variant("Known", vec![("label", VarType::Str)]),
                        unit_variant("Unknown"),
                    ]),
                    default_value: None,
                },
            ]),
            default_value: None,
        }];
        // Access .label outside match — should fail since Unknown has no label.
        let tmpl = "---\nname: t\nparams:\n  - bugs = list<title = str, vt = enum<Known(label = str), Unknown>>\n---\n\
                     > {% for bug in bugs %}{{ bug.vt.label }}{% /for %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("label") && errors[0].contains("Unknown"),
            "got: {}",
            errors[0]
        );
    }

    #[test]
    fn undeclared_variable_in_match_is_error() {
        let decls = vec![];
        let tmpl = "---\nname: t\nparams:\n---\n\
                     > {% match ghost %}{% case X %}x{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(
            errors.iter().any(|e| e.contains("undeclared")),
            "expected undeclared error: {errors:?}"
        );
    }

    #[test]
    fn undeclared_variable_in_expr_is_error() {
        let decls = vec![];
        let tmpl = "---\nname: t\nparams:\n---\n{{ ghost.field }}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(
            errors.iter().any(|e| e.contains("undeclared")),
            "expected undeclared error: {errors:?}"
        );
    }

    // -- Exhaustiveness --------------------------------------------------

    #[test]
    fn multi_arm_match_exhaustive_ok() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("Confirmed", vec![("evidence", VarType::Str)]),
                unit_variant("NotConfirmed"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}yes{% case NotConfirmed %}no{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "exhaustive match: {errors:?}");
    }

    #[test]
    fn multi_arm_match_non_exhaustive_error() {
        let decls = vec![enum_decl(
            "outcome",
            vec![
                variant("A", vec![("x", VarType::Str)]),
                unit_variant("B"),
                unit_variant("C"),
            ],
        )];
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<A(x = str), B, C>\n---\n\
                     > {% match outcome %}{% case A %}a{% case B %}b{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected exhaustiveness error: {errors:?}");
        assert!(
            errors[0].contains("non-exhaustive") && errors[0].contains('C'),
            "got: {}",
            errors[0]
        );
    }

    #[test]
    fn single_arm_inline_not_exhaustive_ok() {
        let decls = vec![enum_decl(
            "outcome",
            vec![unit_variant("A"), unit_variant("B")],
        )];
        // Single inline arm — intentionally non-exhaustive guard.
        let tmpl = "---\nname: t\nparams:\n  - outcome = enum<A, B>\n---\n\
                     > {% match outcome case A %}a{% /match %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "inline guard: {errors:?}");
    }

    #[test]
    fn match_with_no_arms_is_error() {
        // Construct directly since the parser might not produce this.
        let decls = vec![enum_decl("x", vec![unit_variant("A"), unit_variant("B")])];
        let segments = vec![Segment::Match {
            expr: "x".into(),
            arms: vec![],
        }];
        let errors = validate_field_accesses(&segments, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(errors[0].contains("no case arms"), "got: {}", errors[0]);
    }

    // -- For-loop on non-list --------------------------------------------

    #[test]
    fn for_loop_on_str_is_error() {
        let decls = vec![VarDecl {
            name: "name".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - name = str\n---\n\
                     > {% for c in name %}{{ c }}{% /for %}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("expected list") && e.contains("str")),
            "expected for-loop type error: {errors:?}"
        );
    }

    // -- Scalar field access ---------------------------------------------

    #[test]
    fn field_on_str_is_error() {
        let decls = vec![VarDecl {
            name: "name".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - name = str\n---\n{{ name.length }}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("cannot access field") && errors[0].contains("str"),
            "got: {}",
            errors[0]
        );
    }

    #[test]
    fn field_on_int_is_error() {
        let decls = vec![VarDecl {
            name: "count".to_string(),
            var_type: VarType::Int,
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - count = int\n---\n{{ count.abs }}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("cannot access field") && errors[0].contains("int"),
            "got: {}",
            errors[0]
        );
    }

    // -- Dict field validation -------------------------------------------

    #[test]
    fn dict_unknown_field_is_error() {
        let decls = vec![VarDecl {
            name: "config".to_string(),
            var_type: VarType::Dict(vec![VarDecl {
                name: "host".to_string(),
                var_type: VarType::Str,
                default_value: None,
            }]),
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - config = dict<host = str>\n---\n{{ config.port }}";
        let errors = compile_and_check(tmpl, &decls);
        assert_eq!(errors.len(), 1, "expected error: {errors:?}");
        assert!(
            errors[0].contains("port") && errors[0].contains("does not exist"),
            "got: {}",
            errors[0]
        );
    }

    #[test]
    fn dict_known_field_ok() {
        let decls = vec![VarDecl {
            name: "config".to_string(),
            var_type: VarType::Dict(vec![VarDecl {
                name: "host".to_string(),
                var_type: VarType::Str,
                default_value: None,
            }]),
            default_value: None,
        }];
        let tmpl = "---\nname: t\nparams:\n  - config = dict<host = str>\n---\n{{ config.host }}";
        let errors = compile_and_check(tmpl, &decls);
        assert!(errors.is_empty(), "declared field: {errors:?}");
    }
}
