//! Compile-time AST pass for unified `sanitize` filter resolution:
//! - Enclosing XML tag detection (`<outer>{{ x | sanitize }}</outer>`)
//! - Declarative frontmatter parameter sanitization (`params: - x = str | sanitize(...)`)
//! - Template-level `sanitize_notice:` propagation.

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};

use super::{Condition, FilterKind, ParsedFilter, Segment};
use crate::{
    compat::{HashMap, HashSet},
    error::TemplateError,
    filter::{is_ncname_continue_byte, is_ncname_start_byte, parse_sanitize_filter_mode},
    scope::{CompiledExpr, CompiledPath, ConditionOperand},
    types::SanitizeSpec,
};

/// Resolved execution mode for [`FilterKind::Sanitize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SanitizeFilterMode {
    /// In-place token + enclosing-XML-tag sanitization (`| sanitize`).
    Inline {
        /// Optional comma-separated enclosing XML tag names from surrounding static template text.
        enclosing_tags: Option<Cow<'static, str>>,
    },
    /// Boundary-wrapped block sanitization (`| sanitize("tag")` or `| sanitize("tag", "notice")`).
    Block {
        /// Validated tag specification (`primary_tag` first, followed by any comma-separated outer/enclosing tags).
        tag_spec: Cow<'static, str>,
        /// Optional custom notice template (`Some("")` omits the notice; `None` uses [`crate::filter::DEFAULT_SANITIZE_NOTICE`]).
        notice: Option<Cow<'static, str>>,
    },
}

impl ParsedFilter {
    /// Construct a [`ParsedFilter`] from a resolved [`FilterKind`] and optional raw argument string,
    /// validating `sanitize` filter arguments at compile time.
    ///
    /// # Errors
    /// Returns [`TemplateError::Syntax`] if `kind == FilterKind::Sanitize` and `args` has invalid
    /// arity or invalid XML `NCName` tag names.
    pub fn parse(kind: FilterKind, args: Option<&str>) -> Result<Self, TemplateError> {
        let parsed_num = args.and_then(|a| a.parse::<usize>().ok());
        let sanitize_mode = if kind == FilterKind::Sanitize {
            Some(parse_sanitize_filter_mode(args)?)
        } else {
            None
        };
        Ok(Self {
            kind,
            args: args.map(|a| Cow::Owned(a.to_string())),
            parsed_num,
            sanitize_mode,
        })
    }

    /// Construct a [`ParsedFilter`] from a declarative [`SanitizeSpec`] and optional template-level
    /// `sanitize_notice`.
    #[must_use]
    pub fn from_sanitize_spec(spec: &SanitizeSpec, fm_notice: Option<&str>) -> Self {
        let sanitize_mode = match spec {
            SanitizeSpec::Inline => SanitizeFilterMode::Inline {
                enclosing_tags: None,
            },
            SanitizeSpec::Block { tag, notice } => SanitizeFilterMode::Block {
                tag_spec: Cow::Owned(tag.clone()),
                notice: notice
                    .as_deref()
                    .or(fm_notice)
                    .map(|n| Cow::Owned(n.to_string())),
            },
        };
        Self {
            kind: FilterKind::Sanitize,
            args: None,
            parsed_num: None,
            sanitize_mode: Some(sanitize_mode),
        }
    }
}

/// Sentinel tag in `SanitizeFilterMode::Inline::enclosing_tags` indicating that the
/// expression is interpolated on a Markdown ATX heading line (`# ...` .. `###### ...`)
/// and must have line-break characters collapsed to spaces.
pub(crate) const INLINE_HEADING_SENTINEL: &str = "#heading";

#[derive(Clone)]
struct EnclosingScanState {
    open_tags: Vec<String>,
    pending_open_tag: Option<String>,
    in_atx_heading: bool,
    at_line_start: bool,
}

impl Default for EnclosingScanState {
    fn default() -> Self {
        Self {
            open_tags: Vec::new(),
            pending_open_tag: None,
            in_atx_heading: false,
            at_line_start: true,
        }
    }
}

impl EnclosingScanState {
    fn propagate_branch(&self, segments: &mut [Segment], closing_tags: &HashSet<String>) {
        let mut branch_state = self.clone();
        propagate_enclosing_xml_tags(segments, closing_tags, &mut branch_state);
    }
}

/// Scan `segments` for enclosing XML tags and ATX Markdown heading contexts around
/// `FilterKind::Sanitize` expressions and merge them into each filter's [`SanitizeFilterMode`].
pub fn apply_enclosing_xml_tags(segments: &mut [Segment]) {
    if !has_any_sanitize_filter(segments) {
        return;
    }
    let mut closing_tags = HashSet::new();
    collect_closing_xml_tags(segments, &mut closing_tags);
    let mut state = EnclosingScanState::default();
    propagate_enclosing_xml_tags(segments, &closing_tags, &mut state);
}

