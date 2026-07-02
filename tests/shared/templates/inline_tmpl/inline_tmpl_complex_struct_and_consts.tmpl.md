---
params: [user = struct(name = str, role = str)]
---

> {% tmpl card %}

---

params: [u = struct(name = str, role = str)]
consts:

- BADGE = str := "[VERIFIED]"

---

{{ BADGE }} {{ u.name }} ({{ u.role }})

> {% /tmpl %}
> {% include card with u=user %}
