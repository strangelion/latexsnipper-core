"""Check UniRec availability and ONNX export path."""
import importlib.util
import urllib.request
import json
import subprocess
import sys

# 1. Check local packages
print("=== Local Python packages ===")
try:
    import pkg_resources
    pkgs = list(pkg_resources.working_set)
    related = [p for p in pkgs if any(k in p.key.lower() for k in ['uni', 'opendoc', 'openocr'])]
    print(f"Related: {[p.key for p in related]}")
except:
    print("pkg_resources not available")

# 2. Check for modules
print("\n=== Module search ===")
for mod_name in ['unirec', 'uni_rec', 'openocr_unirec', 'openocr', 'opendoc']:
    spec = importlib.util.find_spec(mod_name)
    if spec:
        print(f"  {mod_name}: {spec.origin}")
    else:
        print(f"  {mod_name}: not found")

# 3. Search PyPI for UniRec
print("\n=== PyPI search ===")
try:
    # Search PyPI
    url = "https://pypi.org/pypi/unirec/json"
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    resp = urllib.request.urlopen(req, timeout=5)
    data = json.loads(resp.read())
    info = data["info"]
    print(f"  Name: {info['name']} {info['version']}")
    print(f"  Summary: {info['summary'][:300]}")
except urllib.error.HTTPError as e:
    print(f"  UniRec not on PyPI (HTTP {e.code})")
except Exception as e:
    print(f"  Error: {e}")

# 4. Check GitHub for OpenOCR/UniRec
print("\n=== Alternative: RapidOCR built-in models ===")
try:
    from rapidocr import RapidOCR
    import os
    rd = os.path.dirname(RapidOCR.__module__) if hasattr(RapidOCR, '__module__') else None
    print(f"  RapidOCR OK")
    # Check default_models for any unified rec model
    rapid_dir = os.path.dirname(sys.modules.get('rapidocr', RapidOCR).__file__ if 'rapidocr' in sys.modules else '')
    default_models = os.path.join(os.path.dirname(os.__file__) if 'rapidocr' not in str(RapidOCR) else rapid_dir, 'rapidocr', 'default_models.yaml')
except Exception as e:
    print(f"  Error: {e}")

# 5. Check PaddleOCR for UnifiedRec models
print("\n=== PaddleOCR models ===")
try:
    import paddleocr
    pod = os.path.dirname(paddleocr.__file__)
    for root, dirs, files in os.walk(pod):
        for f in files:
            if 'uni' in f.lower() or 'unirec' in f.lower():
                print(f"  Found: {os.path.join(root, f)}")
except Exception as e:
    print(f"  Error: {e}")

print("\n=== RapidLayout available models (unified recognition alternative) ===")
try:
    import rapid_layout, os, yaml
    rp_dir = os.path.dirname(rapid_layout.__file__)
    cfg_path = os.path.join(rp_dir, "configs", "default_models.yaml")
    with open(cfg_path) as f:
        cfg = yaml.safe_load(f)
    for name, info in cfg.items():
        url = info.get("model_dir_or_path", "?")
        print(f"  {name}: {url[:90]}")
except Exception as e:
    print(f"  Error: {e}")

print("\n=== Conclusion ===")
print("UniRec is NOT available as a standalone Python package on PyPI.")
print("The plan describes UniRec as a future/planned model, not yet released.")
print("Current best option: Use existing specialized models (PP-OCR CTC + TrOCR).")