/// Propagate frontmatter `params:` declarative sanitization (`param_sanitize`) and template-level
/// `sanitize_notice:` onto `segments`, then resolve enclosing XML tags across the updated tree.
pub fn apply_frontmatter_sanitization(
    segments: &mut [Segment],
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
) {
    if !param_sanitize.is_empty() || sanitize_notice.is_some() {
        let mut loop_aliases = HashMap::new();
        propagate_frontmatter_sanitization_inner(
            segments,
            param_sanitize,
            sanitize_notice,
            &mut loop_aliases,
        );
    }
    apply_enclosing_xml_tags(segments);
}

fn has_any_sanitize_filter(segments: &[Segment]) -> bool {
    for seg in segments {
        match seg {
            Segment::Expr { filters, .. } => {
                if filters.iter().any(|f| f.kind == FilterKind::Sanitize) {
                    return true;
                }
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                if has_any_sanitize_filter(body) || has_any_sanitize_filter(else_body) {
                    return true;
                }
            }
            Segment::If {
                branches,
                else_body,
            } => {
                if branches.iter().any(|(_, b)| has_any_sanitize_filter(b))
                    || has_any_sanitize_filter(else_body)
                {
                    return true;
                }
            }
            Segment::Match { arms, .. } => {
                if arms.iter().any(|arm| has_any_sanitize_filter(&arm.body)) {
                    return true;
                }
            }
            Segment::Panic(inner) => {
                if has_any_sanitize_filter(inner) {
                    return true;
                }
            }
            Segment::Include(inc) => {
                if let Some(ic) = &inc.inline_compiled
                    && has_any_sanitize_filter(&ic.segments)
                {
                    return true;
                }
            }
            Segment::Static(_) | Segment::Raw(_) | Segment::Comment(_) => {}
        }
    }
    false
}

fn collect_closing_xml_tags(segments: &[Segment], out: &mut HashSet<String>) {
    for seg in segments {
        match seg {
            Segment::Static(text) => scan_closing_tags_in_text(text, out),
            Segment::ForLoop {
                body, else_body, ..
            } => {
                collect_closing_xml_tags(body, out);
                collect_closing_xml_tags(else_body, out);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    collect_closing_xml_tags(branch_body, out);
                }
                collect_closing_xml_tags(else_body, out);
            }
            Segment::Match { arms, .. } => {
                for arm in arms {
                    collect_closing_xml_tags(&arm.body, out);
                }
            }
            Segment::Panic(inner) => collect_closing_xml_tags(inner, out),
            Segment::Include(inc) => {
                if let Some(ic) = &inc.inline_compiled {
                    collect_closing_xml_tags(&ic.segments, out);
                }
            }
            Segment::Expr { .. } | Segment::Raw(_) | Segment::Comment(_) => {}
        }
    }
}

