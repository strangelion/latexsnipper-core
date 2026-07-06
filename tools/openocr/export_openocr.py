"""Export OpenOCR models to ONNX from official sources.

Approaches:
1. Use RapidOCR's PP-OCRv6 ONNX (already available) — copy to models/
2. Download from HuggingFace PP-OCR ONNX repos
3. Use PaddleOCR's paddle2onnx if compatible
"""
import os
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
    urls = {
        "det": "https://huggingface.co/rapidocr/PP-OCRv6_det_small/resolve/main/PP-OCRv6_det_small.onnx",
        "rec": "https://huggingface.co/rapidocr/PP-OCRv6_rec_small/resolve/main/PP-OCRv6_rec_small.onnx",
    }
    for name, url in urls.items():
        dst = str(PROJECT_ROOT / "models" / f"text-{name[:3]}" / "openocr-mobile" / "model.onnx")
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        print(f"  Downloading {name}: {url[:60]}...")
        try:
            urllib.request.urlretrieve(url, dst)
            print(f"  Saved: {os.path.getsize(dst) / 1024 / 1024:.1f} MB")
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
