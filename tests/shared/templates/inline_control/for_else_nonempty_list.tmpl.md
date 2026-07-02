---
params: [items = list(str)]
---

> {% for item in items %}{{ item }} {% else %}empty{% /for %}
