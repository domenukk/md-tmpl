---
env:
  - MAX_RETRIES = int
  - DEBUG = bool := false

params:
  - name = str
---

Hello {{ name }}!
Retries: {{ MAX_RETRIES }}

> {% if DEBUG %}

Debug mode enabled.

> {% /if %}
