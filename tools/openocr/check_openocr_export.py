"""Check OpenOCR model availability and export paths."""
import urllib.request
import json
import os

print("=== OpenOCR / PaddleOCR Model Status ===\n")

# 1. Check if paddle2onnx can export
print("1. paddle2onnx availability:")
try:
    import paddle2onnx
    print(f"   paddle2onnx {paddle2onnx.__version__} installed")
except Exception as e:
    print(f"   paddle2onnx: {e}")

# 2. Check PaddlePaddle
print("\n2. PaddlePaddle:")
try:
    import paddle
    print(f"   PaddlePaddle {paddle.__version__} installed")
except Exception as e:
    print(f"   PaddlePaddle: {e}")

# 3. Search HuggingFace for PP-OCR ONNX models
print("\n3. HuggingFace PP-OCR ONNX models:")
try:
    url = "https://huggingface.co/api/models?search=ppocr+onnx&sort=downloads&limit=10"
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    resp = urllib.request.urlopen(req, timeout=10)
    data = json.loads(resp.read())
    for m in data:
        print(f"   {m['id']} ({m.get('downloads',0)} downloads)")
except Exception as e:
    print(f"   Search error: {e}")

# 4. Check ModelScope for OpenOCR models
print("\n4. ModelScope OpenOCR models:")
try:
    url = "https://modelscope.cn/api/v1/models?PageSize=5&SortBy=Downloads&Tags=ocr"
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    resp = urllib.request.urlopen(req, timeout=10)
    data = json.loads(resp.read())
    for m in data.get("Data", {}).get("Items", [])[:5]:
        print(f"   {m.get('Name','')} — {m.get('Downloads',0)} downloads")
except Exception as e:
    print(f"   Search error: {e}")

# 5. Check local PaddleOCR model cache
print("\n5. Local PaddleOCR model cache:")
cache_dirs = [
    os.path.expanduser("~/.paddleocr"),
    os.path.expanduser("~/.paddlex"),
]
for d in cache_dirs:
    if os.path.isdir(d):
        for root, dirs, files in os.walk(d):
            for f in files:
                if f.endswith('.onnx'):
                    size = os.path.getsize(os.path.join(root, f)) // 1024 // 1024
                    print(f"   {os.path.relpath(os.path.join(root, f), d)} ({size}MB)")

# 6. Check PP-OCR model zoo (direct download links)
print("\n6. PP-OCRv4/v5/v6 Model Zoo (PaddlePaddle official):")
ppocr_models = [
    ("PP-OCRv4 det", "https://paddleocr.bj.bcebos.com/PP-OCRv4/chinese/ch_PP-OCRv4_det_infer.tar"),
    ("PP-OCRv4 rec", "https://paddleocr.bj.bcebos.com/PP-OCRv4/chinese/ch_PP-OCRv4_rec_infer.tar"),
    ("PP-OCRv5 det", "https://paddleocr.bj.bcebos.com/PP-OCRv5/chinese/ch_PP-OCRv5_det_infer.tar"),
    ("PP-OCRv5 rec", "https://paddleocr.bj.bcebos.com/PP-OCRv5/chinese/ch_PP-OCRv5_rec_infer.tar"),
]
for name, url in ppocr_models:
    print(f"   {name}: {url}")

print("\n=== Export Process ===")
print("""
To export OpenOCR/PaddleOCR models to ONNX:

Option A: Using paddle2onnx (if compatible PaddlePaddle installed)
    pip install paddle2onnx
    paddle2onnx --model_dir ./inference \\
                --model_filename inference.pdmodel \\
                --params_filename inference.pdiparams \\
                --save_file model.onnx \\
                --opset_version 11

Option B: Download pre-exported ONNX from HuggingFace
    huggingface-cli download <model-id> --local-dir ./models

Option C: Use RapidOCR (already has PP-OCRv6 ONNX)
    from rapidocr import RapidOCR
    engine = RapidOCR()
    # Models auto-download to: rapidocr/models/
""")
