//! Fast single-pass detection and stripping of invisible Unicode smuggling characters,
//! bidirectional (Bidi) spoofing controls, Plane 14 Unicode Tag characters,
//! Unicode `Default_Ignorable_Code_Point` / `Cf` format characters, and
//! dangerous C0/DEL/C1 control characters.
//!
//! Preserves standard whitespace (`\t`, `\n`, `\r`), ATX-heading line breaks
//! (`\u{000B}`, `\u{000C}`, `\u{0085}`, `\u{2028}`, `\u{2029}`), and legitimate
//! emoji ZWJ (`U+200D`) / VS16 (`U+FE0F`) sequences (`👨‍👩‍👧‍👦`, `🏳️‍🌈`, `❤️`),
//! while stripping all invisible characters that could split LLM control tokens
//! (`<|im_\u{200B}start|>`) or XML boundary tags (`</\u{E0020}outer>`).

use alloc::{borrow::Cow, string::String};

/// Returns `true` if `b` can be the leading UTF-8 byte of a stripped C0/DEL/C1 control,
/// Bidi override/isolate, `Default_Ignorable_Code_Point` / `Cf` format character,
/// variation selector / ZWJ, or Plane 14 Unicode Tag.
///
/// For all printable ASCII (`0x20..=0x7E`) and standard whitespace (`\t`, `\n`, `\x0B`,
/// `\x0C`, `\r`), this returns `false` in a single byte check.
#[inline]
#[must_use]
pub(crate) const fn is_invisible_lead_byte(b: u8) -> bool {
    matches!(
        b,
        0x00..=0x08
            | 0x0E..=0x1F
            | 0x7F
            | 0xC2
            | 0xCD
            | 0xD8
            | 0xDB
            | 0xDC
            | 0xE0..=0xE3
            | 0xEF
            | 0xF0
            | 0xF3
    )
}

/// Returns `true` if `c` is unconditionally stripped as an invisible, Bidi, format (`Cf`),
/// `Default_Ignorable_Code_Point`, or control character regardless of surrounding context.
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
            // Combining Grapheme Joiner (CGJ)
            | '\u{034F}'
            // Arabic & Syriac Cf format controls
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            // Hangul Choseong & Jungseong fillers
            | '\u{115F}'..='\u{1160}'
            // Khmer inherent vowels
            | '\u{17B4}'..='\u{17B5}'
            // Mongolian Free Variation Selectors (FVS1..FVS4) & Vowel Separator (MVS)
            | '\u{180B}'..='\u{180F}'
            // Zero-width space, ZWNJ, LRM, RLM
            | '\u{200B}'
            | '\u{200C}'
            | '\u{200E}'
            | '\u{200F}'
            // Bidi embeddings and overrides (LRE, RLE, PDF, LRO, RLO)
            | '\u{202A}'..='\u{202E}'
            // Word joiner, invisible math operators, reserved U+2065, Bidi isolates, and deprecated format controls
            | '\u{2060}'..='\u{206F}'
            // Hangul Filler
            | '\u{3164}'
            // Variation Selectors 1..15
            | '\u{FE00}'..='\u{FE0E}'
            // Zero-width no-break space / BOM
            | '\u{FEFF}'
            // Halfwidth Hangul Filler
            | '\u{FFA0}'
            // Reserved Default_Ignorable_Code_Point (FFF0..FFF8) & interlinear annotation controls (FFF9..FFFB)
            | '\u{FFF0}'..='\u{FFFB}'
            // Brahmi Number Joiner & Kaithi Cf number signs
            | '\u{1107F}'
            | '\u{110BD}'
            | '\u{110CD}'
            // Egyptian Hieroglyph Format Controls
            | '\u{13430}'..='\u{1343F}'
            // Shorthand Format Controls
            | '\u{1BCA0}'..='\u{1BCA3}'
            // Musical Symbol Format Controls
            | '\u{1D173}'..='\u{1D17A}'
            // Plane 14 Unicode Tags, Variation Selectors Supplement (VS17..VS256), and reserved Default_Ignorable ranges
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// Returns `true` if `c` is an emoji, pictographic symbol, regional indicator, keycap mark,
/// or Fitzpatrick skin-tone modifier that can legitimately participate in a `U+200D` (ZWJ)
/// or `U+FE0F` (VS16) emoji sequence.
#[inline]
pub(crate) const fn is_emoji_joinable_char(c: char) -> bool {
    matches!(
        c,
        '\u{00A9}'
            | '\u{00AE}'
            | '\u{203C}'
            | '\u{2049}'
            | '\u{20E3}'
            | '\u{2122}'
            | '\u{2139}'
            | '\u{2194}'..='\u{2199}'
            | '\u{21A9}'..='\u{21AA}'
            | '\u{231A}'..='\u{23FA}'
            | '\u{24C2}'
            | '\u{25AA}'..='\u{25AB}'
            | '\u{25B6}'
            | '\u{25C0}'
            | '\u{25FB}'..='\u{25FE}'
            | '\u{2600}'..='\u{27BF}'
            | '\u{2934}'..='\u{2935}'
            | '\u{2B05}'..='\u{2B07}'
            | '\u{2B1B}'..='\u{2B1C}'
            | '\u{2B50}'
            | '\u{2B55}'
            | '\u{3030}'
            | '\u{303D}'
            | '\u{3297}'
            | '\u{3299}'
            | '\u{1F000}'..='\u{1FAFF}'
    )
}

