---
name: role_consumer
description: Consumer template referencing an imported enum type via a param
imports:
  - "[roles_lib](./roles_lib.tmpl.md)"

params:
  - role = roles_lib.WorkRole
---

Role: {{ kind(role) }}
