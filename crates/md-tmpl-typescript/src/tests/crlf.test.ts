/**
 * CRLF normalization tests — verifies that templates with Windows `\r\n`
 * line endings produce byte-identical output to Unix `\n` templates.
 *
 * Mirrors the Rust core tests:
 *   - `crlf_from_source_produces_lf_output`
 *   - `crlf_compile_produces_lf_output`
 *   - `crlf_from_file_produces_lf_output`
 *
 * @module
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template } from "../index.js";

describe("CRLF normalization", () => {
  it("fromSource normalizes \\r\\n to \\n in output", () => {
    // Simulate a template read on Windows — every line ends with \r\n.
    const source =
      "---\r\nparams: [name = str]\r\n---\r\nHello {{ name }}!\r\nGoodbye\r\n";
    const tmpl = Template.fromSource(source);
    const output = tmpl.render({ name: "world" });
    assert.ok(
      !output.includes("\r"),
      `output must not contain \\r: ${JSON.stringify(output)}`,
    );
    assert.strictEqual(output, "Hello world!\nGoodbye\n");
  });

  it("fromSourceWithOptions normalizes \\r\\n", () => {
    const source =
      "---\r\nparams: [x = str]\r\n---\r\nLine1 {{ x }}\r\nLine2\r\n";
    const tmpl = Template.fromSourceWithOptions(source, {});
    const output = tmpl.render({ x: "val" });
    assert.ok(
      !output.includes("\r"),
      `output must not contain \\r: ${JSON.stringify(output)}`,
    );
    assert.strictEqual(output, "Line1 val\nLine2\n");
  });

  it("mixed \\r\\n and \\n produces consistent \\n output", () => {
    // Some lines CRLF, some LF — both should normalize to LF.
    const source = "---\r\nparams: []\n---\r\nA\nB\r\nC\r\n";
    const tmpl = Template.fromSource(source);
    const output = tmpl.renderEmpty();
    assert.ok(
      !output.includes("\r"),
      `output must not contain \\r: ${JSON.stringify(output)}`,
    );
    assert.strictEqual(output, "A\nB\nC\n");
  });

  it("pure \\n source is unchanged (no allocation overhead)", () => {
    const source = "---\nparams: [x = str]\n---\n{{ x }}";
    const tmpl = Template.fromSource(source);
    assert.strictEqual(tmpl.render({ x: "ok" }), "ok");
  });

  it("CRLF in frontmatter block list items", () => {
    const source =
      "---\r\nparams:\r\n  - name = str\r\n  - count = int\r\n---\r\n{{ name }} {{ count }}";
    const tmpl = Template.fromSource(source);
    const output = tmpl.render({ name: "Alice", count: 5 });
    assert.ok(!output.includes("\r"));
    assert.strictEqual(output, "Alice 5");
  });

  it("CRLF in multiline template body with control flow", () => {
    const source =
      "---\r\nparams: [show = bool]\r\n---\r\n> {% if show %}yes{% else %}no{% /if %}\r\n";
    const tmpl = Template.fromSource(source);
    const output = tmpl.render({ show: true });
    assert.ok(!output.includes("\r"));
    assert.strictEqual(output, "yes");
  });
});
