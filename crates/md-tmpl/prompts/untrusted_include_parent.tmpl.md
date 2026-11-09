---
name: untrusted_include_parent
description: Exercises value-carried trust across compile-time file includes inside an enclosing XML tag
params:
  - doc = str
  - trusted_doc = trusted str
---
<outer>

> {% include [child](./untrusted_include_child.tmpl.md) with t = doc %}
> {% include [child](./untrusted_include_child.tmpl.md) with t = trusted_doc %}
> {% include [child](./untrusted_include_child.tmpl.md) with t = unsafe_trust(doc) %}
> {% include [child](./untrusted_include_child.tmpl.md) with t = "<|im_start|>lit" %}
> {% include [child](./untrusted_include_child.tmpl.md) with t = "a | b" | unsafe_trust %}

</outer>