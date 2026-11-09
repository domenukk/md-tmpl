/**
 * Fast single-pass detection and stripping of invisible Unicode smuggling
 * characters, bidirectional (Bidi) spoofing controls, Plane 14 Unicode Tag
 * characters, and dangerous C0/DEL/C1 control characters.
 *
 * Preserves standard whitespace (`\t`, `\n`, `\r`), ATX-heading line breaks
 * (`\u000B`, `\u000C`, `\u0085`, `\u2028`, `\u2029`), and legitimate non-ASCII
 * emoji ZWJ (`U+200D`) / VS16 (`U+FE0F`) sequences (`👨‍👩‍👧‍👦`, `🏳️‍🌈`, `❤️`), while
 * stripping all invisible characters that could split LLM control tokens
 * (`<|im_\u200Bstart|>`) or XML boundary tags (`</\u{E0020}outer>`).
 *
 * @module
 */

/**
 * Returns `true` if UTF-16 code unit `c` may be (or start a surrogate pair for)
 * a stripped C0/DEL/C1 control, Bidi control, zero-width format character, or
 * Plane 14 Unicode Tag.
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
    c === 0x061c ||
    c === 0x180e ||
    (c >= 0x200b && c <= 0x200f) ||
    (c >= 0x202a && c <= 0x202e) ||
    (c >= 0x2060 && c <= 0x206f) ||
    (c >= 0xfe00 && c <= 0xfe0f) ||
    c === 0xfeff ||
    (c >= 0xfff9 && c <= 0xfffb) ||
    c === 0xdb40 // High surrogate for U+E0000..U+E03FF (Plane 14 Tags + Variation Selectors Supplement)
  );
}

/**
 * Returns `true` if Unicode code point `cp` is unconditionally stripped as an
 * invisible, Bidi, or control character regardless of surrounding context.
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
    cp === 0x061c ||
    cp === 0x180e ||
    cp === 0x200b ||
    cp === 0x200c ||
    cp === 0x200e ||
    cp === 0x200f ||
    (cp >= 0x202a && cp <= 0x202e) ||
    (cp >= 0x2060 && cp <= 0x2064) ||
    (cp >= 0x2066 && cp <= 0x206f) ||
    (cp >= 0xfe00 && cp <= 0xfe0e) ||
    cp === 0xfeff ||
    (cp >= 0xfff9 && cp <= 0xfffb) ||
    (cp >= 0xe0000 && cp <= 0xe007f) ||
    (cp >= 0xe0100 && cp <= 0xe01ef)
  );
}

function isEmojiJoinableCodePoint(cp: number): boolean {
  return (
    cp > 0x7f &&
    cp !== 0xff5c /* '｜' */ &&
    cp !== 0x2581 /* '▁' */ &&
    cp !== 0x200d &&
    cp !== 0xfe0f &&
    !isUnconditionalInvisibleCodePoint(cp)
  );
}

function nextVisibleBaseCodePoint(s: string, startIdx: number): number {
  let i = startIdx;
  while (i < s.length) {
    const cp = s.codePointAt(i);
    if (cp === undefined) {
      break;
    }
    const step = cp > 0xffff ? 2 : 1;
    if (
      !isUnconditionalInvisibleCodePoint(cp) &&
      cp !== 0x200d &&
      cp !== 0xfe0f
    ) {
      return cp;
    }
    i += step;
  }
  return -1;
}

function hasCandidateUnit(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    if (isInvisibleCandidateUnit(s.charCodeAt(i))) {
      return true;
    }
  }
  return false;
}

function shouldStripCodePoint(
  cp: number,
  prevJoinable: boolean,
  s: string,
  nextIdx: number,
): [boolean, boolean] {
  if (isUnconditionalInvisibleCodePoint(cp)) {
    return [true, prevJoinable];
  }
  if (cp === 0x200d) {
    const nextCp = nextVisibleBaseCodePoint(s, nextIdx);
    const keep =
      prevJoinable && nextCp !== -1 && isEmojiJoinableCodePoint(nextCp);
    return [!keep, prevJoinable];
  }
  if (cp === 0xfe0f) {
    return [!prevJoinable, prevJoinable];
  }
  return [false, isEmojiJoinableCodePoint(cp)];
}

/**
 * Returns `true` if `s` contains any stripped invisible Unicode, Bidi control,
 * Plane 14 Tag, or C0/DEL/C1 control character.
 */
export function hasInvisibleChars(s: string): boolean {
  if (!hasCandidateUnit(s)) {
    return false;
  }
  let prevJoinable = false;
  let i = 0;
  while (i < s.length) {
    const cp = s.codePointAt(i);
    if (cp === undefined) {
      break;
    }
    const step = cp > 0xffff ? 2 : 1;
    const [strip, nextJoinable] = shouldStripCodePoint(
      cp,
      prevJoinable,
      s,
      i + step,
    );
    prevJoinable = nextJoinable;
    if (strip) {
      return true;
    }
    i += step;
  }
  return false;
}

/**
 * Strip invisible Unicode smuggling characters, Bidi overrides/isolates,
 * Plane 14 Tags, and dangerous C0/DEL/C1 control characters in a single pass,
 * returning `s` unchanged when clean.
 */
export function stripInvisibleStr(s: string): string {
  if (!hasCandidateUnit(s)) {
    return s;
  }

  let out: string | undefined;
  let lastCopied = 0;
  let prevJoinable = false;
  let i = 0;
  while (i < s.length) {
    const cp = s.codePointAt(i);
    if (cp === undefined) {
      break;
    }
    const step = cp > 0xffff ? 2 : 1;
    const [strip, nextJoinable] = shouldStripCodePoint(
      cp,
      prevJoinable,
      s,
      i + step,
    );
    prevJoinable = nextJoinable;
    if (strip) {
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
