#!/usr/bin/env python3
"""Export a deterministic two-method recognizer to an XNNPACK `.pte`.

This is a development/model-packaging tool. The resulting program is consumed
by the native runtime and does not require Python on the end-user machine.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

# ExecuTorch Windows wheels currently package the host compiler outside the
# resource path inspected by exir. Prefer the version-matched wheel tool while
# preserving an explicit caller override.
_flatc_name = "flatc.exe" if sys.platform == "win32" else "flatc"
_packaged_flatc = (
    Path(sys.prefix) / "Lib" / "site-packages" / "executorch" / "data" / "bin" / _flatc_name
)
if _packaged_flatc.is_file():
    os.environ.setdefault("FLATC_EXECUTABLE", str(_packaged_flatc))

import torch
from executorch.backends.xnnpack.partition.xnnpack_partitioner import (
    XnnpackPartitioner,
)
from executorch.exir import to_edge_transform_and_lower


class TinyRecognizer(torch.nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.conv = torch.nn.Conv2d(1, 2, kernel_size=3, padding=1)
        self.linear = torch.nn.Linear(2 * 8 * 8, 4)
        with torch.no_grad():
            self.conv.weight.copy_(
                torch.arange(18, dtype=torch.float32).reshape(2, 1, 3, 3)
                / 40.0
                - 0.2
            )
            self.conv.bias.copy_(torch.tensor([0.05, -0.1]))
            self.linear.weight.copy_(
                torch.arange(4 * 128, dtype=torch.float32).reshape(4, 128)
                / 4096.0
                - 0.06
            )
            self.linear.bias.copy_(torch.tensor([0.1, -0.2, 0.3, -0.4]))

    def features(self, image: torch.Tensor) -> torch.Tensor:
        return torch.relu(self.conv(image))

    def forward(self, image: torch.Tensor) -> torch.Tensor:
        return self.linear(torch.flatten(self.features(image), 1))


class Encoder(torch.nn.Module):
    def __init__(self, recognizer: TinyRecognizer) -> None:
        super().__init__()
        self.recognizer = recognizer

    def forward(self, image: torch.Tensor) -> torch.Tensor:
        return self.recognizer.features(image)


def input_tensor() -> torch.Tensor:
    return (torch.arange(64, dtype=torch.float32).reshape(1, 1, 8, 8) - 31.5) / 16.0


def export(output: Path, expected: Path) -> None:
    torch.manual_seed(0)
    recognizer = TinyRecognizer().eval()
    encoder = Encoder(recognizer).eval()
    image = input_tensor()
    programs = {
        "forward": torch.export.export(recognizer, (image,), strict=True),
        "encode": torch.export.export(encoder, (image,), strict=True),
    }
    edge = to_edge_transform_and_lower(
        programs,
        partitioner=[XnnpackPartitioner()],
    )
    program = edge.to_executorch()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(program.buffer)

    with torch.no_grad():
        reference = {
            "forward": recognizer(image).reshape(-1).tolist(),
            "encode": encoder(image).reshape(-1).tolist(),
        }
    expected.parent.mkdir(parents=True, exist_ok=True)
    expected.write_text(
        json.dumps(reference, indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"exported XNNPACK program: {output}")
    print(f"methods: {sorted(program.methods)}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expected", type=Path, required=True)
    return parser.parse_args()


if __name__ == "__main__":
    args = parse_args()
    export(args.output.resolve(), args.expected.resolve())
