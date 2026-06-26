---
name: type_library
description: Test template demonstrating type alias generation via include_template!
types:
  - Priority = enum<Low, Medium, High, Critical>
  - Status = enum<Open, InProgress, Resolved, Closed>
  - Outcome = enum<Confirmed(evidence = str), Rejected>
  - TaskItem = list<id = str, title = str, priority = Priority>

consts:
  - APP_NAME = str := "TestApp"
  - MAX_RETRIES = int := 3

allow_unused: true
---

> {# Type-only template — no body content #}
