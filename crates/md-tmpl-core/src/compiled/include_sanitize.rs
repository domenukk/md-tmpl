//! Sanitization context propagation across `{% include %}` boundaries.
//!
//! **Trust follows the value, not the declaration.** An include target (inline
//! `{% tmpl %}`, included file, or `tmpl(...)` handle) receives every parameter value either
//! from its caller, who knows whether that value is trusted, or from its own frontmatter
//! defaults, which are developer-authored. Include targets therefore render without their
//! own parameter sanitization policy: a trusted caller value (literal, constant, `trusted
//! str` parameter, `unsafe_trust(...)`) is emitted verbatim, and an untrusted one is
//! sanitized exactly once, at the emission point, with the emission context. The target's
//! declared specs only choose the *style* for untrusted values: a declared block quarantine
//! (`str | sanitize("tag")`) wins over the caller's style (see [`resolve_include_spec`]).
//!
//! Inline `{% tmpl %}` includes are bound into the host AST at compile time, so the
//! regular sanitize passes already see their bodies. File includes, higher-order
//! `tmpl(...)` parameters, and recursive inline includes are resolved at render time,
//! so the call site records an [`IncludeSanitizeContext`] instead:
//!
//! - **Untrusted taint**: `with key = <untrusted expr>` and `for item in <untrusted list>`
//!   mark the corresponding child parameter paths as untrusted, whatever the child declares.
//! - **Enclosing XML tags / ATX heading state** that are open at the call site, so a
//!   value rendered inside the child cannot close a tag opened by the parent.
//! - **Untrusted include paths**: an include path that interpolates an untrusted value
//!   is rejected (it would let untrusted input choose which file is read).
//!
//! At render time the child segments are re-run through the sanitize passes with this
//! context ([`contextualized_segments`]); the result is memoized per call site.

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use super::{CompiledInclude, FilterOrigin, Segment, sanitize_pass};
use crate::{compat::HashMap, error::TemplateError, scope::CompiledPath, types::SanitizeSpec};

/// Sanitization context captured at an `{% include %}` call site that is resolved at
/// render time (file include, `tmpl(...)` parameter, or recursive inline template).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IncludeSanitizeContext {
    /// Static XML tags open at the call site (outermost first).
    pub enclosing_tags: Vec<String>,
    /// Whether the call site sits on a Markdown ATX heading line.
    pub in_atx_heading: bool,
    /// Whether the call site is at the start of a line.
    pub at_line_start: bool,
    /// Partially scanned opening tag name pending at the call site (e.g. `<tag` without `>`).
    pub pending_open_tag: Option<String>,
    /// Child parameter paths (canonical, dot-separated) that receive untrusted values,
    /// sorted by path.
    pub untrusted: Vec<(String, SanitizeSpec)>,
    /// Untrusted expression interpolated into the include path, if any.
    pub untrusted_path_ref: Option<String>,
}

impl IncludeSanitizeContext {
    /// Returns `true` when the context carries no enclosing tags, heading state, or taint.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.enclosing_tags.is_empty()
            && !self.in_atx_heading
            && self.untrusted.is_empty()
            && self.untrusted_path_ref.is_none()
    }

    /// Merge `entries` into [`Self::untrusted`], keeping existing specs for duplicate paths.
    fn merge_untrusted(&mut self, entries: Vec<(String, SanitizeSpec)>) {
        for (path, spec) in entries {
            if let Err(idx) = self
                .untrusted
                .binary_search_by(|(p, _)| p.as_str().cmp(path.as_str()))
            {
                self.untrusted.insert(idx, (path, spec));
            }
        }
    }
}

/// Declared parameter sanitization of an include target: canonical parameter path → spec,
/// with block notices already resolved against the target's own `sanitize_notice:`.
///
/// Include targets take their parameters' trust from the caller, so these specs never
/// sanitize a trusted value; they only choose the style for untrusted caller values (see
/// [`apply_include_context`]).
pub type DeclaredSanitize = HashMap<String, SanitizeSpec>;

