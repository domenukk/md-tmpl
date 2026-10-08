"""Shared pytest fixtures for the md_tmpl Python binding tests."""

import textwrap
from pathlib import Path

import pytest

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture()
def simple_template_path(tmp_path: Path) -> Path:
    """Create a simple greeting template file."""
    path = tmp_path / "greeting.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - name = str
        ---
        Hello {{ name }}!"""))
    return path


@pytest.fixture()
def list_template_path(tmp_path: Path) -> Path:
    """Create a template with typed list params."""
    path = tmp_path / "tasks.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - tasks = list(title = str, priority = str)
        ---
        > {% for task in tasks %}

        - **{{ task.title }}** ({{ task.priority }})

        > {% /for %}"""))
    return path


@pytest.fixture()
def enum_template_path(tmp_path: Path) -> Path:
    """Create a template with enum params including a struct variant."""
    path = tmp_path / "status.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
        ---
        > {% match outcome %}
        > {% case Confirmed %}

        YES: {{ outcome.evidence }}

        > {% case Rejected %}

        NO

        > {% case NeedsWork %}

        MAYBE

        > {% /match %}"""))
    return path


@pytest.fixture()
def default_template_path(tmp_path: Path) -> Path:
    """Create a template with default values."""
    path = tmp_path / "defaults.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - name = str := "World"
          - count = int := 1
        ---
        Hello {{ name }}, count={{ count }}!"""))
    return path


@pytest.fixture()
def struct_template_path(tmp_path: Path) -> Path:
    """Create a template with struct params."""
    path = tmp_path / "config.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - config = struct(host = str, port = int)
        ---
        {{ config.host }}:{{ config.port }}"""))
    return path


@pytest.fixture()
def multi_param_path(tmp_path: Path) -> Path:
    """Create a template with multiple param types."""
    path = tmp_path / "multi.tmpl.md"
    path.write_text(textwrap.dedent("""\
        ---
        params:
          - name = str
          - count = int
          - score = float
          - enabled = bool
        ---
        {{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}"""))
    return path
