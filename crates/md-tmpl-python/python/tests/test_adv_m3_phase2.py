"""Tier 5 White-box Adversarial Coverage Tests for Milestone M3 Phase 2 (Python bindings)."""

import subprocess
import sys
import pytest
import md_tmpl


def test_adv_unchecked_render_api() -> None:
    """Verify that Python bindings support unchecked rendering methods."""
    src = "---\nparams:\n  - x = int\n---\n{{ x }}"
    tmpl = md_tmpl.Template.from_source(src)
    assert hasattr(
        tmpl, "render_unchecked"
    ), "Python bindings should have render_unchecked"
    assert hasattr(
        tmpl, "render_dict_unchecked"
    ), "Python bindings should have render_dict_unchecked"
    assert tmpl.render_unchecked(x=42) == "42"
    assert tmpl.render_dict_unchecked({"x": 42}) == "42"


def test_adv_deeply_nested_structs_and_lists() -> None:
    """Stress test deeply nested dictionaries and large lists."""
    src = """---
params:
  - l1 = struct(l2 = struct(l3 = struct(l4 = struct(l5 = str))))
---
Deep: {{ l1.l2.l3.l4.l5 }}"""
    tmpl = md_tmpl.Template.from_source(src)
    data = {"l1": {"l2": {"l3": {"l4": {"l5": "success"}}}}}
    assert tmpl.render_dict(data) == "Deep: success"

    src_list = """---
params:
  - items = list(int)
---
{{ items | limit(5) | join(",") }}"""
    tmpl_list = md_tmpl.Template.from_source(src_list)
    items = list(range(1000))
    assert tmpl_list.render_dict({"items": items}) == "0,1,2,3,4"


def test_adv_large_integer_overflow() -> None:
    """Verify behavior when passing Python integers exceeding C i64 bounds."""
    src = "---\nparams:\n  - count = int\n---\n{{ count }}"
    tmpl = md_tmpl.Template.from_source(src)

    with pytest.raises(OverflowError):
        tmpl.render_dict({"count": 10**30})


def test_adv_cyclic_dict_handling() -> None:
    """Verify that passing self-referential cyclic structures raises ValueError cleanly without SIGSEGV."""
    src = """---
params:
  - data = struct(self = str)
---
Hello {% if data %}world{% /if %}!"""
    tmpl = md_tmpl.Template.from_source(src)
    d = {}
    d["self"] = d
    with pytest.raises(ValueError, match="cyclic object detected"):
        tmpl.render_dict({"data": d})
