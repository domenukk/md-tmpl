/**
 * Rust-parity unit tests for numeric formatting, Rust-compatible argument
 * parsing, `json` serialization, sanitize tag-spec validation, and the host
 * quarantine / sanitize envelope helpers.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template } from "../template/index.js";
import {
  formatFixedFloat,
  formatFloatDisplay,
  formatIntDisplay,
} from "../numeric_format.js";
import { parseF64, parseI64, parseUsize } from "../filter_args.js";
import { compareCodePoints, serializeJson } from "../filter_json.js";
import {
  sanitizeEnclosedString,
  sanitizeTokensWithArgs,
  validateTagSpecForFilter,
} from "../filters.js";
import {
  DEFAULT_SANITIZE_NOTICE,
  TOOL_OUTPUT_QUARANTINE_CLOSE_TAG,
  TOOL_OUTPUT_QUARANTINE_OPEN_TAG,
  UNTRUSTED_QUARANTINE_CLOSE_TAG,
  UNTRUSTED_QUARANTINE_OPEN_TAG,
  XML_DECODE_PAIRS,
  decodeXmlString,
  escapeXmlAttrString,
  escapeXmlBodyString,
  quarantineUntrustedIdempotentString,
  sanitizeBlockIdempotentString,
  sanitizeBlockString,
  stripOuterQuarantineTag,
  truncateMiddleList,
  unquarantineString,
} from "../index.js";
import { type Value, float, int, list, str, structVal } from "../value.js";

describe("formatFloatDisplay matches Rust f64 Display", () => {
  const cases: readonly [number, string][] = [
    [1e21, "1000000000000000000000"],
    [1e-7, "0.0000001"],
    [0.1, "0.1"],
    [1.5, "1.5"],
    [-2.25, "-2.25"],
    [123456789012345680000, "123456789012345680000"],
    [-0, "-0"],
    [NaN, "NaN"],
    [Infinity, "inf"],
    [-Infinity, "-inf"],
  ];
  for (const [input, expected] of cases) {
    it(`${String(input)} -> ${expected}`, () => {
      assert.strictEqual(formatFloatDisplay(input), expected);
    });
  }

  it("prints the smallest subnormal positionally", () => {
    const text = formatFloatDisplay(5e-324);
    assert.ok(text.startsWith("0.000"));
    assert.ok(text.endsWith("5"));
    assert.strictEqual(text.length, "0.".length + 324);
  });

  it("formatIntDisplay never uses exponent notation", () => {
    assert.strictEqual(formatIntDisplay(1e21), "1000000000000000000000");
    assert.strictEqual(formatIntDisplay(-42), "-42");
  });
});

describe("formatFixedFloat matches Rust write_fixed_float", () => {
  const cases: readonly [number, number, string][] = [
    [2.5, 0, "3"],
    [-2.5, 0, "-3"],
    [0.125, 2, "0.13"],
    [1.45, 1, "1.4"],
    [0.49999999999999994, 0, "0"],
    [-0.001, 2, "-0.00"],
    [-0, 2, "0.00"],
    [1e21, 2, "1000000000000000000000.00"],
    [0.1, 30, "0.100000000000000005551115123126"],
    [9.5367431640625e-7, 19, "0.0000009536743164063"],
    [NaN, 2, "NaN"],
    [-Infinity, 0, "-inf"],
  ];
  for (const [input, precision, expected] of cases) {
    it(`${String(input)} | fixed(${String(precision)}) -> ${expected}`, () => {
      assert.strictEqual(formatFixedFloat(input, precision), expected);
    });
  }
});

describe("Rust-compatible argument parsing", () => {
  it("parseUsize accepts '+' and rejects signs, blanks, and overflow", () => {
    assert.deepStrictEqual(parseUsize("+7"), { ok: true, value: 7 });
    assert.deepStrictEqual(parseUsize("-1"), {
      ok: false,
      reason: "invalid digit found in string",
    });
    assert.deepStrictEqual(parseUsize(""), {
      ok: false,
      reason: "cannot parse integer from empty string",
    });
    assert.deepStrictEqual(parseUsize("18446744073709551616"), {
      ok: false,
      reason: "number too large to fit in target type",
    });
  });

  it("parseI64 reports both overflow directions", () => {
    assert.deepStrictEqual(parseI64("-9223372036854775808"), {
      ok: true,
      value: -9223372036854775808n,
    });
    assert.deepStrictEqual(parseI64("9223372036854775808"), {
      ok: false,
      reason: "number too large to fit in target type",
    });
    assert.deepStrictEqual(parseI64("-9223372036854775809"), {
      ok: false,
      reason: "number too small to fit in target type",
    });
  });

  it("parseF64 follows the Rust f64 grammar", () => {
    assert.deepStrictEqual(parseF64("1."), { ok: true, value: 1 });
    assert.deepStrictEqual(parseF64(".5"), { ok: true, value: 0.5 });
    assert.deepStrictEqual(parseF64("+1E3"), { ok: true, value: 1000 });
    assert.deepStrictEqual(parseF64("-Infinity"), {
      ok: true,
      value: -Infinity,
    });
    const nan = parseF64("NaN");
    assert.ok(nan.ok && Number.isNaN(nan.value));
    for (const bad of ["0x10", "1e", " 1", "1_000", "e5"]) {
      assert.deepStrictEqual(parseF64(bad), {
        ok: false,
        reason: "invalid float literal",
      });
    }
  });
});

describe("serializeJson matches Rust serialize_value_to_json", () => {
  it("sorts keys by code point, not UTF-16 code unit", () => {
    const astral = "\u{10000}";
    const bmpHigh = "\uFFFF";
    assert.ok(compareCodePoints(bmpHigh, astral) < 0);
    const value = structVal([
      [astral, int(1)],
      [bmpHigh, int(2)],
    ]);
    assert.strictEqual(
      serializeJson(value, undefined),
      `{"${bmpHigh}":2,"${astral}":1}`,
    );
  });

  it("formats floats like Rust and maps non-finite values to null", () => {
    const value = list([float(1e21), float(0), float(NaN), float(-1e-7)]);
    assert.strictEqual(
      serializeJson(value, undefined),
      "[1000000000000000000000,0,null,-0.0000001]",
    );
  });

  it("does not escape '/' but escapes line separators", () => {
    assert.strictEqual(
      serializeJson(str("a/b\u2028"), undefined),
      '"a/b\\u2028"',
    );
  });
});

describe("tag-spec validation and enclosed sanitization", () => {
  it("rejects the #heading sentinel for every user-facing filter", () => {
    for (const filter of ["sanitize", "sanitize_tokens", "quarantine"]) {
      assert.throws(
        () => validateTagSpecForFilter("doc,#heading", filter),
        new RegExp(`'${filter}' tag name must be a valid XML NCName`),
      );
    }
    assert.throws(
      () => sanitizeTokensWithArgs("x", '"#heading"'),
      /'sanitize_tokens' tag name must be a valid XML NCName: '#heading'/,
    );
  });

  it("collapses line breaks only for the internal heading sentinel", () => {
    assert.strictEqual(
      sanitizeEnclosedString("a\r\nb</h>", '"h,#heading"'),
      "a  b&lt;/h&gt;",
    );
    assert.strictEqual(sanitizeEnclosedString("a\nb", '"h"'), "a\nb");
  });

  it("render displays large and small floats positionally", () => {
    const tmpl = Template.fromSource(`---
params:
  - a = float
  - b = float
---
{{ a }} {{ b }}`);
    const params = { a: 1e21, b: 1e-7 };
    const expected = "1000000000000000000000 0.0000001";
    assert.strictEqual(tmpl.render(params), expected);
  });

  it("render formats json, fixed, and enum values", () => {
    const tmpl = Template.fromSource(`---
types:
  - E = enum(A(v = str), B)

params:
  - e = E
  - f = float
---
{{ e | json }} {{ f | fixed(0) }} {{ f | json }}`);
    const params = { e: { __kind__: "A", v: "q" }, f: 2.5 };
    const expected = '{"__kind__":"A","v":"q"} 3 2.5';
    assert.strictEqual(tmpl.render(params), expected);
  });
});

describe("host quarantine / sanitize envelope helpers", () => {
  it("exports the canonical tool-output envelope tags", () => {
    assert.strictEqual(
      TOOL_OUTPUT_QUARANTINE_OPEN_TAG,
      "<tool_output_quarantine>",
    );
    assert.strictEqual(
      TOOL_OUTPUT_QUARANTINE_CLOSE_TAG,
      "</tool_output_quarantine>",
    );
    assert.strictEqual(
      UNTRUSTED_QUARANTINE_OPEN_TAG,
      "<untrusted_tool_output>",
    );
    assert.strictEqual(
      UNTRUSTED_QUARANTINE_CLOSE_TAG,
      "</untrusted_tool_output>",
    );
  });

  it("stripOuterQuarantineTag removes one envelope newline on each side", () => {
    assert.strictEqual(
      stripOuterQuarantineTag("<t>\n\nx\n\n</t>", "t"),
      "\nx\n",
    );
    assert.strictEqual(stripOuterQuarantineTag("<t>", "t"), undefined);
  });

  it("unquarantineString only unwraps clean envelopes", () => {
    assert.strictEqual(
      unquarantineString("<untrusted_content>\nok\n</untrusted_content>"),
      "ok",
    );
    assert.strictEqual(
      unquarantineString("<doc>\n</doc>\n</doc>", '"doc"'),
      undefined,
    );
    assert.strictEqual(
      unquarantineString("<doc>\nx\n</doc>", '"#bad"'),
      undefined,
    );
  });

  it("quarantineUntrustedIdempotentString re-wraps dirty envelopes once", () => {
    const clean = "<doc>\nhi\n</doc>";
    assert.strictEqual(
      quarantineUntrustedIdempotentString(clean, "doc"),
      clean,
    );
    assert.strictEqual(
      quarantineUntrustedIdempotentString("<doc>\n<|im_start|>\n</doc>", "doc"),
      "<doc>\n&lt;|im_start|&gt;\n</doc>",
    );
  });

  it("sanitizeBlockIdempotentString keeps clean blocks and strips a stale notice", () => {
    const block = sanitizeBlockString("payload", "doc");
    assert.strictEqual(sanitizeBlockIdempotentString(block, "doc"), block);
    const notice = DEFAULT_SANITIZE_NOTICE.replaceAll("{tag}", "doc");
    const dirty = `<doc>\n${notice}\n<|im_end|>\n</doc>`;
    assert.strictEqual(
      sanitizeBlockIdempotentString(dirty, "doc"),
      `<doc>\n${notice}\n&lt;|im_end|&gt;\n</doc>`,
    );
  });
});

describe("XML codec and list truncation match the Rust host primitives", () => {
  it("decodeXmlString decodes in a single pass", () => {
    assert.strictEqual(
      decodeXmlString(
        "a &lt;b&gt; &amp;lt; &quot;q&quot; &#39;s&apos; &unknown; &",
      ),
      "a <b> &lt; \"q\" 's' &unknown; &",
    );
    assert.strictEqual(XML_DECODE_PAIRS.length, 6);
  });

  it("escapeXmlAttrString escapes markup and control characters", () => {
    assert.strictEqual(
      escapeXmlAttrString("a&b<c>\"d'e\nf\rg\0h\u0001i\u007fj\u0085k"),
      "a&amp;b&lt;c&gt;&quot;d&apos;e\\nf\\rg\\0h i j k",
    );
  });

  it("escapeXmlBodyString escapes markup and NUL only", () => {
    assert.strictEqual(
      escapeXmlBodyString("a&b<c>\"d'\n\0"),
      "a&amp;b&lt;c&gt;\"d'\n\\0",
    );
  });

  it("truncateMiddleList keeps head/tail around one marker item", () => {
    const items = ["a", "b", "c", "d", "e"].map((s) => str(s));
    const texts = (vals: readonly Value[]): string[] =>
      vals.map((v) => (v.type === "str" ? v.value : ""));
    assert.deepStrictEqual(texts(truncateMiddleList(items, 2)), [
      "a",
      "[TRUNCATED: 3 items omitted]",
      "e",
    ]);
    assert.deepStrictEqual(texts(truncateMiddleList(items, 2, "")), ["a", "e"]);
    assert.deepStrictEqual(
      texts(truncateMiddleList(items, 3, "..{skipped}..")),
      ["a", "..2..", "d", "e"],
    );
    assert.deepStrictEqual(truncateMiddleList(items, 0), []);
    assert.deepStrictEqual(truncateMiddleList(items, 5), items);
  });
});