/// Build the [`DeclaredSanitize`] of an include target from its frontmatter policy, giving
/// block specs without their own notice the target's template-level `sanitize_notice`.
#[must_use]
pub fn declared_sanitize(
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
) -> DeclaredSanitize {
    param_sanitize
        .iter()
        .map(|(path, spec)| (path.clone(), transfer_spec(spec, sanitize_notice)))
        .collect()
}

/// Sanitization style for an untrusted caller value arriving at child path `path`.
///
/// The child's declared block quarantine wins because it shapes the child's own output;
/// otherwise the caller's declared style for the value applies.
fn resolve_include_spec(
    path: &str,
    caller_spec: &SanitizeSpec,
    declared: &DeclaredSanitize,
) -> SanitizeSpec {
    match declared.get(path) {
        Some(block @ SanitizeSpec::Block { .. }) => block.clone(),
        Some(SanitizeSpec::Inline) | None => caller_spec.clone(),
    }
}

/// Resolve every untrusted call-site entry against the child's [`DeclaredSanitize`].
fn effective_untrusted_map(
    entries: &[(String, SanitizeSpec)],
    declared: &DeclaredSanitize,
) -> HashMap<String, SanitizeSpec> {
    entries
        .iter()
        .map(|(path, spec)| (path.clone(), resolve_include_spec(path, spec, declared)))
        .collect()
}

/// Per-call-site memo of contextualized child segments (read-mostly: renders take a
/// shared read lock and compare the child by pointer identity).
///
/// Cloning yields an empty memo: a cloned call site may carry a different
/// [`IncludeSanitizeContext`], so cached results must never be shared between copies.
#[derive(Default)]
pub struct IncludeContextMemo {
    #[cfg(feature = "std")]
    slot: std::sync::RwLock<Option<MemoEntry>>,
}

/// Memoized `(source child, contextualized child)` pair; the source is held strongly so
/// its pointer identity cannot be reused by a different child while cached.
#[cfg(feature = "std")]
type MemoEntry = (Arc<[Segment]>, Arc<[Segment]>);

impl Clone for IncludeContextMemo {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl core::fmt::Debug for IncludeContextMemo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IncludeContextMemo")
    }
}

/// Return the child segments to render for the include call site `inc`.
///
/// `child` is the include target compiled without its own parameter policy and `declared`
/// its [`DeclaredSanitize`]. When the call site carries an [`IncludeSanitizeContext`], the
/// child is re-run through the sanitize passes with that context (memoized per call site);
/// otherwise `child` is returned unchanged. Callers check the call site's own path with
/// [`check_untrusted_include_path`] before resolving the child.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if a nested include path in the child interpolates
/// an untrusted value.
pub(crate) fn contextualized_segments(
    inc: &CompiledInclude,
    child: &Arc<[Segment]>,
    declared: &DeclaredSanitize,
) -> Result<Arc<[Segment]>, TemplateError> {
    let Some(ctx) = inc.sanitize_ctx.as_deref() else {
        return Ok(Arc::clone(child));
    };
    memoized(inc, child, || {
        if ctx.untrusted.is_empty() && !sanitize_pass::needs_enclosing_pass(child) {
            // Nothing to rewrite: memoize the child itself so the tree walk runs once.
            return Ok(Arc::clone(child));
        }
        let mut segments = child.to_vec();
        apply_include_context(&mut segments, ctx, declared);
        check_untrusted_include_paths(&segments)?;
        Ok(Arc::from(segments))
    })
}

