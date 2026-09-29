//! Security, escaping, token-sanitization, code-fencing, and XML-quarantine primitives.

use alloc::{borrow::Cow, string::String};

use super::strip_quotes;
use crate::{error::TemplateError, value::Value};

/// Default tag name used when `quarantine` filter is called with no arguments.
pub const DEFAULT_QUARANTINE_TAG: &str = "untrusted_content";
/// Minimum number of backticks for code fences.
const MIN_FENCE_BACKTICKS: usize = 3;
/// Minimum byte length of any rule in [`TOKEN_DELIMITERS`] (`"</s>"`, `4` bytes).
pub(super) const MIN_RULE_LEN: usize = 4;
/// Maximum inner byte length for generic `<|...|>` / `<｜...｜>` special control tokens.
pub(super) const MAX_PIPE_TOKEN_INNER_LEN: usize = 64;
/// Extra byte capacity reserved when first allocating a sanitized string buffer.
const SANITIZE_EXTRA_CAPACITY: usize = 16;
/// Maximum number of comma-separated quarantine tags stored inline on the stack.
const MAX_STACK_QUARANTINE_TAGS: usize = 64;
/// Predefined XML entity for `<`.
const ESCAPED_LT: &str = "&lt;";
/// Predefined XML entity for `>`.
const ESCAPED_GT: &str = "&gt;";
/// UTF-8 prefix for `<｜` (ASCII `<` followed by fullwidth `｜` `U+FF5C`).
const FULLWIDTH_PIPE_OPEN: &str = "<｜";
/// UTF-8 suffix for `｜>` (fullwidth `｜` `U+FF5C` followed by ASCII `>`).
const FULLWIDTH_PIPE_CLOSE: &str = "｜>";

/// Number of leading entries in [`TOKEN_DELIMITERS`] that represent role/turn headers and
/// special control tokens (as opposed to XML tool-schema/reasoning documentation tags).
pub(super) const ROLE_DELIMITER_COUNT: usize = 79;

