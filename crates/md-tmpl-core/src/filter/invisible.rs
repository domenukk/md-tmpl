//! Fast single-pass detection and stripping of invisible Unicode smuggling characters,
//! bidirectional (Bidi) spoofing controls, Plane 14 Unicode Tag characters, and
//! dangerous C0/DEL/C1 control characters.
//!
//! Preserves standard whitespace (`\t`, `\n`, `\r`), ATX-heading line breaks
//! (`\u{000B}`, `\u{000C}`, `\u{0085}`, `\u{2028}`, `\u{2029}`), and legitimate
//! non-ASCII emoji ZWJ (`U+200D`) / VS16 (`U+FE0F`) sequences (`👨‍👩‍👧‍👦`, `🏳️‍🌈`, `❤️`),
//! while stripping all invisible characters that could split LLM control tokens
//! (`<|im_\u{200B}start|>`) or XML boundary tags (`</\u{E0020}outer>`).

use alloc::{borrow::Cow, string::String};

/// Returns `true` if `b` can be the leading UTF-8 byte of a stripped C0/DEL/C1 control,
/// Bidi override/isolate, zero-width format character, or Plane 14 Unicode Tag.
///
/// For all printable ASCII (`0x20..=0x7E`) and standard whitespace (`\t`, `\n`, `\x0B`,
/// `\x0C`, `\r`), this returns `false` in a single byte check.
#[inline]
#[must_use]
pub(crate) const fn is_invisible_lead_byte(b: u8) -> bool {
    matches!(
        b,
        0x00..=0x08 | 0x0E..=0x1F | 0x7F | 0xC2 | 0xD8 | 0xE1 | 0xE2 | 0xEF | 0xF3
    )
}

/// Returns `true` if `c` is unconditionally stripped as an invisible, Bidi, or control
/// character regardless of surrounding context.
#[inline]
#[must_use]
pub const fn is_unconditional_invisible_char(c: char) -> bool {
    matches!(
        c,
        // Dangerous C0 controls (excluding \t 0x09, \n 0x0A, \v 0x0B, \f 0x0C, \r 0x0D) and DEL
        '\u{0000}'..='\u{0008}'
            | '\u{000E}'..='\u{001F}'
            | '\u{007F}'
            // C1 controls (excluding NEL U+0085 so ATX heading line-break collapsing maps it to ' ')
            | '\u{0080}'..='\u{0084}'
            | '\u{0086}'..='\u{009F}'
            // Soft hyphen
            | '\u{00AD}'
            // Arabic Letter Mark
            | '\u{061C}'
            // Mongolian Vowel Separator
            | '\u{180E}'
            // Zero-width space, ZWNJ, LRM, RLM
            | '\u{200B}'
            | '\u{200C}'
            | '\u{200E}'
            | '\u{200F}'
            // Bidi embeddings and overrides (LRE, RLE, PDF, LRO, RLO)
            | '\u{202A}'..='\u{202E}'
            // Word joiner and invisible math operators
            | '\u{2060}'..='\u{2064}'
            // Bidi isolates (LRI, RLI, FSI, PDI) and deprecated format controls
            | '\u{2066}'..='\u{206F}'
            // Variation Selectors 1..15
            | '\u{FE00}'..='\u{FE0E}'
            // Zero-width no-break space / BOM
            | '\u{FEFF}'
            // Interlinear annotation controls
            | '\u{FFF9}'..='\u{FFFB}'
            // Plane 14 Unicode Tags (U+E0000..=U+E007F)
            | '\u{E0000}'..='\u{E007F}'
            // Variation Selectors Supplement (VS17..VS256)
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// Returns `true` if `c` is a non-ASCII base character that can legitimately participate
/// in an emoji `U+200D` (ZWJ) or `U+FE0F` (VS16) sequence without splitting control tokens.
#[inline]
const fn is_emoji_joinable_char(c: char) -> bool {
    c > '\x7F'
        && c != '｜'
        && c != '▁'
        && c != '\u{200D}'
        && c != '\u{FE0F}'
        && !is_unconditional_invisible_char(c)
}

/// Find the next character in `rest` that is not an invisible or variation-selector character.
#[inline]
fn next_visible_base_char(rest: &str) -> Option<char> {
    rest.chars()
        .find(|&c| !is_unconditional_invisible_char(c) && c != '\u{200D}' && c != '\u{FE0F}')
}

#[inline]
fn has_candidate_lead_byte(s: &str) -> bool {
    s.as_bytes().iter().copied().any(is_invisible_lead_byte)
}

/// Evaluate whether `c` should be stripped and advance `prev_joinable` for the
/// invisible-character / emoji-joiner state machine.
#[inline]
fn should_strip_char(c: char, prev_joinable: &mut bool, rest: &str) -> bool {
    if is_unconditional_invisible_char(c) {
        return true;
    }
    if c == '\u{200D}' {
        let next_ok = next_visible_base_char(rest).is_some_and(is_emoji_joinable_char);
        return !(*prev_joinable && next_ok);
    }
    if c == '\u{FE0F}' {
        return !*prev_joinable;
    }
    *prev_joinable = is_emoji_joinable_char(c);
    false
}

/// Returns `true` if `s` contains any stripped invisible Unicode, Bidi control, Plane 14 Tag,
/// or C0/DEL/C1 control character.
#[must_use]
pub fn has_invisible_chars(s: &str) -> bool {
    if !has_candidate_lead_byte(s) {
        return false;
    }
    let mut prev_joinable = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if should_strip_char(c, &mut prev_joinable, chars.as_str()) {
            return true;
        }
    }
    false
}

/// Strip invisible Unicode smuggling characters, Bidi overrides/isolates, Plane 14 Tags,
/// and dangerous C0/DEL/C1 control characters in a single pass, returning [`Cow::Borrowed`]
/// when `s` is already clean.
#[must_use]
pub fn strip_invisible_str(s: &str) -> Cow<'_, str> {
    if !has_candidate_lead_byte(s) {
        return Cow::Borrowed(s);
    }

    let mut out: Option<String> = None;
    let mut last_copied = 0usize;
    let mut prev_joinable = false;
    let mut iter = s.char_indices();
    while let Some((idx, c)) = iter.next() {
        if should_strip_char(c, &mut prev_joinable, iter.as_str()) {
            let buf = out.get_or_insert_with(|| String::with_capacity(s.len()));
            buf.push_str(&s[last_copied..idx]);
            last_copied = idx + c.len_utf8();
        }
    }
    match out {
        Some(mut buf) => {
            buf.push_str(&s[last_copied..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(s),
    }
}