/// Return the segments to render for a `tmpl(...)` handle included at call site `site`.
///
/// A handle is compiled for the host boundary, with its own parameter policy. Included,
/// its parameters take the caller's trust like every other include target, so the
/// declared policy is stripped (see [`strip_declared_sanitization`]) before the call-site
/// context is applied with the handle's `declared` specs. Memoized per call site.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if a nested include path in the handle interpolates
/// an untrusted value.
pub(crate) fn handle_segments(
    site: Option<&CompiledInclude>,
    own: &Arc<[Segment]>,
    declared: &DeclaredSanitize,
) -> Result<Arc<[Segment]>, TemplateError> {
    if declared.is_empty() {
        // No declarative policy: nothing to strip.
        return match site {
            Some(inc) => contextualized_segments(inc, own, declared),
            None => Ok(Arc::clone(own)),
        };
    }
    let derive = || {
        let mut segments = own.to_vec();
        strip_declared_sanitization(&mut segments);
        if let Some(ctx) = site.and_then(|inc| inc.sanitize_ctx.as_deref()) {
            apply_include_context(&mut segments, ctx, declared);
        }
        check_untrusted_include_paths(&segments)?;
        Ok(Arc::from(segments))
    };
    match site {
        Some(inc) => memoized(inc, own, derive),
        None => derive(),
    }
}

/// Return `derive()` for call site `inc` and `child`, memoized by the child's identity.
#[cfg(feature = "std")]
fn memoized(
    inc: &CompiledInclude,
    child: &Arc<[Segment]>,
    derive: impl FnOnce() -> Result<Arc<[Segment]>, TemplateError>,
) -> Result<Arc<[Segment]>, TemplateError> {
    // Fast path: memo hit for this call site and child (one lock + pointer compare).
    {
        let slot = inc
            .memo
            .slot
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((source, derived)) = slot.as_ref()
            && Arc::ptr_eq(source, child)
        {
            return Ok(Arc::clone(derived));
        }
    }
    let derived = derive()?;
    let mut slot = inc
        .memo
        .slot
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *slot = Some((Arc::clone(child), Arc::clone(&derived)));
    Ok(derived)
}

/// `no_std` has no shared memo slot: derive the segments on every render.
#[cfg(not(feature = "std"))]
fn memoized(
    _inc: &CompiledInclude,
    _child: &Arc<[Segment]>,
    derive: impl FnOnce() -> Result<Arc<[Segment]>, TemplateError>,
) -> Result<Arc<[Segment]>, TemplateError> {
    derive()
}

/// Remove every filter a declarative parameter policy added (and the taint it recorded on
/// nested include call sites), turning a host-boundary compilation into an include-target
/// compilation. Explicit template filters, notices, and enclosing-tag state are kept.
fn strip_declared_sanitization(segments: &mut [Segment]) {
    for seg in segments {
        match seg {
            Segment::Expr { filters, .. } => {
                filters.retain(|f| f.origin != FilterOrigin::Declaration);
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                strip_declared_sanitization(body);
                strip_declared_sanitization(else_body);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (_, body) in branches {
                    strip_declared_sanitization(body);
                }
                strip_declared_sanitization(else_body);
            }
            Segment::Match { arms, .. } => {
                for arm in arms {
                    strip_declared_sanitization(&mut arm.body);
                }
            }
            Segment::Panic(inner) => strip_declared_sanitization(inner),
            Segment::Include(inc) => {
                if let Some(ctx) = inc.sanitize_ctx.as_mut() {
                    let ctx = Arc::make_mut(ctx);
                    ctx.untrusted.clear();
                    ctx.untrusted_path_ref = None;
                }
                if inc
                    .sanitize_ctx
                    .as_deref()
                    .is_some_and(IncludeSanitizeContext::is_empty)
                {
                    inc.sanitize_ctx = None;
                }
                if let Some(ic) = &mut inc.inline_compiled {
                    strip_declared_sanitization(Arc::make_mut(&mut ic.segments));
                }
            }
            Segment::Static(_) | Segment::Raw(_) | Segment::Comment(_) => {}
        }
    }
}

