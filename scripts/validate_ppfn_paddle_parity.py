#!/usr/bin/env python3
"""Validate official Paddle Python against the Rust native Runtime.

The same 20 preprocessed f32 tensors are fed to both implementations. The
gate requires exact token IDs, exact EOS positions, and exact tokenizer output.
Python is a development/reference dependency only; the Rust runtime does not
load or invoke Python.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageOps
from tokenizers import Tokenizer


TARGET_SIZE = (384, 384)
EOS_TOKEN_ID = 2


def crop_margin(image: Image.Image) -> Image.Image:
    gray = np.asarray(image.convert("L"), dtype=np.uint8)
    maximum = int(gray.max())
    minimum = int(gray.min())
    if maximum == minimum:
        return image
    normalized = (gray.astype(np.float32) - minimum) / (maximum - minimum) * 255
    ys, xs = np.nonzero(normalized < 200)
    if not len(xs):
        return image
    left, right = int(xs.min()), int(xs.max()) + 1
    top, bottom = int(ys.min()), int(ys.max()) + 1
    width, height = right - left, bottom - top
    if not width or not height or max(width, height) / min(width, height) > 200:
        return image
    return image.crop((left, top, right, bottom))


def preprocess(image_path: Path) -> np.ndarray:
    """Deterministic UniMERNet inference transform from PaddleOCR."""
    image = crop_margin(Image.open(image_path).convert("RGB"))
    width, height = image.size
    short, long = (width, height) if width <= height else (height, width)
    new_short = min(TARGET_SIZE)
    new_long = int(new_short * long / short)
    resized_size = (
        (new_short, new_long) if width <= height else (new_long, new_short)
    )
    image = image.resize(resized_size, resample=Image.Resampling.BILINEAR)
    image.thumbnail(TARGET_SIZE)
    delta_width = TARGET_SIZE[0] - image.width
    delta_height = TARGET_SIZE[1] - image.height
    padding = (
        delta_width // 2,
        delta_height // 2,
        delta_width - delta_width // 2,
        delta_height - delta_height // 2,
    )
    image = ImageOps.expand(image, padding)
    pixels = np.asarray(image.convert("L"), dtype=np.float32) / 255.0
    pixels = (pixels - np.float32(0.7931)) / np.float32(0.1738)
    return np.ascontiguousarray(pixels.reshape(1, 1, *TARGET_SIZE), dtype=np.float32)


def create_predictor(model_home: Path):
    import paddle.inference as paddle_infer

    config = paddle_infer.Config(
        str(model_home / "inference.json"),
        str(model_home / "inference.pdiparams"),
    )
    # Paddle 3.0's Windows CPU oneDNN scale kernel fails inside the exported
    # PP-FormulaNet while loop for real (non-trivial) generations. The native
    # bridge intentionally uses the same portable CPU-kernel configuration.
    config.disable_mkldnn()
    config.disable_glog_info()
    return paddle_infer.create_predictor(config)


def python_reference(predictor, tensor: np.ndarray, tokenizer: Tokenizer) -> dict:
    input_handle = predictor.get_input_handle(predictor.get_input_names()[0])
    input_handle.reshape(tensor.shape)
    input_handle.copy_from_cpu(tensor)
    started = time.perf_counter()
    predictor.run()
    output = predictor.get_output_handle(predictor.get_output_names()[0]).copy_to_cpu()
    elapsed_ms = (time.perf_counter() - started) * 1000
    tokens = [int(value) for value in output.reshape(-1)]
    eos_position = tokens.index(EOS_TOKEN_ID) if EOS_TOKEN_ID in tokens else None
    decode_end = len(tokens) if eos_position is None else eos_position + 1
    return {
        "tokenIds": tokens,
        "eosPosition": eos_position,
        "decodedLatex": tokenizer.decode(tokens[:decode_end], skip_special_tokens=True),
        "elapsedMs": elapsed_ms,
    }


def parse_args() -> argparse.Namespace:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-home", type=Path, required=True)
    parser.add_argument(
        "--model-home",
        type=Path,
        default=repository / "models" / "formula-rec" / "pp-formulanet-s",
    )
    parser.add_argument(
        "--images",
        type=Path,
        default=repository / "evaluation" / "benchmark" / "p08_formulas",
    )
    parser.add_argument("--count", type=int, default=20)
    parser.add_argument(
        "--output",
        type=Path,
        default=repository / "evaluation" / "benchmark" / "paddle_parity.json",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    repository = Path(__file__).resolve().parents[1]
    model_home = args.model_home.resolve()
    runtime_home = args.runtime_home.resolve()
    images = sorted(args.images.resolve().glob("*.png"))[: args.count]
    if len(images) != args.count:
        raise RuntimeError(f"expected {args.count} images, found {len(images)}")
    tokenizer = Tokenizer.from_file(str(model_home / "tokenizer.json"))
    predictor = create_predictor(model_home)

    with tempfile.TemporaryDirectory(prefix="latexsnipper-paddle-parity-") as directory:
        temporary = Path(directory)
        manifest = []
        expected = []
        for image_path in images:
            tensor = preprocess(image_path)
            tensor_path = temporary / f"{image_path.stem}.f32"
            tensor.astype("<f4", copy=False).tofile(tensor_path)
            manifest.append({"id": image_path.name, "tensor": str(tensor_path)})
            print(
                f"python reference: {image_path.name} "
                f"min={float(tensor.min()):.5f} max={float(tensor.max()):.5f}"
            )
            try:
                result = python_reference(predictor, tensor, tokenizer)
            except Exception as error:
                raise RuntimeError(
                    f"official Paddle Python failed for {image_path.name} "
                    f"(shape={tensor.shape}, min={float(tensor.min())}, "
                    f"max={float(tensor.max())})"
                ) from error
            result["id"] = image_path.name
            expected.append(result)

        manifest_path = temporary / "cases.json"
        rust_output_path = temporary / "rust.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        environment = os.environ.copy()
        environment.setdefault("GLOG_minloglevel", "3")
        subprocess.run(
            [
                "cargo",
                "run",
                "--quiet",
                "-p",
                "latexsnipper-runtime-paddle",
                "--example",
                "ppfn_tokens",
                "--",
                str(runtime_home),
                str(model_home),
                str(manifest_path),
                str(rust_output_path),
            ],
            cwd=repository,
            env=environment,
            check=True,
        )
        actual = json.loads(rust_output_path.read_text(encoding="utf-8"))

    actual_by_id = {case["id"]: case for case in actual}
    comparisons = []
    for reference in expected:
        rust = actual_by_id[reference["id"]]
        comparison = {
            "id": reference["id"],
            "tokensExact": reference["tokenIds"] == rust["tokenIds"],
            "eosExact": reference["eosPosition"] == rust["eosPosition"],
            "latexExact": reference["decodedLatex"] == rust["decodedLatex"],
            "python": reference,
            "rust": rust,
        }
        comparisons.append(comparison)

    passed = sum(
        case["tokensExact"] and case["eosExact"] and case["latexExact"]
        for case in comparisons
    )
    report = {
        "schemaVersion": 1,
        "caseCount": len(comparisons),
        "passed": passed,
        "allExact": passed == len(comparisons),
        "cases": comparisons,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"Paddle parity: {passed}/{len(comparisons)} exact; report={args.output}")
    if not report["allExact"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
