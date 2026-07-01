#!/usr/bin/env python3
"""Run all benchmarks and update README.md tables.

This is the main entry point that orchestrates:
  1. Running each benchmark suite (Rust, Python, Go, TS, WASM)
  2. Collecting output
  3. Parsing results via parse_benchmarks.py
  4. Updating markdown tables via update_tables.py

Usage:
  python3 benchmarks/scripts/run_and_update.py [--lang LANG ...] [--dry-run]

  # Run all benchmarks:
  python3 benchmarks/scripts/run_and_update.py

  # Run only Rust + Go:
  python3 benchmarks/scripts/run_and_update.py --lang rust --lang go
"""

from __future__ import annotations

import argparse
import json
import logging
import subprocess
import sys
import tempfile
from pathlib import Path

log = logging.getLogger(__name__)

# Project root is 2 levels up from this script
SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent

PARSE_SCRIPT = SCRIPT_DIR / "parse_benchmarks.py"
UPDATE_SCRIPT = SCRIPT_DIR / "update_tables.py"
README = PROJECT_ROOT / "README.md"
PYTHON_README = PROJECT_ROOT / "crates" / "md-tmpl-python" / "README.md"
GO_README = PROJECT_ROOT / "go" / "md_tmpl" / "README.md"
TS_README = PROJECT_ROOT / "crates" / "md-tmpl-typescript" / "README.md"
WASM_README = PROJECT_ROOT / "crates" / "md-tmpl-wasm" / "README.md"


PYTHON_VENV = PROJECT_ROOT / "crates" / "md-tmpl-python" / ".venv" / "bin"


def run_cmd(
    cmd: list[str],
    cwd: Path | None = None,
    capture: bool = True,
    env_extra: dict[str, str] | None = None,
) -> subprocess.CompletedProcess:
    """Run a command, logging it and optionally capturing output."""
    import os

    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)

    log.info("Running: %s (cwd=%s)", " ".join(cmd), cwd or ".")
    result = subprocess.run(
        cmd,
        cwd=cwd,
        capture_output=capture,
        text=True,
        env=env,
    )
    if result.returncode != 0:
        log.error("Command failed (exit %d): %s", result.returncode, " ".join(cmd))
        if result.stderr:
            log.error("stderr: %s", result.stderr[:2000])
    return result


# ---------------------------------------------------------------------------
# Benchmark runners
# ---------------------------------------------------------------------------


def run_rust_bench() -> Path | None:
    """Run Rust Criterion comparison benchmarks. Returns Criterion output dir or None."""
    log.info("=== Running Rust benchmarks ===")

    # The comparison benchmarks are in benchmarks/ which is its own workspace
    bench_dir = PROJECT_ROOT / "benchmarks"
    result = run_cmd(
        ["cargo", "bench"],
        cwd=bench_dir,
        capture=True,
    )
    if result.returncode != 0:
        log.error("Rust benchmarks failed")
        return None

    criterion_dir = bench_dir / "target" / "criterion"
    if criterion_dir.is_dir():
        return criterion_dir
    log.warning("Criterion dir not found at %s", criterion_dir)
    return None


def run_python_bench() -> Path | None:
    """Run Python benchmarks. Returns path to output file or None."""
    log.info("=== Running Python benchmarks ===")

    # Check venv exists
    python_bin = PYTHON_VENV / "python"
    if not python_bin.exists():
        log.error("Python venv not found at %s", python_bin)
        return None

    # Build release-mode bindings (debug builds are ~10× slower)
    maturin_bin = PYTHON_VENV / "maturin"
    py_crate = PROJECT_ROOT / "crates" / "md-tmpl-python"
    build_result = run_cmd(
        [str(maturin_bin), "develop", "--release"],
        cwd=py_crate,
        capture=True,
    )
    if build_result.returncode != 0:
        log.error("maturin develop --release failed")
        return None

    bench_script = PROJECT_ROOT / "benchmarks" / "python" / "bench_templates.py"
    result = run_cmd(
        [str(python_bin), str(bench_script)],
        cwd=PROJECT_ROOT,
        capture=True,
    )
    if result.returncode != 0:
        log.error("Python benchmarks failed")
        return None

    # Save output to temp file
    outfile = Path(tempfile.mktemp(suffix=".txt", prefix="bench_python_"))
    outfile.write_text(result.stdout)
    log.info("Python output saved to %s", outfile)
    return outfile


