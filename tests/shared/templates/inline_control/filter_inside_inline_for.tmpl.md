---
params: [items = list(name = str)]
---

> {% for item in items %}{{ item.name | lower }} {% /for %}
