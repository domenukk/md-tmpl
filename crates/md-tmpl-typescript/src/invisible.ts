/**
 * Fast single-pass detection and stripping of invisible Unicode smuggling
 * characters, bidirectional (Bidi) spoofing controls, `Default_Ignorable_Code_Point`
 * / `Cf` format controls, Plane 14 Unicode Tag characters, and dangerous
 * C0/DEL/C1 control characters.
 *
 * Preserves standard whitespace (`\t`, `\n`, `\r`), ATX-heading line breaks
 * (`\u000B`, `\u000C`, `\u0085`, `\u2028`, `\u2029`), and legitimate emoji ZWJ
 * (`U+200D`) / VS16 (`U+FE0F`) sequences (`👨‍👩‍👧‍👦`, `🏳️‍🌈`, `❤️`), while stripping
 * all invisible characters that could split LLM control tokens
 * (`<|im_\u200Bstart|>`) or XML boundary tags (`</\u{E0020}outer>`).
 *
 * @module
 */

/**
 * Returns `true` if UTF-16 code unit `c` may be (or start a surrogate pair for)
 * a stripped C0/DEL/C1 control, Bidi control, `Default_Ignorable_Code_Point` /
 * `Cf` format character, variation selector / ZWJ, or Plane 14 Unicode Tag.
 *
 * For all printable ASCII (`0x20..0x7E`) and standard whitespace (`\t`, `\n`,
 * `\x0B`, `\x0C`, `\r`), this returns `false` in a single integer comparison.
 */
export function isInvisibleCandidateUnit(c: number): boolean {
  if (c >= 0x20 && c < 0x7f) {
    return false;
  }
  if (c < 0x20) {
    return c <= 0x08 || c >= 0x0e;
  }
  return (
    c === 0x7f ||
    (c >= 0x80 && c <= 0x9f && c !== 0x85) ||
    c === 0xad ||
    c === 0x034f ||
    (c >= 0x0600 && c <= 0x0605) ||
    c === 0x061c ||
    c === 0x06dd ||
    c === 0x070f ||
    (c >= 0x0890 && c <= 0x0891) ||
    c === 0x08e2 ||
    (c >= 0x115f && c <= 0x1160) ||
    (c >= 0x17b4 && c <= 0x17b5) ||
    (c >= 0x180b && c <= 0x180f) ||
    (c >= 0x200b && c <= 0x200f) ||
    (c >= 0x202a && c <= 0x202e) ||
    (c >= 0x2060 && c <= 0x206f) ||
    c === 0x3164 ||
    (c >= 0xfe00 && c <= 0xfe0f) ||
    c === 0xfeff ||
    c === 0xffa0 ||
    (c >= 0xfff0 && c <= 0xfffb) ||
    // High surrogates for U+1107F/110BD/110CD, U+13430..1343F, U+1BCA0..1BCA3, U+1D173..1D17A, U+E0000..E0FFF
    c === 0xd804 ||
    c === 0xd80d ||
    c === 0xd82f ||
    c === 0xd834 ||
    (c >= 0xdb40 && c <= 0xdb43)
  );
}

/**
 * Returns `true` if Unicode code point `cp` is unconditionally stripped as an
 * invisible, Bidi, `Cf` format, `Default_Ignorable_Code_Point`, or control
 * character regardless of surrounding context.
 */
export function isUnconditionalInvisibleCodePoint(cp: number): boolean {
  if (cp >= 0x20 && cp < 0x7f) {
    return false;
  }
  if (cp < 0x20) {
    return cp <= 0x08 || cp >= 0x0e;
  }
  return (
    cp === 0x7f ||
    (cp >= 0x80 && cp <= 0x9f && cp !== 0x85) ||
    cp === 0xad ||
    cp === 0x034f ||
    (cp >= 0x0600 && cp <= 0x0605) ||
    cp === 0x061c ||
    cp === 0x06dd ||
    cp === 0x070f ||
    (cp >= 0x0890 && cp <= 0x0891) ||
    cp === 0x08e2 ||
    (cp >= 0x115f && cp <= 0x1160) ||
    (cp >= 0x17b4 && cp <= 0x17b5) ||
    (cp >= 0x180b && cp <= 0x180f) ||
    cp === 0x200b ||
    cp === 0x200c ||
    cp === 0x200e ||
    cp === 0x200f ||
    (cp >= 0x202a && cp <= 0x202e) ||
    (cp >= 0x2060 && cp <= 0x206f) ||
    cp === 0x3164 ||
    (cp >= 0xfe00 && cp <= 0xfe0e) ||
    cp === 0xfeff ||
    cp === 0xffa0 ||
    (cp >= 0xfff0 && cp <= 0xfffb) ||
    cp === 0x1107f ||
    cp === 0x110bd ||
    cp === 0x110cd ||
    (cp >= 0x13430 && cp <= 0x1343f) ||
    (cp >= 0x1bca0 && cp <= 0x1bca3) ||
    (cp >= 0x1d173 && cp <= 0x1d17a) ||
    (cp >= 0xe0000 && cp <= 0xe0fff)
  );
}

/**
 * Returns `true` if `cp` is an emoji, pictographic symbol, regional indicator,
 * keycap mark, or Fitzpatrick skin-tone modifier that can legitimately participate
 * in a `U+200D` (ZWJ) or `U+FE0F` (VS16) emoji sequence (mirrors Rust `is_emoji_joinable_char`).
 */
