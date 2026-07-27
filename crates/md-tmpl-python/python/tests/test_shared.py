"""Shared cross-language conformance test runner in Python.

Executes all 569 test cases across tests/shared/*.toml through the md_tmpl
Python extension module and asserts behavioral parity with Rust, Go, and TS.
"""

from __future__ import annotations

import copy
import tomllib
from pathlib import Path
from typing import Any

import pytest

from md_tmpl import Template

SHARED_DIR = Path(__file__).resolve().parents[4] / "tests" / "shared"
SHARED_FILES = (
    "inline_tmpl_tests.toml",
    "include_tests.toml",
    "inline_control_tests.toml",
    "tmpl_param_tests.toml",
    "feature_e2e_tests.toml",
    "env_tests.toml",
)


def _transform_val(val: Any) -> Any:
    """Implement TOML Option convention: None -> None, Some(x) -> x."""
    if isinstance(val, str):
        if val == "None":
            return None
        if val.startswith("Some(") and val.endswith(")"):
            return val[5:-1]
        return val
    if isinstance(val, list):
        return [_transform_val(x) for x in val]
    if isinstance(val, dict):
        return {k: _transform_val(v) for k, v in val.items()}
    return val


def _match_error(err_str: str, pattern: str) -> bool:
    lower_err = err_str.lower()
    for alt in pattern.split("|"):
        if alt.strip().lower() in lower_err:
            return True
    return False


def _extract_expected_key(d: dict[str, Any], key: str) -> tuple[str | None, bool]:
    if key in d and isinstance(d[key], str):
        val = d.pop(key)
        return val, True
    for v in d.values():
        if isinstance(v, dict):
            val, found = _extract_expected_key(v, key)
            if found:
                return val, True
    return None, False


def _resolve_tmpl_str(tc: dict[str, Any], key: str, lines_key: str) -> str:
    val = tc.get(key)
    if isinstance(val, str) and val:
        if val.endswith(".tmpl.md"):
            path = SHARED_DIR / val
            if path.exists():
                return path.read_text()
        return val
    lines = tc.get(lines_key)
    if isinstance(lines, list):
        return "\n".join(str(x) for x in lines)
    return ""


def _resolve_file_content(val: Any) -> str:
    if isinstance(val, str):
        if val.endswith(".tmpl.md"):
            path = SHARED_DIR / val
            if path.exists():
                return path.read_text()
        return val
    if isinstance(val, list):
        return "\n".join(str(x) for x in val)
    return ""


def _load_all_cases() -> list[tuple[str, str, dict[str, Any]]]:
    all_cases: list[tuple[str, str, dict[str, Any]]] = []
    for fname in SHARED_FILES:
        data = tomllib.loads((SHARED_DIR / fname).read_text())
        for case in data.get("tests", []):
            case_id = f"{fname}::{case.get('name', 'unnamed')}"
            all_cases.append((fname, case_id, case))
    return all_cases


_ALL_CASES = _load_all_cases()


@pytest.mark.parametrize(
    ("fname", "case_id", "raw_case"),
    _ALL_CASES,
    ids=[c[1] for c in _ALL_CASES],
)
def test_shared_case(
    fname: str,
    case_id: str,
    raw_case: dict[str, Any],
    tmp_path: Path,
) -> None:
    # Make a deep mutable copy so we can pop keys during expected extraction
    case = copy.deepcopy(raw_case)
    name = case.get("name", case_id)

    want_output, has_output = _extract_expected_key(case, "expected_output")
    want_error, has_error = _extract_expected_key(case, "expected_error")

    has_files = "files" in case
    has_parent_tmpl = "parent_template" in case
    has_parent_lines = "parent_template_lines" in case

    base_dir: Path | None = None
    if has_files or has_parent_tmpl or has_parent_lines:
        files_map = case.get("files") or {}
        for rel_path, raw_content in files_map.items():
            content = _resolve_file_content(raw_content)
            target = tmp_path / rel_path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)
        base_dir = tmp_path
        source = _resolve_tmpl_str(case, "parent_template", "parent_template_lines")
    else:
        source = _resolve_tmpl_str(case, "template", "template_lines")

    raw_env = case.get("env")
    env_dict: dict[str, Any] | None = None
    if isinstance(raw_env, dict):
        env_dict = {k: _transform_val(v) for k, v in raw_env.items()}

    raw_params = case.get("params") or {}
    transformed_params = _transform_val(raw_params)
    if not isinstance(transformed_params, dict):
        transformed_params = {}

    tmpl: Template | None = None
    compile_err: str | None = None
    try:
        tmpl = Template.from_source_with_options(
            source,
            base_dir=base_dir,
            env=env_dict,
        )
    except Exception as exc:  # noqa: BLE001 - harness inspects message
        compile_err = str(exc)

    if has_output:
        assert (
            compile_err is None
        ), f"[{name}] expected output but compile failed: {compile_err}"
        assert tmpl is not None
        got_output = tmpl.render_dict(transformed_params)
        assert got_output == want_output, f"[{name}] output mismatch"
        return

    if has_error:
        assert want_error is not None
        if compile_err is not None:
            assert _match_error(
                compile_err, want_error
            ), f"[{name}] compile error {compile_err!r} did not match pattern {want_error!r}"
            return
        assert tmpl is not None
        render_err: str | None = None
        try:
            tmpl.render_dict(transformed_params)
        except Exception as exc:  # noqa: BLE001
            render_err = str(exc)

        assert (
            render_err is not None
        ), f"[{name}] expected error pattern {want_error!r}, but compile & render succeeded"
        assert _match_error(
            render_err, want_error
        ), f"[{name}] render error {render_err!r} did not match pattern {want_error!r}"
        return

    pytest.fail(f"[{name}] test case has neither expected_output nor expected_error")