/// Well-known LLM and template control token pairs across all supported dialects:
/// (`raw_token`, `sanitized_replacement`).
///
/// Ordered so the leading [`ROLE_TOKEN_DELIMITERS`] slice covers role/turn headers and special
/// control tokens, followed by XML tool-calling, reasoning, and quarantine envelope tags.
pub const TOKEN_DELIMITERS: &[(&str, &str)] = &[
    // ChatML / OpenAI / Qwen
    ("<|im_start|>", "&lt;|im_start|&gt;"),
    ("<|im_end|>", "&lt;|im_end|&gt;"),
    ("<|endoftext|>", "&lt;|endoftext|&gt;"),
    // Llama 3
    ("<|start_header_id|>", "&lt;|start_header_id|&gt;"),
    ("<|end_header_id|>", "&lt;|end_header_id|&gt;"),
    ("<|eot_id|>", "&lt;|eot_id|&gt;"),
    // Llama 2
    ("[INST]", "&#91;INST&#93;"),
    ("[/INST]", "&#91;/INST&#93;"),
    ("<<SYS>>", "&lt;&lt;SYS&gt;&gt;"),
    ("<</SYS>>", "&lt;&lt;/SYS&gt;&gt;"),
    ("</s>", "&lt;/s&gt;"),
    // Gemma 2 & Gemma 4
    ("<start_of_turn>", "&lt;start_of_turn&gt;"),
    ("<end_of_turn>", "&lt;end_of_turn&gt;"),
    ("<|turn>", "&lt;|turn&gt;"),
    ("<turn|>", "&lt;turn|&gt;"),
    ("<|tool>", "&lt;|tool&gt;"),
    ("<tool|>", "&lt;tool|&gt;"),
    ("<|tool_call>", "&lt;|tool_call&gt;"),
    ("<tool_call|>", "&lt;tool_call|&gt;"),
    ("<|tool_response>", "&lt;|tool_response&gt;"),
    ("<tool_response|>", "&lt;tool_response|&gt;"),
    ("<|thought", "&lt;|thought"),
    ("<thought|>", "&lt;thought|&gt;"),
    ("<|think|>", "&lt;|think|&gt;"),
    ("<|\"|>", "&lt;|\"|&gt;"),
    ("<bos>", "&lt;bos&gt;"),
    ("<eos>", "&lt;eos&gt;"),
    // T5 / CodeGemma / ChatGLM / GLM-4 / Cohere / Yi / InternLM
    ("<extra_id_", "&lt;extra_id_"),
    ("[gMASK]", "&#91;gMASK&#93;"),
    ("<sop>", "&lt;sop&gt;"),
    ("<eop>", "&lt;eop&gt;"),
    ("<SPECIAL_", "&lt;SPECIAL_"),
    ("<beginning_of_sentence>", "&lt;beginning_of_sentence&gt;"),
    ("<end_of_sentence>", "&lt;end_of_sentence&gt;"),
    // DeepSeek V3
    ("<｜begin▁of▁sentence｜>", "&lt;｜begin▁of▁sentence｜&gt;"),
    ("<｜end▁of▁sentence｜>", "&lt;｜end▁of▁sentence｜&gt;"),
    ("<｜begin▁of▁thought｜>", "&lt;｜begin▁of▁thought｜&gt;"),
    ("<｜end▁of▁thought｜>", "&lt;｜end▁of▁thought｜&gt;"),
    ("<｜User｜>", "&lt;｜User｜&gt;"),
    ("<｜Assistant｜>", "&lt;｜Assistant｜&gt;"),
    ("<｜tool▁calls▁begin｜>", "&lt;｜tool▁calls▁begin｜&gt;"),
    ("<｜tool▁calls▁end｜>", "&lt;｜tool▁calls▁end｜&gt;"),
    ("<｜tool▁call▁begin｜>", "&lt;｜tool▁call▁begin｜&gt;"),
    ("<｜tool▁call▁end｜>", "&lt;｜tool▁call▁end｜&gt;"),
    ("<｜tool▁sep｜>", "&lt;｜tool▁sep｜&gt;"),
    ("<｜tool▁outputs▁begin｜>", "&lt;｜tool▁outputs▁begin｜&gt;"),
    ("<｜tool▁outputs▁end｜>", "&lt;｜tool▁outputs▁end｜&gt;"),
    ("<｜tool▁output▁begin｜>", "&lt;｜tool▁output▁begin｜&gt;"),
    ("<｜tool▁output▁end｜>", "&lt;｜tool▁output▁end｜&gt;"),
    // Phi-3/4, Command-R, Bailing & Harmony
    ("<|user|>", "&lt;|user|&gt;"),
    ("<|assistant|>", "&lt;|assistant|&gt;"),
    ("<|system|>", "&lt;|system|&gt;"),
    ("<|end|>", "&lt;|end|&gt;"),
    ("<|start|>", "&lt;|start|&gt;"),
    ("<|channel|>", "&lt;|channel|&gt;"),
    ("<|message|>", "&lt;|message|&gt;"),
    ("<|call|>", "&lt;|call|&gt;"),
    ("<|return|>", "&lt;|return|&gt;"),
    ("<|constrain|>", "&lt;|constrain|&gt;"),
    ("<|role_end|>", "&lt;|role_end|&gt;"),
    ("<|START_OF_TURN_TOKEN|>", "&lt;|START_OF_TURN_TOKEN|&gt;"),
    ("<|END_OF_TURN_TOKEN|>", "&lt;|END_OF_TURN_TOKEN|&gt;"),
    ("<role>", "&lt;role&gt;"),
    ("</role>", "&lt;/role&gt;"),
    // Mistral & Ministral
    ("[SYSTEM_PROMPT]", "&#91;SYSTEM_PROMPT&#93;"),
    ("[/SYSTEM_PROMPT]", "&#91;/SYSTEM_PROMPT&#93;"),
    ("[TOOL_CALLS]", "&#91;TOOL_CALLS&#93;"),
    ("[AVAILABLE_TOOLS]", "&#91;AVAILABLE_TOOLS&#93;"),
    ("[/TOOL_CALLS]", "&#91;/TOOL_CALLS&#93;"),
    ("[/AVAILABLE_TOOLS]", "&#91;/AVAILABLE_TOOLS&#93;"),
    ("[TOOL_RESULTS]", "&#91;TOOL_RESULTS&#93;"),
    ("[/TOOL_RESULTS]", "&#91;/TOOL_RESULTS&#93;"),
    ("[TOOL_CONTENT]", "&#91;TOOL_CONTENT&#93;"),
    ("[ARGS]", "&#91;ARGS&#93;"),
    ("[/ARGS]", "&#91;/ARGS&#93;"),
    ("[THINK]", "&#91;THINK&#93;"),
    ("[/THINK]", "&#91;/THINK&#93;"),
    // Anthropic
    ("\n\nHuman:", "\n\nHuman&#58;"),
    ("\n\nAssistant:", "\n\nAssistant&#58;"),
    // Tool Calling & Qwen3-Coder / Anthropic XML (entries 79..102, excluded from ROLE_TOKEN_DELIMITERS)
    ("<tool_call>", "&lt;tool_call&gt;"),
    ("</tool_call>", "&lt;/tool_call&gt;"),
    ("<tool_response>", "&lt;tool_response&gt;"),
    ("</tool_response>", "&lt;/tool_response&gt;"),
    ("<function=", "&lt;function="),
    ("</function>", "&lt;/function&gt;"),
    ("<function>", "&lt;function&gt;"),
    ("<parameter=", "&lt;parameter="),
    ("</parameter>", "&lt;/parameter&gt;"),
    ("<tools>", "&lt;tools&gt;"),
    ("</tools>", "&lt;/tools&gt;"),
    ("<function_calls>", "&lt;function_calls&gt;"),
    ("</function_calls>", "&lt;/function_calls&gt;"),
    ("<function_results>", "&lt;function_results&gt;"),
    ("</function_results>", "&lt;/function_results&gt;"),
    // Reasoning XML
    ("<think>", "&lt;think&gt;"),
    ("</think>", "&lt;/think&gt;"),
    // Quarantine envelopes
    ("<untrusted_tool_output>", "&lt;untrusted_tool_output&gt;"),
    ("</untrusted_tool_output>", "&lt;/untrusted_tool_output&gt;"),
    ("<tool_output_quarantine>", "&lt;tool_output_quarantine&gt;"),
    (
        "</tool_output_quarantine>",
        "&lt;/tool_output_quarantine&gt;",
    ),
    ("<untrusted_content>", "&lt;untrusted_content&gt;"),
    ("</untrusted_content>", "&lt;/untrusted_content&gt;"),
];

