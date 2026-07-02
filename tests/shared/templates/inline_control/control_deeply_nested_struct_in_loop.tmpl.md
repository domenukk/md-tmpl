---
params:
  [
    org = struct(name = str,
    teams = list(name = str,
    members = list(name = str,
    role = str))),
  ]
---

{{ org.name }}:

> {% for team in org.teams %}

Team {{ team.name }}:

> {% for member in team.members %}

- {{ member.name }} ({{ member.role }})

> {% /for %}

> {% /for %}
