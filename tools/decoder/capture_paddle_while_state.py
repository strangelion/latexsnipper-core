#!/usr/bin/env python3
"""Inspect and, when Paddle is available, prepare decoder while-state capture.

This script fails closed. Static PIR value IDs and declared shapes are useful
artifact evidence, but they are never promoted to runtime state names, semantic
roles, or step-dependent shapes without a Paddle execution capture.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import pathlib
from typing import Any, Iterator


def walk(value: Any, path: str = "$") -> Iterator[tuple[str, Any]]:
    yield path, value
    if isinstance(value, dict):
        for key, child in value.items():
            yield from walk(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from walk(child, f"{path}[{index}]")


def dtype_and_shape(type_info: dict[str, Any] | None) -> tuple[str, list[int] | None]:
    if not type_info:
        return "unknown", None
    data = type_info.get("D")
    if not isinstance(data, list) or len(data) < 2:
        return "unknown", None
    dtype_info, shape = data[0], data[1]
    dtype = "unknown"
    if isinstance(dtype_info, dict):
        dtype = str(dtype_info.get("#", "unknown")).removeprefix("0.t_")
    return dtype, shape if isinstance(shape, list) else None


def inspect_pir(program_path: pathlib.Path) -> dict[str, Any]:
    raw = program_path.read_bytes()
    document = json.loads(raw)
    while_ops: list[tuple[str, dict[str, Any]]] = []
    for path, value in walk(document):
        if isinstance(value, dict) and value.get("#") == "1.while":
            while_ops.append((path, value))
    inspected = []
    for path, operation in while_ops:
        inputs = operation.get("I", [])
        outputs = operation.get("O", [])
        states = []
        # PIR while input 0 is the loop condition. Its 29 remaining block
        # arguments correspond in count to 29 results. Position is recorded
        # only as a graph edge; no layer/attention semantic is inferred.
        for index, output in enumerate(outputs):
            dtype, shape = dtype_and_shape(output.get("TT"))
            input_index = index + 1
            input_value = (
                inputs[input_index].get("%") if input_index < len(inputs) else None
            )
            states.append(
                {
                    "paddleName": None,
                    "pirInputValueId": input_value,
                    "pirOutputValueId": output.get("%"),
                    "dtype": dtype,
                    "declaredShape": shape,
                    "shapeAtStep0": None,
                    "shapeAtStep1": None,
                    "shapeAtStep2": None,
                    "growthAxis": None,
                    "layerIndex": None,
                    "attentionKind": "unknown",
                    "semanticRole": "unknown",
                    "updateRule": "unknown",
                    "evidenceSource": f"{program_path.name}:{path}",
                    "confidence": "low",
                }
            )
        inspected.append(
            {
                "path": path,
                "inputCount": len(inputs),
                "outputCount": len(outputs),
                "stateCandidateCount": len(states),
                "states": states,
            }
        )
    return {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "format": "paddle-pir-json",
        "whileOperations": inspected,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--feed-npz", type=pathlib.Path)
    args = parser.parse_args()
    program_path = args.model_dir / "inference.json"
    params_path = args.model_dir / "inference.pdiparams"
    if not program_path.is_file() or not params_path.is_file():
        raise SystemExit("inference.json and inference.pdiparams are required")

    static_evidence = inspect_pir(program_path)
    paddle_available = importlib.util.find_spec("paddle") is not None
    blockers = []
    if not paddle_available:
        blockers.append("Python environment does not contain PaddlePaddle.")
    if args.feed_npz is None:
        blockers.append(
            "No licensed, representative decoder feed was supplied via --feed-npz."
        )
    if paddle_available and args.feed_npz is not None:
        blockers.append(
            "Runtime hook for PIR while block arguments is not available through the "
            "supported Paddle inference API; an instrumented export is required."
        )

    output = {
        "schemaVersion": 1,
        "status": "blocked",
        "modelDirectory": args.model_dir.as_posix(),
        "program": static_evidence,
        "parametersSha256": hashlib.sha256(params_path.read_bytes()).hexdigest(),
        "paddleAvailable": paddle_available,
        "runtimeStepsCaptured": [],
        "stateSchemaFrozen": False,
        "blockers": blockers,
        "safety": [
            "PIR value position is not used to guess layer or attention semantics.",
            "Declared dynamic dimensions are not substituted with invented step shapes.",
            "No paddle-state-step fixture is emitted without a real runtime capture.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
