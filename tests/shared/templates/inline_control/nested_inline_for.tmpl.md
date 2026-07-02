---
params: [groups = list(items = list(str))]
---

> {% for group in groups %}[{% for item in group.items %}{{ item }}{% /for %}]{% /for %}
