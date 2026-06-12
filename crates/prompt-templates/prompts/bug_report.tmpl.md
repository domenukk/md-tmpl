---
name: bug_report
description: A bug report template with types
types:
  - Severity = enum<Critical, High, Medium, Low>
params:
  - title = str
  - severity = Severity
  - bugs = list<name = str, priority = Severity>
---

# Bug Report: {{ title }}

Severity: {{ severity }}

> {% for bug in bugs %}

- {{ bug.name }} ({{ bug.priority }})
  > {% /for %}
