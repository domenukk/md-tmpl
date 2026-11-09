//! XML `NCName` tag validation, breakout scanning, code-fencing, and quarantine primitives.

use alloc::{borrow::Cow, string::String};

use super::{
    ControlTokenMatch, DEFAULT_QUARANTINE_TAG, ESCAPED_GT, ESCAPED_LT, MAX_STACK_QUARANTINE_TAGS,
    MIN_FENCE_BACKTICKS, ROLE_TOKEN_DELIMITERS, SANITIZE_EXTRA_CAPACITY, TOKEN_DELIMITERS,
    match_control_token_at,
};
use crate::{error::TemplateError, filter::strip_quotes, value::Value};

/// Compute the 27-bit ASCII case-insensitive `NCName` start-character bit (`'a'..='z'` -> `0..26`, `'_'` -> `26`),
/// or `0` if `b` is not a valid ASCII `NCName` start byte.
#[inline]
const fn ncname_first_char_bit(b: u8) -> u32 {
    let lower = b | 0x20;
    if lower >= b'a' && lower <= b'z' {
        1u32 << (lower - b'a')
    } else if b == b'_' {
        1u32 << 26
    } else {
        0
    }
}

/// Check whether a byte is a valid ASCII XML `NCName` start character.
pub(crate) const fn is_ncname_start_byte(b: u8) -> bool {
    ncname_first_char_bit(b) != 0
}

/// Check whether a byte is a valid ASCII XML `NCName` continuation character.
pub(crate) const fn is_ncname_continue_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'
}

/// Validate whether an XML tag name is a valid ASCII XML `NCName`.
fn is_valid_ncname(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(is_ncname_start_byte) && bytes.all(is_ncname_continue_byte)
}

/// Validate a single or comma-separated list of XML `NCName` boundary tags for `filter_name`,
/// returning `(primary_tag, full_spec)`.
fn validate_tag_spec_for_filter<'a>(
    spec: &'a str,
    filter_name: &str,
) -> Result<(&'a str, &'a str), TemplateError> {
    if spec.is_empty() {
        return Err(TemplateError::syntax(alloc::format!(
            "'{filter_name}' tag name must be a valid XML NCName: ''"
        )));
    }
    let mut primary: Option<&str> = None;
    for raw_part in spec.split(',') {
        let part = raw_part.trim();
        if !is_valid_ncname(part) {
            return Err(TemplateError::syntax(alloc::format!(
                "'{filter_name}' tag name must be a valid XML NCName: '{spec}'"
            )));
        }
        if primary.is_none() {
            primary = Some(part);
        }
    }
    Ok((primary.unwrap_or(DEFAULT_QUARANTINE_TAG), spec))
}

/// Validate a single or comma-separated list of XML `NCName` boundary tags,
/// returning `(primary_tag, full_spec)`.
pub(in crate::filter) fn validate_quarantine_tag_spec(
    spec: &str,
) -> Result<(&str, &str), TemplateError> {
    validate_tag_spec_for_filter(spec, crate::consts::FILTER_QUARANTINE)
}

/// Validate a single or comma-separated list of XML `NCName` boundary tags for `sanitize`,
/// returning `(primary_tag, full_spec)`.
pub(crate) fn validate_sanitize_tag_spec(spec: &str) -> Result<(&str, &str), TemplateError> {
    validate_tag_spec_for_filter(spec, crate::consts::FILTER_SANITIZE)
}

/// Pre-parsed quarantine tag specification with a 27-bit ASCII case-insensitive
/// first-character bitmask so non-matching `<` characters reject in 1 bit-test
/// without splitting `tag_spec` inside the byte loop.
struct ParsedTagSpec<'a> {
    first_char_mask: u32,
    len: usize,
    tags: [&'a [u8]; MAX_STACK_QUARANTINE_TAGS],
    overflow_spec: Option<&'a str>,
}

