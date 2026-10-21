---
name: untrusted_include_parent
description: Passes an untrusted value into a plain-str child inside an enclosing XML tag
params:
  - doc = untrusted str
---
<outer>

> {% include [child](./untrusted_include_child.tmpl.md) with t = doc %}

</outer>