fn scan_closing_tags_in_text(text: &str, out: &mut HashSet<String>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if bytes[i] == b'<' {
            let mut pos = i + 1;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos < bytes.len() && bytes[pos] == b'/' {
                pos += 1;
                while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos < bytes.len() && is_ncname_start_byte(bytes[pos]) {
                    let start = pos;
                    pos += 1;
                    while pos < bytes.len() && is_ncname_continue_byte(bytes[pos]) {
                        pos += 1;
                    }
                    let name = &text[start..pos];
                    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                        pos += 1;
                    }
                    if pos < bytes.len() && bytes[pos] == b'>' {
                        out.insert(name.to_ascii_lowercase());
                        i = pos + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
}

fn is_atx_heading_prefix(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes)
        && trimmed
            .as_bytes()
            .get(hashes)
            .is_some_and(|&b| b == b' ' || b == b'\t')
}

fn update_heading_line_state(text: &str, in_atx_heading: &mut bool, at_line_start: &mut bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last_nl) = text.rfind('\n') {
        let tail = &text[last_nl + 1..];
        *in_atx_heading = is_atx_heading_prefix(tail);
        *at_line_start = tail.trim_matches([' ', '\t']).is_empty();
    } else if *at_line_start {
        if is_atx_heading_prefix(text) {
            *in_atx_heading = true;
            *at_line_start = false;
        } else if !text.trim_matches([' ', '\t']).is_empty() {
            *at_line_start = false;
        }
    }
}

fn update_open_xml_tags_from_text(
    text: &str,
    closing_tags: &HashSet<String>,
    open_tags: &mut Vec<String>,
    pending_open_tag: &mut Option<String>,
) {
    let bytes = text.as_bytes();
    let mut i = 0usize;

    if let Some(pending_name) = pending_open_tag.take() {
        let mut close_idx = 0usize;
        while close_idx < bytes.len() && bytes[close_idx] != b'>' && bytes[close_idx] != b'<' {
            close_idx += 1;
        }
        if close_idx < bytes.len() && bytes[close_idx] == b'>' {
            let mut before_gt = close_idx;
            while before_gt > 0 && bytes[before_gt - 1].is_ascii_whitespace() {
                before_gt -= 1;
            }
            let self_closing = before_gt > 0 && bytes[before_gt - 1] == b'/';
            if !self_closing && closing_tags.contains(&pending_name.to_ascii_lowercase()) {
                open_tags.push(pending_name);
            }
            i = close_idx + 1;
        } else if close_idx == bytes.len() {
            *pending_open_tag = Some(pending_name);
            return;
        } else {
            i = close_idx;
        }
    }

    while i + 2 < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let mut pos = i + 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos < bytes.len() && bytes[pos] == b'/' {
            pos += 1;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos < bytes.len() && is_ncname_start_byte(bytes[pos]) {
                let start = pos;
                pos += 1;
                while pos < bytes.len() && is_ncname_continue_byte(bytes[pos]) {
                    pos += 1;
                }
                let name = &text[start..pos];
                while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos < bytes.len() && bytes[pos] == b'>' {
                    if let Some(idx) = open_tags.iter().rposition(|t| t.eq_ignore_ascii_case(name))
                    {
                        open_tags.remove(idx);
                    }
                    i = pos + 1;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        // Opening XML tag: `<` must be immediately followed by an NCName start byte.
        if i + 1 < bytes.len() && is_ncname_start_byte(bytes[i + 1]) {
            let start = i + 1;
            let mut end = start + 1;
            while end < bytes.len() && is_ncname_continue_byte(bytes[end]) {
                end += 1;
            }
            let name = &text[start..end];
            if end < bytes.len()
                && (bytes[end] == b'>' || bytes[end] == b'/' || bytes[end].is_ascii_whitespace())
            {
                let mut close_idx = end;
                while close_idx < bytes.len()
                    && bytes[close_idx] != b'>'
                    && bytes[close_idx] != b'<'
                {
                    close_idx += 1;
                }
                if close_idx < bytes.len() && bytes[close_idx] == b'>' {
                    let mut before_gt = close_idx;
                    while before_gt > end && bytes[before_gt - 1].is_ascii_whitespace() {
                        before_gt -= 1;
                    }
                    let self_closing = before_gt > end && bytes[before_gt - 1] == b'/';
                    if !self_closing && closing_tags.contains(&name.to_ascii_lowercase()) {
                        open_tags.push(name.to_string());
                    }
                    i = close_idx + 1;
                    continue;
                } else if close_idx == bytes.len()
                    && closing_tags.contains(&name.to_ascii_lowercase())
                {
                    *pending_open_tag = Some(name.to_string());
                    return;
                }
            }
        }
        i += 1;
    }
}

fn merge_enclosing_tags_into_spec(
    existing: Option<&str>,
    open_tags: &[String],
    in_atx_heading: bool,
) -> Option<String> {
    if open_tags.is_empty() && !in_atx_heading {
        return existing.map(ToString::to_string);
    }
    let mut combined: Vec<&str> = Vec::new();
    if let Some(ex) = existing {
        for part in ex.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() && !combined.iter().any(|c| c.eq_ignore_ascii_case(trimmed)) {
                combined.push(trimmed);
            }
        }
    }
    for tag in open_tags.iter().rev() {
        if !combined.iter().any(|c| c.eq_ignore_ascii_case(tag)) {
            combined.push(tag.as_str());
        }
    }
    if in_atx_heading && !combined.contains(&INLINE_HEADING_SENTINEL) {
        combined.push(INLINE_HEADING_SENTINEL);
    }
    if combined.is_empty() {
        None
    } else {
        Some(combined.join(","))
    }
}

fn ensure_sanitize_mode(f: &mut ParsedFilter) {
    if f.sanitize_mode.is_none() {
        match parse_sanitize_filter_mode(f.args.as_deref()) {
            Ok(mode) => f.sanitize_mode = Some(mode),
            Err(err) => debug_assert!(false, "unvalidated sanitize filter args: {err}"),
        }
    }
}

fn attach_open_tags_to_filters(
    filters: &mut [ParsedFilter],
    open_tags: &[String],
    in_atx_heading: bool,
) {
    if open_tags.is_empty() && !in_atx_heading {
        return;
    }
    for f in filters {
        if f.kind != FilterKind::Sanitize {
            continue;
        }
        ensure_sanitize_mode(f);
        match &mut f.sanitize_mode {
            Some(SanitizeFilterMode::Inline { enclosing_tags }) => {
                if let Some(merged) = merge_enclosing_tags_into_spec(
                    enclosing_tags.as_deref(),
                    open_tags,
                    in_atx_heading,
                ) {
                    *enclosing_tags = Some(Cow::Owned(merged));
                }
            }
            Some(SanitizeFilterMode::Block { tag_spec, .. }) => {
                if let Some(merged) =
                    merge_enclosing_tags_into_spec(Some(tag_spec), open_tags, false)
                {
                    *tag_spec = Cow::Owned(merged);
                }
            }
            None => {}
        }
    }
}

fn propagate_enclosing_xml_tags(
    segments: &mut [Segment],
    closing_tags: &HashSet<String>,
    state: &mut EnclosingScanState,
) {
    for seg in segments {
        match seg {
            Segment::Static(text) => {
                update_open_xml_tags_from_text(
                    text,
                    closing_tags,
                    &mut state.open_tags,
                    &mut state.pending_open_tag,
                );
                update_heading_line_state(text, &mut state.in_atx_heading, &mut state.at_line_start);
            }
            Segment::Expr { filters, .. } => {
                attach_open_tags_to_filters(filters, &state.open_tags, state.in_atx_heading);
                state.at_line_start = false;
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                state.propagate_branch(body, closing_tags);
                state.propagate_branch(else_body, closing_tags);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    state.propagate_branch(branch_body, closing_tags);
                }
                state.propagate_branch(else_body, closing_tags);
            }
            Segment::Match { arms, .. } => {
                for arm in arms {
                    state.propagate_branch(&mut arm.body, closing_tags);
                }
            }
            Segment::Panic(inner) => {
                state.propagate_branch(inner, closing_tags);
            }
            Segment::Include(inc) => {
                if let Some(ic) = &mut inc.inline_compiled {
                    state.propagate_branch(
                        alloc::sync::Arc::make_mut(&mut ic.segments),
                        closing_tags,
                    );
                }
            }
            Segment::Raw(text) => {
                update_heading_line_state(text, &mut state.in_atx_heading, &mut state.at_line_start);
            }
            Segment::Comment(_) => {}
        }
    }
}

fn resolve_canonical_path(path: &CompiledPath, loop_aliases: &HashMap<String, String>) -> String {
    let parts = path.parts();
    if parts.is_empty() {
        return String::new();
    }
    let slice = if parts.len() > 1 && parts[0] == "params" {
        &parts[1..]
    } else {
        parts
    };
    let root = &slice[0];
    let resolved_root = loop_aliases.get(root).map_or(root.as_str(), String::as_str);
    if slice.len() == 1 {
        resolved_root.to_string()
    } else {
        let mut out = String::with_capacity(resolved_root.len() + path.as_str().len());
        out.push_str(resolved_root);
        for part in &slice[1..] {
            out.push(crate::consts::PATH_SEP);
            out.push_str(part);
        }
        out
    }
}

fn apply_sanitize_to_path_filters(
    path: &CompiledPath,
    filters: &mut Vec<ParsedFilter>,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
    loop_aliases: &HashMap<String, String>,
) {
    let canonical = resolve_canonical_path(path, loop_aliases);
    if let Some(spec) = param_sanitize.get(&canonical) {
        let already_sanitized = filters.iter().any(|f| {
            matches!(
                f.kind,
                FilterKind::Sanitize | FilterKind::Quarantine | FilterKind::SanitizeTokens
            )
        });
        if !already_sanitized {
            filters.push(ParsedFilter::from_sanitize_spec(spec, sanitize_notice));
        }
    }
    if let Some(fm_notice) = sanitize_notice {
        for f in filters.iter_mut() {
            if f.kind == FilterKind::Sanitize {
                ensure_sanitize_mode(f);
                if let Some(SanitizeFilterMode::Block { notice, .. }) = &mut f.sanitize_mode
                    && notice.is_none()
                {
                    *notice = Some(Cow::Owned(fm_notice.to_string()));
                }
            }
        }
    }
}

fn apply_notice_to_filters(filters: &mut [ParsedFilter], sanitize_notice: Option<&str>) {
    let Some(fm_notice) = sanitize_notice else {
        return;
    };
    for f in filters.iter_mut() {
        if f.kind == FilterKind::Sanitize {
            ensure_sanitize_mode(f);
            if let Some(SanitizeFilterMode::Block { notice, .. }) = &mut f.sanitize_mode
                && notice.is_none()
            {
                *notice = Some(Cow::Owned(fm_notice.to_string()));
            }
        }
    }
}

fn propagate_condition_sanitization(
    cond: &mut Condition,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
    loop_aliases: &mut HashMap<String, String>,
) {
    match cond {
        Condition::Truthy(op) => {
            propagate_operand_sanitization(op, param_sanitize, sanitize_notice, loop_aliases);
        }
        Condition::Not(inner) => {
            propagate_condition_sanitization(inner, param_sanitize, sanitize_notice, loop_aliases);
        }
        Condition::Comparison { left, right, .. } => {
            propagate_operand_sanitization(left, param_sanitize, sanitize_notice, loop_aliases);
            propagate_operand_sanitization(right, param_sanitize, sanitize_notice, loop_aliases);
        }
        Condition::And(a, b) | Condition::Or(a, b) => {
            propagate_condition_sanitization(a, param_sanitize, sanitize_notice, loop_aliases);
            propagate_condition_sanitization(b, param_sanitize, sanitize_notice, loop_aliases);
        }
        Condition::MatchVariant { .. } => {}
    }
}

fn propagate_operand_sanitization(
    op: &mut ConditionOperand,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
    loop_aliases: &mut HashMap<String, String>,
) {
    match op {
        ConditionOperand::Path { filters, .. } => {
            apply_notice_to_filters(filters, sanitize_notice);
        }
        ConditionOperand::InterpolatedStr(segs) => {
            propagate_frontmatter_sanitization_inner(
                segs,
                param_sanitize,
                sanitize_notice,
                loop_aliases,
            );
        }
        ConditionOperand::Literal(_)
        | ConditionOperand::Idx(_)
        | ConditionOperand::Len(_)
        | ConditionOperand::Kind(_)
        | ConditionOperand::Kinds(_)
        | ConditionOperand::Has(_) => {}
    }
}

fn propagate_frontmatter_sanitization_inner(
    segments: &mut [Segment],
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
    loop_aliases: &mut HashMap<String, String>,
) {
    for seg in segments {
        match seg {
            Segment::Expr { expr, filters } => {
                if let CompiledExpr::Path(path) = expr {
                    apply_sanitize_to_path_filters(
                        path,
                        filters,
                        param_sanitize,
                        sanitize_notice,
                        loop_aliases,
                    );
                } else {
                    apply_notice_to_filters(filters, sanitize_notice);
                }
            }
            Segment::ForLoop {
                binding,
                list_expr,
                body,
                else_body,
                ..
            } => {
                let prev = if let CompiledExpr::Path(list_path) = list_expr {
                    let canonical_list = resolve_canonical_path(list_path, loop_aliases);
                    loop_aliases.insert(binding.to_string(), canonical_list)
                } else {
                    None
                };
                propagate_frontmatter_sanitization_inner(
                    body,
                    param_sanitize,
                    sanitize_notice,
                    loop_aliases,
                );
                if let Some(old) = prev {
                    loop_aliases.insert(binding.to_string(), old);
                } else {
                    loop_aliases.remove(binding.as_ref());
                }
                propagate_frontmatter_sanitization_inner(
                    else_body,
                    param_sanitize,
                    sanitize_notice,
                    loop_aliases,
                );
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (cond, branch_body) in branches {
                    propagate_condition_sanitization(
                        cond,
                        param_sanitize,
                        sanitize_notice,
                        loop_aliases,
                    );
                    propagate_frontmatter_sanitization_inner(
                        branch_body,
                        param_sanitize,
                        sanitize_notice,
                        loop_aliases,
                    );
                }
                propagate_frontmatter_sanitization_inner(
                    else_body,
                    param_sanitize,
                    sanitize_notice,
                    loop_aliases,
                );
            }
            Segment::Match { arms, .. } => {
                for arm in arms {
                    if let Some(guard) = &mut arm.guard {
                        propagate_condition_sanitization(
                            guard,
                            param_sanitize,
                            sanitize_notice,
                            loop_aliases,
                        );
                    }
                    propagate_frontmatter_sanitization_inner(
                        &mut arm.body,
                        param_sanitize,
                        sanitize_notice,
                        loop_aliases,
                    );
                }
            }
            Segment::Panic(inner) => {
                propagate_frontmatter_sanitization_inner(
                    inner,
                    param_sanitize,
                    sanitize_notice,
                    loop_aliases,
                );
            }
            Segment::Static(_) | Segment::Raw(_) | Segment::Include(_) | Segment::Comment(_) => {}
        }
    }
}
