---
types:
  - Role = enum(Admin, Editor, Viewer)

params: [role_str = str]
---

> {% if "Superuser" in kinds(Role) %}YES {{ role_str }}{% else %}NO{% /if %}
