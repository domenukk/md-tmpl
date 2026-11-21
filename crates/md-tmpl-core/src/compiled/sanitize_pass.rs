//! Compile-time AST pass for unified `sanitize` filter resolution:
//! - Enclosing XML tag detection (`<outer>{{ x | sanitize }}</outer>`)
//! - Declarative frontmatter parameter sanitization (`params: - x = str | sanitize(...)`)
//! - Template-level `sanitize_notice:` propagation.

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};

use super::{Condition, FilterKind, FilterOrigin, ParsedFilter, Segment};
use crate::{
    compat::{HashMap, HashSet},
    error::TemplateError,
    filter::{
        is_ncname_continue_byte, is_ncname_start_byte, is_valid_ncname, parse_sanitize_filter_mode,
        skip_tag_whitespace,
    },
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
        if kind == FilterKind::UnsafeTrust && args.is_some() {
            return Err(TemplateError::syntax(
                "'unsafe_trust' does not take arguments",
            ));
        }
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
            origin: FilterOrigin::Template,
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
            origin: FilterOrigin::Declaration,
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
    pending_quote: Option<u8>,
    in_atx_heading: bool,
    at_line_start: bool,
}

impl Default for EnclosingScanState {
    fn default() -> Self {
        Self {
            open_tags: Vec::new(),
            pending_open_tag: None,
            pending_quote: None,
            in_atx_heading: false,
            at_line_start: true,
        }
    }
}

impl EnclosingScanState {
    fn run_branch(&self, segments: &mut [Segment], closing_tags: &HashSet<String>) -> Self {
        let mut branch_state = self.clone();
        propagate_enclosing_xml_tags(segments, closing_tags, &mut branch_state);
        branch_state
    }

    fn scan_isolated_branch(&self, segments: &mut [Segment], closing_tags: &HashSet<String>) {
        let mut branch_state = self.clone();
        propagate_enclosing_xml_tags(segments, closing_tags, &mut branch_state);
    }

    fn merge_branches(&mut self, states: &[Self]) {
        if states.is_empty() {
            return;
        }
        let mut union_tags: Vec<String> = Vec::new();
        let mut pending: Option<(String, Option<u8>)> = None;
        let mut any_heading = false;
        let mut any_line_start = false;
        for st in states {
            for tag in &st.open_tags {
                if !union_tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                    union_tags.push(tag.clone());
                }
            }
            if pending.is_none()
                && let Some(ref p) = st.pending_open_tag
            {
                pending = Some((p.clone(), st.pending_quote));
            }
            any_heading |= st.in_atx_heading;
            any_line_start |= st.at_line_start;
        }
        self.open_tags = union_tags;
        if let Some((p, q)) = pending {
            self.pending_open_tag = Some(p);
            self.pending_quote = q;
        } else {
            self.pending_open_tag = None;
            self.pending_quote = None;
        }
        self.in_atx_heading = any_heading;
        self.at_line_start = any_line_start;
    }
}

/// Scan `segments` for enclosing XML tags and ATX Markdown heading contexts around
/// `FilterKind::Sanitize` / `FilterKind::SanitizeTokens` / `FilterKind::Quarantine`
/// expressions and merge them into each filter.
pub fn apply_enclosing_xml_tags(segments: &mut [Segment]) {
    apply_enclosing_xml_tags_from(segments, &[], false, true, None);
}