def run_go_bench() -> Path | None:
    """Run Go benchmarks. Returns path to output file or None."""
    log.info("=== Running Go benchmarks ===")

    # Build FFI library first
    result = run_cmd(
        ["cargo", "build", "-p", "md-tmpl-ffi", "--release"],
        cwd=PROJECT_ROOT,
        capture=True,
    )
    if result.returncode != 0:
        log.error("FFI build failed")
        return None

    go_dir = PROJECT_ROOT / "go" / "md_tmpl"
    result = run_cmd(
        ["go", "test", "-bench=.", "-benchmem", "-count=1", "./..."],
        cwd=go_dir,
        capture=True,
    )
    if result.returncode != 0:
        log.error("Go benchmarks failed")
        log.error("stdout: %s", result.stdout[:2000] if result.stdout else "(empty)")
        return None

    outfile = Path(tempfile.mktemp(suffix=".txt", prefix="bench_go_"))
    outfile.write_text(result.stdout)
    log.info("Go output saved to %s", outfile)
    return outfile


def run_ts_bench() -> Path | None:
    """Run TypeScript benchmarks. Returns path to output file or None."""
    log.info("=== Running TypeScript benchmarks ===")

    ts_dir = PROJECT_ROOT / "crates" / "md-tmpl-typescript"

    # Build TS first
    result = run_cmd(["npx", "tsc"], cwd=ts_dir, capture=True)
    if result.returncode != 0:
        log.error("TypeScript build failed")
        return None

    result = run_cmd(
        ["node", "dist/benchmarks/bench.js"],
        cwd=ts_dir,
        capture=True,
    )
    if result.returncode != 0:
        log.error("TypeScript benchmarks failed")
        return None

    outfile = Path(tempfile.mktemp(suffix=".txt", prefix="bench_ts_"))
    outfile.write_text(result.stdout)
    log.info("TS output saved to %s", outfile)
    return outfile


def run_ts_comparison_bench() -> Path | None:
    """Run TypeScript comparison benchmarks (vs Handlebars/Mustache)."""
    log.info("=== Running TypeScript comparison benchmarks ===")

    ts_dir = PROJECT_ROOT / "crates" / "md-tmpl-typescript"

    # Build TS first (may already be built from run_ts_bench)
    result = run_cmd(["npx", "tsc"], cwd=ts_dir, capture=True)
    if result.returncode != 0:
        log.error("TypeScript build failed")
        return None

    result = run_cmd(
        ["node", "dist/benchmarks/comparison.js"],
        cwd=ts_dir,
        capture=True,
    )
    if result.returncode != 0:
        log.error("TypeScript comparison benchmarks failed")
        return None

    outfile = Path(tempfile.mktemp(suffix=".txt", prefix="bench_ts_comp_"))
    outfile.write_text(result.stdout)
    log.info("TS comparison output saved to %s", outfile)
    return outfile