/// Subset of [`TOKEN_DELIMITERS`] covering role/turn headers and special control tokens,
/// excluding XML tool-schema and reasoning documentation tags (`<tools>`, `<tool_call>`,
/// `<tool_response>`, `<function=`, `<parameter=`, `<think>`, `<untrusted_tool_output>`, etc.)
/// that legitimately appear in trusted system prompts.
pub const ROLE_TOKEN_DELIMITERS: &[(&str, &str)] =
    TOKEN_DELIMITERS.split_at(ROLE_DELIMITER_COUNT).0;

/// Subset of `[` bracket control-token rules (all located within [`ROLE_TOKEN_DELIMITERS`]).
const BRACKET_TOKEN_RULES: &[(&str, &str)] = &[
    ("[INST]", "&#91;INST&#93;"),
    ("[/INST]", "&#91;/INST&#93;"),
    ("[gMASK]", "&#91;gMASK&#93;"),
    ("[SYSTEM_PROMPT]", "&#91;SYSTEM_PROMPT&#93;"),
    ("[/SYSTEM_PROMPT]", "&#91;/SYSTEM_PROMPT&#93;"),
    ("[TOOL_CALLS]", "&#91;TOOL_CALLS&#93;"),
    ("[AVAILABLE_TOOLS]", "&#91;AVAILABLE_TOOLS&#93;"),
    ("[/TOOL_CALLS]", "&#91;/TOOL_CALLS&#93;"),
    ("[/AVAILABLE_TOOLS]", "&#91;/AVAILABLE_TOOLS&#93;"),
    ("[TOOL_RESULTS]", "&#91;TOOL_RESULTS&#93;"),
    ("[/TOOL_RESULTS]", "&#91;/TOOL_RESULTS&#93;"),
    ("[TOOL_CONTENT]", "&#91;TOOL_CONTENT&#93;"),
    ("[ARGS]", "&#91;ARGS&#93;"),
    ("[/ARGS]", "&#91;/ARGS&#93;"),
    ("[THINK]", "&#91;THINK&#93;"),
    ("[/THINK]", "&#91;/THINK&#93;"),
];

