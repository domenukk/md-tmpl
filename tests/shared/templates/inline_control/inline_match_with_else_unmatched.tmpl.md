---
types:
  - Status = enum(Active, Stopped, Other)

params: [x = Status]
---

> {% match x case Active %}ON{% else %}OFF{% /match %}
