/**
 * Correctness cases (WASM vs TS): Additional SPEC.md gap coverage.
 */

import { testContent, testExact } from "./harness_correctness.js";

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

> {% raw %}...{% /raw %}

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
