#!/usr/bin/env python3
"""Export runtime tensor dtype/shape/statistics from one or more NPZ steps."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def require_numpy():
    try:
        import numpy
    except ImportError as error:
        raise SystemExit("numpy is required: python -m pip install numpy") from error
    return numpy


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", action="append", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    numpy = require_numpy()

    steps = []
    for step_index, path in enumerate(args.state):
        with numpy.load(path, allow_pickle=False) as tensors:
            states = []
            for name in sorted(tensors.files):
                tensor = tensors[name]
                finite = tensor[numpy.isfinite(tensor)] if tensor.dtype.kind in "fc" else tensor
                states.append(
                    {
                        "name": name,
                        "dtype": str(tensor.dtype),
                        "shape": list(tensor.shape),
                        "minimum": float(finite.min()) if finite.size else None,
                        "maximum": float(finite.max()) if finite.size else None,
                        "mean": float(finite.mean()) if finite.size else None,
                    }
                )
            steps.append({"step": step_index, "source": str(path), "states": states})

    report = {"schemaVersion": 1, "steps": steps}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
