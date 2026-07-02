---
params: [rows = list(name = str, score = int)]
---

> {% for row in rows %}

- {{ row.name }}: {{ row.score }}

> {% /for %}