/// Apply a call-site [`IncludeSanitizeContext`] to freshly compiled child `segments` in place.
///
/// `segments` must be compiled without the child's own parameter policy; `declared` is the
/// child's [`DeclaredSanitize`], used to choose the style for untrusted caller values. Used
/// when an include body is bound into its call site ahead of rendering (e.g. by the
/// compile-time proc macros, which resolve file includes into `inline_compiled`).
pub fn apply_include_context(
    segments: &mut [Segment],
    ctx: &IncludeSanitizeContext,
    declared: &DeclaredSanitize,
) {
    if !ctx.untrusted.is_empty() {
        let map = effective_untrusted_map(&ctx.untrusted, declared);
        sanitize_pass::propagate_param_sanitization(segments, &map, None);
    }
    sanitize_pass::apply_enclosing_xml_tags_from(
        segments,
        &ctx.enclosing_tags,
        ctx.in_atx_heading,
        ctx.at_line_start,
        ctx.pending_open_tag.as_deref(),
    );
}

/// Reject an include call site whose path interpolates an untrusted value.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] naming the offending expression.
pub(crate) fn check_untrusted_include_path(inc: &CompiledInclude) -> Result<(), TemplateError> {
    match inc
        .sanitize_ctx
        .as_deref()
        .and_then(|c| c.untrusted_path_ref.as_deref())
    {
        Some(expr) => Err(untrusted_include_path_error(&inc.path, expr)),
        None => Ok(()),
    }
}

/// Build the error for an include path that interpolates an untrusted value.
fn untrusted_include_path_error(path: &str, expr: &str) -> TemplateError {
    TemplateError::syntax(alloc::format!(
        "include path '{path}' interpolates untrusted value '{expr}': untrusted input must not select which template is included"
    ))
}

/// Walk `segments` (including compile-time bound inline includes) and reject any include
/// call site whose path interpolates an untrusted value.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] for the first offending include.
pub fn check_untrusted_include_paths(segments: &[Segment]) -> Result<(), TemplateError> {
    for seg in segments {
        match seg {
            Segment::Include(inc) => {
                check_untrusted_include_path(inc)?;
                if let Some(ic) = &inc.inline_compiled {
                    check_untrusted_include_paths(&ic.segments)?;
                }
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                check_untrusted_include_paths(body)?;
                check_untrusted_include_paths(else_body)?;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (_, body) in branches {
                    check_untrusted_include_paths(body)?;
                }
                check_untrusted_include_paths(else_body)?;
            }
            Segment::Match { arms, .. } => {
                for arm in arms {
                    check_untrusted_include_paths(&arm.body)?;
                }
            }
            Segment::Panic(inner) => check_untrusted_include_paths(inner)?,
            Segment::Static(_) | Segment::Raw(_) | Segment::Comment(_) | Segment::Expr { .. } => {}
        }
    }
    Ok(())
}

/// Record `entries` (child untrusted parameter paths) on a runtime-resolved include call site.
pub(super) fn record_untrusted(inc: &mut CompiledInclude, entries: Vec<(String, SanitizeSpec)>) {
    if entries.is_empty() {
        return;
    }
    let ctx = Arc::make_mut(inc.sanitize_ctx.get_or_insert_with(Default::default));
    ctx.merge_untrusted(entries);
}

/// Record an untrusted include-path interpolation on an include call site.
pub(super) fn record_untrusted_path(inc: &mut CompiledInclude, expr: String) {
    let ctx = Arc::make_mut(inc.sanitize_ctx.get_or_insert_with(Default::default));
    if ctx.untrusted_path_ref.is_none() {
        ctx.untrusted_path_ref = Some(expr);
    }
}

/// Record the enclosing-tag / heading scan state at a runtime-resolved include call site.
pub(super) fn record_enclosing(
    inc: &mut CompiledInclude,
    open_tags: &[String],
    in_atx_heading: bool,
    at_line_start: bool,
    pending_open_tag: Option<&str>,
) {
    if open_tags.is_empty() && !in_atx_heading {
        return;
    }
    let ctx = Arc::make_mut(inc.sanitize_ctx.get_or_insert_with(Default::default));
    ctx.enclosing_tags = open_tags.to_vec();
    ctx.in_atx_heading = in_atx_heading;
    ctx.at_line_start = at_line_start;
    ctx.pending_open_tag = pending_open_tag.map(ToString::to_string);
}

