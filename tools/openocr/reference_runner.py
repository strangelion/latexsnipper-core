#!/usr/bin/env python3
"""
OpenOCR Reference Runner (development only).

Generates golden reference JSON for OpenOCR DBNet detection + SVTRv2/CTC
recognition, using RapidOCR (ONNX Runtime) as the inference engine.

Outputs:
    tests/fixtures/openocr/reference/model-metadata.json    – ONNX I/O metadata
    tests/fixtures/openocr/reference/openocr-mobile-det.json  – detection results
    tests/fixtures/openocr/reference/openocr-mobile-rec.json  – recognition results
    tests/fixtures/openocr/reference/full-ocr-results.json    – combined results
"""

import argparse
import json
import os
import sys
from pathlib import Path


def find_project_root() -> Path:
    p = Path(__file__).resolve().parent
    for _ in range(10):
        if (p / "Cargo.toml").exists():
            return p
        p = p.parent
    raise RuntimeError("Cannot find project root")


ROOT = find_project_root()
FIXTURES = ROOT / "tests" / "fixtures" / "openocr"
REFERENCE = FIXTURES / "reference"
EXPECTED = FIXTURES / "expected"


# ── ONNX metadata inspection (always works) ────────────────────────────


def inspect_onnx(model_path: Path) -> dict:
    try:
        import onnxruntime as ort

        session = ort.InferenceSession(str(model_path))
        meta = {
            "model_path": str(model_path.relative_to(ROOT)),
            "inputs": [],
            "outputs": [],
        }
        for inp in session.get_inputs():
            meta["inputs"].append({
                "name": inp.name,
                "shape": list(inp.shape) if inp.shape else [],
                "dtype": str(inp.type),
            })
        for out in session.get_outputs():
            meta["outputs"].append({
                "name": out.name,
                "shape": list(out.shape) if out.shape else [],
                "dtype": str(out.type),
            })
        return meta
    except Exception as e:
        return {"error": str(e)}


# ── RapidOCR-based detection + recognition ─────────────────────────────

_RAPID_ENGINE = None


def _get_engine():
    global _RAPID_ENGINE
    if _RAPID_ENGINE is None:
        from rapidocr import RapidOCR
        _RAPID_ENGINE = RapidOCR()
    return _RAPID_ENGINE


def run_rapidocr(image_path: Path) -> list[dict]:
    """Run full OCR (det + rec) using RapidOCR's ONNX Runtime engine.

    Returns list of {quad, text, confidence} dicts.
    """
    engine = _get_engine()
    result = engine(str(image_path))

    regions = []
    if result and result.txts:
        for box, txt, score in zip(result.boxes, result.txts, result.scores):
            # box is shape (4, 2): [[x1,y1],[x2,y2],[x3,y3],[x4,y4]]
            quad = [[float(p[0]), float(p[1])] for p in box]
            regions.append({
                "quad": quad,
                "text": str(txt),
                "confidence": float(score),
            })
    return regions


# ── fallback: ONNX Runtime direct (for text-det only without text-rec) ──


