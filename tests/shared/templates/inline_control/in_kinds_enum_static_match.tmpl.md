---
types:
  - Role = enum(Admin, Editor, Viewer)

params: [role_str = str]
---

> {% if role_str in kinds(Role) %}VALID_ROLE{% else %}INVALID_ROLE{% /if %}