/// Returns `true` if `s` is a plain dotted identifier path (`a`, `my-var`, `a.b-c.d`).
fn is_plain_path(s: &str) -> bool {
    !s.is_empty()
        && s.split(crate::consts::PATH_SEP).all(|part| {
            let mut bytes = part.bytes();
            bytes
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

/// Resolve a declarative spec for transfer to a child template: a block spec without an
/// explicit notice inherits the host template's `sanitize_notice`.
fn transfer_spec(spec: &SanitizeSpec, host_notice: Option<&str>) -> SanitizeSpec {
    match spec {
        SanitizeSpec::Block { tag, notice: None } if host_notice.is_some() => SanitizeSpec::Block {
            tag: tag.clone(),
            notice: host_notice.map(ToString::to_string),
        },
        _ => spec.clone(),
    }
}

/// Map an untrusted host path expression onto child path `target`.
///
/// - exact match `K == C` → `target`
/// - nested field `K == C.rest` → `target.rest`
/// - sub-path of an untrusted value `C == K.rest` → `target`
fn map_path(
    target: &str,
    path_expr: &str,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    host_notice: Option<&str>,
    loop_aliases: &HashMap<String, String>,
    out: &mut Vec<(String, SanitizeSpec)>,
) {
    let canonical =
        sanitize_pass::canonical_path(&CompiledPath::compile(path_expr.trim()), loop_aliases);
    // Iterate in key order so the resulting context is deterministic (TypeScript parity).
    let mut keys: Vec<&String> = param_sanitize.keys().collect();
    keys.sort_unstable();
    for key in keys {
        let spec = &param_sanitize[key];
        if *key == canonical {
            out.push((target.to_string(), transfer_spec(spec, host_notice)));
        } else if let Some(rest) = key.strip_prefix(canonical.as_str())
            && rest.starts_with(crate::consts::PATH_SEP)
        {
            let mut child = String::with_capacity(target.len() + rest.len());
            child.push_str(target);
            child.push_str(rest);
            out.push((child, transfer_spec(spec, host_notice)));
        } else if let Some(rest) = canonical.strip_prefix(key.as_str())
            && rest.starts_with(crate::consts::PATH_SEP)
        {
            out.push((target.to_string(), transfer_spec(spec, host_notice)));
        }
    }
}

/// Returns `true` when `expr` explicitly opts out of untrusted taint via
/// `unsafe_trust(x)` or `x | unsafe_trust`.
fn has_unsafe_trust_opt_out(expr: &str) -> bool {
    let (base, filter_chain) = crate::parser::split_pipe_aware(expr.trim());
    if let Some((func_name, _)) = crate::scope::parse_function_call(base.trim())
        && func_name == crate::consts::FN_UNSAFE_TRUST
    {
        return true;
    }
    if !filter_chain.is_empty() {
        for f in crate::parser::split_filters_aware(filter_chain) {
            let (name, _) = crate::filter::parse_filter(f.trim());
            if name == crate::consts::FILTER_UNSAFE_TRUST {
                return true;
            }
        }
    }
    false
}

/// Static trust class of the base of an include argument or interpolation (the part before
/// the first top-level `|`).
enum BaseTrust {
    /// A plain variable path whose trust is looked up in the declarative policy.
    Path,
    /// A literal or a built-in function returning a non-injectable value (`len`, `idx`,
    /// `kind`, `kinds`, `has`).
    Trusted,
    /// Anything else: treated as untrusted so no unrecognized form can launder taint.
    Unknown,
}

/// Classify `base` for trust propagation; see [`BaseTrust`].
fn classify_base(base: &str) -> BaseTrust {
    if is_plain_path(base) {
        return BaseTrust::Path;
    }
    if let Some(lit) = crate::consts::strip_string_literal(base) {
        return if lit.contains(crate::consts::EXPR_START) {
            BaseTrust::Unknown
        } else {
            BaseTrust::Trusted
        };
    }
    if crate::parser::parse_scalar_literal(base).is_some() {
        return BaseTrust::Trusted;
    }
    match crate::scope::parse_function_call(base) {
        Some((
            crate::consts::FN_LEN
            | crate::consts::FN_IDX
            | crate::consts::FN_KIND
            | crate::consts::FN_KINDS
            | crate::consts::FN_HAS,
            _,
        )) => BaseTrust::Trusted,
        Some(_) | None => BaseTrust::Unknown,
    }
}

/// Returns the first `{{ expr }}` interpolation in `text` whose value is untrusted (an
/// untrusted path or an unrecognized expression), unless it opts out via `unsafe_trust`.
fn first_untrusted_interpolation(
    text: &str,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    loop_aliases: &HashMap<String, String>,
) -> Option<String> {
    let mut search = text;
    while let Some(start) = search.find(crate::consts::EXPR_START) {
        let rest = &search[start + crate::consts::EXPR_START.len()..];
        let end = rest.find(crate::consts::EXPR_END)?;
        let inner = rest[..end].trim();
        if !has_unsafe_trust_opt_out(inner) {
            let (base, _) = crate::parser::split_pipe_aware(inner);
            let base = base.trim();
            match classify_base(base) {
                BaseTrust::Path => {
                    let mut hits = Vec::new();
                    map_path("", base, param_sanitize, None, loop_aliases, &mut hits);
                    if !hits.is_empty() {
                        return Some(base.to_string());
                    }
                }
                BaseTrust::Trusted => {}
                BaseTrust::Unknown => return Some(base.to_string()),
            }
        }
        search = &rest[end + crate::consts::EXPR_END.len()..];
    }
    None
}

/// Append the untrusted child entries for argument `expr` passed to child path `target`.
fn push_argument_taint(
    target: &str,
    expr: &str,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    host_notice: Option<&str>,
    loop_aliases: &HashMap<String, String>,
    entries: &mut Vec<(String, SanitizeSpec)>,
) {
    let expr = expr.trim();
    if has_unsafe_trust_opt_out(expr) {
        return;
    }
    let (base, _) = crate::parser::split_pipe_aware(expr);
    let base = base.trim();
    if let Some(lit) = crate::consts::strip_string_literal(base) {
        if first_untrusted_interpolation(lit, param_sanitize, loop_aliases).is_some() {
            entries.push((target.to_string(), SanitizeSpec::Inline));
        }
        return;
    }
    match classify_base(base) {
        BaseTrust::Path => map_path(
            target,
            base,
            param_sanitize,
            host_notice,
            loop_aliases,
            entries,
        ),
        BaseTrust::Trusted => {}
        BaseTrust::Unknown => entries.push((target.to_string(), SanitizeSpec::Inline)),
    }
}

/// Compute the child untrusted parameter map for include call site `inc` from the host
/// template's declarative `param_sanitize` policy, and flag untrusted include paths.
pub(super) fn propagate_include_taint(
    inc: &mut CompiledInclude,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    host_notice: Option<&str>,
    loop_aliases: &HashMap<String, String>,
) {
    if param_sanitize.is_empty() {
        return;
    }
    if let Some(expr) = first_untrusted_interpolation(&inc.path, param_sanitize, loop_aliases) {
        record_untrusted_path(inc, expr);
    }
    let mut entries = Vec::new();
    for (key, expr) in &inc.with_vars {
        push_argument_taint(
            key,
            expr,
            param_sanitize,
            host_notice,
            loop_aliases,
            &mut entries,
        );
    }
    if let Some((binding, list_expr)) = &inc.for_each {
        push_argument_taint(
            binding,
            list_expr,
            param_sanitize,
            host_notice,
            loop_aliases,
            &mut entries,
        );
    }
    if entries.is_empty() {
        return;
    }
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    entries.dedup_by(|(a, _), (b, _)| a == b);
    if let Some(ic) = &mut inc.inline_compiled {
        let map = effective_untrusted_map(&entries, &ic.declared_sanitize);
        sanitize_pass::propagate_param_sanitization(Arc::make_mut(&mut ic.segments), &map, None);
    } else {
        record_untrusted(inc, entries);
    }
}
