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

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};

use super::{
    ComparisonOp, CompiledInclude, Condition, Segment,
    type_resolve::{
        operand_to_str, resolve_compiled_expr_type, resolve_compiled_path_type,
        resolve_operand_type, resolve_path_type, validate_operand,
    },
};
use crate::{
    compat::{HashMap, HashSet},
    scope::{CompiledExpr, CompiledPath, ConditionOperand},
    types::{VarDecl, VarType, VariantDecl},
};

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
    validate_field_accesses_with_opaque(segments, declarations, &HashSet::new())
}

/// Like [`validate_field_accesses`], but also accepts a set of "opaque roots" —
/// names that are valid variables but whose internal structure is not
/// statically typed (e.g. import stems for imported constants).
/// Paths rooted at opaque names skip field-level validation.
#[must_use]
pub fn validate_field_accesses_with_opaque(
    segments: &[Segment],
    declarations: &[VarDecl],
    opaque_roots: &HashSet<String>,
) -> Vec<String> {
    validate_field_accesses_full(segments, declarations, &HashMap::new(), opaque_roots)
}

/// Full field-level validation including type aliases from frontmatter.
#[must_use]
pub fn validate_field_accesses_full(
    segments: &[Segment],
    declarations: &[VarDecl],
    type_aliases: &HashMap<String, VarType>,
    opaque_roots: &HashSet<String>,
) -> Vec<String> {
    let mut type_env = TypeEnv::from_declarations_and_types(declarations, type_aliases);
    type_env.opaque_roots.clone_from(opaque_roots);
    let mut errors = Vec::new();
    let mut visited = HashSet::new();
    walk_segments(segments, &mut type_env, &mut errors, &mut visited);
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
pub(crate) struct TypeEnv<'a> {
    /// Root variables from frontmatter declarations.
    vars: HashMap<&'a str, &'a VarType>,
    /// Type aliases defined via `types:` in frontmatter.
    type_aliases: HashMap<&'a str, &'a VarType>,
    /// Overrides applied inside match arms (narrowed enum types).
    /// Key is the root variable name, value is the narrowed `VarType`.
    pub(super) narrowed: HashMap<String, VarType>,
    /// Names that are valid roots but opaque to field-level type checking
    /// (e.g. import stems for imported constants like `config.NOTEBOOK_FILENAME`).
    opaque_roots: HashSet<String>,
}

impl<'a> TypeEnv<'a> {
    fn from_declarations(declarations: &'a [VarDecl]) -> Self {
        let mut vars = HashMap::with_capacity(declarations.len());
        for decl in declarations {
            vars.insert(decl.name.as_str(), &decl.var_type);
        }
        Self {
            vars,
            type_aliases: HashMap::new(),
            narrowed: HashMap::new(),
            opaque_roots: HashSet::new(),
        }
    }

    fn from_declarations_and_types(
        declarations: &'a [VarDecl],
        type_aliases: &'a HashMap<String, VarType>,
    ) -> Self {
        let mut vars = HashMap::with_capacity(declarations.len());
        for decl in declarations {
            vars.insert(decl.name.as_str(), &decl.var_type);
        }
        let mut aliases = HashMap::with_capacity(type_aliases.len());
        for (name, ty) in type_aliases {
            aliases.insert(name.as_str(), ty);
        }
        Self {
            vars,
            type_aliases: aliases,
            narrowed: HashMap::new(),
            opaque_roots: HashSet::new(),
        }
    }

    /// Resolve the type of a root variable, checking narrowed overrides first.
    pub(super) fn lookup(&self, name: &str) -> Option<&VarType> {
        self.narrowed
            .get(name)
            .or_else(|| self.vars.get(name).copied())
            .or_else(|| self.type_aliases.get(name).copied())
    }

