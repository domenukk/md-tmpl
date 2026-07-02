---
types:
  Role:
    enum: [Admin, Editor, Viewer]
params:
  role_str: str
---

> {% if role_str in kinds(Role) %}YES{% else %}NO{% /if %}
