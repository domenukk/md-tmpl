---
name: untrusted_include_child
description: Child that declares a plain str param (regression for untrusted taint propagation)
params:
  - t = str
---
[{{ t }}]