#[inline]
const fn is_bracket_second_byte(b1: u8) -> bool {
    matches!(b1, b'I' | b'/' | b'g' | b'S' | b'T' | b'A')
}

#[inline]
const fn is_angle_second_byte(b1: u8) -> bool {
    matches!(
        b1,
        b'|' | 0xEF | b'/' | b'<' | b's' | b'e' | b't' | b'b' | b'S' | b'r' | b'f' | b'p' | b'u'
    )
}

const _: () = {
    assert!(ROLE_DELIMITER_COUNT <= TOKEN_DELIMITERS.len());
    let mut idx = 0;
    let mut bracket_count = 0;
    let mut newline_count = 0;
    while idx < TOKEN_DELIMITERS.len() {
        let (raw, escaped) = TOKEN_DELIMITERS[idx];
        assert!(raw.len() >= MIN_RULE_LEN);
        assert!(escaped.len() >= MIN_RULE_LEN);
        let b0 = raw.as_bytes()[0];
        let b1 = raw.as_bytes()[1];
        match b0 {
            b'<' => assert!(is_angle_second_byte(b1)),
            b'[' => {
                assert!(idx < ROLE_DELIMITER_COUNT);
                assert!(is_bracket_second_byte(b1));
                bracket_count += 1;
            }
            b'\n' => {
                assert!(idx < ROLE_DELIMITER_COUNT);
                assert!(b1 == b'\n');
                newline_count += 1;
            }
            _ => panic!("unexpected rule start byte"),
        }
        idx += 1;
    }
    assert!(bracket_count == BRACKET_TOKEN_RULES.len());
    assert!(newline_count == 2);
};

/// Escape XML/HTML special characters in a string slice in a single pass,
/// returning [`Cow::Borrowed`] when no escaping or control-character stripping is required.
#[must_use]
pub fn escape_xml_str(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    let mut out: Option<String> = None;
    let mut last_copied = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        let replacement = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&apos;",
            0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F => "",
            _ => continue,
        };
        let buf = out.get_or_insert_with(|| String::with_capacity(s.len() + 8));
        buf.push_str(&s[last_copied..i]);
        buf.push_str(replacement);
        last_copied = i + 1;
    }
    match out {
        Some(mut buf) => {
            buf.push_str(&s[last_copied..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(s),
    }
}

/// Escape XML/HTML special characters in a string [`Value`].
pub(super) fn apply_escape_xml(value: &Value) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => match escape_xml_str(s) {
            Cow::Borrowed(_) => Ok(value.clone()),
            Cow::Owned(escaped) => Ok(Value::Str(escaped)),
        },
        _ => Err(TemplateError::syntax("'escape_xml' requires a string")),
    }
}

/// Escape JSON string body characters in a string slice in a single pass,
/// returning [`Cow::Borrowed`] when no escaping is required.
#[must_use]
pub fn escape_json_str(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    let mut out: Option<String> = None;
    let mut last_copied = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        let replacement = match b {
            b'\\' => Some("\\\\"),
            b'"' => Some("\\\""),
            b'/' => Some("\\/"),
            b'\n' => Some("\\n"),
            b'\r' => Some("\\r"),
            b'\t' => Some("\\t"),
            0x08 => Some("\\b"),
            0x0C => Some("\\f"),
            0x00..=0x1F => None,
            0xE2 if i + 2 < bytes.len() && bytes[i + 1] == 0x80 => match bytes[i + 2] {
                0xA8 => {
                    let buf = out.get_or_insert_with(|| String::with_capacity(s.len() + 8));
                    buf.push_str(&s[last_copied..i]);
                    buf.push_str("\\u2028");
                    i += 3;
                    last_copied = i;
                    continue;
                }
                0xA9 => {
                    let buf = out.get_or_insert_with(|| String::with_capacity(s.len() + 8));
                    buf.push_str(&s[last_copied..i]);
                    buf.push_str("\\u2029");
                    i += 3;
                    last_copied = i;
                    continue;
                }
                _ => {
                    i += 1;
                    continue;
                }
            },
            _ => {
                i += 1;
                continue;
            }
        };
        let buf = out.get_or_insert_with(|| String::with_capacity(s.len() + 8));
        buf.push_str(&s[last_copied..i]);
        if let Some(rep) = replacement {
            buf.push_str(rep);
        } else {
            use core::fmt::Write;
            write!(buf, "\\u{b:04x}").expect("fmt::Write to String is infallible");
        }
        i += 1;
        last_copied = i;
    }
    match out {
        Some(mut buf) => {
            buf.push_str(&s[last_copied..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(s),
    }
}

/// Escape JSON string characters in a string [`Value`].
pub(super) fn apply_escape_json(value: &Value) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => match escape_json_str(s) {
            Cow::Borrowed(_) => Ok(value.clone()),
            Cow::Owned(escaped) => Ok(Value::Str(escaped)),
        },
        _ => Err(TemplateError::syntax("'escape_json' requires a string")),
    }
}

