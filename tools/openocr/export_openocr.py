"""Export OpenOCR models to ONNX from official sources.

Approaches:
1. Use RapidOCR's PP-OCRv6 ONNX (already available) — copy to models/
2. Download from HuggingFace PP-OCR ONNX repos
3. Use PaddleOCR's paddle2onnx if compatible
"""
import os
import json
import shutil
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
PROJECT_ROOT = ROOT

def find_project_root():
    p = ROOT
    for _ in range(10):
        if (p / "Cargo.toml").exists():
            return p
        p = p.parent
    raise RuntimeError("Cannot find project root")


def method1_rapidocr_copy():
    """Copy RapidOCR's PP-OCRv6 ONNX models to our models/ directory."""
    print("\n=== Method 1: Copy from RapidOCR ===")
    try:
        import rapidocr
        rapid_dir = os.path.dirname(rapidocr.__file__)
        models_dir = os.path.join(rapid_dir, "models")

        # Det model
        det_src = os.path.join(models_dir, "PP-OCRv6_det_small.onnx")
        det_dst = str(PROJECT_ROOT / "models" / "text-det" / "openocr-mobile" / "model.onnx")
        if os.path.exists(det_src):
            os.makedirs(os.path.dirname(det_dst), exist_ok=True)
            shutil.copy2(det_src, det_dst)
            print(f"  Copied det model: {os.path.getsize(det_dst) / 1024 / 1024:.1f} MB")
        else:
            print(f"  Source not found: {det_src}")
            return False

        # Rec model
        rec_src = os.path.join(models_dir, "PP-OCRv6_rec_small.onnx")
        rec_dst = str(PROJECT_ROOT / "models" / "text-rec" / "openocr-mobile" / "model.onnx")
        if os.path.exists(rec_src):
            os.makedirs(os.path.dirname(rec_dst), exist_ok=True)
            shutil.copy2(rec_src, rec_dst)
            print(f"  Copied rec model: {os.path.getsize(rec_dst) / 1024 / 1024:.1f} MB")
        else:
            print(f"  Source not found: {rec_src}")
            return False

        return True
    except Exception as e:
        print(f"  Error: {e}")
        return False


def method2_huggingface():
    """Download PP-OCR ONNX from HuggingFace."""
    print("\n=== Method 2: Download from HuggingFace ===")
    files = [
        (
            "det",
            "https://huggingface.co/PaddlePaddle/PP-OCRv6_small_det_onnx/resolve/main/inference.onnx",
            PROJECT_ROOT / "models" / "text-det" / "openocr-mobile" / "model.onnx",
        ),
        (
            "det-yml",
            "https://huggingface.co/PaddlePaddle/PP-OCRv6_small_det_onnx/resolve/main/inference.yml",
            PROJECT_ROOT / "models" / "text-det" / "openocr-mobile" / "inference.yml",
        ),
        (
            "det-json",
            "https://huggingface.co/PaddlePaddle/PP-OCRv6_small_det_onnx/resolve/main/inference.json",
            PROJECT_ROOT / "models" / "text-det" / "openocr-mobile" / "inference.json",
        ),
        (
            "rec",
            "https://huggingface.co/PaddlePaddle/PP-OCRv6_small_rec_onnx/resolve/main/inference.onnx",
            PROJECT_ROOT / "models" / "text-rec" / "openocr-mobile" / "model.onnx",
        ),
        (
            "rec-yml",
            "https://huggingface.co/PaddlePaddle/PP-OCRv6_small_rec_onnx/resolve/main/inference.yml",
            PROJECT_ROOT / "models" / "text-rec" / "openocr-mobile" / "inference.yml",
        ),
        (
            "rec-json",
            "https://huggingface.co/PaddlePaddle/PP-OCRv6_small_rec_onnx/resolve/main/inference.json",
            PROJECT_ROOT / "models" / "text-rec" / "openocr-mobile" / "inference.json",
        ),
    ]
    for name, url, dst in files:
        os.makedirs(dst.parent, exist_ok=True)
        print(f"  Downloading {name}: {url[:70]}...")
        try:
            urllib.request.urlretrieve(url, dst)
            print(f"  Saved: {dst.stat().st_size / 1024 / 1024:.1f} MB")
        except Exception as e:
            print(f"  Failed: {e}")
            return False
    return True


