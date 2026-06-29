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

use super::{CompiledInclude, Condition, Segment};
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
    opaque_roots: &HashSet<&str>,
) -> Vec<String> {
    let mut type_env = TypeEnv::from_declarations(declarations);
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
struct TypeEnv<'a> {
    /// Root variables from frontmatter declarations.
    vars: HashMap<&'a str, &'a VarType>,
    /// Overrides applied inside match arms (narrowed enum types).
    /// Key is the root variable name, value is the narrowed `VarType`.
    narrowed: HashMap<String, VarType>,
    /// Names that are valid roots but opaque to field-level type checking
    /// (e.g. import stems for imported constants like `config.NOTEBOOK_FILENAME`).
    opaque_roots: HashSet<&'a str>,
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
            opaque_roots: HashSet::new(),
        }
    }

    /// Resolve the type of a root variable, checking narrowed overrides first.
    fn lookup(&self, name: &str) -> Option<&VarType> {
        self.narrowed
            .get(name)
            .or_else(|| self.vars.get(name).copied())
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

        Segment::Expr { expr, .. } => match expr {
            CompiledExpr::Path(path) => {
                validate_compiled_path(path, env, errors);
                // Displayability check: only scalar types can appear in {{ }}.
                if let Some(resolved) = resolve_compiled_path_type(path, env) {
                    if !resolved.is_displayable() {
                        let hint = match resolved {
                            VarType::List(_) => "use {% for %} to iterate, or | join()",
                            VarType::Struct(_) => {
                                "access fields with dot notation, e.g. {{ x.field }}"
                            }
                            VarType::Enum(_) => "use kind(x) for the variant name, or {% match %}",
                            VarType::Tmpl(_) => "use {% include %} to render a template",
                            VarType::Option(_) => "use {% if has(x) %} to unwrap, or {% match %}",
                            _ => "only str, int, float, bool can be displayed",
                        };
                        errors.push(format!(
                            "'{}': cannot display value of type {resolved} — {hint}",
                            path.as_str()
                        ));
                    }
                }
            }
            CompiledExpr::Len(path) | CompiledExpr::Kind(path) | CompiledExpr::Has(path) => {
                validate_compiled_path(path, env, errors);
            }
            CompiledExpr::Idx(_) => {}
        },

        Segment::ForLoop {
            binding,
            list_path,
            body,
            else_body,
        } => {
            validate_compiled_path(list_path, env, errors);
            // Resolve the element type and validate it's a list.
            let resolved = resolve_compiled_path_type(list_path, env).cloned();
            match resolved {
                Some(VarType::List(ref fields)) => {
                    // Register the loop binding with element type.
                    let elem_ty = VarType::Struct(fields.clone());
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
                    errors.push(format!(
                        "for loop over '{}': expected list, got {other}",
                        list_path.as_str()
                    ));
                }
                None => {
                    // Path validation already reported the error.
                    walk_segments(body, env, errors, visited);
                }
            }
            walk_segments(else_body, env, errors, visited);
        }

        Segment::If {
            branches,
            else_body,
        } => {
            for (condition, branch_body) in branches {
                validate_condition(condition, env, errors);

                // Flow-sensitive narrowing: {% if has(x) %} narrows x to Some.
                let narrowing = extract_has_narrowing(condition, env);
                if let Some((ref path_str, ref narrowed_type)) = narrowing {
                    let prev = env.narrow(path_str, narrowed_type.clone());
                    walk_segments(branch_body, env, errors, visited);
                    match prev {
                        Some(t) => {
                            env.narrow(path_str, t);
                        }
                        None => {
                            env.unnarrow(path_str);
                        }
                    }
                } else {
                    walk_segments(branch_body, env, errors, visited);
                }
            }
            walk_segments(else_body, env, errors, visited);
        }

        Segment::Match { expr, arms, .. } => {
            validate_match(expr, arms, env, errors, visited);
        }

        Segment::Include(inc) => {
            validate_include(inc, env, errors, visited);
        }
    }
}

// ---------------------------------------------------------------------------
// Match validation
// ---------------------------------------------------------------------------

