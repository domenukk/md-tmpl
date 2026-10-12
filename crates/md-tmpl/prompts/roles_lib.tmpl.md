---
name: roles_lib
description: Shared work-role enum for cross-template imported-type reuse tests
allow_unused: true
types:
  - WorkRole = enum(Researcher, Judge, Orchestrator)
  - WorkerSpec = struct(handle = str, quota = int)
  - TaskRows = list(task_id = str, points = int)
---

> {# Type-only library — no body content #}
