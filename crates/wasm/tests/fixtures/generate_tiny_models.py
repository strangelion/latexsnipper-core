"""Generate deterministic ONNX fixtures for the browser inference test.

The models are synthetic and contain no third-party weights. They intentionally
exercise the real Tract, detector, recognizer, pipeline, and AST path while
remaining small enough to run in headless browser CI.
"""

from pathlib import Path

import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper


ROOT = Path(__file__).resolve().parent


def save_model(graph: onnx.GraphProto, name: str) -> None:
    model = helper.make_model(
        graph,
        producer_name="latexsnipper-core-tests",
        opset_imports=[helper.make_opsetid("", 13)],
    )
    model.ir_version = 8
    onnx.checker.check_model(model)
    onnx.save_model(model, ROOT / name)


def detector() -> None:
    input_info = helper.make_tensor_value_info(
        "x", TensorProto.FLOAT, [1, 3, 32, 32]
    )
    output_info = helper.make_tensor_value_info(
        "output", TensorProto.FLOAT, [1, 1, 32, 32]
    )
    probability_map = np.zeros((1, 1, 32, 32), dtype=np.float32)
    probability_map[:, :, 6:26, 4:28] = 0.95
    zero = numpy_helper.from_array(np.array(0.0, dtype=np.float32), "zero")
    expected = numpy_helper.from_array(probability_map, "expected")
    reduce = helper.make_node(
        "ReduceMean", ["x"], ["input_mean"], axes=[0, 1, 2, 3], keepdims=0
    )
    multiply = helper.make_node("Mul", ["input_mean", "zero"], ["zero_from_input"])
    add = helper.make_node("Add", ["expected", "zero_from_input"], ["output"])
    save_model(
        helper.make_graph(
            [reduce, multiply, add],
            "tiny_text_detector",
            [input_info],
            [output_info],
            initializer=[zero, expected],
        ),
        "tiny-text-det.onnx",
    )


def recognizer() -> None:
    input_info = helper.make_tensor_value_info(
        "x", TensorProto.FLOAT, [1, 3, 48, 320]
    )
    output_info = helper.make_tensor_value_info(
        "output", TensorProto.FLOAT, [1, 3, 4]
    )
    logits = np.array(
        [[[0.0, 0.0, 9.0, 0.0], [9.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 9.0]]],
        dtype=np.float32,
    )
    zero = numpy_helper.from_array(np.array(0.0, dtype=np.float32), "zero")
    expected = numpy_helper.from_array(logits, "expected")
    reduce = helper.make_node(
        "ReduceMean", ["x"], ["input_mean"], axes=[0, 1, 2, 3], keepdims=0
    )
    multiply = helper.make_node("Mul", ["input_mean", "zero"], ["zero_from_input"])
    add = helper.make_node("Add", ["expected", "zero_from_input"], ["output"])
    graph = helper.make_graph(
        [reduce, multiply, add],
        "tiny_text_recognizer",
        [input_info],
        [output_info],
        initializer=[zero, expected],
    )
    save_model(graph, "tiny-text-rec.onnx")


if __name__ == "__main__":
    detector()
    recognizer()
