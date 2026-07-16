"""Cross-language conformance harness (Python side).

Replays the shared TOML corpus in ``<repo>/tests/conformance`` through the
``md_tmpl`` Python bindings and asserts that every case matches the recorded
expectation. The exact same corpus is replayed by the Rust, TypeScript, and Go
harnesses; if all pass, the four backends are behaviourally identical on the
covered surface.

TOML has no ``null``, so option-``None`` is encoded in the corpus as the
sentinel inline table ``{ __none__ = true }`` and decoded back to Python
``None`` on load.
"""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Any

import pytest

from md_tmpl import Template

# tests/ -> python/ -> md-tmpl-python/ -> crates/ -> <repo root>.
CORPUS_DIR = Path(__file__).resolve().parents[4] / "tests" / "conformance"
CORPUS_FILES = (
    "render.toml",
    "interpolation.toml",
    "frontmatter.toml",
    "errors.toml",
    "escapes.toml",
    "comments.toml",
    "literals.toml",
)


def _denull(value: Any) -> Any:
    """Decode the ``{ __none__ = true }`` option-None sentinel back to ``None``."""
    if isinstance(value, list):
        return [_denull(item) for item in value]
    if isinstance(value, dict):
        if len(value) == 1 and value.get("__none__") is True:
            return None
        return {key: _denull(item) for key, item in value.items()}
    return value


def _load_all() -> list[tuple[str, dict]]:
    cases: list[tuple[str, dict]] = []
    for fname in CORPUS_FILES:
        data = tomllib.loads((CORPUS_DIR / fname).read_text())
        for case in data["cases"]:
            cases.append((f"{fname}:{case['name']}", _denull(case)))
    return cases


_ALL_CASES = _load_all()


def _compile(case: dict) -> Template:
    env = case.get("env")
    source = case["source"]
    if env is not None:
        return Template.from_source_with_env(source, env)
    return Template.from_source(source)


def _try_compile(case: dict) -> tuple[Template | None, str | None]:
    """Compile a case, capturing a compile-time error as a string."""
    try:
        return _compile(case), None
    except Exception as exc:  # noqa: BLE001 - the harness inspects the message
        return None, str(exc)


def _assert_needle(needle: str | None, haystack: str) -> None:
    if needle is not None:
        assert needle in haystack, f"error {haystack!r} lacks substring {needle!r}"


def _check_render(case: dict) -> None:
    expect = case["expect"]
    out = _compile(case).render_dict(case.get("params") or {})
    assert out == expect["output"]


def _check_default(case: dict) -> None:
    expect = case["expect"]
    defs = _compile(case).defaults()
    assert defs == expect["defaults"]


def _check_error(case: dict) -> None:
    expect = case["expect"]
    phase = expect["phase"]
    needle = expect.get("error_contains")
    tmpl, compile_err = _try_compile(case)

    if phase == "compile":
        assert compile_err is not None, "expected a COMPILE error but compile succeeded"
        _assert_needle(needle, compile_err)
        return

    # "render" and "any" both require a successful compile before rendering,
    # except "any" also accepts a compile-time failure (leak-safety may trip at
    # either phase; the phase is allowed to differ between backends).
    if compile_err is not None:
        assert (
            phase == "any"
        ), f"expected a RENDER error but failed at COMPILE: {compile_err}"
        _assert_needle(needle, compile_err)
        return

    assert tmpl is not None, "compile reported success but produced no template"
    render_err: str | None = None
    try:
        tmpl.render_dict(case.get("params") or {})
    except Exception as exc:  # noqa: BLE001 - the harness inspects the message
        render_err = str(exc)
    assert render_err is not None, "expected a RENDER error but render succeeded"
    _assert_needle(needle, render_err)


@pytest.mark.parametrize(
    "case",
    [case for _, case in _ALL_CASES],
    ids=[case_id for case_id, _ in _ALL_CASES],
)
def test_conformance(case: dict) -> None:
    kind = case["expect"]["kind"]
    if kind == "render":
        _check_render(case)
    elif kind == "default":
        _check_default(case)
    elif kind == "error":
        _check_error(case)
    else:
        pytest.fail(f"unknown expect.kind {kind!r}")
