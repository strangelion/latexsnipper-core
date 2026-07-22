#!/usr/bin/env python3
"""Validate PyTorch eager outputs against the Rust ExecuTorch runtime."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def flatten_runtime_outputs(result: dict, method: str) -> list[float]:
    outputs = result[method]
    if len(outputs) != 1:
        raise RuntimeError(f"{method} returned {len(outputs)} outputs, expected one")
    return list(next(iter(outputs.values()))["values"])


def assert_close(method: str, expected: list[float], actual: list[float]) -> float:
    if len(expected) != len(actual):
        raise RuntimeError(
            f"{method} length mismatch: Python={len(expected)}, Rust={len(actual)}"
        )
    maximum = max((abs(left - right) for left, right in zip(expected, actual)), default=0.0)
    for index, (left, right) in enumerate(zip(expected, actual)):
        tolerance = 1e-5 + 1e-4 * abs(left)
        if abs(left - right) > tolerance:
            raise RuntimeError(
                f"{method}[{index}] mismatch: Python={left}, Rust={right}, "
                f"tolerance={tolerance}"
            )
    return maximum


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    repository = Path(__file__).resolve().parents[3]
    parser.add_argument("--runtime-home", type=Path, required=True)
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="latexsnipper-executorch-parity-") as directory:
        temporary = Path(directory)
        program = temporary / "tiny-recognizer-xnnpack.pte"
        expected_path = temporary / "expected.json"
        environment = os.environ.copy()
        environment.setdefault("PYTHONUTF8", "1")
        subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("export_smoke_model.py")),
                "--output",
                str(program),
                "--expected",
                str(expected_path),
            ],
            check=True,
            cwd=repository,
            env=environment,
        )
        subprocess.run(
            [
                args.cargo,
                "build",
                "--quiet",
                "-p",
                "latexsnipper-runtime-executorch",
                "--example",
                "executorch_parity",
            ],
            check=True,
            cwd=repository,
        )
        runner = repository / "target" / "debug" / "examples" / (
            "executorch_parity.exe" if sys.platform == "win32" else "executorch_parity"
        )
        completed = subprocess.run(
            [str(runner), str(args.runtime_home.resolve()), str(program)],
            check=True,
            cwd=repository,
            text=True,
            capture_output=True,
        )
        expected = json.loads(expected_path.read_text(encoding="utf-8"))
        actual = json.loads(completed.stdout)
        maxima = {
            method: assert_close(
                method,
                expected[method],
                flatten_runtime_outputs(actual, method),
            )
            for method in ["forward", "encode"]
        }
        print("ExecuTorch parity passed")
        for method, maximum in maxima.items():
            print(f"{method}: max_abs_error={maximum:.9g}")


if __name__ == "__main__":
    main()
