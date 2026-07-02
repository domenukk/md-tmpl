---
types:
  - Status = enum(Active, Paused, Stopped)

params: [s = Status]
---

> {% match s case Active %}on{% else %}off{% /match %}