#[inline]
fn has_candidate_lead_byte(s: &str) -> bool {
    s.as_bytes().iter().copied().any(is_invisible_lead_byte)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Vs16Mode {
    Disallowed,
    Emoji,
    KeycapBase,
}

/// Linear-time (`O(n)`) state machine for stripping invisible characters while preserving
/// valid emoji `U+FE0F` (at most one per emoji or keycap base) and `U+200D` (at most one between
/// two emoji bases) sequences.
struct InvisibleScanner<'a> {
    s: &'a str,
    vs16_mode: Vs16Mode,
    can_zwj: bool,
    /// Monotonically advancing cached `(byte_offset, is_joinable)` of the next visible base character.
    lookahead: Option<(usize, bool)>,
}

impl<'a> InvisibleScanner<'a> {
    #[inline]
    const fn new(s: &'a str) -> Self {
        Self {
            s,
            vs16_mode: Vs16Mode::Disallowed,
            can_zwj: false,
            lookahead: None,
        }
    }

    /// Return whether the next non-invisible, non-ZWJ/VS16 character at or after `from_byte`
    /// is [`is_emoji_joinable_char`], advancing `self.lookahead` monotonically so total
    /// lookahead work across the entire string is `O(n)`.
    #[inline]
    fn next_visible_is_joinable(&mut self, from_byte: usize) -> bool {
        if let Some((pos, joinable)) = self.lookahead
            && from_byte <= pos
        {
            return joinable;
        }
        let tail = &self.s[from_byte..];
        for (rel_idx, ch) in tail.char_indices() {
            if !is_unconditional_invisible_char(ch) && ch != '\u{200D}' && ch != '\u{FE0F}' {
                let joinable = is_emoji_joinable_char(ch);
                self.lookahead = Some((from_byte + rel_idx, joinable));
                return joinable;
            }
        }
        self.lookahead = Some((self.s.len(), false));
        false
    }

    /// Evaluate whether `c` at `[idx..next_idx]` should be stripped and update state.
    #[inline]
    fn should_strip(&mut self, c: char, next_idx: usize) -> bool {
        if is_unconditional_invisible_char(c) {
            return true;
        }
        if c == '\u{FE0F}' {
            let mode = self.vs16_mode;
            self.vs16_mode = Vs16Mode::Disallowed;
            return match mode {
                Vs16Mode::Emoji => false,
                Vs16Mode::KeycapBase => !self.s[next_idx..].starts_with('\u{20E3}'),
                Vs16Mode::Disallowed => true,
            };
        }
        if c == '\u{200D}' {
            let keep = self.can_zwj && self.next_visible_is_joinable(next_idx);
            self.vs16_mode = Vs16Mode::Disallowed;
            self.can_zwj = false;
            return !keep;
        }
        if is_emoji_joinable_char(c) {
            self.vs16_mode = Vs16Mode::Emoji;
            self.can_zwj = true;
        } else if matches!(c, '#' | '*' | '0'..='9') {
            self.vs16_mode = Vs16Mode::KeycapBase;
            self.can_zwj = false;
        } else {
            self.vs16_mode = Vs16Mode::Disallowed;
            self.can_zwj = false;
        }
        false
    }
}

/// Returns `true` if `s` contains any stripped invisible Unicode, Bidi control, `Cf` format,
/// `Default_Ignorable_Code_Point`, Plane 14 Tag, or C0/DEL/C1 control character.
#[must_use]
pub fn has_invisible_chars(s: &str) -> bool {
    if !has_candidate_lead_byte(s) {
        return false;
    }
    let mut scanner = InvisibleScanner::new(s);
    for (idx, c) in s.char_indices() {
        if scanner.should_strip(c, idx + c.len_utf8()) {
            return true;
        }
    }
    false
}

/// Strip invisible Unicode smuggling characters, Bidi overrides/isolates, `Cf` /
/// `Default_Ignorable_Code_Point` characters, Plane 14 Tags, and dangerous C0/DEL/C1
/// control characters in a single `O(n)` pass, returning [`Cow::Borrowed`] when `s` is clean.
#[must_use]
pub fn strip_invisible_str(s: &str) -> Cow<'_, str> {
    if !has_candidate_lead_byte(s) {
        return Cow::Borrowed(s);
    }

    let mut out: Option<String> = None;
    let mut last_copied = 0usize;
    let mut scanner = InvisibleScanner::new(s);
    for (idx, c) in s.char_indices() {
        let next_idx = idx + c.len_utf8();
        if scanner.should_strip(c, next_idx) {
            let buf = out.get_or_insert_with(|| String::with_capacity(s.len()));
            buf.push_str(&s[last_copied..idx]);
            last_copied = next_idx;
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