impl<'a> ParsedTagSpec<'a> {
    #[inline]
    fn from_opt(tag_spec: Option<&'a str>) -> Self {
        let mut parsed = Self {
            first_char_mask: 0,
            len: 0,
            tags: [&[]; MAX_STACK_QUARANTINE_TAGS],
            overflow_spec: None,
        };
        let Some(spec) = tag_spec else {
            return parsed;
        };
        if !spec.as_bytes().contains(&b',') {
            let trimmed = spec.trim().as_bytes();
            if let Some(&first) = trimmed.first() {
                let bit = ncname_first_char_bit(first);
                if bit != 0 {
                    parsed.first_char_mask = bit;
                    parsed.tags[0] = trimmed;
                    parsed.len = 1;
                }
            }
            return parsed;
        }
        for part in spec.split(',') {
            let trimmed = part.trim().as_bytes();
            let Some(&first) = trimmed.first() else {
                continue;
            };
            let bit = ncname_first_char_bit(first);
            if bit == 0 {
                continue;
            }
            parsed.first_char_mask |= bit;
            if parsed.len < MAX_STACK_QUARANTINE_TAGS {
                parsed.tags[parsed.len] = trimmed;
                parsed.len += 1;
            } else {
                parsed.overflow_spec = Some(spec);
            }
        }
        parsed
    }

    #[inline]
    fn match_at(&self, bytes: &[u8], i: usize) -> Option<usize> {
        if self.first_char_mask == 0 {
            return None;
        }
        let pos = quarantine_tag_name_start(bytes, i);
        let &first_b = bytes.get(pos)?;
        if (self.first_char_mask & ncname_first_char_bit(first_b)) == 0 {
            return None;
        }
        let first_lower = first_b | 0x20;
        for &tag_bytes in &self.tags[..self.len] {
            if (tag_bytes[0] | 0x20) == first_lower
                && let Some(after) = match_quarantine_tag_tail_at(bytes, pos, tag_bytes)
            {
                return Some(after);
            }
        }
        if let Some(spec) = self.overflow_spec {
            for part in spec.split(',').skip(MAX_STACK_QUARANTINE_TAGS) {
                let tag_bytes = part.trim().as_bytes();
                if let Some(&tb0) = tag_bytes.first()
                    && (tb0 | 0x20) == first_lower
                    && let Some(after) = match_quarantine_tag_tail_at(bytes, pos, tag_bytes)
                {
                    return Some(after);
                }
            }
        }
        None
    }
}

/// Advance past `<`, optional ASCII whitespace, optional `/`, and optional ASCII whitespace,
/// returning the byte index where the XML tag name begins.
#[inline]
fn quarantine_tag_name_start(bytes: &[u8], i: usize) -> usize {
    let mut pos = i + 1;
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    if pos < bytes.len() && bytes[pos] == b'/' {
        pos += 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
    }
    pos
}

/// Check if `bytes[pos..]` matches `tag_bytes` (ASCII case-insensitively) and is not
/// followed by an XML `NCName` continuation byte.
#[inline]
fn match_quarantine_tag_tail_at(bytes: &[u8], pos: usize, tag_bytes: &[u8]) -> Option<usize> {
    let after_tag = pos.checked_add(tag_bytes.len())?;
    if after_tag <= bytes.len()
        && bytes[pos..after_tag].eq_ignore_ascii_case(tag_bytes)
        && !(after_tag < bytes.len() && is_ncname_continue_byte(bytes[after_tag]))
    {
        Some(after_tag)
    } else {
        None
    }
}

#[inline]
fn has_breakout_clean(
    bytes: &[u8],
    rules: &[(&'static str, &'static str)],
    parsed_tags: &ParsedTagSpec<'_>,
) -> bool {
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'<' || b == b'[' || b == b'\n' {
            if !rules.is_empty() && match_control_token_at(bytes, i, rules).is_some() {
                return true;
            }
            if b == b'<' && parsed_tags.match_at(bytes, i).is_some() {
                return true;
            }
        }
    }
    false
}

#[inline]
fn has_breakout_with_rules(
    s: &str,
    rules: &[(&'static str, &'static str)],
    tag_spec: Option<&str>,
) -> bool {
    let bytes = s.as_bytes();
    let parsed_tags = ParsedTagSpec::from_opt(tag_spec);
    let mut checked_invisible = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'<' || b == b'[' || b == b'\n' {
            if !rules.is_empty() && match_control_token_at(bytes, i, rules).is_some() {
                return true;
            }
            if b == b'<' && parsed_tags.match_at(bytes, i).is_some() {
                return true;
            }
        } else if !checked_invisible && crate::filter::invisible::is_invisible_lead_byte(b) {
            checked_invisible = true;
            if let Cow::Owned(stripped) = crate::filter::invisible::strip_invisible_str(s) {
                return !rules.is_empty()
                    || has_breakout_clean(stripped.as_bytes(), rules, &parsed_tags);
            }
        }
    }
    false
}

