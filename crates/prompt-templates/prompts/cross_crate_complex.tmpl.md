---
name: cross_crate_complex
description: Complex template for cross-crate integration tests
types:
  - Role = enum<Admin, Editor, Viewer>
params:
  - username = str
  - role = Role
  - score = float
  - active = bool
  - tags = list<label = str>
---

User: {{ username }}
Role: {{ kind(role) }}
Score: {{ score }}
Active: {{ active }}

Tags:

> {% for tag in tags %}

- {{ tag.label }}

  > {% /for %}
