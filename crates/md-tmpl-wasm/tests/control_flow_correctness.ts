/**
 * Correctness cases (WASM vs TS): Block control flow: content-match and WASM-only exact cases.
 */

import { testContent, testWasm } from "./harness_correctness.js";

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
