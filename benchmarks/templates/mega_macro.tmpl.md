---
params:
  - org = str
  - teams = list<name = str, lead = str, active = bool, idx = int, members = list<name = str, role = str, score = float, skills = list<name = str>>>
---

# {{ org }} Organization Report

> {% for team in teams %}

## {{ team.idx }}. {{ team.name }}

Lead: {{ team.lead }}

> {% if team.active %}

Status: ACTIVE

> {% else %}

Status: INACTIVE

> {% /if %}
> {% for member in team.members %}

### {{ member.name }} ({{ member.role }})

Score: {{ member.score | fixed(1) }}

> {% if member.score > 90 %}

Rating: Outstanding

> {% elif member.score > 70 %}

Rating: Good

> {% elif member.score > 50 %}

Rating: Average

> {% else %}

Rating: Needs Improvement

> {% /if %}

Skills:

> {% for skill in member.skills %}

- {{ skill.name }}

  > {% /for %}
  > {% /for %}

---

> {% /for %}