#[inline]
fn sanitize_with_rules_clean<'a>(
    s: &'a str,
    rules: &[(&'static str, &'static str)],
    parsed_tags: &ParsedTagSpec<'_>,
) -> Cow<'a, str> {
    let bytes = s.as_bytes();
    let mut out: Option<String> = None;
    let mut last_copied = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'<' || b == b'[' || b == b'\n' {
            if b == b'<'
                && let Some(after_tag) = parsed_tags.match_at(bytes, i)
            {
                let buf = out.get_or_insert_with(|| {
                    String::with_capacity(s.len() + SANITIZE_EXTRA_CAPACITY)
                });
                buf.push_str(&s[last_copied..i]);
                let mut j = after_tag;
                while j < bytes.len() && bytes[j] != b'>' && bytes[j] != b'<' {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'>' {
                    buf.push_str(ESCAPED_LT);
                    let inner = &s[i + 1..j];
                    if rules.is_empty() {
                        buf.push_str(inner);
                    } else {
                        let empty_tags = ParsedTagSpec::from_opt(None);
                        buf.push_str(&sanitize_with_rules_clean(inner, rules, &empty_tags));
                    }
                    buf.push_str(ESCAPED_GT);
                    i = j + 1;
                    last_copied = i;
                    continue;
                }
                buf.push_str(ESCAPED_LT);
                i += 1;
                last_copied = i;
                continue;
            }
            if !rules.is_empty()
                && let Some(matched) = match_control_token_at(bytes, i, rules)
            {
                let buf = out.get_or_insert_with(|| {
                    String::with_capacity(s.len() + SANITIZE_EXTRA_CAPACITY)
                });
                buf.push_str(&s[last_copied..i]);
                match matched {
                    ControlTokenMatch::Rule { len, replacement } => {
                        buf.push_str(replacement);
                        i += len;
                    }
                    ControlTokenMatch::GenericPipe { end_idx } => {
                        buf.push_str(ESCAPED_LT);
                        let empty_tags = ParsedTagSpec::from_opt(None);
                        buf.push_str(&sanitize_with_rules_clean(
                            &s[i + 1..end_idx - 1],
                            rules,
                            &empty_tags,
                        ));
                        buf.push_str(ESCAPED_GT);
                        i = end_idx;
                    }
                }
                last_copied = i;
                continue;
            }
        }
        i += 1;
    }
    match out {
        Some(mut buf) => {
            buf.push_str(&s[last_copied..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(s),
    }
}

#[inline]
fn sanitize_with_rules<'a>(
    s: &'a str,
    rules: &[(&'static str, &'static str)],
    tag_spec: Option<&str>,
) -> Cow<'a, str> {
    let bytes = s.as_bytes();
    let parsed_tags = ParsedTagSpec::from_opt(tag_spec);
    let mut out: Option<String> = None;
    let mut last_copied = 0usize;
    let mut checked_invisible = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'<' || b == b'[' || b == b'\n' {
            if b == b'<'
                && let Some(after_tag) = parsed_tags.match_at(bytes, i)
            {
                let buf = out.get_or_insert_with(|| {
                    String::with_capacity(s.len() + SANITIZE_EXTRA_CAPACITY)
                });
                buf.push_str(&s[last_copied..i]);
                let mut j = after_tag;
                while j < bytes.len() && bytes[j] != b'>' && bytes[j] != b'<' {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'>' {
                    buf.push_str(ESCAPED_LT);
                    let inner = &s[i + 1..j];
                    if rules.is_empty() {
                        buf.push_str(inner);
                    } else {
                        buf.push_str(&sanitize_with_rules(inner, rules, None));
                    }
                    buf.push_str(ESCAPED_GT);
                    i = j + 1;
                    last_copied = i;
                    continue;
                }
                buf.push_str(ESCAPED_LT);
                i += 1;
                last_copied = i;
                continue;
            }
            if !rules.is_empty()
                && let Some(matched) = match_control_token_at(bytes, i, rules)
            {
                let buf = out.get_or_insert_with(|| {
                    String::with_capacity(s.len() + SANITIZE_EXTRA_CAPACITY)
                });
                buf.push_str(&s[last_copied..i]);
                match matched {
                    ControlTokenMatch::Rule { len, replacement } => {
                        buf.push_str(replacement);
                        i += len;
                    }
                    ControlTokenMatch::GenericPipe { end_idx } => {
                        buf.push_str(ESCAPED_LT);
                        buf.push_str(&sanitize_with_rules(&s[i + 1..end_idx - 1], rules, None));
                        buf.push_str(ESCAPED_GT);
                        i = end_idx;
                    }
                }
                last_copied = i;
                continue;
            }
        } else if !checked_invisible && crate::filter::invisible::is_invisible_lead_byte(b) {
            checked_invisible = true;
            if let Cow::Owned(stripped) = crate::filter::invisible::strip_invisible_str(s) {
                return Cow::Owned(
                    match sanitize_with_rules_clean(&stripped, rules, &parsed_tags) {
                        Cow::Borrowed(_) => stripped,
                        Cow::Owned(sanitized) => sanitized,
                    },
                );
            }
        }
        i += 1;
    }
    match out {
        Some(mut buf) => {
            buf.push_str(&s[last_copied..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(s),
    }
}

/// Returns `true` if `s` contains any known LLM control token in [`TOKEN_DELIMITERS`]
/// or any generic `<|...|>` / `<｜...｜>` special control token.
#[must_use]
pub fn has_control_tokens(s: &str) -> bool {
    has_breakout_with_rules(s, TOKEN_DELIMITERS, None)
}

/// Returns `true` if `s` contains any role/turn/special token in [`ROLE_TOKEN_DELIMITERS`]
/// or any generic `<|...|>` / `<｜...｜>` special control token.
#[must_use]
pub fn has_role_control_tokens(s: &str) -> bool {
    has_breakout_with_rules(s, ROLE_TOKEN_DELIMITERS, None)
}

/// Neutralize LLM control tokens, chat turn delimiters, and `<|...|>` / `<｜...｜>`
/// special tokens in a single pass, returning [`Cow::Borrowed`] when `s` is clean.
#[must_use]
pub fn sanitize_tokens_str(s: &str) -> Cow<'_, str> {
    sanitize_with_rules(s, TOKEN_DELIMITERS, None)
}

/// Neutralize role/turn headers ([`ROLE_TOKEN_DELIMITERS`]) and `<|...|>` / `<｜...｜>`
/// special tokens in a single pass while preserving XML tool/reasoning documentation tags,
/// returning [`Cow::Borrowed`] when `s` is clean.
#[must_use]
pub fn sanitize_role_tokens_str(s: &str) -> Cow<'_, str> {
    sanitize_with_rules(s, ROLE_TOKEN_DELIMITERS, None)
}

/// Apply `sanitize_tokens` (or `sanitize_tokens("tag1,tag2")`) to a string slice,
/// returning [`Cow::Borrowed`] when `s` is clean.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if `tag_spec` is provided and any tag name
/// is not a valid ASCII XML `NCName`.
pub(in crate::filter) fn sanitize_tokens_with_args<'a>(
    s: &'a str,
    tag_spec: Option<&str>,
) -> Result<Cow<'a, str>, TemplateError> {
    match tag_spec {
        None => Ok(sanitize_tokens_str(s)),
        Some(raw_arg) => {
            let tag_cow = strip_quotes(raw_arg);
            let (_, valid_spec) =
                validate_tag_spec_for_filter(&tag_cow, crate::consts::FILTER_SANITIZE_TOKENS)?;
            Ok(match sanitize_untrusted_str(s, valid_spec) {
                Cow::Borrowed(_) => Cow::Borrowed(s),
                Cow::Owned(sanitized) => Cow::Owned(sanitized),
            })
        }
    }
}

