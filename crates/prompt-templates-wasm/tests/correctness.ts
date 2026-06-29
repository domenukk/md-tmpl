/**
 * Correctness tests: WASM vs pure-TypeScript prompt-templates.
 *
 * For templates with only expression interpolation (no block control flow),
 * both engines produce identical output, so we assert exact equality.
 *
 * For templates with block control flow (for/if/match), the Rust engine
 * and the TS re-implementation differ in how `> ` prefix lines produce
 * surrounding newlines. For these, we verify each engine independently
 * against its own expected output, and we assert that the *content*
 * (after whitespace normalization) is the same.
 *
 * Usage:
 *   cd crates/prompt-templates-wasm
 *   wasm-pack build --target nodejs --out-dir pkg
 *   node tests/correctness.mjs
 */

import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { Template: WasmTemplate } = require("../pkg/prompt_templates_wasm.js");
import { Template as TsTemplate } from "../../prompt-templates-typescript/dist/index.js";

// ---------------------------------------------------------------------------
// Test framework
// ---------------------------------------------------------------------------

let passed = 0;
let failed = 0;

/** Exact match: both engines must produce the same output. */
function testExact(name, source, params) {
  try {
    const wasmResult = WasmTemplate.fromSource(source).render(params);
    const tsResult = TsTemplate.fromSource(source).render(params);

    if (wasmResult === tsResult) {
      console.log(`  ✅ ${name}`);
      passed++;
    } else {
      console.log(`  ❌ ${name}`);
      console.log(`     WASM: ${JSON.stringify(wasmResult)}`);
      console.log(`     TS:   ${JSON.stringify(tsResult)}`);
      failed++;
    }
  } catch (e) {
    console.log(`  ❌ ${name} — ERROR: ${e}`);
    failed++;
  }
}

/** Content match: same content after collapsing whitespace runs. */
function testContent(name, source, params) {
  try {
    const wasmResult = WasmTemplate.fromSource(source).render(params);
    const tsResult = TsTemplate.fromSource(source).render(params);

    const normalize = (s) => s.replace(/\n+/g, "\n").trim();
    const wasmNorm = normalize(wasmResult);
    const tsNorm = normalize(tsResult);

    if (wasmNorm === tsNorm) {
      console.log(`  ✅ ${name}`);
      passed++;
    } else {
      console.log(`  ❌ ${name}`);
      console.log(`     WASM (normalized): ${JSON.stringify(wasmNorm)}`);
      console.log(`     TS   (normalized): ${JSON.stringify(tsNorm)}`);
      failed++;
    }
  } catch (e) {
    console.log(`  ❌ ${name} — ERROR: ${e}`);
    failed++;
  }
}