/// Check if `bytes[i..]` starts a generic `<|...|>` or `<｜...｜>` special control token,
/// returning `Some(end_byte_idx)` (exclusive) if matched within [`MAX_PIPE_TOKEN_INNER_LEN`] bytes.
#[inline]
fn match_generic_pipe_token(bytes: &[u8], i: usize) -> Option<usize> {
    let rest = bytes.get(i..)?;
    let (open_len, close_bytes) = if rest.starts_with(b"<|") {
        (2, b"|>" as &[u8])
    } else if rest.starts_with(FULLWIDTH_PIPE_OPEN.as_bytes()) {
        (FULLWIDTH_PIPE_OPEN.len(), FULLWIDTH_PIPE_CLOSE.as_bytes())
    } else {
        return None;
    };
    let after_open = rest.get(open_len..)?;
    let max_scan = core::cmp::min(
        after_open.len(),
        MAX_PIPE_TOKEN_INNER_LEN + close_bytes.len(),
    );
    let window = &after_open[..max_scan];
    let mut k = 0usize;
    while k + close_bytes.len() <= window.len() {
        if window[k..].starts_with(close_bytes) {
            return if k > 0 {
                Some(i + open_len + k + close_bytes.len())
            } else {
                None
            };
        }
        let b = window[k];
        if k == MAX_PIPE_TOKEN_INNER_LEN || b.is_ascii_whitespace() || b == b'<' || b == b'>' {
            return None;
        }
        k += 1;
    }
    None
}

enum ControlTokenMatch {
    Rule {
        len: usize,
        replacement: &'static str,
    },
    GenericPipe {
        end_idx: usize,
    },
}

/// Single-pass matcher at byte offset `i` for either an explicit rule in `rules`
/// or a generic `<|...|>` / `<｜...｜>` pipe-delimited special token.
#[inline]
fn match_control_token_at(
    bytes: &[u8],
    i: usize,
    rules: &[(&'static str, &'static str)],
) -> Option<ControlTokenMatch> {
    if i + MIN_RULE_LEN > bytes.len() {
        return None;
    }
    let b0 = bytes[i];
    let b1 = bytes[i + 1];
    match b0 {
        b'\n' => {
            if b1 == b'\n' {
                let tail = &bytes[i..];
                if tail.starts_with(b"\n\nHuman:") {
                    return Some(ControlTokenMatch::Rule {
                        len: 8,
                        replacement: "\n\nHuman&#58;",
                    });
                }
                if tail.starts_with(b"\n\nAssistant:") {
                    return Some(ControlTokenMatch::Rule {
                        len: 12,
                        replacement: "\n\nAssistant&#58;",
                    });
                }
            } else if i == 0 {
                if bytes.starts_with(b"\nHuman:") {
                    return Some(ControlTokenMatch::Rule {
                        len: 7,
                        replacement: "\nHuman&#58;",
                    });
                }
                if bytes.starts_with(b"\nAssistant:") {
                    return Some(ControlTokenMatch::Rule {
                        len: 11,
                        replacement: "\nAssistant&#58;",
                    });
                }
            }
            None
        }
        b'[' => {
            if !is_bracket_second_byte(b1) {
                return None;
            }
            let b2 = bytes[i + 2];
            let b3 = bytes[i + 3];
            let tail = &bytes[i..];
            for &(pat, replacement) in BRACKET_TOKEN_RULES {
                let pb = pat.as_bytes();
                if pb[1] == b1 && pb[2] == b2 && pb[3] == b3 && tail.starts_with(pb) {
                    return Some(ControlTokenMatch::Rule {
                        len: pb.len(),
                        replacement,
                    });
                }
            }
            None
        }
        b'<' => {
            if !is_angle_second_byte(b1) {
                return None;
            }
            if (b1 == b'|' || b1 == FULLWIDTH_PIPE_OPEN.as_bytes()[1])
                && let Some(end_idx) = match_generic_pipe_token(bytes, i)
            {
                return Some(ControlTokenMatch::GenericPipe { end_idx });
            }
            let b2 = bytes[i + 2];
            let b3 = bytes[i + 3];
            let tail = &bytes[i..];
            for &(pat, replacement) in rules {
                let pb = pat.as_bytes();
                if pb[0] == b'<'
                    && pb[1] == b1
                    && pb[2] == b2
                    && pb[3] == b3
                    && tail.starts_with(pb)
                {
                    return Some(ControlTokenMatch::Rule {
                        len: pb.len(),
                        replacement,
                    });
                }
            }
            None
        }
        _ => None,
    }
}

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
pub(crate) fn is_valid_ncname(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(is_ncname_start_byte) && bytes.all(is_ncname_continue_byte)
}

