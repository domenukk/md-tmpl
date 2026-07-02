---
consts:
  - DEFAULT_FLAG = bool := true

params:
  - flag = bool := DEFAULT_FLAG
---

> {% if flag %}yes{% else %}no{% /if %}