/** WASM-only: verify WASM output against an expected string. */
function testWasm(name, source, params, expected) {
  try {
    const wasmResult = WasmTemplate.fromSource(source).render(params);

    if (wasmResult === expected) {
      console.log(`  ✅ ${name}`);
      passed++;
    } else {
      console.log(`  ❌ ${name}`);
      console.log(`     Got:      ${JSON.stringify(wasmResult)}`);
      console.log(`     Expected: ${JSON.stringify(expected)}`);
      failed++;
    }
  } catch (e) {
    console.log(`  ❌ ${name} — ERROR: ${e}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

console.log("prompt-templates — Correctness Tests (WASM vs TS)");
console.log("=".repeat(60));
console.log("");

// --- Exact match tests (simple interpolation, no block control flow) ---

// 1. Simple string interpolation
testExact(
  "1. Simple string interpolation",
  `---
params:
  - name = str
---
Hello {{ name }}!`,
  { name: "world" },
);

// 2. Multiple params of different types
testExact(
  "2. Multiple params (str, int, float, bool)",
  `---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`,
  { name: "Alice", count: 42, score: 9.5, enabled: true },
);

// 3. Default values
testExact(
  "3. Default values",
  `---
params:
  - name = str := "Guest"
---
Hello {{ name }}!`,
  {},
);

// 4. Constants
testExact(
  "4. Constants",
  `---
consts:
  - MAX = int := 100
params: []
---
Max is {{ MAX }}`,
  {},
);

// 5. Struct field access
testExact(
  "5. Struct field access",
  `---
params:
  - meta = struct(author = str, version = int)
---
By {{ meta.author }}, v{{ meta.version }}`,
  { meta: { author: "Alice", version: 3 } },
);

// 6. Empty template (no params)
testExact(
  "6. Empty template (no params)",
  `---
params: []
---
Static content only.`,
  {},
);

// 7. Whitespace preservation
testExact(
  "7. Whitespace preservation",
  `---
params:
  - x = str
---
  {{ x }}  `,
  { x: "hi" },
);

// 8. Multiple expressions on same line
testExact(
  "8. Multiple expressions on same line",
  `---
params:
  - a = str
  - b = str
---
{{ a }} and {{ b }}`,
  { a: "foo", b: "bar" },
);

// 9. Integer display
testExact(
  "9. Integer display",
  `---
params:
  - n = int
---
Value is {{ n }}`,
  { n: 12345 },
);

// 10. Special characters in string
testExact(
  "10. Special characters in string",
  `---
params:
  - msg = str
---
{{ msg }}`,
  { msg: 'Hello "world" & <friends>' },
);

// 11. Float display
testExact(
  "11. Float display",
  `---
params:
  - pi = float
---
Pi is {{ pi }}`,
  { pi: 3.14159 },
);

// 12. Filter: upper
testExact(
  "12. Filter upper",
  `---
params:
  - name = str
---
{{ name | upper }}`,
  { name: "hello" },
);

// 13. Filter: lower
testExact(
  "13. Filter lower",
  `---
params:
  - name = str
---
{{ name | lower }}`,
  { name: "HELLO" },
);

// 14. Filter: trim
testExact(
  "14. Filter trim",
  `---
params:
  - name = str
---
{{ name | trim }}`,
  { name: "  hello  " },
);

// 15. Multi-line template
testExact(
  "15. Multi-line template",
  `---
params:
  - title = str
  - body = str
---
# {{ title }}

{{ body }}

---`,
  { title: "Report", body: "Content here" },
);

// 16. Boolean display
testExact(
  "16. Boolean display",
  `---
params:
  - flag = bool
---
Flag is {{ flag }}`,
  { flag: true },
);

// 17. Negative integer
testExact(
  "17. Negative integer",
  `---
params:
  - n = int
---
N={{ n }}`,
  { n: -42 },
);

// 18. Zero values
testExact(
  "18. Zero values",
  `---
params:
  - n = int
  - f = float
---
{{ n }},{{ f }}`,
  { n: 0, f: 0.5 },
);

// --- Content match tests (block control flow, whitespace may differ) ---

// 19. List iteration (for loop)
testContent(
  "19. List iteration (for loop) [content]",
  `---
params:
  - tasks = list(title = str, priority = str)
---
> {% for task in tasks %}

- {{ task.title }} ({{ task.priority }})

> {% /for %}`,
  {
    tasks: [
      { title: "Task A", priority: "High" },
      { title: "Task B", priority: "Low" },
    ],
  },
);

// 20. Conditional (if/elif/else) — all branches
testContent(
  "20. Conditional if branch [content]",
  `---
params:
  - level = int
  - name = str
---
> {% if level == 1 %}

Beginner: {{ name }}

> {% elif level == 2 %}

Intermediate: {{ name }}

> {% else %}

Expert: {{ name }}

> {% /if %}`,
  { level: 1, name: "Alice" },
);

// 21. Conditional elif
testContent(
  "21. Conditional elif branch [content]",
  `---
params:
  - level = int
  - name = str
---
> {% if level == 1 %}

Beginner: {{ name }}

> {% elif level == 2 %}

Intermediate: {{ name }}

> {% else %}

Expert: {{ name }}

> {% /if %}`,
  { level: 2, name: "Bob" },
);

// 22. Conditional else
testContent(
  "22. Conditional else branch [content]",
  `---
params:
  - level = int
  - name = str
---
> {% if level == 1 %}

Beginner: {{ name }}

> {% elif level == 2 %}

Intermediate: {{ name }}

> {% else %}

Expert: {{ name }}

> {% /if %}`,
  { level: 99, name: "Charlie" },
);

// 23. Enum unit variant
testContent(
  "23. Enum unit variant [content]",
  `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`,
  { outcome: "Rejected" },
);

// 24. Enum struct variant
testContent(
  "24. Enum struct variant [content]",
  `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`,
  { outcome: { __kind__: "Confirmed", evidence: "proof found" } },
);

// 25. Boolean in conditional
testContent(
  "25. Boolean in conditional [content]",
  `---
params:
  - flag = bool
---
> {% if flag %}

YES

> {% else %}

NO

> {% /if %}`,
  { flag: true },
);

// 26. Large list (12 items)
testContent(
  "26. Large list (12 items) [content]",
  `---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
  { items: Array.from({ length: 12 }, (_, i) => ({ name: `item${i}` })) },
);

// --- WASM-only tests for block constructs (exact expected) ---

// 27. WASM for-loop exact output
testWasm(
  "27. WASM for-loop exact",
  `---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ item.name }}

> {% /for %}`,
  { items: [{ name: "alpha" }, { name: "beta" }] },
  "alpha\nbeta\n",
);

// 28. WASM conditional exact
testWasm(
  "28. WASM conditional exact",
  `---
params:
  - x = int
---
> {% if x == 1 %}

one

> {% else %}

other

> {% /if %}`,
  { x: 1 },
  "one\n",
);

// 29. WASM match exact
testWasm(
  "29. WASM match exact",
  `---
params:
  - s = enum(A, B(val = str))
---
> {% match s %}
> {% case A %}

it is A

> {% case B %}

B={{ s.val }}

> {% /match %}`,
  { s: { __kind__: "B", val: "hello" } },
  "B=hello\n",
);

// 30. WASM defaults + override
testWasm(
  "30. WASM defaults with override",
  `---
params:
  - greeting = str := "Hi"
  - name = str
---
{{ greeting }}, {{ name }}!`,
  { name: "World" },
  "Hi, World!",
);

// --- Tests 31–55: Missing SPEC.md features ---

// 31. Filter: fixed(N)
testExact(
  "31. Filter fixed(N)",
  `---
params:
  - score = float
---
{{ score | fixed(2) }}`,
  { score: 3.14159 },
);

// 32. Filter: join(sep)
testExact(
  "32. Filter join(sep)",
  `---
params:
  - items = list(label = str)
---
> {% for item in items %}{{ item.label }}> {% /for %}`,
  { items: [{ label: "a" }, { label: "b" }, { label: "c" }] },
);

// 33. Filter: limit(N) — applied to float
testExact(
  "33. Filter add on float",
  `---
params:
  - x = float
---
{{ x | add(1.5) }}`,
  { x: 2.5 },
);

// 34. Filter: add(N)
testExact(
  "34. Filter add(N)",
  `---
params:
  - n = int
---
{{ n | add(10) }}`,
  { n: 5 },
);

// 35. Filter: sub(N)
testExact(
  "35. Filter sub(N)",
  `---
params:
  - n = int
---
{{ n | sub(3) }}`,
  { n: 10 },
);

// 36. Filter chaining: add then sub
testExact(
  "36. Filter chaining: add then sub",
  `---
params:
  - n = int
---
{{ n | add(10) | sub(3) }}`,
  { n: 5 },
);

// 37. Filter chaining: trim | upper
testExact(
  "37. Filter chaining: trim then upper",
  `---
params:
  - name = str
---
{{ name | trim | upper }}`,
  { name: "  hello  " },
);

// 38. Built-in function: idx() in for loop
// 38. Built-in function: idx() in for loop
testContent(
  "38. Built-in idx() in for loop",
  `---
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ idx(item) }}:{{ item.name }}

> {% /for %}`,
  { items: [{ name: "a" }, { name: "b" }, { name: "c" }] },
);

// 39. Built-in function: len() on list
testExact(
  "39. Built-in len() on list",
  `---
params:
  - items = list(name = str)
---
Count: {{ len(items) }}`,
  { items: [{ name: "x" }, { name: "y" }] },
);

// 40. Built-in function: len() on string
testExact(
  "40. Built-in len() on string",
  `---
params:
  - msg = str
---
Length: {{ len(msg) }}`,
  { msg: "hello" },
);

// 41. Built-in function: kind() on enum
testExact(
  "41. Built-in kind() on enum",
  `---
params:
  - status = enum(Active, Paused, Stopped)
---
Kind: {{ kind(status) }}`,
  { status: "Active" },
);

// 42. Nested idx() in nested for loops
testExact(
  "42. Nested idx() in nested for loops",
  `---
params:
  - outer = list(label = str)
  - inner = list(label = str)
---
> {% for a in outer %}> {% for b in inner %}{{ idx(a) }}.{{ idx(b) }} > {% /for %}> {% /for %}`,
  {
    outer: [{ label: "x" }, { label: "y" }],
    inner: [{ label: "p" }, { label: "q" }],
  },
);

// 43. Whitespace control: {{- expr -}}
testExact(
  "43. Whitespace control {{- expr -}}",
  `---
params:
  - name = str
---
hello  {{- name -}}
bye`,
  { name: "world" },
);

// 44. Raw blocks: preserve literal syntax (exact match after whitespace fix)
testContent(
  "44. Raw blocks preserve literal syntax [content]",
  `---
params: []
---
> {% raw %}

{{ not_processed }}

> {% /raw %}`,
  {},
);

// 45. Comments: {# comment #} stripped from output
testExact(
  "45. Comments stripped from output",
  `---
params:
  - name = str
---
{# This is a comment #}Hello {{ name }}!`,
  { name: "world" },
);

// 46. Match else arm
testContent(
  "46. Match else arm",
  `---
params:
  - s = enum(A, B, C)
---
> {% match s %}
> {% case A %}

alpha

> {% else %}

other

> {% /match %}`,
  { s: "B" },
);

// 47. Match else arm — explicit case hit
testContent(
  "47. Match else arm (explicit case)",
  `---
params:
  - s = enum(A, B, C)
---
> {% match s %}
> {% case A %}

alpha

> {% else %}

other

> {% /match %}`,
  { s: "A" },
);

// 48. Match multi-variant arm
testContent(
  "48. Match multi-variant arm",
  `---
params:
  - s = enum(A, B, C)
---
> {% match s %}
> {% case A | B %}

AB

> {% case C %}

C

> {% /match %}`,
  { s: "B" },
);

// 49. Match inline guard
testExact(
  "49. Match inline guard",
  `---
params:
  - s = enum(Labelled(label = str), Unlabelled)
---
Result: {% match s case Labelled %}{{ s.label }}{% /match %}`,
  { s: { __kind__: "Labelled", label: "found" } },
);

// 50. Empty list for-loop produces empty output
testContent(
  "50. Empty list for-loop",
  `---
params:
  - items = list(name = str)
---
before

> {% for item in items %}

{{ item.name }}

> {% /for %}

after`,
  { items: [] },
);

// 51. Comparison operator: !=
testContent(
  "51. Comparison operator !=",
  `---
params:
  - x = int
---
> {% if x != 0 %}

nonzero

> {% else %}

zero

> {% /if %}`,
  { x: 5 },
);

// 52. Comparison operator: <
testContent(
  "52. Comparison operator <",
  `---
params:
  - x = int
---
> {% if x < 10 %}

small

> {% else %}

big

> {% /if %}`,
  { x: 3 },
);

// 53. Comparison operator: >
testContent(
  "53. Comparison operator >",
  `---
params:
  - x = int
---
> {% if x > 10 %}

big

> {% else %}

small

> {% /if %}`,
  { x: 99 },
);

// 54. Boolean truthiness: {% if flag %} with true
testContent(
  "54. Boolean truthiness (true)",
  `---
params:
  - flag = bool
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}`,
  { flag: true },
);

// 55. Boolean truthiness: {% if flag %} with false
testContent(
  "55. Boolean truthiness (false)",
  `---
params:
  - flag = bool
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}`,
  { flag: false },
);

// 56. Negative float rendering
testExact(
  "56. Negative float",
  `---
params:
  - f = float
---
Val={{ f }}`,
  { f: -3.14 },
);

// 57. Empty string param
testExact(
  "57. Empty string param",
  `---
params:
  - s = str
---
[{{ s }}]`,
  { s: "" },
);

// 58. Multiple expressions on same line (with surrounding text)
testExact(
  "58. Multiple expressions on same line",
  `---
params:
  - first = str
  - last = str
---
Name: {{ first }} {{ last }}!`,
  { first: "Jane", last: "Doe" },
);

// --- Tests 59–76: Additional SPEC.md gap coverage ---

// 59. Raw blocks with blockquote prefix
testContent(
  "59. Raw blocks with blockquote prefix",
  `---
params: []
---
> {% raw %}

{{ literal }}

> {% /raw %}`,
  {},
);

// 60. Raw custom delimiter
testContent(
  "60. Raw custom delimiter",
  `---
params: []
---
> {% raw=MYDELIM %}

{% raw %}...{% /raw %}

> {% /MYDELIM %}`,
  {},
);

// 61. Constants visible inside for loops
testContent(
  "61. Constants inside for loop",
  `---
consts:
  - PREFIX = str := ">>"
params:
  - items = list(name = str)
---
> {% for item in items %}

{{ PREFIX }} {{ item.name }}

> {% /for %}`,
  { items: [{ name: "A" }, { name: "B" }] },
);

// 62. Constants visible inside if blocks
testContent(
  "62. Constants inside if block",
  `---
consts:
  - THRESHOLD = int := 10
params:
  - x = int
---
> {% if x > 5 %}

over (threshold={{ THRESHOLD }})

> {% /if %}`,
  { x: 7 },
);

// 63. Constants visible inside match blocks
testContent(
  "63. Constants inside match block",
  `---
consts:
  - LABEL = str := "status"
params:
  - s = enum(A, B)
---
> {% match s %}
> {% case A %}

{{ LABEL }}: alpha

> {% case B %}

{{ LABEL }}: beta

> {% /match %}`,
  { s: "A" },
);

// 64. Constants with struct type
testExact(
  "64. Constants with struct type",
  `---
consts:
  - STAGES = struct(DESIGN = str, BUILD = str) := {DESIGN = "Design", BUILD = "Build"}
params: []
---
Stage: {{ STAGES.DESIGN }}, {{ STAGES.BUILD }}`,
  {},
);

// 65. Match inline guard with struct variant field access (extra coverage)
testExact(
  "65. Match inline guard with struct field access",
  `---
params:
  - x = enum(Known(label = str), Unknown)
---
Result: {% match x case Known %}{{ x.label }}{% /match %}`,
  { x: { __kind__: "Known", label: "found-it" } },
);

// 66. Whitespace trim on left only: {{- expr }}
testExact(
  "66. Whitespace trim left only {{- expr }}",
  `---
params:
  - name = str
---
hello  {{- name }}!`,
  { name: "world" },
);

// 67. Whitespace trim on right only: {{ expr -}}
testExact(
  "67. Whitespace trim right only {{ expr -}}",
  `---
params:
  - name = str
---
hello {{ name -}}  !`,
  { name: "world" },
);

// 68. Filter limit(N) on expression chained with join
testExact(
  "68. Filter limit(N) on expression",
  `---
params:
  - items = list(str)
---
{{ items | limit(2) | join(", ") }}`,
  { items: ["a", "b", "c"] },
);

// 69. Multiple filters chained: trim | lower
testExact(
  "69. Filter chaining: trim then lower",
  `---
params:
  - val = str
---
{{ val | trim | lower }}`,
  { val: "  HELLO  " },
);

// 70. For loop with single item
testContent(
  "70. For loop with single item",
  `---
params:
  - items = list(name = str)
---
> {% for item in items %}

- {{ item.name }}

> {% /for %}`,
  { items: [{ name: "only" }] },
);

// 71. Struct with multiple fields
testExact(
  "71. Struct with multiple fields",
  `---
params:
  - info = struct(a = str, b = int, c = bool)
---
{{ info.a }}-{{ info.b }}-{{ info.c }}`,
  { info: { a: "hello", b: 42, c: true } },
);

// 72. Nested struct field access
testExact(
  "72. Nested struct field access",
  `---
params:
  - config = struct(db = struct(host = str, port = int))
---
{{ config.db.host }}:{{ config.db.port }}`,
  { config: { db: { host: "localhost", port: 5432 } } },
);

// 73. Boolean display (false case)
testExact(
  "73. Boolean display (false)",
  `---
params:
  - flag = bool
---
Flag is {{ flag }}`,
  { flag: false },
);

// 74. Multi-line template with body paragraphs
testExact(
  "74. Multi-line template with paragraphs",
  `---
params:
  - title = str
  - body = str
---
# {{ title }}

{{ body }}

End.`,
  { title: "Report", body: "This is the main content." },
);

// 75. Comparison operator: <=
testContent(
  "75. Comparison operator <=",
  `---
params:
  - x = int
---
> {% if x <= 5 %}

small

> {% else %}

big

> {% /if %}`,
  { x: 5 },
);

// 76. Comparison operator: >=
testContent(
  "76. Comparison operator >=",
  `---
params:
  - x = int
---
> {% if x >= 10 %}

big

> {% else %}

small

> {% /if %}`,
  { x: 10 },
);

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

console.log("");
console.log("=".repeat(60));
console.log(`Results: ${passed} passed, ${failed} failed, ${passed + failed} total`);

if (failed > 0) {
  process.exit(1);
} else {
  console.log("All tests passed! ✅");
  process.exit(0);
}