/// Like [`apply_enclosing_xml_tags`], but starts the scan from an include call site's
/// enclosing state (tags already open in the including template, heading/line state).
pub(super) fn apply_enclosing_xml_tags_from(
    segments: &mut [Segment],
    open_tags: &[String],
    in_atx_heading: bool,
    at_line_start: bool,
    pending_open_tag: Option<&str>,
) {
    if !needs_enclosing_pass(segments) {
        return;
    }
    let mut closing_tags = HashSet::new();
    collect_closing_xml_tags(segments, &mut closing_tags);
    if let Some(pending) = pending_open_tag {
        closing_tags.insert(pending.to_ascii_lowercase());
    }
    let mut state = EnclosingScanState {
        open_tags: open_tags.to_vec(),
        pending_open_tag: pending_open_tag.map(ToString::to_string),
        pending_quote: None,
        in_atx_heading,
        at_line_start,
    };
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

/// Returns `true` if `segments` contain a `sanitize`, `sanitize_tokens`, `quarantine`, or `fence`
/// filter, or an include resolved at render time (whose call-site enclosing state must be recorded).
pub(super) fn needs_enclosing_pass(segments: &[Segment]) -> bool {
    for seg in segments {
        match seg {
            Segment::Expr { filters, .. } => {
                if filters.iter().any(|f| {
                    matches!(
                        f.kind,
                        FilterKind::Sanitize
                            | FilterKind::SanitizeTokens
                            | FilterKind::Quarantine
                            | FilterKind::Fence
                    )
                }) {
                    return true;
                }
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                if needs_enclosing_pass(body) || needs_enclosing_pass(else_body) {
                    return true;
                }
            }
            Segment::If {
                branches,
                else_body,
            } => {
                if branches.iter().any(|(_, b)| needs_enclosing_pass(b))
                    || needs_enclosing_pass(else_body)
                {
                    return true;
                }
            }
            Segment::Match { arms, .. } => {
                if arms.iter().any(|arm| needs_enclosing_pass(&arm.body)) {
                    return true;
                }
            }
            Segment::Panic(inner) => {
                if needs_enclosing_pass(inner) {
                    return true;
                }
            }
            Segment::Include(inc) => match &inc.inline_compiled {
                Some(ic) => {
                    if needs_enclosing_pass(&ic.segments) {
                        return true;
                    }
                }
                None => return true,
            },
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
            let mut pos = skip_tag_whitespace(text, i + 1);
            if pos < bytes.len() && bytes[pos] == b'/' {
                pos = skip_tag_whitespace(text, pos + 1);
                if pos < bytes.len() && is_ncname_start_byte(bytes[pos]) {
                    let start = pos;
                    pos += 1;
                    while pos < bytes.len() && is_ncname_continue_byte(bytes[pos]) {
                        pos += 1;
                    }
                    let name = &text[start..pos];
                    pos = skip_tag_whitespace(text, pos);
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

/// Strip leading spaces/tabs and `CommonMark` blockquote (`>`) / list (`- `, `* `, `+ `, `1. `, `1) `)
/// container markers from a line prefix.
fn strip_markdown_container_prefixes(mut line: &str) -> &str {
    loop {
        let s = line.trim_start_matches([' ', '\t']);
        if let Some(after_gt) = s.strip_prefix('>') {
            line = after_gt;
            continue;
        }
        if let Some(after_bullet) = s.strip_prefix(['-', '*', '+'])
            && after_bullet.starts_with([' ', '\t'])
        {
            line = after_bullet;
            continue;
        }
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        if (1..=9).contains(&digits)
            && let Some(after_delim) = s[digits..].strip_prefix(['.', ')'])
            && after_delim.starts_with([' ', '\t'])
        {
            line = after_delim;
            continue;
        }
        return s;
    }
}

fn is_atx_heading_prefix(line: &str) -> bool {
    let trimmed = strip_markdown_container_prefixes(line);
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes)
        && trimmed
            .as_bytes()
            .get(hashes)
            .is_some_and(|&b| b == b' ' || b == b'\t')
}

pub(super) fn update_heading_line_state(
    text: &str,
    in_atx_heading: &mut bool,
    at_line_start: &mut bool,
) {
    if text.is_empty() {
        return;
    }
    if let Some(last_nl) = text.rfind(['\n', '\r']) {
        let tail = &text[last_nl + 1..];
        *in_atx_heading = is_atx_heading_prefix(tail);
        *at_line_start = strip_markdown_container_prefixes(tail).is_empty();
    } else if *at_line_start {
        if is_atx_heading_prefix(text) {
            *in_atx_heading = true;
            *at_line_start = false;
        } else if !strip_markdown_container_prefixes(text).is_empty() {
            *at_line_start = false;
        }
    }
}

enum OpenTagTailOutcome {
    Closed {
        close_idx: usize,
        self_closing: bool,
    },
    Pending {
        quote: Option<u8>,
    },
    AbortedAtLt {
        lt_idx: usize,
    },
}

/// Scan the attribute tail of an opening XML tag starting at `start_idx`, ignoring `>`, `<`,
/// and `/` inside single- or double-quoted attribute values.
fn scan_open_tag_tail(
    bytes: &[u8],
    start_idx: usize,
    initial_quote: Option<u8>,
) -> OpenTagTailOutcome {
    let mut quote = initial_quote;
    let mut last_unquoted_non_ws: Option<u8> = None;
    let mut idx = start_idx;
    while idx < bytes.len() {
        let b = bytes[idx];
        if let Some(q) = quote {
            if b == q {
                quote = None;
                last_unquoted_non_ws = Some(b);
            }
            idx += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => {
                quote = Some(b);
            }
            b'>' => {
                return OpenTagTailOutcome::Closed {
                    close_idx: idx,
                    self_closing: last_unquoted_non_ws == Some(b'/'),
                };
            }
            b'<' => {
                return OpenTagTailOutcome::AbortedAtLt { lt_idx: idx };
            }
            _ => {
                if !matches!(b, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r') {
                    last_unquoted_non_ws = Some(b);
                }
            }
        }
        idx += 1;
    }
    OpenTagTailOutcome::Pending { quote }
}

fn update_open_xml_tags_from_text(
    text: &str,
    closing_tags: &HashSet<String>,
    state: &mut EnclosingScanState,
) {
    let bytes = text.as_bytes();
    let mut i = 0usize;

    if let Some(pending_name) = state.pending_open_tag.take() {
        let prev_quote = state.pending_quote.take();
        match scan_open_tag_tail(bytes, 0, prev_quote) {
            OpenTagTailOutcome::Closed {
                close_idx,
                self_closing,
            } => {
                if !self_closing && closing_tags.contains(&pending_name.to_ascii_lowercase()) {
                    state.open_tags.push(pending_name);
                }
                i = close_idx + 1;
            }
            OpenTagTailOutcome::Pending { quote } => {
                state.pending_open_tag = Some(pending_name);
                state.pending_quote = quote;
                return;
            }
            OpenTagTailOutcome::AbortedAtLt { lt_idx } => {
                i = lt_idx;
            }
        }
    }

    while i + 1 < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let mut pos = skip_tag_whitespace(text, i + 1);
        if pos < bytes.len() && bytes[pos] == b'/' {
            pos = skip_tag_whitespace(text, pos + 1);
            if pos < bytes.len() && is_ncname_start_byte(bytes[pos]) {
                let start = pos;
                pos += 1;
                while pos < bytes.len() && is_ncname_continue_byte(bytes[pos]) {
                    pos += 1;
                }
                let name = &text[start..pos];
                pos = skip_tag_whitespace(text, pos);
                if pos < bytes.len() && bytes[pos] == b'>' {
                    if let Some(idx) = state
                        .open_tags
                        .iter()
                        .rposition(|t| t.eq_ignore_ascii_case(name))
                    {
                        state.open_tags.remove(idx);
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
            if end == bytes.len() {
                if closing_tags.contains(&name.to_ascii_lowercase()) {
                    state.pending_open_tag = Some(name.to_string());
                    state.pending_quote = None;
                }
                return;
            }
            if bytes[end] == b'>' || bytes[end] == b'/' || skip_tag_whitespace(text, end) > end {
                match scan_open_tag_tail(bytes, end, None) {
                    OpenTagTailOutcome::Closed {
                        close_idx,
                        self_closing,
                    } => {
                        if !self_closing && closing_tags.contains(&name.to_ascii_lowercase()) {
                            state.open_tags.push(name.to_string());
                        }
                        i = close_idx + 1;
                        continue;
                    }
                    OpenTagTailOutcome::Pending { quote } => {
                        if closing_tags.contains(&name.to_ascii_lowercase()) {
                            state.pending_open_tag = Some(name.to_string());
                            state.pending_quote = quote;
                        }
                        return;
                    }
                    OpenTagTailOutcome::AbortedAtLt { lt_idx } => {
                        i = lt_idx;
                        continue;
                    }
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
    let len = filters.len();
    for idx in 0..len {
        if filters[idx].kind == FilterKind::Quarantine {
            if !open_tags.is_empty() {
                let raw = filters[idx]
                    .args
                    .as_deref()
                    .map_or(crate::filter::DEFAULT_QUARANTINE_TAG, |a| {
                        a.trim().trim_matches(['"', '\''])
                    });
                if let Some(merged) = merge_enclosing_tags_into_spec(Some(raw), open_tags, false) {
                    filters[idx].args = Some(Cow::Owned(merged));
                }
            }
            continue;
        }
        if filters[idx].kind == FilterKind::SanitizeTokens {
            // Keep the compiler-inserted `[SanitizeTokens, Quarantine]` pair intact so
            // `try_apply_str_filters_into` fuses them into `quarantine_untrusted_into`
            // (`Quarantine` immediately following already receives `open_tags`).
            if filters[idx].args.is_none()
                && idx + 1 < len
                && filters[idx + 1].kind == FilterKind::Quarantine
            {
                continue;
            }
            let existing_valid: Option<String> = match filters[idx].args.as_deref() {
                None => Some(String::new()),
                Some(raw) => {
                    let unquoted = crate::consts::strip_string_literal(raw.trim()).map_or_else(
                        || raw.trim().to_string(),
                        crate::consts::unescape_string_literal,
                    );
                    let trimmed_spec = unquoted.trim();
                    if !trimmed_spec.is_empty()
                        && trimmed_spec
                            .split(',')
                            .all(|part| is_valid_ncname(part.trim()))
                    {
                        Some(trimmed_spec.to_string())
                    } else {
                        None
                    }
                }
            };
            if let Some(ex) = existing_valid {
                let ex_opt = if ex.is_empty() {
                    None
                } else {
                    Some(ex.as_str())
                };
                if let Some(merged) =
                    merge_enclosing_tags_into_spec(ex_opt, open_tags, in_atx_heading)
                {
                    filters[idx].kind = FilterKind::Sanitize;
                    filters[idx].args = None;
                    filters[idx].sanitize_mode = Some(SanitizeFilterMode::Inline {
                        enclosing_tags: Some(Cow::Owned(merged)),
                    });
                }
            }
            continue;
        }
        if filters[idx].kind != FilterKind::Sanitize {
            continue;
        }
        ensure_sanitize_mode(&mut filters[idx]);
        match &mut filters[idx].sanitize_mode {
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
                if !closing_tags.is_empty() {
                    update_open_xml_tags_from_text(text, closing_tags, state);
                }
                update_heading_line_state(
                    text,
                    &mut state.in_atx_heading,
                    &mut state.at_line_start,
                );
            }
            Segment::Expr { filters, .. } => {
                attach_open_tags_to_filters(filters, &state.open_tags, state.in_atx_heading);
                state.at_line_start = false;
            }
            Segment::ForLoop {
                body, else_body, ..
            } => {
                state.scan_isolated_branch(body, closing_tags);
                state.scan_isolated_branch(else_body, closing_tags);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                let mut post_states = Vec::with_capacity(branches.len() + 1);
                for (_, branch_body) in branches {
                    post_states.push(state.run_branch(branch_body, closing_tags));
                }
                if else_body.is_empty() {
                    post_states.push(state.clone());
                } else {
                    post_states.push(state.run_branch(else_body, closing_tags));
                }
                state.merge_branches(&post_states);
            }
            Segment::Match { arms, .. } => {
                let mut post_states = Vec::with_capacity(arms.len());
                for arm in arms {
                    post_states.push(state.run_branch(&mut arm.body, closing_tags));
                }
                state.merge_branches(&post_states);
            }
            Segment::Panic(inner) => {
                state.scan_isolated_branch(inner, closing_tags);
            }
            Segment::Include(inc) => {
                if let Some(ic) = &mut inc.inline_compiled {
                    state.scan_isolated_branch(
                        alloc::sync::Arc::make_mut(&mut ic.segments),
                        closing_tags,
                    );
                } else {
                    super::include_sanitize::record_enclosing(
                        inc,
                        &state.open_tags,
                        state.in_atx_heading,
                        state.at_line_start,
                        state.pending_open_tag.as_deref(),
                    );
                }
                state.at_line_start = false;
            }
            Segment::Raw(text) => {
                update_heading_line_state(
                    text,
                    &mut state.in_atx_heading,
                    &mut state.at_line_start,
                );
            }
            Segment::Comment(_) => {}
        }
    }
}

/// Propagate a declarative `param_sanitize` policy onto `segments` (no loop aliases in scope).
pub(super) fn propagate_param_sanitization(
    segments: &mut [Segment],
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
) {
    let mut loop_aliases = HashMap::new();
    propagate_frontmatter_sanitization_inner(
        segments,
        param_sanitize,
        sanitize_notice,
        &mut loop_aliases,
    );
}

/// Canonical (loop-alias resolved, namespace-prefix stripped) dot path for `path`.
pub(super) fn canonical_path(
    path: &CompiledPath,
    loop_aliases: &HashMap<String, String>,
) -> String {
    resolve_canonical_path(path, loop_aliases)
}

fn resolve_canonical_path(path: &CompiledPath, loop_aliases: &HashMap<String, String>) -> String {
    let parts = path.parts();
    if parts.is_empty() {
        return String::new();
    }
    let root = &parts[0];
    let resolved_root = loop_aliases.get(root).map_or(root.as_str(), String::as_str);
    if parts.len() == 1 {
        resolved_root.to_string()
    } else {
        let mut out = String::with_capacity(resolved_root.len() + path.as_str().len());
        out.push_str(resolved_root);
        for part in &parts[1..] {
            out.push(crate::consts::PATH_SEP);
            out.push_str(part);
        }
        out
    }
}

/// Look up the effective [`SanitizeSpec`] for `canonical` when emitted directly,
/// falling back to any untrusted ancestor path `key` (`canonical == "{key}.{rest}"`).
fn lookup_direct_path_spec<'a>(
    canonical: &str,
    param_sanitize: &'a HashMap<String, SanitizeSpec>,
) -> Option<&'a SanitizeSpec> {
    if let Some(exact) = param_sanitize.get(canonical) {
        return Some(exact);
    }
    for (key, spec) in param_sanitize {
        if let Some(rest) = canonical.strip_prefix(key.as_str())
            && rest.starts_with(crate::consts::PATH_SEP)
        {
            return Some(spec);
        }
    }
    None
}

/// Returns `true` if `canonical` or any nested field (`"{canonical}.{rest}"`) or ancestor
/// (`canonical == "{key}.{rest}"`) is marked untrusted in `param_sanitize`.
fn path_has_any_untrusted(canonical: &str, param_sanitize: &HashMap<String, SanitizeSpec>) -> bool {
    if param_sanitize.contains_key(canonical) {
        return true;
    }
    param_sanitize.keys().any(|key| {
        key.strip_prefix(canonical)
            .is_some_and(|rest| rest.starts_with(crate::consts::PATH_SEP))
            || canonical
                .strip_prefix(key.as_str())
                .is_some_and(|rest| rest.starts_with(crate::consts::PATH_SEP))
    })
}

fn apply_sanitize_to_path_filters(
    path: &CompiledPath,
    filters: &mut Vec<ParsedFilter>,
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
    loop_aliases: &HashMap<String, String>,
) {
    let canonical = resolve_canonical_path(path, loop_aliases);
    let has_tojson = filters.iter().any(|f| f.kind == FilterKind::ToJson);
    let resolved_spec: Option<Cow<'_, SanitizeSpec>> = if has_tojson {
        if path_has_any_untrusted(&canonical, param_sanitize) {
            Some(Cow::Owned(SanitizeSpec::Inline))
        } else {
            None
        }
    } else {
        lookup_direct_path_spec(&canonical, param_sanitize).map(Cow::Borrowed)
    };
    if let Some(spec) = resolved_spec {
        let has_token_sanitize = filters.iter().any(|f| {
            matches!(
                f.kind,
                FilterKind::Sanitize | FilterKind::SanitizeTokens | FilterKind::UnsafeTrust
            )
        });
        if let Some(q_idx) = filters
            .iter()
            .position(|f| f.kind == FilterKind::Quarantine)
        {
            if !has_token_sanitize {
                filters.insert(
                    q_idx,
                    ParsedFilter {
                        kind: FilterKind::SanitizeTokens,
                        args: None,
                        parsed_num: None,
                        sanitize_mode: None,
                        origin: FilterOrigin::Declaration,
                    },
                );
            }
        } else if !has_token_sanitize {
            filters.push(ParsedFilter::from_sanitize_spec(&spec, sanitize_notice));
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

fn propagate_condition_sanitization(cond: &mut Condition, sanitize_notice: Option<&str>) {
    match cond {
        Condition::Truthy(op) => {
            propagate_operand_sanitization(op, sanitize_notice);
        }
        Condition::Not(inner) => {
            propagate_condition_sanitization(inner, sanitize_notice);
        }
        Condition::Comparison { left, right, .. } => {
            propagate_operand_sanitization(left, sanitize_notice);
            propagate_operand_sanitization(right, sanitize_notice);
        }
        Condition::And(a, b) | Condition::Or(a, b) => {
            propagate_condition_sanitization(a, sanitize_notice);
            propagate_condition_sanitization(b, sanitize_notice);
        }
        Condition::MatchVariant { .. } => {}
    }
}

fn propagate_operand_sanitization(op: &mut ConditionOperand, sanitize_notice: Option<&str>) {
    match op {
        ConditionOperand::Path { filters, .. } => {
            apply_notice_to_filters(filters, sanitize_notice);
        }
        ConditionOperand::Filtered { base, filters } => {
            propagate_operand_sanitization(base, sanitize_notice);
            apply_notice_to_filters(filters, sanitize_notice);
        }
        ConditionOperand::InterpolatedStr(segs) => {
            let empty_sanitize = HashMap::new();
            let mut empty_aliases = HashMap::new();
            propagate_frontmatter_sanitization_inner(
                segs,
                &empty_sanitize,
                sanitize_notice,
                &mut empty_aliases,
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

/// Propagate declarative sanitization into a for-loop body (with `binding` aliased to the
/// canonical list path) and its `else` body.
fn propagate_for_loop_sanitization(
    binding: &str,
    list_expr: &CompiledExpr,
    body: &mut [Segment],
    else_body: &mut [Segment],
    param_sanitize: &HashMap<String, SanitizeSpec>,
    sanitize_notice: Option<&str>,
    loop_aliases: &mut HashMap<String, String>,
) {
    let prev = if let CompiledExpr::Path(list_path) = list_expr {
        let canonical_list = resolve_canonical_path(list_path, loop_aliases);
        loop_aliases.insert(binding.to_string(), canonical_list)
    } else {
        None
    };
    propagate_frontmatter_sanitization_inner(body, param_sanitize, sanitize_notice, loop_aliases);
    if let Some(old) = prev {
        loop_aliases.insert(binding.to_string(), old);
    } else {
        loop_aliases.remove(binding);
    }
    propagate_frontmatter_sanitization_inner(
        else_body,
        param_sanitize,
        sanitize_notice,
        loop_aliases,
    );
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
            } => propagate_for_loop_sanitization(
                binding,
                list_expr,
                body,
                else_body,
                param_sanitize,
                sanitize_notice,
                loop_aliases,
            ),
            Segment::If {
                branches,
                else_body,
            } => {
                for (cond, branch_body) in branches {
                    propagate_condition_sanitization(cond, sanitize_notice);
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
                        propagate_condition_sanitization(guard, sanitize_notice);
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
            Segment::Include(inc) => {
                super::include_sanitize::propagate_include_taint(
                    inc,
                    param_sanitize,
                    sanitize_notice,
                    loop_aliases,
                );
            }
            Segment::Static(_) | Segment::Raw(_) | Segment::Comment(_) => {}
        }
    }
}