    /// Check if a name is a known opaque root (valid but not typed).
    fn is_opaque(&self, name: &str) -> bool {
        self.opaque_roots.contains(name)
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

fn walk_segments(
    segments: &[Segment],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    for seg in segments {
        walk_segment(seg, env, errors, visited);
    }
}

fn walk_segment(
    seg: &Segment,
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    match seg {
        Segment::Static(_) | Segment::Raw(_) | Segment::Comment(_) => {}
        Segment::Panic(segs) => {
            for s in segs {
                walk_segment(s, env, errors, visited);
            }
        }

        Segment::Expr { expr, filters, .. } => match expr {
            CompiledExpr::Path(path) => {
                validate_compiled_path(path, env, errors);
                if filters.is_empty() {
                    if let Some(resolved) = resolve_compiled_path_type(path, env) {
                        if !resolved.is_displayable() {
                            let hint = match resolved {
                                VarType::List(_) => "use {% for %} to iterate, or | join()",
                                VarType::Struct(_) => {
                                    "access fields with dot notation, e.g. {{ x.field }}"
                                }
                                VarType::Enum(_) => {
                                    "use kind(x) for the variant name, or {% match %}"
                                }
                                VarType::Tmpl(_) => "use {% include %} to render a template",
                                VarType::Option(_) => {
                                    "use {% if has(x) %} to unwrap, or {% match %}"
                                }
                                _ => "only str, int, float, bool can be displayed",
                            };
                            errors.push(format!(
                                "'{}': cannot display value of type {resolved} — {hint}",
                                path.as_str()
                            ));
                        }
                    }
                }
            }
            CompiledExpr::Len(path) | CompiledExpr::Kind(path) | CompiledExpr::Has(path) => {
                validate_compiled_path(path, env, errors);
            }
            CompiledExpr::Kinds(_) => {
                resolve_compiled_expr_type(expr, env, errors);
            }
            CompiledExpr::Idx(_) => {}
        },

        Segment::ForLoop {
            binding,
            list_expr,
            body,
            else_body,
        } => {
            validate_for_loop(binding, list_expr, body, else_body, env, errors, visited);
        }

        Segment::If {
            branches,
            else_body,
        } => {
            validate_if_segment(branches, else_body, env, errors, visited);
        }

        Segment::Match { expr, arms, .. } => {
            validate_match(expr, arms, env, errors, visited);
        }

        Segment::Include(inc) => {
            validate_include(inc, env, errors, visited);
        }
    }
}

fn validate_for_loop(
    binding: &str,
    list_expr: &CompiledExpr,
    body: &[Segment],
    else_body: &[Segment],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    let resolved = resolve_compiled_expr_type(list_expr, env, errors);
    match resolved {
        Some(VarType::List(ref fields)) => {
            let elem_ty = if fields.len() == 1 && fields[0].name.is_empty() {
                fields[0].var_type.clone()
            } else {
                VarType::Struct(fields.clone())
            };
            let prev = env.narrow(binding, elem_ty);
            walk_segments(body, env, errors, visited);
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
            let expr_str = match list_expr {
                CompiledExpr::Path(p)
                | CompiledExpr::Len(p)
                | CompiledExpr::Kind(p)
                | CompiledExpr::Kinds(p)
                | CompiledExpr::Has(p) => p.as_str(),
                CompiledExpr::Idx(b) => b.as_ref(),
            };
            if matches!(other, VarType::Enum(_)) {
                errors.push(format!(
                    "for loop over '{expr_str}': expected list, got enum — use kinds({expr_str}) to iterate over variant names"
                ));
            } else {
                errors.push(format!(
                    "for loop over '{expr_str}': expected list, got {other}"
                ));
            }
            walk_segments(body, env, errors, visited);
        }
        None => {
            walk_segments(body, env, errors, visited);
        }
    }
    walk_segments(else_body, env, errors, visited);
}

fn validate_if_segment(
    branches: &[(Condition, Vec<Segment>)],
    else_body: &[Segment],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    for (condition, branch_body) in branches {
        validate_condition(condition, env, errors);

        // Collect all has()-based narrowings from the condition.
        // For `&&` chains, each sub-condition can contribute a narrowing.
        let narrowings = extract_all_has_narrowings(condition, env);
        if narrowings.is_empty() {
            walk_segments(branch_body, env, errors, visited);
        } else {
            // Apply all narrowings, walk body, then restore.
            let mut prev_types: Vec<(String, Option<VarType>)> = Vec::new();
            for (path_str, narrowed_type) in &narrowings {
                let prev = env.narrow(path_str, narrowed_type.clone());
                prev_types.push((path_str.clone(), prev));
            }
            walk_segments(branch_body, env, errors, visited);
            for (path_str, prev) in prev_types.into_iter().rev() {
                match prev {
                    Some(t) => {
                        env.narrow(&path_str, t);
                    }
                    None => {
                        env.unnarrow(&path_str);
                    }
                }
            }
        }
    }
    walk_segments(else_body, env, errors, visited);
}

// ---------------------------------------------------------------------------
// Match validation
// ---------------------------------------------------------------------------

fn validate_match(
    expr: &CompiledPath,
    arms: &[super::MatchArm],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    // A match without any case is always wrong.
    if arms.is_empty() {
        errors.push(format!(
            "match on '{}': no case arms — add at least one {{% case %}}",
            expr.as_str()
        ));
        return;
    }

    // Resolve the full path type (e.g. `task.cat` → enum, not just `task` → dict).
    let expr_type = resolve_compiled_path_type(expr, env).cloned();
    // println!("DEBUG validate_match: expr={}, expr_type={:?}", expr.as_str(), expr_type);

    match expr_type {
        Some(VarType::Enum(ref declared)) => {
            // For narrowing, we need to track by the full expr path so that
            // `task.cat.label` resolves correctly inside the arm body.
            // We narrow by the root variable name and replace its type with
            // one where the matched field is narrowed.
            validate_match_arms_with_narrowing(expr, declared, arms, env, errors, visited);
        }
        Some(VarType::Option(ref inner)) => {
            // Option matching: arms should be "Some" and/or "None".
            // Validate arms contain only valid option variant names.
            for arm in arms {
                let is_some = arm
                    .variants
                    .iter()
                    .any(|v| v.as_ref() == crate::consts::OPTION_SOME);
                for v in &arm.variants {
                    let name = v.as_ref();
                    if name != crate::consts::OPTION_SOME
                        && name != crate::consts::OPTION_NONE
                        && name != crate::consts::MATCH_DEFAULT
                    {
                        errors.push(format!(
                            "match on '{}': invalid option variant '{name}' — \
                             expected 'Some', 'None', or '_'",
                            expr.as_str()
                        ));
                    }
                }
                if let Some(ref guard) = arm.guard {
                    validate_condition(guard, env, errors);
                }
                if is_some {
                    let prev = env.narrow(expr.as_str(), inner.as_ref().clone());
                    walk_segments(&arm.body, env, errors, visited);
                    match prev {
                        Some(t) => {
                            env.narrow(expr.as_str(), t);
                        }
                        None => {
                            env.unnarrow(expr.as_str());
                        }
                    }
                } else {
                    walk_segments(&arm.body, env, errors, visited);
                }
            }
        }
        Some(ref other) => {
            errors.push(format!(
                "match on '{}': expected enum or option, got {other}",
                expr.as_str()
            ));
            for arm in arms {
                walk_segments(&arm.body, env, errors, visited);
            }
        }
        None => {
            let root = &expr.parts()[0];
            if !env.is_opaque(root) {
                errors.push(format!(
                    "match on '{}': undeclared variable '{root}'",
                    expr.as_str()
                ));
            }
        }
    }
}

/// Validate arms of a match on a known enum type.
///
/// Narrows the matched expression's type inside each arm so that field
/// accesses are validated against the correct variant(s).
fn validate_match_arms_with_narrowing(
    expr: &CompiledPath,
    declared: &[VariantDecl],
    arms: &[super::MatchArm],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    let mut covered_variants: Vec<&str> = Vec::new();
    let mut has_default = false;

    for arm in arms {
        let is_default_arm = arm
            .variants
            .iter()
            .any(|v| v.as_ref() == crate::consts::MATCH_DEFAULT);

        if is_default_arm {
            has_default = true;

            // Narrow to the remaining (uncovered) variants for the default body.
            let remaining_variants: Vec<VariantDecl> = declared
                .iter()
                .filter(|v| !covered_variants.contains(&v.name.as_str()))
                .cloned()
                .collect();

            if remaining_variants.is_empty() {
                validate_arm_body(arm, None, expr, env, errors, visited);
            } else {
                validate_arm_body(
                    arm,
                    Some(VarType::Enum(remaining_variants)),
                    expr,
                    env,
                    errors,
                    visited,
                );
            }
            continue;
        }

        // Check that all case variant names exist in the enum.
        for case_name in &arm.variants {
            if declared.iter().any(|v| v.name == case_name.as_ref()) {
                covered_variants.push(case_name.as_ref());
            } else {
                let valid: Vec<&str> = declared.iter().map(|v| v.name.as_str()).collect();
                errors.push(format!(
                    "match on '{}': unknown variant '{case_name}' \
                     (declared variants: {})",
                    expr.as_str(),
                    valid.join(", ")
                ));
            }
        }

        // Narrow the matched expression for this arm's body.
        let narrowed_variants: Vec<VariantDecl> = declared
            .iter()
            .filter(|v| arm.variants.iter().any(|c| c.as_ref() == v.name))
            .cloned()
            .collect();

        let narrowed_type = if narrowed_variants.is_empty() {
            None
        } else {
            Some(VarType::Enum(narrowed_variants))
        };
        validate_arm_body(arm, narrowed_type, expr, env, errors, visited);
    }

    check_exhaustiveness(expr, declared, arms, has_default, errors);
}

/// Validate guard and body of a single match arm, optionally narrowing
/// the expression's type during the body walk.
fn validate_arm_body(
    arm: &super::MatchArm,
    narrowed_type: Option<VarType>,
    expr: &CompiledPath,
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    if let Some(ref guard) = arm.guard {
        validate_condition(guard, env, errors);
    }
    if let Some(nt) = narrowed_type {
        let prev = env.narrow(expr.as_str(), nt);
        walk_segments(&arm.body, env, errors, visited);
        match prev {
            Some(t) => {
                env.narrow(expr.as_str(), t);
            }
            None => {
                env.unnarrow(expr.as_str());
            }
        }
    } else {
        walk_segments(&arm.body, env, errors, visited);
    }
}

/// Check that a multi-arm match covers all declared variants.
fn check_exhaustiveness(
    expr: &CompiledPath,
    declared: &[VariantDecl],
    arms: &[super::MatchArm],
    has_default: bool,
    errors: &mut Vec<String>,
) {
    if arms.len() <= 1 || has_default {
        return;
    }

    let covered: Vec<&str> = arms
        .iter()
        .flat_map(|a| a.variants.iter())
        .map(Cow::as_ref)
        .collect();

    let missing: Vec<&str> = declared
        .iter()
        .filter(|v| !covered.contains(&v.name.as_str()))
        .map(|v| v.name.as_str())
        .collect();
    if !missing.is_empty() {
        let cases = missing
            .iter()
            .map(|m| format!("{{% case {m} %}}"))
            .collect::<Vec<_>>()
            .join(" ");
        let suggestion = if missing.len() > 1 {
            let combined = missing.join(" | ");
            format!("Try adding explicit arms: {cases} or combined arm: {{% case {combined} %}}")
        } else {
            format!("Try adding explicit arm: {cases}")
        };
        errors.push(format!(
            "match on '{}': non-exhaustive — missing variant(s): {}. {suggestion}",
            expr.as_str(),
            missing.join(", ")
        ));
    }
}

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Validate a dotted path like `outcome.evidence` against the type env.
fn validate_path(path: &str, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
    // Skip function calls like `idx(item)`, `len(list)`, `str(x)`.
    if path.contains(crate::consts::PAREN_OPEN) {
        return;
    }
    // Skip string literals.
    if crate::consts::strip_string_literal(path).is_some() {
        return;
    }
    // Skip numeric literals.
    if path.bytes().next().is_some_and(|b| b.is_ascii_digit()) {
        return;
    }
    let compiled = CompiledPath::compile(path);
    validate_compiled_path(&compiled, env, errors);
}

pub(crate) fn validate_compiled_path(
    path: &CompiledPath,
    env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    let root = &path.parts()[0];

    let Some(root_type) = env.lookup(root) else {
        // Import stems and const names are opaque — valid but not typed.
        if env.is_opaque(root) {
            return;
        }
        errors.push(format!("'{root}': undeclared variable"));
        return;
    };

    let mut current_type = root_type;
    let mut traversed = root.clone();

    for field in &path.parts()[1..] {
        traversed.push(crate::consts::PATH_SEP);
        traversed.push_str(field);

        // Check narrowed overrides first (e.g. "task.cat" narrowed by match).
        if let Some(narrowed) = env.narrowed.get(&traversed) {
            current_type = narrowed;
            continue;
        }

        if let VarType::Enum(variants) = current_type {
            if variants.iter().any(|v| v.name == *field) {
                continue;
            }
        }

        match resolve_field(current_type, field) {
            FieldResult::Ok(ty) => {
                current_type = ty;
            }
            FieldResult::NotAvailable { reason } => {
                if let VarType::Option(_) = current_type {
                    errors.push(format!(
                        "'{traversed}': cannot access field '{field}' on {current_type} — \
                         option values must be checked before access. \
                         Try guard: {{% if has({traversed}) %}}{{{{{traversed}.{field}}}}}{{% /if %}} or \
                         match: {{% match {traversed} %}}{{% case Some %}}{{{{{traversed}.{field}}}}}{{% /match %}}"
                    ));
                } else {
                    errors.push(format!("'{traversed}': {reason}"));
                }
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
pub(super) enum FieldResult<'a> {
    /// Field found, here's the resolved type (borrowed from the `VarType` tree).
    Ok(&'a VarType),
    /// Field not available — with a human-readable reason.
    NotAvailable { reason: String },
    /// Reached a terminal/leaf type — stop resolving deeper but no error.
    /// Used for built-in fields like `.tag` that return a known scalar.
    Terminal,
}

/// Resolve a single field access on a type.
pub(super) fn resolve_field<'a>(ty: &'a VarType, field: &str) -> FieldResult<'a> {
    match ty {
        VarType::Enum(variants) => resolve_enum_field(variants, field),

        VarType::Struct(fields) => {
            if fields.is_empty() {
                // Untyped dict (no declared fields) — allow any field.
                FieldResult::Terminal
            } else if let Some(d) = fields.iter().find(|d| d.name == field) {
                FieldResult::Ok(&d.var_type)
            } else {
                let mut declared: Vec<&str> = fields.iter().map(|d| d.name.as_str()).collect();
                declared.sort_unstable();
                let mut best: Option<(&str, usize)> = None;
                for candidate in &declared {
                    let dist = crate::error::levenshtein_distance(field, candidate);
                    if dist > 0 && dist <= 2 && best.is_none_or(|b| dist < b.1) {
                        best = Some((*candidate, dist));
                    }
                }
                let suggestion = best
                    .map(|(s, _)| format!(" Did you mean '{s}'?"))
                    .unwrap_or_default();
                FieldResult::NotAvailable {
                    reason: format!(
                        "field '{field}' does not exist on dict.{suggestion} \
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

        // Scalars and templates have no fields — always an error.
        VarType::Str | VarType::Int | VarType::Float | VarType::Bool | VarType::Tmpl(_) => {
            FieldResult::NotAvailable {
                reason: format!("cannot access field '{field}' on {ty}"),
            }
        }

        // option(T) — field access on an option is not valid; use has() + match.
        VarType::Option(_) => FieldResult::NotAvailable {
            reason: format!(
                "cannot access field '{field}' on {ty} — \
                 use {{% if has(...) %}} or {{% match %}} to unwrap first"
            ),
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
        let mut all_fields: Vec<&str> = variants
            .iter()
            .flat_map(|v| v.fields.iter().map(|f| f.name.as_str()))
            .collect();
        all_fields.sort_unstable();
        all_fields.dedup();
        let mut best: Option<(&str, usize)> = None;
        for candidate in &all_fields {
            let dist = crate::error::levenshtein_distance(field, candidate);
            if dist > 0 && dist <= 2 && best.is_none_or(|b| dist < b.1) {
                best = Some((*candidate, dist));
            }
        }
        let suggestion = best
            .map(|(s, _)| format!(" Did you mean '{s}'?"))
            .unwrap_or_default();
        let variant_names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
        FieldResult::NotAvailable {
            reason: format!(
                "field '{field}' does not exist on any variant.{suggestion} ({})",
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
        Condition::Truthy(operand) => {
            validate_operand(operand, env, errors);
        }
        Condition::Not(inner) => {
            validate_condition(inner, env, errors);
        }
        Condition::And(left, right) | Condition::Or(left, right) => {
            validate_condition(left, env, errors);
            validate_condition(right, env, errors);
        }
        Condition::Comparison { left, op, right } => {
            if matches!(op, ComparisonOp::In) {
                validate_in_comparison(left, right, env, errors);
                return;
            }

            let left_is_enum =
                resolve_operand_type(left, env).is_some_and(|ty| matches!(ty, VarType::Enum(_)));
            let right_is_enum =
                resolve_operand_type(right, env).is_some_and(|ty| matches!(ty, VarType::Enum(_)));

            if left_is_enum || right_is_enum {
                let enum_side = if left_is_enum { left } else { right };
                errors.push(format!(
                    "cannot compare enum '{}' with '==' — use {{% match %}} instead",
                    operand_to_str(enum_side)
                ));
                return;
            }

            validate_operand(left, env, errors);
            validate_operand(right, env, errors);
        }
        Condition::MatchVariant { expr, variants, .. } => {
            validate_compiled_path(expr, env, errors);
            // Validate the expression type and variant names.
            if let Some(resolved_type) = resolve_compiled_path_type(expr, env) {
                match resolved_type {
                    VarType::Enum(declared) => {
                        for v in variants {
                            let name = v.as_ref();
                            if name != crate::consts::MATCH_DEFAULT
                                && !declared.iter().any(|d| d.name == name)
                            {
                                let valid: Vec<&str> =
                                    declared.iter().map(|d| d.name.as_str()).collect();
                                errors.push(format!(
                                    "match-as-condition on '{}': unknown variant '{name}' \
                                     (declared variants: {})",
                                    expr.as_str(),
                                    valid.join(", ")
                                ));
                            }
                        }
                    }
                    VarType::Option(_) => {
                        // option(T) — only Some/None variants are valid.
                        for v in variants {
                            let name = v.as_ref();
                            if name != crate::consts::OPTION_SOME
                                && name != crate::consts::OPTION_NONE
                                && name != crate::consts::MATCH_DEFAULT
                            {
                                errors.push(format!(
                                    "match-as-condition on '{}': unknown variant '{name}' \
                                     (option type supports only 'Some' and 'None')",
                                    expr.as_str(),
                                ));
                            }
                        }
                    }
                    other => {
                        errors.push(format!(
                            "match-as-condition on '{}': expected enum or option type, got {other}",
                            expr.as_str(),
                        ));
                    }
                }
            }
        }
    }
}

fn in_types_compatible(a: &VarType, b: &VarType) -> bool {
    match (a, b) {
        (VarType::Str, VarType::Enum(_)) | (VarType::Enum(_), VarType::Str) => true,
        _ => types_compatible(a, b),
    }
}

fn resolve_operand_vartype(operand: &ConditionOperand, env: &TypeEnv<'_>) -> Option<VarType> {
    match operand {
        ConditionOperand::Literal(lit) => match lit {
            crate::value::Value::Str(_) => Some(VarType::Str),
            crate::value::Value::Int(_) => Some(VarType::Int),
            crate::value::Value::Float(_) => Some(VarType::Float),
            crate::value::Value::Bool(_) => Some(VarType::Bool),
            _ => None,
        },
        ConditionOperand::InterpolatedStr(_) | ConditionOperand::Kind(_) => Some(VarType::Str),
        ConditionOperand::Kinds(_) => Some(VarType::List(vec![VarDecl {
            name: String::new(),
            var_type: VarType::Str,
            default_value: None,
        }])),
        ConditionOperand::Len(_) | ConditionOperand::Idx(_) => Some(VarType::Int),
        ConditionOperand::Has(_) => Some(VarType::Bool),
        ConditionOperand::Path { path, .. } => resolve_compiled_path_type(path, env).cloned(),
    }
}

fn validate_in_comparison(
    left: &ConditionOperand,
    right: &ConditionOperand,
    env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    validate_operand(left, env, errors);
    validate_operand(right, env, errors);

    // Static enum variant check: if right is kinds(EnumPath), check left literal string(s).
    if let ConditionOperand::Kinds(path) = right {
        if let Some(VarType::Enum(variants)) = resolve_compiled_path_type(path, env) {
            if let ConditionOperand::Literal(crate::value::Value::Str(str_val)) = left {
                if !variants.iter().any(|v| v.name == *str_val) {
                    errors.push(format!(
                        "static string \"{str_val}\" is not a valid variant of enum '{}'",
                        path.as_str()
                    ));
                }
            }
        }
    }

    let left_ty = resolve_operand_vartype(left, env);
    let right_ty = resolve_operand_vartype(right, env);

    let (Some(l_ty), Some(r_ty)) = (&left_ty, &right_ty) else {
        return;
    };

    match r_ty {
        VarType::Str => {
            let valid = match l_ty {
                VarType::Str => true,
                VarType::List(fields)
                    if !fields.is_empty() && fields[0].var_type == VarType::Str =>
                {
                    true
                }
                _ => false,
            };
            if !valid {
                errors.push(format!(
                    "type mismatch for 'in': checking substring in string requires string or list of strings on left, got {l_ty}"
                ));
            }
        }
        VarType::List(fields) => {
            let elem_ty = if fields.is_empty() {
                &VarType::Str // fallback for empty list
            } else {
                &fields[0].var_type
            };
            match l_ty {
                VarType::List(left_fields) => {
                    let left_elem = if left_fields.is_empty() {
                        &VarType::Str
                    } else {
                        &left_fields[0].var_type
                    };
                    if !in_types_compatible(left_elem, elem_ty) {
                        errors.push(format!(
                            "list element type mismatch in subset check: expected list of {elem_ty}, got list of {left_elem}"
                        ));
                    }
                }
                scalar => {
                    if !in_types_compatible(scalar, elem_ty) {
                        errors.push(format!(
                            "element type mismatch for 'in': expected {elem_ty}, got {scalar}"
                        ));
                    }
                }
            }
        }
        other => {
            errors.push(format!(
                "cannot use 'in' with right operand '{}': expected list or string, got {other}",
                operand_to_str(right)
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// has() flow-sensitive narrowing
// ---------------------------------------------------------------------------

/// If the condition is `has(path)` and `path` resolves to an option type,
/// return `(path_str, narrowed_type)` where `narrowed_type` is the inner
/// type of the option (transparent unwrap).
///
/// This enables `{% if has(x) %} {{ x }} {% /if %}` to type-check.
fn extract_has_narrowing(condition: &Condition, env: &TypeEnv<'_>) -> Option<(String, VarType)> {
    let Condition::Truthy(ConditionOperand::Has(path)) = condition else {
        return None;
    };

    let path_str = path.as_str();

    // Resolve the type for this path.
    let ty = resolve_compiled_path_type(path, env)?;

    if !ty.is_option() {
        return None;
    }

    // Narrow option to its inner type.
    match ty {
        // New-style option(T): unwrap to T directly.
        VarType::Option(inner) => Some((path_str.to_string(), inner.as_ref().clone())),
        // Legacy enum-based option: extract just the Some variant.
        VarType::Enum(variants) => {
            let some_only: Vec<VariantDecl> = variants
                .iter()
                .filter(|v| v.name == crate::consts::OPTION_SOME)
                .cloned()
                .collect();
            if some_only.is_empty() {
                return None;
            }
            Some((path_str.to_string(), VarType::Enum(some_only)))
        }
        _ => None,
    }
}

/// Extract all has()-based narrowings from a condition tree.
///
/// For `&&` chains like `has(a) && has(b)`, this returns both narrowings.
/// For `||` and `!`, no narrowings are extracted (they would be unsound).
fn extract_all_has_narrowings(condition: &Condition, env: &TypeEnv<'_>) -> Vec<(String, VarType)> {
    let mut narrowings = Vec::new();
    collect_and_narrowings(condition, env, &mut narrowings);
    narrowings
}

/// Recursively collect has()-based narrowings from `&&` chains.
fn collect_and_narrowings(
    condition: &Condition,
    env: &TypeEnv<'_>,
    out: &mut Vec<(String, VarType)>,
) {
    match condition {
        Condition::And(left, right) => {
            collect_and_narrowings(left, env, out);
            collect_and_narrowings(right, env, out);
        }
        other => {
            if let Some(narrowing) = extract_has_narrowing(other, env) {
                out.push(narrowing);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Include validation
// ---------------------------------------------------------------------------

/// Validate an include directive with full cross-boundary type checking:
///
/// 1. Check that `with` / `for` expressions are valid in the parent scope.
/// 2. If the included template's compiled body is available (`inline_compiled`):
///    a. **Contract**: all declared params must be provided.
///    b. **Type matching**: `with` value types must match the included template's
///    declared types.
///    c. **Body walk**: recursively validate the included body — with cycle
///    detection to handle recursive templates.
fn validate_include(
    inc: &CompiledInclude,
    env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    // 1. Check that `with` expressions are valid in parent scope.
    for (_, val_expr) in &inc.with_vars {
        validate_path(val_expr, env, errors);
    }
    if let Some((_, list_expr)) = &inc.for_each {
        validate_path(list_expr, env, errors);
    }

    // 2. Cross-boundary checks when the included template is available.
    let Some(compiled) = &inc.inline_compiled else {
        return;
    };

    // 2a. Contract: all declared params must be provided.
    validate_include_contract(inc, &compiled.declarations, errors);

    // 2b. Type matching: check that provided types match expected types.
    validate_include_type_match(inc, &compiled.declarations, env, errors);

    // 2c. Walk the included body — but only if not a cycle.
    // Use Arc pointer as identity: same file/content = same Arc = deduped,
    // different files with the same name = different Arcs = both walked.
    let identity_key = format!(
        "{}@{:p}",
        inc.path,
        alloc::sync::Arc::as_ptr(&compiled.segments)
    );
    if visited.insert(identity_key.clone()) {
        // First visit: build child TypeEnv from included template's declarations.
        let mut child_env = TypeEnv::from_declarations(&compiled.declarations);
        // Propagate opaque roots — included templates may also reference
        // imported constants (e.g. `config.NOTEBOOK_FILENAME`) from their own imports.
        child_env.opaque_roots.clone_from(&env.opaque_roots);
        for k in compiled.imported_consts.keys() {
            child_env.opaque_roots.insert(k.clone());
        }
        for k in compiled.consts.keys() {
            child_env.opaque_roots.insert(k.clone());
        }
        walk_segments(&compiled.segments, &mut child_env, errors, visited);
        visited.remove(&identity_key);
    }
    // Cycle: boundary checks (2a, 2b) were done — body already validated on
    // first encounter, so we skip recursing to avoid infinite loops.
}

/// Find which declared parameters are NOT provided by the include directive.
///
/// Shared between compile-time (`type_check.rs`) and runtime (`include.rs`)
/// contract checking. Returns references to missing declarations.
pub(crate) fn find_missing_include_params<I>(
    declarations: &[VarDecl],
    provided_keys: I,
) -> Vec<&VarDecl>
where
    I: Iterator<Item: AsRef<str>>,
{
    // For typical 1–5 params, linear scan beats HashSet overhead.
    let provided: Vec<String> = provided_keys.map(|k| k.as_ref().to_string()).collect();
    declarations
        .iter()
        .filter(|d| d.default_value.is_none() && !provided.iter().any(|p| p == &d.name))
        .collect()
}

/// Check that all declared parameters in the included template are provided
/// via `with` or `for`. Pushes formatted errors into the error list.
fn validate_include_contract(
    inc: &CompiledInclude,
    included_declarations: &[VarDecl],
    errors: &mut Vec<String>,
) {
    let provided = inc
        .with_vars
        .iter()
        .map(|(k, _)| k.as_ref().to_string())
        .chain(inc.for_each.iter().map(|(b, _)| b.as_ref().to_string()));

    let missing = find_missing_include_params(included_declarations, provided);
    if !missing.is_empty() {
        // Single-pass: collect descriptions and fix hints together.
        let (descs, hints): (Vec<_>, Vec<_>) = missing
            .iter()
            .map(|d| {
                (
                    format!("{}: {}", d.name, d.var_type),
                    format!("{}={}", d.name, d.name),
                )
            })
            .unzip();
        errors.push(format!(
            "include '{}': missing required param(s): {}. \
             Use 'with {}' to pass them",
            inc.path,
            descs.join(", "),
            hints.join(", "),
        ));
    }
}

/// Type-check that provided `with` variables have compatible types with
/// the included template's declarations.
fn validate_include_type_match(
    inc: &CompiledInclude,
    included_declarations: &[VarDecl],
    parent_env: &TypeEnv<'_>,
    errors: &mut Vec<String>,
) {
    for (key, val_expr) in &inc.with_vars {
        let Some(included_decl) = included_declarations
            .iter()
            .find(|d| d.name == key.as_ref())
        else {
            continue; // Extra vars are OK (not declared in included template).
        };

        // Skip literals — they don't have a type in the parent env.
        let val = val_expr.trim();
        if crate::consts::strip_string_literal(val).is_some()
            || val.starts_with(crate::consts::ANGLE_OPEN)
            || val.bytes().next().is_some_and(|b| b.is_ascii_digit())
        {
            continue;
        }

        // Resolve the type of the expression in the parent env.
        if let Some(parent_type) = resolve_path_type(val, parent_env) {
            if !types_compatible(parent_type, &included_decl.var_type) {
                errors.push(format!(
                    "include '{}': type mismatch for '{}': \
                     parent provides '{}' but included template expects '{}'",
                    inc.path, key, parent_type, included_decl.var_type,
                ));
            }
        }
        // If unresolvable, validate_path already reported the error.
    }
}

/// Check if two types are compatible for include parameter passing.
///
/// Types are compatible if they are structurally equal. Containers with
/// empty field lists (which may arise internally) are treated as compatible
/// with any same-kind type. Note: untyped `list()` and `struct()` are
/// rejected at parse time, so this case only applies to internal types.
fn types_compatible(provided: &VarType, expected: &VarType) -> bool {
    match (provided, expected) {
        // Exact scalar match.
        (VarType::Str, VarType::Str)
        | (VarType::Int, VarType::Int)
        | (VarType::Float, VarType::Float)
        | (VarType::Bool, VarType::Bool) => true,

        // Untyped containers are compatible with any same-kind type.
        (VarType::List(a), VarType::List(b)) | (VarType::Struct(a), VarType::Struct(b)) => {
            a.is_empty() || b.is_empty() || a == b
        }

        // Enum types: compare variant names and field types.
        (VarType::Enum(a), VarType::Enum(b)) => a == b,

        // Template types: compare signatures.
        (VarType::Tmpl(a), VarType::Tmpl(b)) => a == b,

        _ => false,
    }
}

#[cfg(all(test, feature = "std"))]
#[path = "type_check_tests.rs"]
mod type_check_tests;
