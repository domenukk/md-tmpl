---
name: option_test
params:
  - name = str
  - nickname = option<str>
  - age = option<int>
---

Hello {{ name }}!

> {% if has(nickname) %}

Nickname: {{ nickname.val }}

> {% /if %}

> {% if has(age) %}

Age: {{ age.val }}

> {% /if %}
