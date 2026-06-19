---
name: code_review
description: A code review template
params:
  - file_path = str
  - severity = str
  - findings = list<line = int, message = str>
---

# Code Review: {{ file_path }}

Severity: {{ severity }}

## Findings

> {% for finding in findings %}

- Line {{ finding.line }}: {{ finding.message }}

  > {% /for %}