/// Neutralize LLM control tokens, chat delimiters, and optional XML boundary tags in a [`Value`].
pub(in crate::filter) fn apply_sanitize_tokens(
    value: &Value,
    args: Option<&str>,
) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => match sanitize_tokens_with_args(s, args)? {
            Cow::Borrowed(_) => Ok(value.clone()),
            Cow::Owned(sanitized) => Ok(Value::Str(sanitized)),
        },
        _ => Err(TemplateError::syntax("'sanitize_tokens' requires a string")),
    }
}

/// Append `s` wrapped in Markdown code fences with adaptive backtick counts directly into `buf`.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if `lang` contains backticks or whitespace.
pub(crate) fn fence_into(
    s: &str,
    lang: Option<&str>,
    buf: &mut String,
) -> Result<(), TemplateError> {
    let lang_cow = strip_quotes(lang.unwrap_or(""));
    if lang_cow.chars().any(|c| c == '`' || c.is_whitespace()) {
        return Err(TemplateError::syntax(alloc::format!(
            "'fence' language must not contain backticks or whitespace: '{lang_cow}'"
        )));
    }
    let mut max_backticks = 0usize;
    let mut current_backticks = 0usize;
    for b in s.bytes() {
        if b == b'`' {
            current_backticks += 1;
            if current_backticks > max_backticks {
                max_backticks = current_backticks;
            }
        } else {
            current_backticks = 0;
        }
    }
    let fence_len = core::cmp::max(MIN_FENCE_BACKTICKS, max_backticks + 1);
    buf.reserve(s.len() + fence_len * 2 + lang_cow.len() + 2);
    for _ in 0..fence_len {
        buf.push('`');
    }
    buf.push_str(&lang_cow);
    buf.push('\n');
    buf.push_str(s);
    if !s.ends_with('\n') {
        buf.push('\n');
    }
    for _ in 0..fence_len {
        buf.push('`');
    }
    Ok(())
}

