---
params: [name = str]
---

> {% tmpl inner %}

---

params: [val = str]
---

[{{ val }}]

> {% /tmpl %}

> {% tmpl outer %}

---

params: [name = str]
---

> {% include inner with val=name %}

> {% /tmpl %}

> {% include outer with name=name %}
