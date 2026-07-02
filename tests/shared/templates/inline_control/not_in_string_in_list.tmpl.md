---
params: [x = str, items = list(str)]
---

> {% if !(x in items) %}MISSING{% else %}FOUND{% /if %}