def run_wasm_bench() -> Path | None:
    """Run WASM benchmarks. Returns path to output file or None."""
    log.info("=== Running WASM benchmarks ===")

    wasm_dir = PROJECT_ROOT / "crates" / "md-tmpl-wasm"

    # Build WASM first
    result = run_cmd(
        ["wasm-pack", "build", "--target", "nodejs", "--out-dir", "pkg", "--release"],
        cwd=wasm_dir,
        capture=True,
    )
    if result.returncode != 0:
        log.error("WASM build failed")
        return None

    result = run_cmd(["npm", "run", "build"], cwd=wasm_dir, capture=True)
    if result.returncode != 0:
        log.error("WASM TS build failed")
        return None

    result = run_cmd(
        ["node", "dist/bench.js", "--json"],
        cwd=wasm_dir,
        capture=True,
    )
    if result.returncode != 0:
        log.error("WASM benchmarks failed")
        return None

    outfile = Path(tempfile.mktemp(suffix=".txt", prefix="bench_wasm_"))
    outfile.write_text(result.stdout)
    log.info("WASM output saved to %s", outfile)
    return outfile


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

ALL_LANGS = ["rust", "python", "go", "ts", "wasm"]


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s: %(message)s",
        datefmt="%H:%M:%S",
    )

    parser = argparse.ArgumentParser(
        description="Run benchmarks and update markdown tables."
    )
    parser.add_argument(
        "--lang",
        action="append",
        choices=ALL_LANGS,
        help="Which language benchmarks to run (can repeat). Default: all.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Parse and print results but don't update markdown files.",
    )
    parser.add_argument(
        "--no-update",
        action="store_true",
        help="Run benchmarks but skip updating markdown files.",
    )

    args = parser.parse_args()
    langs = args.lang or ALL_LANGS

    # Run benchmarks
    parse_args: list[str] = [sys.executable, str(PARSE_SCRIPT)]
    temp_files: list[Path] = []

    if "rust" in langs:
        criterion_dir = run_rust_bench()
        if criterion_dir:
            parse_args.extend(["--rust-dir", str(criterion_dir)])

    if "python" in langs:
        py_out = run_python_bench()
        if py_out:
            parse_args.extend(["--python-output", str(py_out)])
            temp_files.append(py_out)

    if "go" in langs:
        go_out = run_go_bench()
        if go_out:
            parse_args.extend(["--go-output", str(go_out)])
            temp_files.append(go_out)

    if "ts" in langs:
        ts_out = run_ts_bench()
        if ts_out:
            parse_args.extend(["--ts-output", str(ts_out)])
            temp_files.append(ts_out)

        ts_comp_out = run_ts_comparison_bench()
        if ts_comp_out:
            parse_args.extend(["--ts-comparison-output", str(ts_comp_out)])
            temp_files.append(ts_comp_out)

    if "wasm" in langs:
        wasm_out = run_wasm_bench()
        if wasm_out:
            parse_args.extend(["--wasm-output", str(wasm_out)])
            temp_files.append(wasm_out)

    # Parse results
    log.info("=== Parsing benchmark results ===")
    result = subprocess.run(parse_args, capture_output=True, text=True)
    if result.returncode != 0:
        log.error("Parsing failed: %s", result.stderr)
        sys.exit(1)

    unified_json = result.stdout

    # Save unified JSON for reference
    json_path = PROJECT_ROOT / "target" / "bench_results.json"
    json_path.parent.mkdir(parents=True, exist_ok=True)
    json_path.write_text(unified_json)
    log.info("Unified results saved to %s", json_path)

    if args.dry_run:
        print(unified_json)
        return

    if args.no_update:
        log.info("Skipping markdown update (--no-update)")
        return

    # Update markdown tables
    log.info("=== Updating markdown tables ===")
    update_args = [
        sys.executable,
        str(UPDATE_SCRIPT),
        "--json",
        str(json_path),
        "--readme",
        str(README),
        "--python-readme",
        str(PYTHON_README),
        "--go-readme",
        str(GO_README),
        "--ts-readme",
        str(TS_README),
        "--wasm-readme",
        str(WASM_README),
    ]
    result = subprocess.run(update_args, text=True)
    if result.returncode != 0:
        log.error("Table update failed")
        sys.exit(1)

    # Clean up temp files
    for tf in temp_files:
        try:
            tf.unlink()
        except OSError:
            pass

    log.info("Done!")


if __name__ == "__main__":
    main()
