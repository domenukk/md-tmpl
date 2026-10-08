/**
 * Correctness cases (WASM vs TS): Exact-match cases (simple interpolation, no block control flow).
 */

import { testExact } from "./harness_correctness.js";

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