# ── main ────────────────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(description="OpenOCR Reference Runner")
    parser.add_argument(
        "--model-dir",
        type=Path,
        default=ROOT / "models",
        help="Models directory (default: <project>/models)",
    )
    parser.add_argument(
        "--fixture-dir",
        type=Path,
        default=FIXTURES,
        help="Fixtures directory (default: <project>/tests/fixtures/openocr)",
    )
    parser.add_argument(
        "--inspect",
        type=Path,
        nargs="*",
        default=[],
        help="Inspect ONNX model(s) metadata without running inference",
    )
    args = parser.parse_args()

    REFERENCE.mkdir(parents=True, exist_ok=True)
    EXPECTED.mkdir(parents=True, exist_ok=True)

    # ── Step 1: inspect ONNX model metadata ────────────────────────
    models_to_inspect = list(args.inspect)
    if not models_to_inspect:
        models_to_inspect = sorted(args.model_dir.rglob("*.onnx"))

    print(f"\n{'='*60}")
    print("Step 1: ONNX Model Metadata Inspection")
    print(f"{'='*60}")
    all_meta = {}
    for mp in models_to_inspect:
        rel = mp.relative_to(ROOT)
        print(f"  {rel}")
        meta = inspect_onnx(mp)
        all_meta[str(rel)] = meta
        if "error" in meta:
            print(f"    Error: {meta['error']}")
        else:
            for inp in meta["inputs"]:
                print(f"    Input:  {inp['name']} {inp['shape']} {inp['dtype']}")
            for out in meta["outputs"]:
                print(f"    Output: {out['name']} {out['shape']} {out['dtype']}")

    meta_path = REFERENCE / "model-metadata.json"
    with open(meta_path, "w") as f:
        json.dump(all_meta, f, indent=2)
    print(f"  -> {meta_path.relative_to(ROOT)}")

    # ── Step 2: run detection on OCR fixtures ──────────────────────
    print(f"\n{'='*60}")
    print("Step 2: Text Detection (RapidOCR DBNet)")
    print(f"{'='*60}")

    ocr_images = sorted(args.fixture_dir.glob("*.png"))

    det_reference = {}
    for img_path in ocr_images:
        print(f"  {img_path.name} ...", end=" ", flush=True)
        try:
            results = run_rapidocr(img_path)
            det_reference[img_path.name] = {
                "boxes": [
                    {"quad": r["quad"], "confidence": r["confidence"]}
                    for r in results
                ],
            }
            print(f"{len(results)} boxes")
        except Exception as e:
            print(f"ERROR: {e}")
            det_reference[img_path.name] = {"error": str(e)}

    det_path = REFERENCE / "openocr-mobile-det.json"
    with open(det_path, "w") as f:
        json.dump(det_reference, f, indent=2)
    print(f"  -> {det_path.relative_to(ROOT)}")

    # ── Step 3: run recognition on OCR fixtures ────────────────────
    print(f"\n{'='*60}")
    print("Step 3: Text Recognition (RapidOCR CTC)")
    print(f"{'='*60}")

    rec_reference = {}
    for img_path in ocr_images:
        print(f"  {img_path.name} ...", end=" ", flush=True)
        try:
            results = run_rapidocr(img_path)
            rec_reference[img_path.name] = {
                "texts": [
                    {"text": r["text"], "confidence": r["confidence"]}
                    for r in results
                ],
            }
            print(f"{len(results)} texts")
            for r in results:
                print(f"    '{r['text']}' (conf={r['confidence']:.3f})")
        except Exception as e:
            print(f"ERROR: {e}")
            rec_reference[img_path.name] = {"error": str(e)}

    rec_path = REFERENCE / "openocr-mobile-rec.json"
    with open(rec_path, "w") as f:
        json.dump(rec_reference, f, indent=2)
    print(f"  -> {rec_path.relative_to(ROOT)}")

    # ── Step 4: full OCR on all fixture images ────────────────────
    print(f"\n{'='*60}")
    print("Step 4: Full OCR on All Fixtures")
    print(f"{'='*60}")

    all_fixtures = sorted(ROOT.glob("fixtures/*.png")) + sorted(
        args.fixture_dir.glob("*.png")
    )
    # Deduplicate
    seen = set()
    unique_fixtures = []
    for f in all_fixtures:
        if f.name not in seen:
            seen.add(f.name)
            unique_fixtures.append(f)

    full_results = {}
    for img_path in unique_fixtures:
        print(f"  {img_path.name} ...", end=" ", flush=True)
        try:
            from PIL import Image
            with Image.open(img_path) as im:
                iw, ih = im.size
            results = run_rapidocr(img_path)
            full_results[img_path.name] = {
                "image_size": [iw, ih],
                "regions": [
                    {
                        "quad": r["quad"],
                        "text": r["text"],
                        "confidence": r["confidence"],
                    }
                    for r in results
                ],
            }
            print(f"{len(results)} regions")
        except Exception as e:
            print(f"ERROR: {e}")

    full_path = REFERENCE / "full-ocr-results.json"
    with open(full_path, "w") as f:
        json.dump(full_results, f, indent=2)
    print(f"  -> {full_path.relative_to(ROOT)}")

    # ── Summary ─────────────────────────────────────────────────────
    print(f"\n{'='*60}")
    print("Summary")
    print(f"{'='*60}")
    for f in sorted(REFERENCE.iterdir()):
        if f.is_file():
            print(f"  {f.name} ({f.stat().st_size} bytes)")
    print("\nDone.")


if __name__ == "__main__":
    main()
