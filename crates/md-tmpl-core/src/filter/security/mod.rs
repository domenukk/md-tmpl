//! Security, escaping, token-sanitization, code-fencing, and XML-quarantine primitives.

use alloc::{borrow::Cow, string::String};

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

pub(super) enum ControlTokenMatch {
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
pub(super) fn match_control_token_at(
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

mod quarantine;
pub(super) use quarantine::{
    apply_fence, apply_quarantine, apply_sanitize_tokens, sanitize_tokens_with_args,
    validate_quarantine_tag_spec,
};
pub(crate) use quarantine::{
    fence_into, is_ncname_continue_byte, is_ncname_start_byte, quarantine_into,
    quarantine_untrusted_into, validate_sanitize_tag_spec,
};
pub use quarantine::{
    fence_str, has_control_tokens, has_quarantine_tag_breakout, has_role_control_tokens,
    has_untrusted_breakout, is_quarantined_str, quarantine_str, quarantine_untrusted_str,
    sanitize_quarantine_payload, sanitize_role_tokens_str, sanitize_tokens_str,
    sanitize_untrusted_str,
};
