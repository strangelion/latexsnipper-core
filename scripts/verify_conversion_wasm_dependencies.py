#!/usr/bin/env python3
"""Reject recognition/codec dependencies in the isolated conversion WASM profile."""
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
command = ["cargo", "tree", "--locked", "-p", "latexsnipper-wasm", "--no-default-features",
           "--features", "conversion-only", "--target", "wasm32-unknown-unknown",
           "--edges", "normal", "--prefix", "none"]
result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True)
packages = set(re.findall(r"^([a-zA-Z0-9_-]+) v", result.stdout, re.MULTILINE))
forbidden = {"latexsnipper-engine", "latexsnipper-runtime", "latexsnipper-model", "latexsnipper-inference",
             "latexsnipper-tensor", "latexsnipper-pipeline", "latexsnipper-image", "latexsnipper-tract",
             "latexsnipper-export", "ort", "image", "imageproc", "tokenizers", "ravif", "rav1e"}
found = sorted(packages & forbidden | {name for name in packages if name.startswith("tract-")})
if found:
    raise SystemExit("Conversion WASM includes forbidden dependencies: " + ", ".join(found))
required = {"latexsnipper-conversion", "latexsnipper-ast", "latexsnipper-syntax", "latexsnipper-api-types"}
if not required <= packages:
    raise SystemExit("Conversion WASM lost required conversion/protocol packages")
print(f"Conversion-only WASM verified: {len(packages)} normal-dependency packages, no recognition/runtime/codec stack")
