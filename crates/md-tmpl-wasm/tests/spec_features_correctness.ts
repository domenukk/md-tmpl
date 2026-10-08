/**
 * Correctness cases (WASM vs TS): SPEC.md feature coverage: filters, built-ins, whitespace, raw, match, comparisons.
 */

import { testContent, testExact } from "./harness_correctness.js";

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
Hello {# comment inside #}{{ name }}!`,
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