/// Wrap a string slice in Markdown code fences with adaptive backtick counts.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if `lang` contains backticks or whitespace.
pub fn fence_str(s: &str, lang: Option<&str>) -> Result<String, TemplateError> {
    let mut buf = String::new();
    fence_into(s, lang, &mut buf)?;
    Ok(buf)
}

/// Wrap a string value in markdown code fences with adaptive backtick counts.
pub(in crate::filter) fn apply_fence(
    value: &Value,
    args: Option<&str>,
) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => Ok(Value::Str(fence_str(s, args)?)),
        _ => Err(TemplateError::syntax("'fence' requires a string")),
    }
}

/// Returns `true` if `s` contains any opening or closing XML tag matching `tag_spec`
/// (a single `NCName` or comma-separated `NCName`s, matched ASCII case-insensitively
/// with optional whitespace after `<` or `</`).
#[must_use]
pub fn has_quarantine_tag_breakout(s: &str, tag_spec: &str) -> bool {
    has_breakout_with_rules(s, &[], Some(tag_spec))
}

/// Returns `true` if `s` contains any known LLM control token ([`TOKEN_DELIMITERS`]),
/// generic `<|...|>` / `<｜...｜>` special control token, or opening/closing XML tag
/// matching `tag_spec` in a single byte-cursor pass.
#[must_use]
pub fn has_untrusted_breakout(s: &str, tag_spec: &str) -> bool {
    has_breakout_with_rules(s, TOKEN_DELIMITERS, Some(tag_spec))
}

/// Sanitize untrusted content within quarantine boundaries for one or more
/// comma-separated XML tag names, returning [`Cow::Borrowed`] when no matching
/// tags are present.
#[must_use]
pub fn sanitize_quarantine_payload<'a>(s: &'a str, tag_spec: &str) -> Cow<'a, str> {
    sanitize_with_rules(s, &[], Some(tag_spec))
}

/// Neutralize all known LLM control tokens ([`TOKEN_DELIMITERS`]), generic
/// `<|...|>` / `<｜...｜>` special tokens, and opening/closing XML boundary tags
/// matching `tag_spec` in a single pass, returning [`Cow::Borrowed`] when `s` is clean.
#[must_use]
pub fn sanitize_untrusted_str<'a>(s: &'a str, tag_spec: &str) -> Cow<'a, str> {
    sanitize_with_rules(s, TOKEN_DELIMITERS, Some(tag_spec))
}

fn write_quarantine_envelope(primary_tag: &str, sanitized: &str, buf: &mut String) {
    buf.reserve(sanitized.len() + primary_tag.len() * 2 + 7);
    buf.push('<');
    buf.push_str(primary_tag);
    buf.push_str(">\n");
    buf.push_str(sanitized);
    if !sanitized.ends_with('\n') {
        buf.push('\n');
    }
    buf.push_str("</");
    buf.push_str(primary_tag);
    buf.push('>');
}