export function isEmojiJoinableCodePoint(cp: number): boolean {
  return (
    cp === 0x00a9 ||
    cp === 0x00ae ||
    cp === 0x203c ||
    cp === 0x2049 ||
    cp === 0x20e3 ||
    cp === 0x2122 ||
    cp === 0x2139 ||
    (cp >= 0x2194 && cp <= 0x2199) ||
    (cp >= 0x21a9 && cp <= 0x21aa) ||
    (cp >= 0x231a && cp <= 0x23fa) ||
    cp === 0x24c2 ||
    (cp >= 0x25aa && cp <= 0x25ab) ||
    cp === 0x25b6 ||
    cp === 0x25c0 ||
    (cp >= 0x25fb && cp <= 0x25fe) ||
    (cp >= 0x2600 && cp <= 0x27bf) ||
    (cp >= 0x2934 && cp <= 0x2935) ||
    (cp >= 0x2b05 && cp <= 0x2b07) ||
    (cp >= 0x2b1b && cp <= 0x2b1c) ||
    cp === 0x2b50 ||
    cp === 0x2b55 ||
    cp === 0x3030 ||
    cp === 0x303d ||
    cp === 0x3297 ||
    cp === 0x3299 ||
    (cp >= 0x1f000 && cp <= 0x1faff)
  );
}

function hasCandidateUnit(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    if (isInvisibleCandidateUnit(s.charCodeAt(i))) {
      return true;
    }
  }
  return false;
}

const VS16_DISALLOWED = 0;
const VS16_EMOJI = 1;
const VS16_KEYCAP_BASE = 2;

function isAsciiKeycapBase(cp: number): boolean {
  return cp === 0x23 || cp === 0x2a || (cp >= 0x30 && cp <= 0x39);
}

class InvisibleScanner {
  private readonly s: string;
  private vs16Mode: 0 | 1 | 2 = VS16_DISALLOWED;
  private canZwj = false;
  private lookaheadPos = 0;
  private lookaheadValid = false;
  private lookaheadJoinable = false;

  constructor(s: string) {
    this.s = s;
  }

  private nextVisibleIsJoinable(fromIdx: number): boolean {
    if (this.lookaheadValid && fromIdx <= this.lookaheadPos) {
      return this.lookaheadJoinable;
    }
    let pos = fromIdx;
    while (pos < this.s.length) {
      const cp = this.s.codePointAt(pos);
      if (cp === undefined) {
        break;
      }
      const step = cp > 0xffff ? 2 : 1;
      if (
        !isUnconditionalInvisibleCodePoint(cp) &&
        cp !== 0x200d &&
        cp !== 0xfe0f
      ) {
        this.lookaheadPos = pos;
        this.lookaheadValid = true;
        this.lookaheadJoinable = isEmojiJoinableCodePoint(cp);
        return this.lookaheadJoinable;
      }
      pos += step;
    }
    this.lookaheadPos = this.s.length;
    this.lookaheadValid = true;
    this.lookaheadJoinable = false;
    return false;
  }

  shouldStrip(cp: number, nextIdx: number): boolean {
    if (isUnconditionalInvisibleCodePoint(cp)) {
      return true;
    }
    if (cp === 0xfe0f) {
      const mode = this.vs16Mode;
      this.vs16Mode = VS16_DISALLOWED;
      if (mode === VS16_EMOJI) {
        return false;
      }
      if (mode === VS16_KEYCAP_BASE) {
        return this.s.codePointAt(nextIdx) !== 0x20e3;
      }
      return true;
    }
    if (cp === 0x200d) {
      const keep = this.canZwj && this.nextVisibleIsJoinable(nextIdx);
      this.vs16Mode = VS16_DISALLOWED;
      this.canZwj = false;
      return !keep;
    }
    if (isEmojiJoinableCodePoint(cp)) {
      this.vs16Mode = VS16_EMOJI;
      this.canZwj = true;
    } else if (isAsciiKeycapBase(cp)) {
      this.vs16Mode = VS16_KEYCAP_BASE;
      this.canZwj = false;
    } else {
      this.vs16Mode = VS16_DISALLOWED;
      this.canZwj = false;
    }
    return false;
  }
}

/**
 * Returns `true` if `s` contains any stripped invisible Unicode, Bidi control,
 * `Cf` / `Default_Ignorable_Code_Point` character, Plane 14 Tag, or C0/DEL/C1
 * control character.
 */
export function hasInvisibleChars(s: string): boolean {
  if (!hasCandidateUnit(s)) {
    return false;
  }
  const scanner = new InvisibleScanner(s);
  let i = 0;
  while (i < s.length) {
    const cp = s.codePointAt(i);
    if (cp === undefined) {
      break;
    }
    const step = cp > 0xffff ? 2 : 1;
    if (scanner.shouldStrip(cp, i + step)) {
      return true;
    }
    i += step;
  }
  return false;
}

/**
 * Strip invisible Unicode smuggling characters, Bidi overrides/isolates,
 * `Cf` / `Default_Ignorable_Code_Point` characters, Plane 14 Tags, and
 * dangerous C0/DEL/C1 control characters in a single `O(n)` pass, returning
 * `s` unchanged when clean.
 */
export function stripInvisibleStr(s: string): string {
  if (!hasCandidateUnit(s)) {
    return s;
  }

  let out: string | undefined;
  let lastCopied = 0;
  const scanner = new InvisibleScanner(s);
  let i = 0;
  while (i < s.length) {
    const cp = s.codePointAt(i);
    if (cp === undefined) {
      break;
    }
    const step = cp > 0xffff ? 2 : 1;
    if (scanner.shouldStrip(cp, i + step)) {
      if (out === undefined) {
        out = s.slice(lastCopied, i);
      } else {
        out += s.slice(lastCopied, i);
      }
      lastCopied = i + step;
    }
    i += step;
  }
  if (out === undefined) {
    return s;
  }
  return out + s.slice(lastCopied);
}

/** Alias matching the `*String` naming convention of other TS security filters. */
export const stripInvisibleString = stripInvisibleStr;
