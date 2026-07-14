/**
 * Tests for Milestone M1.2: Accurate 1-indexed syntax error diagnostics
 * (line, column, and snippet) on TemplateSyntaxError instances across md-tmpl-typescript.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  Template,
  TemplateError,
  TemplateSyntaxError,
  MissingParamsError,
  TypeMismatchError,
} from "../index.js";

describe("M1.2: Accurate error location diagnostics (line, column, snippet)", () => {
  describe("Frontmatter syntax errors", () => {
    it("reports 1-indexed line and snippet for malformed declaration line", () => {
      const src = `---
params:
  - invalid declaration without equal sign
---
Hello`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          3,
          "expected error on line 3 of frontmatter",
        );
        assert.strictEqual(
          err.column,
          1,
          "expected error column 1 for frontmatter declaration",
        );
        assert.ok(
          err.snippet?.includes("invalid declaration"),
          `unexpected snippet: ${err.snippet}`,
        );
      }
    });

    it("reports 1-indexed line for duplicate parameter name", () => {
      const src = `---
params:
  - foo = str
  - foo = int
---
Hello {{ foo }}`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          4,
          "expected error on line 4 for duplicate param",
        );
        assert.strictEqual(
          err.column,
          1,
          "expected error column 1 for duplicate param",
        );
        assert.ok(
          err.message.includes("duplicate"),
          `unexpected message: ${err.message}`,
        );
      }
    });
  });

  describe("Parser syntax errors in body", () => {
    it("reports 1-indexed line for unclosed if block", () => {
      const src = `---
params:
  - flag = bool
---
Line 5
> {% if flag %}
Line 7
Line 8`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          6,
          "expected unclosed if tag reported on line 6",
        );
        assert.strictEqual(
          err.column,
          1,
          "expected error column 1 for standalone statement tag neighbor violation",
        );
        assert.ok(
          err.snippet?.includes("if flag"),
          `unexpected snippet: ${err.snippet}`,
        );
      }
    });

    it("reports 1-indexed line and column for unclosed if block with valid blockquote spacing", () => {
      const src = `---
params:
  - flag = bool
---
Line 5

> {% if flag %}

Line 9
Line 10`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          7,
          "expected unclosed if tag reported on line 7",
        );
        assert.strictEqual(
          err.column,
          3,
          "expected error column 3 for unclosed if block opening tag",
        );
        assert.ok(
          err.snippet?.includes("if flag"),
          `unexpected snippet: ${err.snippet}`,
        );
        assert.ok(
          err.message.includes("unclosed '{% if %}' block"),
          `unexpected message: ${err.message}`,
        );
      }
    });

    it("reports 1-indexed line for unknown statement tag", () => {
      const src = `---
params:
  - name = str
---
Hello
{% foobar %}
World`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          6,
          "expected unknown tag reported on line 6",
        );
        assert.strictEqual(
          err.column,
          1,
          "expected error column 1 for non-prefixed statement tag",
        );
        assert.ok(
          err.snippet?.includes("foobar"),
          `unexpected snippet: ${err.snippet}`,
        );
      }
    });

    it("reports 1-indexed line for malformed for loop syntax", () => {
      const src = `---
params:
  - items = list(str)
---
Line 5
{% for item items %}
Line 7
{% /for %}`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          6,
          "expected malformed for loop on line 6",
        );
        assert.strictEqual(
          err.column,
          1,
          "expected error column 1 for malformed for loop on line 6",
        );
        assert.ok(
          err.snippet?.includes("for item items"),
          `unexpected snippet: ${err.snippet}`,
        );
      }
    });
  });

  describe("Validation and displayability errors", () => {
    it("reports 1-indexed line for non-displayable struct interpolation", () => {
      const src = `---
params:
  - user = struct(name = str, age = int)
---
Hello world
User info: {{ user }}
End of template`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          6,
          "expected displayability error on line 6",
        );
        assert.strictEqual(
          err.column,
          12,
          "expected error column 12 for non-displayable struct interpolation",
        );
        assert.ok(
          err.snippet?.includes("{{ user }}"),
          `unexpected snippet: ${err.snippet}`,
        );
      }
    });
  });

  describe("Template parameter check errors", () => {
    it("reports 1-indexed line for unused parameter declaration", () => {
      const src = `---
params:
  - used_param = str
  - unused_param = int
---
Hello {{ used_param }}`;
      try {
        Template.fromSource(src);
        assert.fail("should have thrown TemplateSyntaxError");
      } catch (err) {
        assert.ok(
          err instanceof TemplateSyntaxError,
          `expected TemplateSyntaxError, got ${err}`,
        );
        assert.strictEqual(
          err.line,
          4,
          "expected unused param error reported on declaration line 4",
        );
        assert.strictEqual(
          err.column,
          1,
          "expected error column 1 for unused param",
        );
        assert.ok(
          err.snippet?.includes("unused_param"),
          `unexpected snippet: ${err.snippet}`,
        );
      }
    });
  });

  describe("Early compile-time strictness checks", () => {
    it("reports unknown filter at compile time in fromSource", () => {
      const src = `---
params:
  - name = str
---
Hello {{ name | nonexistent_filter }}`;
      assert.throws(
        () => Template.fromSource(src),
        (err: unknown) => {
          assert.ok(err instanceof Error);
          assert.strictEqual(err.constructor.name, "UnknownFilterError");
          return true;
        },
      );
    });

    it("reports empty variant name in match case at compile time", () => {
      const src = `---
params:
  - status = str
---
> {% match status %}
> {% case %}

> empty

> {% /match %}`;
      assert.throws(
        () => Template.fromSource(src),
        (err: unknown) => {
          assert.ok(err instanceof TemplateSyntaxError);
          assert.ok(
            (err as TemplateSyntaxError).message.includes("empty variant name"),
          );
          return true;
        },
      );
    });

    it("reports match without any case arms at compile time", () => {
      const src = `---
params:
  - status = str
---
> {% match status %}nothing here{% /match %}`;
      assert.throws(
        () => Template.fromSource(src),
        (err: unknown) => {
          assert.ok(err instanceof TemplateSyntaxError);
          assert.ok(
            (err as TemplateSyntaxError).message.includes(
              "no {% case %} arms found",
            ),
          );
          return true;
        },
      );
    });

    it("reports unexpected closing tag at compile time", () => {
      const src = `---
params: []
---
Hello

> {% /if %}

World`;
      assert.throws(
        () => Template.fromSource(src),
        (err: unknown) => {
          assert.ok(err instanceof TemplateSyntaxError);
          assert.ok(
            (err as TemplateSyntaxError).message.includes(
              "unexpected '{% /if %}'",
            ),
          );
          return true;
        },
      );
    });
  });
});

describe("Stable machine-readable error kinds (.kind)", () => {
  it("syntax error -> kind 'syntax'", () => {
    const src = `---
params:
  - name = str
---
Hello {{ name | nonexistent_filter }}`;
    try {
      Template.fromSource(src);
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(
        err instanceof TemplateError,
        `expected TemplateError, got ${err}`,
      );
      // Unknown filter surfaces as an UnknownFilterError at compile time.
      assert.strictEqual(err.kind, "unknown_filter");
      assert.strictEqual(err.name, "UnknownFilterError");
    }
  });

  it("malformed frontmatter -> TemplateSyntaxError with kind 'syntax'", () => {
    const src = `---
params:
  - invalid declaration without equal sign
---
Hello`;
    try {
      Template.fromSource(src);
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(
        err instanceof TemplateSyntaxError,
        `expected TemplateSyntaxError, got ${err}`,
      );
      assert.strictEqual(err.kind, "syntax");
      assert.strictEqual(err.name, "TemplateSyntaxError");
      // Structured fields remain intact alongside the new kind.
      assert.strictEqual(err.line, 3);
    }
  });

  it("missing param -> MissingParamsError with kind 'missing_params'", () => {
    const tmpl = Template.fromSource(`---
params:
  - name = str
---
Hello {{ name }}!`);
    try {
      tmpl.render({});
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(
        err instanceof MissingParamsError,
        `expected MissingParamsError, got ${err}`,
      );
      assert.strictEqual(err.kind, "missing_params");
      assert.strictEqual(err.name, "MissingParamsError");
      // Structured field is preserved.
      assert.deepStrictEqual([...err.missing], ["name"]);
    }
  });

  it("type mismatch -> TypeMismatchError with kind 'type_mismatch'", () => {
    const tmpl = Template.fromSource(`---
params:
  - n = int
---
{{ n }}`);
    try {
      tmpl.render({ n: "not_a_number" });
      assert.fail("should have thrown");
    } catch (err) {
      assert.ok(
        err instanceof TypeMismatchError,
        `expected TypeMismatchError, got ${err}`,
      );
      assert.strictEqual(err.kind, "type_mismatch");
      assert.strictEqual(err.name, "TypeMismatchError");
      assert.strictEqual(err.expected, "int");
    }
  });

  it("base TemplateError defaults kind to the empty-string sentinel", () => {
    const err = new TemplateError("generic");
    assert.strictEqual(err.kind, "");
    assert.strictEqual(err.name, "TemplateError");
  });
});
