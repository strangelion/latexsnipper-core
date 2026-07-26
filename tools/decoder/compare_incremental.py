#!/usr/bin/env python3
"""Compare full-sequence and incremental decoder tensor dumps."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


DEFAULT_LENGTHS = [1, 2, 3, 6, 9, 15, 30]


def require_numpy():
    try:
        import numpy
    except ImportError as error:
        raise SystemExit("numpy is required: python -m pip install numpy") from error
    return numpy


def top_k(array, count, numpy):
    count = min(count, array.shape[-1])
    return numpy.argsort(array, axis=-1)[..., -count:][..., ::-1]


def compare(left, right, numpy):
    if left.shape != right.shape:
        return {"shapeMatch": False, "leftShape": list(left.shape), "rightShape": list(right.shape)}
    left64 = left.astype(numpy.float64, copy=False)
    right64 = right.astype(numpy.float64, copy=False)
    difference = numpy.abs(left64 - right64)
    left_flat = left64.ravel()
    right_flat = right64.ravel()
    denominator = numpy.linalg.norm(left_flat) * numpy.linalg.norm(right_flat)
    cosine = float(numpy.dot(left_flat, right_flat) / denominator) if denominator else 1.0
    return {
        "shapeMatch": True,
        "maxAbs": float(difference.max(initial=0.0)),
        "meanAbs": float(difference.mean()) if difference.size else 0.0,
        "cosineSimilarity": cosine,
        "top1Agreement": float(numpy.mean(top_k(left64, 1, numpy) == top_k(right64, 1, numpy))),
        "top5Agreement": float(
            numpy.mean(
                [
                    bool(set(a).intersection(b))
                    for a, b in zip(
                        top_k(left64, 5, numpy).reshape(-1, 5),
                        top_k(right64, 5, numpy).reshape(-1, 5),
                    )
                ]
            )
        ),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--full-pattern", required=True, help="Pattern containing {length}")
    parser.add_argument("--incremental-pattern", required=True, help="Pattern containing {length}")
    parser.add_argument("--length", action="append", type=int)
    parser.add_argument("--atol", type=float, default=1e-4)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    numpy = require_numpy()

    cases = []
    first_divergence = None
    for length in args.length or DEFAULT_LENGTHS:
        full_path = Path(args.full_pattern.format(length=length))
        incremental_path = Path(args.incremental_pattern.format(length=length))
        with numpy.load(full_path, allow_pickle=False) as full, numpy.load(
            incremental_path, allow_pickle=False
        ) as incremental:
            names = sorted(set(full.files) | set(incremental.files))
            tensors = {}
            case_passed = True
            for name in names:
                if name not in full or name not in incremental:
                    metrics = {"missing": "full" if name not in full else "incremental"}
                    case_passed = False
                else:
                    metrics = compare(full[name], incremental[name], numpy)
                    if not metrics.get("shapeMatch") or metrics.get("maxAbs", 0.0) > args.atol:
                        case_passed = False
                tensors[name] = metrics
            case = {"length": length, "passed": case_passed, "tensors": tensors}
            cases.append(case)
            if first_divergence is None and not case_passed:
                first_divergence = case

    report = {
        "schemaVersion": 1,
        "absoluteTolerance": args.atol,
        "passed": first_divergence is None,
        "firstDivergence": first_divergence,
        "cases": cases,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if first_divergence is not None:
        raise SystemExit(2)


if __name__ == "__main__":
    main()
