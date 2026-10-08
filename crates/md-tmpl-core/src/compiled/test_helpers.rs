//! Shared helpers for the compiled template engine tests.

use super::{
    CompiledInlineTemplate, Segment,
    analysis::{collect_referenced_params, parse_condition},
    render::{eval_condition, render_segments},
};
use crate::{
    compat::{HashMap, HashSet},
    context::Context,
    error::TemplateError,
    scope::Scope,
};

/// Wrapper: compile with empty parent type aliases (no inline inheritance needed).
pub(super) fn compile(
    input: &str,
) -> Result<(Vec<Segment>, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    let empty = HashMap::new();
    super::compile(input, &empty)
}

/// Wrapper: `extract_inline_templates` with empty parent type aliases.
pub(super) fn extract_inline_templates(
    input: &str,
) -> Result<(String, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    let empty = HashMap::new();
    super::inline::extract_inline_templates(input, &empty)
}

/// Helper: compile + render.
pub(super) fn compiled_render(template: &str, ctx: &Context) -> Result<String, TemplateError> {
    let (segments, _) = compile(template)?;
    let mut scope = Scope::new(ctx);
    render_segments(&segments, &mut scope, None)
}

/// Assert the compiled path produces a result without error.
pub(super) fn assert_same(template: &str, ctx: &Context) {
    compiled_render(template, ctx).expect("compiled render failed");
}

/// Compile a template body and return the set of referenced parameter names.
pub(super) fn refs(src: &str) -> HashSet<String> {
    collect_referenced_params(&compile(src).unwrap().0)
}

/// Helper: evaluate a condition string through the production path
/// (`parse_condition` → `eval_condition`).
pub(super) fn eval_cond(condition: &str, ctx: &Context) -> Result<bool, TemplateError> {
    let cond = parse_condition(condition)?;
    let scope = Scope::new(ctx);
    eval_condition(&cond, &scope)
}