/// Validate a single or comma-separated list of XML `NCName` boundary tags for `filter_name`,
/// returning `(primary_tag, full_spec)`.
pub(crate) fn validate_tag_spec_for_filter<'a>(
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
pub(super) fn validate_quarantine_tag_spec(spec: &str) -> Result<(&str, &str), TemplateError> {
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
fn has_breakout_with_rules(
    bytes: &[u8],
    rules: &[(&'static str, &'static str)],
    tag_spec: Option<&str>,
) -> bool {
    let parsed_tags = ParsedTagSpec::from_opt(tag_spec);
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
fn sanitize_with_rules<'a>(
    s: &'a str,
    rules: &[(&'static str, &'static str)],
    tag_spec: Option<&str>,
) -> Cow<'a, str> {
    let bytes = s.as_bytes();
    let parsed_tags = ParsedTagSpec::from_opt(tag_spec);
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
    has_breakout_with_rules(s.as_bytes(), TOKEN_DELIMITERS, None)
}

/// Returns `true` if `s` contains any role/turn/special token in [`ROLE_TOKEN_DELIMITERS`]
/// or any generic `<|...|>` / `<｜...｜>` special control token.
#[must_use]
pub fn has_role_control_tokens(s: &str) -> bool {
    has_breakout_with_rules(s.as_bytes(), ROLE_TOKEN_DELIMITERS, None)
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
pub(super) fn sanitize_tokens_with_args<'a>(
    s: &'a str,
    tag_spec: Option<&str>,
) -> Result<Cow<'a, str>, TemplateError> {
    match tag_spec {
        None => Ok(sanitize_tokens_str(s)),
        Some(raw_arg) => {
            let tag_cow = strip_quotes(raw_arg);
            let (_, valid_spec) = validate_quarantine_tag_spec(&tag_cow)?;
            Ok(match sanitize_untrusted_str(s, valid_spec) {
                Cow::Borrowed(_) => Cow::Borrowed(s),
                Cow::Owned(sanitized) => Cow::Owned(sanitized),
            })
        }
    }
}

/// Neutralize LLM control tokens, chat delimiters, and optional XML boundary tags in a [`Value`].
pub(super) fn apply_sanitize_tokens(
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
pub(super) fn apply_fence(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
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
    has_breakout_with_rules(s.as_bytes(), &[], Some(tag_spec))
}

/// Returns `true` if `s` contains any known LLM control token ([`TOKEN_DELIMITERS`]),
/// generic `<|...|>` / `<｜...｜>` special control token, or opening/closing XML tag
/// matching `tag_spec` in a single byte-cursor pass.
#[must_use]
pub fn has_untrusted_breakout(s: &str, tag_spec: &str) -> bool {
    has_breakout_with_rules(s.as_bytes(), TOKEN_DELIMITERS, Some(tag_spec))
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
pub(super) fn apply_quarantine(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => Ok(Value::Str(quarantine_str(s, args)?)),
        _ => Err(TemplateError::syntax("'quarantine' requires a string")),
    }
}