def method3_rapidocr_engine():
    """Use RapidOCR to generate config metadata and dict."""
    print("\n=== Method 3: Generate config metadata via RapidOCR ===")
    try:
        from rapidocr import RapidOCR
        engine = RapidOCR()

        # Run on a test image to verify model works
        test_img = PROJECT_ROOT / "fixtures" / "text.png"
        if test_img.exists():
            result = engine(str(test_img))
            if result and result.txts:
                print(f"  RapidOCR working: {len(result.txts)} text regions detected")
                for i, txt in enumerate(result.txts[:3]):
                    print(f"    {i}: '{txt}'")
            else:
                print("  RapidOCR: no text detected on test image")
        else:
            print(f"  Test image not found: {test_img}")

        return True
    except Exception as e:
        print(f"  Error: {e}")
        return False


def generate_dict():
    """Generate a minimal dict.txt from RapidOCR's character set."""
    print("\n=== Generating dict.txt ===")
    # PP-OCR uses a specific character set - we can extract it from RapidOCR
    dict_dst = str(PROJECT_ROOT / "models" / "text-rec" / "openocr-mobile" / "dict.txt")
    os.makedirs(os.path.dirname(dict_dst), exist_ok=True)

    # PP-OCRv6 character set: digits + uppercase + lowercase + common punctuation + CJK
    # This is a minimal set - the actual dict should be extracted from the model
    chars = []
    # ASCII printable range
    for i in range(32, 127):
        chars.append(chr(i))
    # Fullwidth range
    for i in range(0xFF01, 0xFF5E + 1):
        chars.append(chr(i))
    # Add common CJK chars (basic set)
    for i in range(0x4E00, 0x9FFF + 1):
        chars.append(chr(i))

    with open(dict_dst, "w", encoding="utf-8") as f:
        f.write("\n".join(chars))
    print(f"  Generated dict.txt: {len(chars)} chars, {os.path.getsize(dict_dst) / 1024:.1f} KB")
    print("  NOTE: This is a minimal dictionary. Replace with actual model dict for production.")


def write_core_configs():
    """Write latexsnipper-core config.json files for OpenOCR-style variants."""
    print("\n=== Writing latexsnipper-core config.json ===")
    det_dir = PROJECT_ROOT / "models" / "text-det" / "openocr-mobile"
    rec_dir = PROJECT_ROOT / "models" / "text-rec" / "openocr-mobile"

    det_config = {
        "model_type": "dbnet",
        "model_family": "OpenOCR Mobile Text Detection",
        "license": "Apache-2.0",
        "task_type": "detection",
        "dynamic_shapes": True,
        "input": {
            "name": "x",
            "shape": [1, 3, -1, -1],
            "dtype": "float32",
            "range": [0.0, 1.0],
        },
        "output": {
            "name": "fetch_name_0",
            "shape": [1, 1, -1, -1],
            "description": "DBNet probability map",
        },
        "preprocessing": {
            "resize": {"keep_ratio": True, "pad_value": 0.0},
            "normalization": {
                "mean": [0.0, 0.0, 0.0],
                "std": [1.0, 1.0, 1.0],
            },
            "color_format": "BGR",
        },
        "postprocessing": {
            "type": "dbnet",
            "threshold": 0.3,
            "box_threshold": 0.5,
            "max_candidates": 1000,
            "unclip_ratio": 1.5,
        },
        "pipeline": {
            "min_area": 100.0,
            "min_confidence": 0.2,
            "model_files": {"primary": "model.onnx"},
        },
    }

    rec_config = {
        "model_type": "crnn_ctc",
        "model_family": "OpenOCR Mobile Text Recognition",
        "license": "Apache-2.0",
        "task_type": "ocr",
        "dynamic_shapes": True,
        "input": {
            "name": "x",
            "shape": [1, 3, 48, 3200],
            "dtype": "float32",
            "range": [-1.0, 1.0],
        },
        "output": {
            "name": "fetch_name_0",
            "shape": [1, -1, 18710],
            "description": "CTC logits",
        },
        "preprocessing": {
            "resize": {
                "width": 3200,
                "height": 48,
                "keep_ratio": True,
                "pad_value": 0.0,
            },
            "normalization": {
                "mean": [0.5, 0.5, 0.5],
                "std": [0.5, 0.5, 0.5],
            },
            "color_format": "BGR",
        },
        "decoding": {
            "type": "ctc_greedy",
            "blank_id": 0,
            "keys_file": "inference.yml",
        },
        "pipeline": {
            "model_files": {
                "primary": "model.onnx",
                "tokenizer": "inference.yml",
            },
        },
    }

    for path, data in [
        (det_dir / "config.json", det_config),
        (rec_dir / "config.json", rec_config),
    ]:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
        print(f"  Wrote: {path}")