fn validate_match(
    expr: &CompiledPath,
    arms: &[(Vec<Cow<'static, str>>, Vec<Segment>)],
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

    match expr_type {
        Some(VarType::Enum(ref declared)) => {
            // For narrowing, we need to track by the full expr path so that
            // `task.cat.label` resolves correctly inside the arm body.
            // We narrow by the root variable name and replace its type with
            // one where the matched field is narrowed.
            validate_match_arms_with_narrowing(expr, declared, arms, env, errors, visited);
        }
        Some(VarType::Option(_)) => {
            // Option matching: arms should be "Some" and/or "None".
            // Validate arms contain only valid option variant names.
            for (variants, arm_body) in arms {
                for v in variants {
                    let name = v.as_ref();
                    if name != "Some" && name != "None" && name != "_" {
                        errors.push(format!(
                            "match on '{}': invalid option variant '{name}' — \
                             expected 'Some', 'None', or '_'",
                            expr.as_str()
                        ));
                    }
                }
                walk_segments(arm_body, env, errors, visited);
            }
        }
        Some(other_type) => {
            // Match on a non-enum type is a compile error.
            errors.push(format!(
                "match on '{}': expected enum, got {other_type} — \
                 use {{% if %}} with == for non-enum dispatch",
                expr.as_str()
            ));
            // Still walk arm bodies for other errors.
            for (_, arm_body) in arms {
                walk_segments(arm_body, env, errors, visited);
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
    arms: &[(Vec<Cow<'static, str>>, Vec<Segment>)],
    env: &mut TypeEnv<'_>,
    errors: &mut Vec<String>,
    visited: &mut HashSet<String>,
) {
    let mut covered_variants: Vec<&str> = Vec::new();
    let mut has_default = false;

    for (case_variants, arm_body) in arms {
        let is_default_arm = case_variants.iter().any(|v| v.as_ref() == "_");

        if is_default_arm {
            has_default = true;

            // Narrow to the remaining (uncovered) variants for the default body.
            let remaining_variants: Vec<VariantDecl> = declared
                .iter()
                .filter(|v| !covered_variants.contains(&v.name.as_str()))
                .cloned()
                .collect();

            if remaining_variants.is_empty() {
                // All variants already covered — default is dead code but not an error.
                walk_segments(arm_body, env, errors, visited);
            } else {
                let narrowed_type = VarType::Enum(remaining_variants);
                let prev = env.narrow(expr.as_str(), narrowed_type);
                walk_segments(arm_body, env, errors, visited);
                match prev {
                    Some(t) => {
                        env.narrow(expr.as_str(), t);
                    }
                    None => {
                        env.unnarrow(expr.as_str());
                    }
                }
            }
            continue;
        }

        // 1) Check that all case variant names exist in the enum.
        for case_name in case_variants {
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

        // 2) Narrow the matched expression for this arm's body.
        //    Key is the full expr path (e.g. "task.cat"), not just the root.
        let narrowed_variants: Vec<VariantDecl> = declared
            .iter()
            .filter(|v| case_variants.iter().any(|c| c.as_ref() == v.name))
            .cloned()
            .collect();

        if narrowed_variants.is_empty() {
            // All case names were invalid — still walk for other errors.
            walk_segments(arm_body, env, errors, visited);
        } else {
            let narrowed_type = VarType::Enum(narrowed_variants);
            let prev = env.narrow(expr.as_str(), narrowed_type);
            walk_segments(arm_body, env, errors, visited);
            match prev {
                Some(t) => {
                    env.narrow(expr.as_str(), t);
                }
                None => {
                    env.unnarrow(expr.as_str());
                }
            }
        }
    }

    // 3) Exhaustiveness: all declared variants must be covered.
    //    Single-arm inline guards ({% match x case Y %}) are exempt.
    //    A {% else %} arm satisfies exhaustiveness.
    if arms.len() > 1 && !has_default {
        let missing: Vec<&str> = declared
            .iter()
            .filter(|v| !covered_variants.contains(&v.name.as_str()))
            .map(|v| v.name.as_str())
            .collect();
        if !missing.is_empty() {
            errors.push(format!(
                "match on '{}': non-exhaustive — missing variant(s): {}",
                expr.as_str(),
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

fn validate_compiled_path(path: &CompiledPath, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
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

        VarType::Struct(fields) => {
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
        Condition::Truthy(operand) => validate_operand(operand, env, errors),
        Condition::Comparison { left, right, .. } => {
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
        VarType::Option(inner) => Some((path_str.to_string(), (**inner).clone())),
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve a dotted path to its declared type.
///
/// Returns `None` if the root variable is unknown or if any field
/// in the path doesn't resolve to a known type.
///
/// Also checks narrowed overrides at each level, so `task.cat` can be
/// narrowed inside a match arm and `task.cat.label` resolves correctly.
fn resolve_path_type<'a>(path: &str, env: &'a TypeEnv<'_>) -> Option<&'a VarType> {
    let compiled = CompiledPath::compile(path);
    resolve_compiled_path_type(&compiled, env)
}

fn resolve_compiled_path_type<'a>(
    path: &CompiledPath,
    env: &'a TypeEnv<'_>,
) -> Option<&'a VarType> {
    let root = &path.parts()[0];
    let mut current = env.lookup(root)?;
    let mut traversed = root.clone();

    for field in &path.parts()[1..] {
        // Before resolving the field, check if the full path so far + field
        // has a narrowed override (e.g. "task.cat" narrowed inside a match).
        traversed.push(crate::consts::PATH_SEP);
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

fn resolve_operand_type<'a>(
    operand: &ConditionOperand,
    env: &'a TypeEnv<'_>,
) -> Option<&'a VarType> {
    match operand {
        ConditionOperand::Literal(_) => None,
        ConditionOperand::Path { path, .. }
        | ConditionOperand::Len(path)
        | ConditionOperand::Kind(path)
        | ConditionOperand::Has(path) => resolve_compiled_path_type(path, env),
        ConditionOperand::Idx(_) => Some(&VarType::Int),
    }
}

fn operand_to_str(operand: &ConditionOperand) -> &str {
    match operand {
        ConditionOperand::Literal(_) => "literal",
        ConditionOperand::Path { path, .. }
        | ConditionOperand::Len(path)
        | ConditionOperand::Kind(path)
        | ConditionOperand::Has(path) => path.as_str(),
        ConditionOperand::Idx(binding) => binding.as_ref(),
    }
}

fn validate_operand(operand: &ConditionOperand, env: &TypeEnv<'_>, errors: &mut Vec<String>) {
    match operand {
        ConditionOperand::Literal(_) | ConditionOperand::Idx(_) => {}
        ConditionOperand::Path { path, .. }
        | ConditionOperand::Len(path)
        | ConditionOperand::Kind(path)
        | ConditionOperand::Has(path) => {
            validate_compiled_path(path, env, errors);
        }
    }
}

#[cfg(all(test, feature = "std"))]
#[path = "type_check_tests.rs"]
mod type_check_tests;
