/**
 * XML entity decoding and attribute / element-body escaping (ports of Rust
 * `decode_xml_str`, `escape_xml_attr_str`, and `escape_xml_body_str`).
 *
 * @module
 */

const AMP = "&";

/** Canonical XML entity replacements and their decoded forms (`[entity, decoded]`). */
export const XML_DECODE_PAIRS: readonly (readonly [string, string])[] = [
  ["&quot;", '"'],
  ["&#39;", "'"],
  ["&apos;", "'"],
  ["&lt;", "<"],
  ["&gt;", ">"],
  ["&#10;", "\n"],
  ["&#13;", "\r"],
  ["&amp;", "&"],
];

/**
 * Returns `true` if Unicode scalar value `cp` is illegal in XML 1.0 documents
 * (disallowed C0 controls, DEL/C1 controls, lone UTF-16 surrogates, or Unicode
 * noncharacters `U+FDD0..=U+FDEF` and `U+nFFFE` / `U+nFFFF`).
 */
export function isIllegalXmlCodePoint(cp: number): boolean {
  if (cp <= 0x1f) {
    return cp <= 0x08 || cp === 0x0b || cp === 0x0c || cp >= 0x0e;
  }
  if (cp >= 0x7f && cp <= 0x9f) {
    return true;
  }
  if (cp >= 0xd800 && cp <= 0xdfff) {
    return true;
  }
  if (cp >= 0xfdd0 && cp <= 0xfdef) {
    return true;
  }
  return (cp & 0xfffe) === 0xfffe;
}

/** Apply `replacementFor` across code points / lone surrogates of `s` in a single pass. */
export function replaceEachXmlCodePoint(
  s: string,
  replacementFor: (cp: number) => string | undefined,
): string {
  let out: string | undefined;
  let lastCopied = 0;
  let i = 0;
  while (i < s.length) {
    const unit = s.charCodeAt(i);
    if (
      unit >= 0x20 &&
      unit <= 0x7e &&
      unit !== 38 /* '&' */ &&
      unit !== 60 /* '<' */ &&
      unit !== 62 /* '>' */ &&
      unit !== 34 /* '"' */ &&
      unit !== 39 /* "'" */
    ) {
      i++;
      continue;
    }
    let cp = unit;
    let step = 1;
    if (unit >= 0xd800 && unit <= 0xdbff && i + 1 < s.length) {
      const low = s.charCodeAt(i + 1);
      if (low >= 0xdc00 && low <= 0xdfff) {
        cp = ((unit - 0xd800) << 10) + (low - 0xdc00) + 0x10000;
        step = 2;
      }
    }
    const replacement = replacementFor(cp);
    if (replacement !== undefined) {
      out = (out ?? "") + s.slice(lastCopied, i) + replacement;
      lastCopied = i + step;
    }
    i += step;
  }
  return out !== undefined ? out + s.slice(lastCopied) : s;
}

/**
 * Escape a string for an XML attribute value: `&`, `<`, `>`, `"`, `'` become
 * predefined entities; `\n` and `\r` become `&#10;` and `&#13;`; illegal XML 1.0
 * controls and noncharacters are stripped.
 */
export function escapeXmlAttrString(s: string): string {
  return replaceEachXmlCodePoint(s, (cp) => {
    switch (cp) {
      case 38 /* '&' */:
        return "&amp;";
      case 60 /* '<' */:
        return "&lt;";
      case 62 /* '>' */:
        return "&gt;";
      case 34 /* '"' */:
        return "&quot;";
      case 39 /* "'" */:
        return "&apos;";
      case 10 /* '\n' */:
        return "&#10;";
      case 13 /* '\r' */:
        return "&#13;";
      default:
        return isIllegalXmlCodePoint(cp) ? "" : undefined;
    }
  });
}

/**
 * Escape an XML element body value: `&`, `<`, `>` become entities and illegal
 * XML 1.0 controls and noncharacters are stripped.
 */
export function escapeXmlBodyString(s: string): string {
  return replaceEachXmlCodePoint(s, (cp) => {
    switch (cp) {
      case 38 /* '&' */:
        return "&amp;";
      case 60 /* '<' */:
        return "&lt;";
      case 62 /* '>' */:
        return "&gt;";
      default:
        return isIllegalXmlCodePoint(cp) ? "" : undefined;
    }
  });
}

/**
 * Decode the standard XML entity references in {@link XML_DECODE_PAIRS} in a
 * single left-to-right pass (decoded output is never re-scanned).
 */
export function decodeXmlString(s: string): string {
  if (!s.includes(AMP)) {
    return s;
  }
  let out = "";
  let lastCopied = 0;
  let i = s.indexOf(AMP);
  while (i !== -1) {
    const pair = XML_DECODE_PAIRS.find(([entity]) => s.startsWith(entity, i));
    if (pair === undefined) {
      i = s.indexOf(AMP, i + AMP.length);
      continue;
    }
    const [entity, decoded] = pair;
    out += s.slice(lastCopied, i) + decoded;
    lastCopied = i + entity.length;
    i = s.indexOf(AMP, lastCopied);
  }
  return out + s.slice(lastCopied);
}
