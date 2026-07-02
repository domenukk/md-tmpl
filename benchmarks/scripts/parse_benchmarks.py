#!/usr/bin/env python3
"""Parse benchmark outputs from all language suites into a unified JSON structure.

Parses:
  - Rust (Criterion JSON from target/criterion/**/new/estimates.json)
  - Python (tabular stdout from bench_templates.py)
  - Go (standard `go test -bench` format)
  - TypeScript (stdout from bench.ts)
  - WASM (stdout from dist/bench.js, or --json mode)

Outputs a unified JSON structure to stdout.

Usage:
  python3 benchmarks/scripts/parse_benchmarks.py \\
      [--rust-dir target/criterion] \\
      [--python-output FILE] \\
      [--go-output FILE] \\
      [--ts-output FILE] \\
      [--wasm-output FILE]
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import re
import sys
from pathlib import Path
from typing import Any

log = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Rust Criterion parsing
# ---------------------------------------------------------------------------

# Criterion benchmark groups map to scenarios
CRITERION_GROUPS = ["simple", "loop", "conditional", "hero", "mega"]
CRITERION_ENGINES = ["md_tmpl", "tera", "minijinja", "handlebars"]


def parse_criterion_dir(criterion_dir: Path) -> dict[str, Any]:
    """Parse Criterion JSON output from target/criterion directories.

    Returns: {scenario: {engine: ns_value, ...}, ...}
    """
    results: dict[str, dict[str, float]] = {}

    for group in CRITERION_GROUPS:
        group_dir = criterion_dir / group
        if not group_dir.is_dir():
            log.warning("Criterion group dir not found: %s", group_dir)
            continue

        results[group] = {}
        for engine in CRITERION_ENGINES:
            estimates_path = group_dir / engine / "new" / "estimates.json"
            if not estimates_path.exists():
                log.warning("Missing estimates: %s", estimates_path)
                continue

            try:
                with open(estimates_path) as f:
                    data = json.load(f)
                # point_estimate.point_estimate is in nanoseconds
                ns = data["mean"]["point_estimate"]
                results[group][engine] = ns
            except (KeyError, json.JSONDecodeError) as e:
                log.error("Failed to parse %s: %s", estimates_path, e)

    return results


# ---------------------------------------------------------------------------
# Python benchmark parsing
# ---------------------------------------------------------------------------

# The Python benchmark outputs three sections:
#   COMPILE TIME (parsing template source → compiled object)
#   RENDER TIME (pre-compiled template + data → output string)
#   COMPILE + RENDER TIME (source → compiled → output string)
# Each section has a table like:
#   Scenario    md-tmpl   Jinja2  ...
#   simple          4.63 µs    468.72 µs  ...

PYTHON_SECTION_RE = re.compile(
    r"(COMPILE TIME|RENDER TIME|COMPILE \+ RENDER TIME)\s*"
    r"\(([^)]+)\)",
    re.IGNORECASE,
)

PYTHON_VALUE_RE = re.compile(r"([\d.]+)\s*µs")


def parse_python_output(text: str) -> dict[str, Any]:
    """Parse Python benchmark stdout into structured results.

    Returns: {
        "render": {scenario: {engine: us_value, ...}, ...},
        "compile": {scenario: {engine: us_value, ...}, ...},
        "end_to_end": {scenario: {engine: us_value, ...}, ...},
    }
    """
    results: dict[str, dict[str, dict[str, float]]] = {
        "render": {},
        "compile": {},
        "end_to_end": {},
    }

    sections = text.split("=" * 60)
    current_key = None

    for section in sections:
        # Identify section type
        if "COMPILE TIME" in section and "RENDER" not in section:
            current_key = "compile"
        elif "RENDER TIME" in section and "COMPILE" not in section:
            current_key = "render"
        elif "COMPILE + RENDER TIME" in section or "COMPILE + RENDER" in section:
            current_key = "end_to_end"
        else:
            if current_key is None:
                continue

        # Look for the header + data table in this section
        lines = section.strip().split("\n")
        header_line = None
        engine_names: list[str] = []

        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith("Scenario"):
                header_line = i
                # Parse engine names from header
                parts = stripped.split()
                # First part is "Scenario", rest are engine names (may have spaces)
                # Use the column positions from the formatted output
                engine_names = _parse_header_engines(stripped)
                break

        if header_line is None or not engine_names:
            continue

        # Parse data rows
        separator_count = 0
        for line in lines[header_line + 1 :]:
            stripped = line.strip()
            if stripped.startswith("-"):
                separator_count += 1
                if separator_count > 1:
                    break  # End of table
                continue
            if not stripped:
                continue

            # Parse scenario row
            row_data = _parse_python_row(stripped, engine_names)
            if row_data:
                scenario_name, values = row_data
                if current_key not in results:
                    results[current_key] = {}
                results[current_key][scenario_name] = values

    return results


def _parse_header_engines(header: str) -> list[str]:
    """Parse engine names from a Python benchmark header line.

    The header looks like:
      Scenario    md-tmpl     Jinja2       Mako   Chevron     Django  str.Tmpl
    """
    # Split on 2+ spaces to separate columns
    parts = re.split(r"\s{2,}", header.strip())
    # First part is "Scenario", rest are engine names
    return parts[1:] if len(parts) > 1 else []


def _parse_python_row(
    line: str, engine_names: list[str]
) -> tuple[str, dict[str, float]] | None:
    """Parse a single data row from Python benchmark output.

    Returns (scenario_name, {engine: microseconds}).
    """
    # Split on 2+ spaces
    parts = re.split(r"\s{2,}", line.strip())
    if len(parts) < 2:
        return None

    scenario = parts[0].strip()
    values: dict[str, float] = {}

    for i, engine in enumerate(engine_names):
        if i + 1 < len(parts):
            val_str = parts[i + 1].strip()
            if val_str == "N/A":
                continue
            match = PYTHON_VALUE_RE.search(val_str)
            if match:
                values[engine] = float(match.group(1))

    return (scenario, values) if values else None


# ---------------------------------------------------------------------------
# Go benchmark parsing
# ---------------------------------------------------------------------------

# Go bench format:
# BenchmarkRenderSmall-16    1234567    891.2 ns/op    0 B/op    0 allocs/op
GO_BENCH_RE = re.compile(
    r"^Benchmark(\w+)-(\d+)\s+"
    r"(\d+)\s+"
    r"([\d.]+)\s*(ns|µs|ms)/op"
    r"(?:\s+(\d+)\s*B/op)?"
    r"(?:\s+(\d+)\s*allocs/op)?",
)


def parse_go_output(text: str) -> dict[str, Any]:
    """Parse Go benchmark output.

    Returns: {
        "pt": {phase_scenario: {ns: float, bytes: int, allocs: int}, ...},
        "go": {phase_scenario: {ns: float, bytes: int, allocs: int}, ...},
    }
    """
    results: dict[str, dict[str, dict[str, float | int]]] = {"pt": {}, "go": {}}

    for line in text.split("\n"):
        match = GO_BENCH_RE.match(line.strip())
        if not match:
            continue

        name = match.group(1)
        ns_val = float(match.group(4))
        unit = match.group(5)
        bytes_per_op = int(match.group(6)) if match.group(6) else 0
        allocs_per_op = int(match.group(7)) if match.group(7) else 0

        # Convert to nanoseconds
        if unit == "µs":
            ns_val *= 1000
        elif unit == "ms":
            ns_val *= 1_000_000

        # Classify: Go_ prefix means stdlib, otherwise md-tmpl
        entry = {"ns": ns_val, "bytes": bytes_per_op, "allocs": allocs_per_op}
        if name.startswith("Go_"):
            # e.g., Go_RenderSmall -> RenderSmall
            results["go"][name[3:]] = entry
        else:
            results["pt"][name] = entry

    return results


# ---------------------------------------------------------------------------
# TypeScript benchmark parsing
# ---------------------------------------------------------------------------

# TS bench format:
#   parse: simple (1 param)                          12345 ns/op  (1,234,567 ops/s)
TS_BENCH_RE = re.compile(
    r"^\s+(.+?)\s{2,}(\d+)\s*ns/op",
)


def parse_ts_output(text: str) -> dict[str, Any]:
    """Parse TypeScript benchmark output.

    Returns: {"parse": {name: ns, ...}, "render": {name: ns, ...}, "unchecked": {name: ns, ...}}
    """
    results: dict[str, dict[str, float]] = {
        "parse": {},
        "render": {},
        "unchecked": {},
    }

    current_section = None
    for line in text.split("\n"):
        if "Parse benchmarks:" in line:
            current_section = "parse"
            continue
        elif "Render benchmarks:" in line and "Unchecked" not in line:
            current_section = "render"
            continue
        elif "Render unchecked" in line:
            current_section = "unchecked"
            continue
        elif line.startswith("="):
            current_section = None
            continue

        if current_section is None:
            continue

        match = TS_BENCH_RE.match(line)
        if match:
            name = match.group(1).strip()
            ns = float(match.group(2))
            results[current_section][name] = ns

    return results


# ---------------------------------------------------------------------------
# TypeScript comparison benchmark parsing
# ---------------------------------------------------------------------------

# comparison.ts output format (from printRow):
#   simple            1,236 ns 🏆   1,459 ns      1,273 ns
TS_COMP_ROW_RE = re.compile(
    r"^\s+(.+?)\s{2,}([\d,]+)\s*ns\s*(?:🏆)?\s+([\d,]+)\s*ns\s*(?:🏆)?\s+([\d,]+)\s*ns",
)


def parse_ts_comparison_output(text: str) -> dict[str, Any]:
    """Parse TypeScript comparison benchmark output (comparison.ts).

    Returns: {
        "render": {scenario: {"pt": ns, "hbs": ns, "mus": ns}, ...},
        "roundtrip": {scenario: {...}, ...},
        "unchecked": {scenario: {...}, ...},
    }
    """
    results: dict[str, dict[str, dict[str, float]]] = {
        "render": {},
        "roundtrip": {},
        "unchecked": {},
    }

    current: str | None = None
    for line in text.split("\n"):
        if "Render (pre-compiled):" in line:
            current = "render"
            continue
        elif "Round-trip" in line:
            current = "roundtrip"
            continue
        elif "Render unchecked" in line:
            current = "unchecked"
            continue
        elif line.startswith("=") or "Compile (parse):" in line:
            current = None
            continue

        if current is None:
            continue

        match = TS_COMP_ROW_RE.match(line)
        if match:
            scenario = match.group(1).strip()
            pt_ns = float(match.group(2).replace(",", ""))
            hbs_ns = float(match.group(3).replace(",", ""))
            mus_ns = float(match.group(4).replace(",", ""))
            results[current][scenario] = {
                "pt": pt_ns,
                "hbs": hbs_ns,
                "mus": mus_ns,
            }

    return results


# ---------------------------------------------------------------------------
# WASM benchmark parsing
# ---------------------------------------------------------------------------

# WASM bench format (table row):
# render simple (1 param)            | WASM   |      2.3µs |      2.4µs | ...
# Also the delta lines:
#   → WASM is 1.23× faster (median)
WASM_ROW_RE = re.compile(
    r"^(.+?)\s+\|\s+(WASM|TS)\s+\|"
    r"\s+([\d.]+(?:ns|µs|ms))\s+\|"  # Median
    r"\s+([\d.]+(?:ns|µs|ms))\s+\|",  # Mean
)


def _parse_wasm_time(s: str) -> float:
    """Convert a formatted time string to nanoseconds."""
    s = s.strip()
    if s.endswith("ms"):
        return float(s[:-2]) * 1_000_000
    elif s.endswith("µs"):
        return float(s[:-2]) * 1_000
    elif s.endswith("ns"):
        return float(s[:-2])
    return float(s)


def parse_wasm_output(text: str) -> dict[str, Any]:
    """Parse WASM benchmark output.

    Returns: {scenario: {"wasm": {median_ns: float}, "ts": {median_ns: float}, "speedup": float}, ...}
    """
    # First try JSON mode
    json_start = text.find("""--- JSON BEGIN ---""")
    json_end = text.find("""--- JSON END ---""")
    if json_start >= 0 and json_end > json_start:
        json_text = text[json_start + len("""--- JSON BEGIN ---""") : json_end].strip()
        try:
            data = json.load(json_text) if isinstance(json_text, bytes) else json.loads(json_text)
            return {
                r["scenario"]: {
                    "wasm": {"median_ns": r["wasm"]["median_ns"]},
                    "ts": {"median_ns": r["ts"]["median_ns"]},
                    "speedup": r["speedup"],
                }
                for r in data.get("results", [])
            }
        except (json.JSONDecodeError, KeyError) as e:
            log.warning("Failed to parse WASM JSON output: %s", e)

    # Fallback: parse table output
    results: dict[str, dict[str, Any]] = {}
    for line in text.split("\n"):
        match = WASM_ROW_RE.match(line.strip())
        if match:
            scenario = match.group(1).strip()
            engine = match.group(2).lower()
            median_ns = _parse_wasm_time(match.group(3))

            if scenario not in results:
                results[scenario] = {}
            results[scenario][engine] = {"median_ns": median_ns}

    # Compute speedups
    for scenario, data in results.items():
        if "wasm" in data and "ts" in data:
            data["speedup"] = data["ts"]["median_ns"] / data["wasm"]["median_ns"]

    return results


# ---------------------------------------------------------------------------
# Unified output
# ---------------------------------------------------------------------------


def build_unified_output(
    rust: dict[str, Any] | None = None,
    python: dict[str, Any] | None = None,
    go: dict[str, Any] | None = None,
    ts: dict[str, Any] | None = None,
    ts_comparison: dict[str, Any] | None = None,
    wasm: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build the unified JSON output structure."""
    output: dict[str, Any] = {}

    if rust:
        output["rust"] = rust
    if python:
        output["python"] = python
    if go:
        output["go"] = go
    if ts:
        output["ts"] = ts
    if ts_comparison:
        output["ts_comparison"] = ts_comparison
    if wasm:
        output["wasm"] = wasm

    return output


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(
        description="Parse benchmark outputs into unified JSON."
    )
    parser.add_argument(
        "--rust-dir",
        type=Path,
        help="Path to Criterion output dir (e.g. target/criterion)",
    )
    parser.add_argument(
        "--python-output",
        type=Path,
        help="Path to file containing Python benchmark stdout",
    )
    parser.add_argument(
        "--go-output",
        type=Path,
        help="Path to file containing Go benchmark stdout",
    )
    parser.add_argument(
        "--ts-output",
        type=Path,
        help="Path to file containing TypeScript benchmark stdout",
    )
    parser.add_argument(
        "--ts-comparison-output",
        type=Path,
        help="Path to file containing TS comparison benchmark stdout",
    )
    parser.add_argument(
        "--wasm-output",
        type=Path,
        help="Path to file containing WASM benchmark stdout",
    )

    args = parser.parse_args()

    rust = None
    python = None
    go = None
    ts = None
    ts_comparison = None
    wasm = None

    if args.rust_dir and args.rust_dir.is_dir():
        log.info("Parsing Rust/Criterion from %s", args.rust_dir)
        rust = parse_criterion_dir(args.rust_dir)

    if args.python_output and args.python_output.exists():
        log.info("Parsing Python from %s", args.python_output)
        python = parse_python_output(args.python_output.read_text())

    if args.go_output and args.go_output.exists():
        log.info("Parsing Go from %s", args.go_output)
        go = parse_go_output(args.go_output.read_text())

    if args.ts_output and args.ts_output.exists():
        log.info("Parsing TypeScript from %s", args.ts_output)
        ts = parse_ts_output(args.ts_output.read_text())

    if args.ts_comparison_output and args.ts_comparison_output.exists():
        log.info("Parsing TypeScript comparison from %s", args.ts_comparison_output)
        ts_comparison = parse_ts_comparison_output(args.ts_comparison_output.read_text())

    if args.wasm_output and args.wasm_output.exists():
        log.info("Parsing WASM from %s", args.wasm_output)
        wasm = parse_wasm_output(args.wasm_output.read_text())

    output = build_unified_output(rust=rust, python=python, go=go, ts=ts, ts_comparison=ts_comparison, wasm=wasm)
    json.dump(output, sys.stdout, indent=2)
    print()  # trailing newline


if __name__ == "__main__":
    main()