fn quarantine_with_rules_into(
    s: &str,
    rules: &[(&'static str, &'static str)],
    tag_spec: Option<&str>,
    buf: &mut String,
) -> Result<(), TemplateError> {
    let tag_cow = match tag_spec {
        Some(raw_arg) => strip_quotes(raw_arg),
        None => Cow::Borrowed(DEFAULT_QUARANTINE_TAG),
    };
    let (primary_tag, valid_spec) = validate_quarantine_tag_spec(&tag_cow)?;
    let sanitized = sanitize_with_rules(s, rules, Some(valid_spec));
    write_quarantine_envelope(primary_tag, &sanitized, buf);
    Ok(())
}

/// Append `s` wrapped in boundary XML tags (`<primary_tag>\n...\n</primary_tag>`) directly into `buf`,
/// escaping any embedded opening/closing tags matching `tag_spec`.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
pub(crate) fn quarantine_into(
    s: &str,
    tag_spec: Option<&str>,
    buf: &mut String,
) -> Result<(), TemplateError> {
    quarantine_with_rules_into(s, &[], tag_spec, buf)
}

/// Append `s` wrapped in boundary XML tags (`<primary_tag>\n...\n</primary_tag>`) directly into `buf`,
/// neutralizing control tokens ([`TOKEN_DELIMITERS`]) and boundary XML tags (`tag_spec`) in a single pass.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
pub(crate) fn quarantine_untrusted_into(
    s: &str,
    tag_spec: Option<&str>,
    buf: &mut String,
) -> Result<(), TemplateError> {
    quarantine_with_rules_into(s, TOKEN_DELIMITERS, tag_spec, buf)
}

/// Wrap a string slice in boundary XML tags (`<primary_tag>\n...\n</primary_tag>`),
/// escaping any embedded opening/closing tags matching `tag_spec` (which may be a
/// single `NCName` or comma-separated `NCName`s like `"untrusted_tool_output,event"`).
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
pub fn quarantine_str(s: &str, tag_spec: Option<&str>) -> Result<String, TemplateError> {
    let mut buf = String::new();
    quarantine_into(s, tag_spec, &mut buf)?;
    Ok(buf)
}

/// Sanitize both control tokens ([`TOKEN_DELIMITERS`] + generic pipe tokens) and
/// boundary XML tags (`tag_spec`) in a single pass and wrap the result in
/// `<primary_tag>\n...\n</primary_tag>`.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
pub fn quarantine_untrusted_str(s: &str, tag_spec: Option<&str>) -> Result<String, TemplateError> {
    let mut buf = String::new();
    quarantine_untrusted_into(s, tag_spec, &mut buf)?;
    Ok(buf)
}

/// Returns `true` if `s` is already wrapped in `<primary_tag>...</primary_tag>`
/// and its inner payload contains neither unescaped tags from `tag_spec` nor
/// any un-neutralized control tokens ([`has_untrusted_breakout`]).
#[must_use]
pub fn is_quarantined_str(s: &str, tag_spec: Option<&str>) -> bool {
    let tag_cow = match tag_spec {
        Some(raw_arg) => strip_quotes(raw_arg),
        None => Cow::Borrowed(DEFAULT_QUARANTINE_TAG),
    };
    let Ok((primary_tag, valid_spec)) = validate_quarantine_tag_spec(&tag_cow) else {
        return false;
    };
    let Some(rest) = s
        .strip_prefix('<')
        .and_then(|r| r.strip_prefix(primary_tag))
        .and_then(|r| r.strip_prefix('>'))
    else {
        return false;
    };
    let Some(body) = rest
        .strip_suffix('>')
        .and_then(|r| r.strip_suffix(primary_tag))
        .and_then(|r| r.strip_suffix("</"))
    else {
        return false;
    };
    !has_untrusted_breakout(body, valid_spec)
}

/// Wrap a string value in boundary XML tags, escaping any embedded opening/closing tags.
pub(in crate::filter) fn apply_quarantine(
    value: &Value,
    args: Option<&str>,
) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => Ok(Value::Str(quarantine_str(s, args)?)),
        _ => Err(TemplateError::syntax("'quarantine' requires a string")),
    }
}
