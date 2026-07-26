#!/usr/bin/env python3
"""Prepare the versioned 50-image formula-recognition benchmark.

The existing deterministic PP-FormulaNet corpus is retained for categories it
already covers. Six structurally important matrix/cases fixtures are rendered
with Typst because matplotlib mathtext cannot represent those environments.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[1]
SOURCE = REPOSITORY / "evaluation" / "benchmark" / "p08_formulas"
OUTPUT = REPOSITORY / "benchmarks" / "formula-recognition" / "v1"
IMAGES = OUTPUT / "images"
NORMALIZATION_VERSION = "formula-normalization-v1"

CATEGORIES_BY_INDEX = {
    0: ["simple_inline"],
    1: ["simple_inline", "superscript_subscript"],
    2: ["fractions_roots"],
    3: ["simple_inline", "superscript_subscript"],
    4: ["simple_inline", "greek_font_variant"],
    5: ["simple_inline", "greek_font_variant"],
    6: ["simple_inline", "superscript_subscript"],
    7: ["integral_sum_limit"],
    8: ["integral_sum_limit"],
    9: ["integral_sum_limit"],
    10: ["fractions_roots"],
    11: ["fractions_roots"],
    12: ["integral_sum_limit"],
    13: ["integral_sum_limit"],
    14: ["greek_font_variant"],
    15: ["integral_sum_limit", "superscript_subscript"],
    16: ["superscript_subscript"],
    17: ["fractions_roots"],
    18: ["fractions_roots", "superscript_subscript"],
    19: ["fractions_roots", "integral_sum_limit"],
    20: ["matrix_piecewise"],
    21: ["matrix_piecewise"],
    22: ["matrix_piecewise", "greek_font_variant"],
    23: ["matrix_piecewise", "superscript_subscript"],
    24: ["matrix_piecewise", "mixed_chinese_english"],
    25: ["matrix_piecewise", "mixed_chinese_english"],
    26: ["superscript_subscript"],
    27: ["superscript_subscript"],
    28: ["simple_inline"],
    29: ["long_multi_relation"],
    30: ["long_multi_relation", "fractions_roots"],
    31: ["long_multi_relation", "integral_sum_limit"],
    32: ["long_multi_relation", "integral_sum_limit"],
    33: ["greek_font_variant", "superscript_subscript"],
    34: ["long_multi_relation", "fractions_roots"],
    35: ["long_multi_relation", "integral_sum_limit"],
    36: ["integral_sum_limit"],
    37: ["long_multi_relation", "greek_font_variant"],
    38: ["long_multi_relation", "greek_font_variant", "integral_sum_limit"],
    39: ["long_multi_relation", "integral_sum_limit"],
    40: ["simple_inline", "tilted_perspective_noise"],
    41: ["simple_inline", "blurred_low_resolution"],
    42: ["fractions_roots", "tilted_perspective_noise"],
    43: ["superscript_subscript", "tilted_perspective_noise"],
    44: ["fractions_roots", "tilted_perspective_noise"],
    45: ["fractions_roots", "tilted_perspective_noise"],
    46: ["integral_sum_limit", "tilted_perspective_noise"],
    47: ["greek_font_variant", "blurred_low_resolution"],
    48: ["superscript_subscript", "blurred_low_resolution"],
    49: ["greek_font_variant", "blurred_low_resolution"],
}

MATRIX_FIXTURES = {
    20: (
        r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        '$ mat(delim: "(", a, b; c, d) $',
        [],
    ),
    21: (
        r"\begin{bmatrix}1&0\\0&1\end{bmatrix}",
        '$ mat(delim: "[", 1, 0; 0, 1) $',
        [],
    ),
    22: (
        r"A=\begin{pmatrix}\alpha&\beta\\\gamma&\delta\end{pmatrix}",
        '$ A = mat(delim: "(", alpha, beta; gamma, delta) $',
        ["greek_font_variant"],
    ),
    23: (
        r"\det\begin{pmatrix}x&1\\1&x\end{pmatrix}=x^2-1",
        '$ det mat(delim: "(", x, 1; 1, x) = x^2 - 1 $',
        ["superscript_subscript"],
    ),
    24: (
        r"\begin{array}{cc}\text{速度}&v\\\text{时间}&t\end{array}",
        '$ mat("速度", v; "时间", t) $',
        ["mixed_chinese_english"],
    ),
    25: (
        r"f(x)=\begin{cases}\text{正数},&x>0\\\text{非正数},&x\leq0\end{cases}",
        '$ f(x) := cases("正数" & x > 0, "非正数" & x <= 0) $',
        ["mixed_chinese_english"],
    ),
}

LOW_RESOLUTION_FIXTURES = {
    41: (r"A=\pi r^2", "$ A = pi r^2 $"),
    47: (r"z=\frac{x-\mu}{\sigma}", "$ z = (x - mu) / sigma $"),
    48: (r"F=G\frac{m_1m_2}{r^2}", "$ F = G (m_1 m_2) / r^2 $"),
    49: (r"E=\hbar\omega", "$ E = ℏ omega $"),
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def normalize(latex: str) -> str:
    value = latex.replace("\r\n", "\n").replace("\r", "\n").strip()
    if len(value) >= 4 and value.startswith("$$") and value.endswith("$$"):
        return value[2:-2].strip()
    if len(value) >= 2 and value.startswith("$") and value.endswith("$"):
        return value[1:-1].strip()
    if len(value) >= 4 and (
        (value.startswith(r"\(") and value.endswith(r"\)"))
        or (value.startswith(r"\[") and value.endswith(r"\]"))
    ):
        return value[2:-2].strip()
    return value


def render_typst(source: str, destination: Path, ppi: int = 160) -> None:
    document = "\n".join(
        (
            "#set page(width: auto, height: auto, margin: 10pt, fill: white)",
            "#set text(size: 18pt, font: (\"New Computer Modern\", \"Microsoft YaHei\"))",
            "#set math.equation(numbering: none)",
            source,
        )
    )
    with tempfile.TemporaryDirectory(prefix="latexsnipper-formula-v1-") as directory:
        source_path = Path(directory) / "fixture.typ"
        source_path.write_text(document, encoding="utf-8")
        subprocess.run(
            [
                "typst",
                "compile",
                "--format",
                "png",
                "--ppi",
                str(ppi),
                str(source_path),
                str(destination),
            ],
            check=True,
        )


def difficulty_for(index: int) -> str:
    if index < 8:
        return "easy"
    if index < 32:
        return "medium"
    return "hard"


def quality_for(category: list[str]) -> str:
    if "blurred_low_resolution" in category:
        return "low_resolution"
    if "tilted_perspective_noise" in category:
        return "noisy_tilted"
    return "clean"


def main() -> None:
    source_entries = json.loads((SOURCE / "ground_truth.json").read_text(encoding="utf-8"))
    if len(source_entries) != 50 or len(CATEGORIES_BY_INDEX) != 50:
        raise RuntimeError("formula benchmark v1 requires exactly 50 source samples")

    IMAGES.mkdir(parents=True, exist_ok=True)
    samples = []
    for index, source_entry in enumerate(source_entries):
        filename = source_entry["id"]
        destination = IMAGES / filename
        latex = source_entry["ground_truth"]
        extra_categories: list[str] = []
        renderer = "matplotlib_mathtext"
        if index in MATRIX_FIXTURES:
            latex, typst_source, extra_categories = MATRIX_FIXTURES[index]
            render_typst(typst_source, destination)
            renderer = "typst"
        elif index in LOW_RESOLUTION_FIXTURES:
            latex, typst_source = LOW_RESOLUTION_FIXTURES[index]
            render_typst(typst_source, destination, ppi=60)
            renderer = "typst-low-resolution"
        else:
            shutil.copy2(SOURCE / filename, destination)

        categories = list(CATEGORIES_BY_INDEX[index])
        for category in extra_categories:
            if category not in categories:
                categories.append(category)
        if 40 <= index:
            categories.append("synthetic_handwriting_noise")

        samples.append(
            {
                "id": filename.removesuffix(".png"),
                "image": f"images/{filename}",
                "imageSha256": sha256(destination),
                "groundTruthLatex": latex,
                "normalizedGroundTruth": normalize(latex),
                "category": categories,
                "source": "synthetic",
                "license": "CC0-1.0",
                "difficulty": difficulty_for(index),
                "imageQuality": quality_for(categories),
                "notes": (
                    f"Deterministic synthetic fixture rendered with {renderer}; "
                    "contains no user data."
                ),
            }
        )

    manifest = {
        "schemaVersion": 1,
        "datasetId": "latexsnipper-formula-recognition",
        "datasetVersion": "1.0.0",
        "normalizationVersion": NORMALIZATION_VERSION,
        "seed": 42,
        "samples": samples,
    }
    (OUTPUT / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"prepared {len(samples)} formula fixtures at {OUTPUT}")


if __name__ == "__main__":
    main()
