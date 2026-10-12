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
  ["&amp;", "&"],
];

/** First and last code units of the C1 control block (Unicode `Cc`, with C0 and DEL). */
const C0_CONTROL_MAX = 0x1f;
const DEL_CODE = 0x7f;
const C1_CONTROL_MAX = 0x9f;

/** Returns `true` for Unicode general category `Cc` (Rust `char::is_control`). */
function isControlCode(code: number): boolean {
  return code <= C0_CONTROL_MAX || (code >= DEL_CODE && code <= C1_CONTROL_MAX);
}

/** Escapes shared by attribute and body escaping. */
const MARKUP_ESCAPES: ReadonlyMap<string, string> = new Map([
  ["&", "&amp;"],
  ["<", "&lt;"],
  [">", "&gt;"],
  ["\0", "\\0"],
]);

/** Additional escapes for attribute values. */
const ATTR_ONLY_ESCAPES: ReadonlyMap<string, string> = new Map([
  ['"', "&quot;"],
  ["'", "&apos;"],
  ["\n", "\\n"],
  ["\r", "\\r"],
]);

/** Replacement for any other control character inside an attribute value. */
const ATTR_CONTROL_REPLACEMENT = " ";

/** Apply `replacementFor` to every code unit of `s` in a single pass. */
function replaceEach(
  s: string,
  replacementFor: (ch: string, code: number) => string | undefined,
): string {
  let out: string | undefined;
  let lastCopied = 0;
  for (let i = 0; i < s.length; i++) {
    const ch = s.charAt(i);
    const replacement = replacementFor(ch, s.charCodeAt(i));
    if (replacement !== undefined) {
      out = (out ?? "") + s.slice(lastCopied, i) + replacement;
      lastCopied = i + 1;
    }
  }
  return out !== undefined ? out + s.slice(lastCopied) : s;
}

/**
 * Escape a string for an XML attribute value: `&`, `<`, `>`, `"`, `'` become
 * entities; `\n`, `\r`, `\0` become the two-character escapes `\n`, `\r`, `\0`;
 * any other control character becomes a space.
 */
export function escapeXmlAttrString(s: string): string {
  return replaceEach(
    s,
    (ch, code) =>
      MARKUP_ESCAPES.get(ch) ??
      ATTR_ONLY_ESCAPES.get(ch) ??
      (isControlCode(code) ? ATTR_CONTROL_REPLACEMENT : undefined),
  );
}

/** Escape an XML element body value: `&`, `<`, `>` become entities and `\0` becomes `\0`. */
export function escapeXmlBodyString(s: string): string {
  return replaceEach(s, (ch) => MARKUP_ESCAPES.get(ch));
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
