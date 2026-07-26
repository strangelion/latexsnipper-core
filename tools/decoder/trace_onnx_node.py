#!/usr/bin/env python3
"""Trace an ONNX node and its shape-producing ancestors to JSON."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


TRACE_OPS = {
    "Reshape",
    "Transpose",
    "Slice",
    "Concat",
    "Expand",
    "Gather",
    "Unsqueeze",
}


def require_onnx():
    try:
        import onnx
    except ImportError as error:
        raise SystemExit("onnx is required: python -m pip install onnx") from error
    return onnx


def dimensions(value) -> list[int | str | None]:
    tensor = value.type.tensor_type
    result = []
    for dimension in tensor.shape.dim:
        if dimension.HasField("dim_value"):
            result.append(dimension.dim_value)
        elif dimension.HasField("dim_param"):
            result.append(dimension.dim_param)
        else:
            result.append(None)
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--node", default="Add.34")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    onnx = require_onnx()
    model = onnx.shape_inference.infer_shapes(onnx.load(args.model))
    producers = {
        output: node for node in model.graph.node for output in node.output if output
    }
    values = {
        value.name: dimensions(value)
        for value in (
            list(model.graph.input)
            + list(model.graph.output)
            + list(model.graph.value_info)
        )
    }
    targets = [
        node
        for node in model.graph.node
        if node.name == args.node or args.node in node.output
    ]
    if len(targets) != 1:
        raise SystemExit(
            f"expected exactly one node matching {args.node!r}, found {len(targets)}"
        )

    visited = set()

    def trace(node):
        identity = node.name or "|".join(node.output)
        if identity in visited:
            return {"node": identity, "cycleOrShared": True}
        visited.add(identity)
        ancestors = []
        for input_name in node.input:
            producer = producers.get(input_name)
            if producer is not None and producer.op_type in TRACE_OPS:
                ancestors.append(trace(producer))
        return {
            "name": node.name,
            "opType": node.op_type,
            "inputs": [
                {"name": name, "staticShape": values.get(name)} for name in node.input
            ],
            "outputs": [
                {"name": name, "staticShape": values.get(name)} for name in node.output
            ],
            "ancestors": ancestors,
        }

    report = {
        "schemaVersion": 1,
        "model": str(args.model),
        "modelIrVersion": model.ir_version,
        "opset": [
            {"domain": item.domain, "version": item.version}
            for item in model.opset_import
        ],
        "target": trace(targets[0]),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