def method5_slanet_plus():
    """Copy SLANet_plus ONNX from RapidTable if available."""
    print("\n=== Method 5: Copy SLANet_plus from RapidTable ===")
    try:
        import rapid_table

        rt_dir = Path(rapid_table.__file__).resolve().parent
        src = rt_dir / "models" / "slanet-plus.onnx"
        dst_dir = PROJECT_ROOT / "models" / "table-struct" / "slanet-plus"
        dst = dst_dir / "model.onnx"

        if not src.exists():
            print(f"  Source not found: {src}")
            return False

        dst_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        config = {
            "model_type": "slanet",
            "model_family": "SLANet Plus Table Structure Recognition",
            "license": "Apache-2.0",
            "task_type": "structure",
            "input": {
                "name": "x",
                "shape": [1, 3, 488, 488],
                "dtype": "float32",
                "range": [-2.1179, 2.64],
            },
            "output": {
                "name": "structure_probs",
                "shape": [1, -1, 50],
                "description": "SLANet cell coordinates and structure token logits",
            },
            "preprocessing": {
                "resize": {
                    "width": 488,
                    "height": 488,
                    "keep_ratio": True,
                    "pad_value": 0.0,
                },
                "normalization": {
                    "mean": [0.485, 0.456, 0.406],
                    "std": [0.229, 0.224, 0.225],
                },
                "color_format": "RGB",
            },
            "pipeline": {"model_files": {"primary": "model.onnx"}},
        }
        (dst_dir / "config.json").write_text(
            json.dumps(config, indent=2) + "\n", encoding="utf-8"
        )
        print(f"  Copied SLANet_plus: {dst.stat().st_size / 1024 / 1024:.1f} MB")
        print(f"  Wrote: {dst_dir / 'config.json'}")
        return True
    except Exception as e:
        print(f"  Error: {e}")
        return False


def main():
    print(f"Project root: {PROJECT_ROOT}\n")

    # Try method 1 (RapidOCR copy)
    success = method1_rapidocr_copy()

    if not success:
        # Try method 2 (HuggingFace)
        success = method2_huggingface()

    if success:
        # Verify with RapidOCR
        method3_rapidocr_engine()

        # Generate dict.txt
        generate_dict()

        # Generate config.json files consumed by latexsnipper-core
        write_core_configs()

        # Verify config files exist
        det_dir = PROJECT_ROOT / "models" / "text-det" / "openocr-mobile"
        rec_dir = PROJECT_ROOT / "models" / "text-rec" / "openocr-mobile"
        print(f"\n=== Verification ===")
        print(f"  det dir: {det_dir}")
        for f in sorted(det_dir.iterdir()):
            print(f"    {f.name} ({f.stat().st_size / 1024:.1f} KB)")
        print(f"  rec dir: {rec_dir}")
        for f in sorted(rec_dir.iterdir()):
            print(f"    {f.name} ({f.stat().st_size / 1024:.1f} KB)")

        # Copy layout model
        method4_layout_model()

        # Copy table structure model
        method5_slanet_plus()
    else:
        print("\nFailed to export models from any source.")


def method4_layout_model():
    """Copy layout model from RapidLayout."""
    print("\n=== Method 4: Copy layout model from RapidLayout ===")
    try:
        import rapid_layout
        rl_dir = os.path.dirname(rapid_layout.__file__)
        src = os.path.join(rl_dir, "models", "layout_cdla.onnx")
        dst = str(PROJECT_ROOT / "models" / "layout" / "pp-layout-cdla" / "model.onnx")
        if os.path.exists(src):
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.copy2(src, dst)
            print(f"  Copied layout model: {os.path.getsize(dst) / 1024 / 1024:.1f} MB")
        else:
            print(f"  Source not found: {src}")
    except Exception as e:
        print(f"  Error: {e}")


if __name__ == "__main__":
    main()
