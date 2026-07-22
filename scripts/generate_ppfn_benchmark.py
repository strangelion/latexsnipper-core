#!/usr/bin/env python3
"""
Generate 50 synthetic formula benchmark images with known ground truth.

Categories:
  10  simple printed formulas
  10  fractions / roots / integrals / sums
  10  matrices / cases / aligned / multi-line
  10  long complex formulas
  10  handwritten-style (simulated with noise + rotation)

Each image is rendered via matplotlib mathtext and saved as grayscale PNG.
Ground truth is saved to ground_truth.json alongside the images.
"""
import os, json, random
import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

OUT_DIR = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..",
    "evaluation", "benchmark", "p08_formulas"
)
os.makedirs(OUT_DIR, exist_ok=True)

random.seed(42)
np.random.seed(42)

# ═══════════════════════════════════════════════════════════
# Formula definitions
# ═══════════════════════════════════════════════════════════

SIMPLE = [
    r"$E=mc^2$",
    r"$a^2+b^2=c^2$",
    r"$x=\frac{-b\pm\sqrt{b^2-4ac}}{2a}$",
    r"$f(x)=x^2+2x+1$",
    r"$\sin^2\theta+\cos^2\theta=1$",
    r"$e^{i\pi}+1=0$",
    r"$\log_a(xy)=\log_a x+\log_a y$",
    r"$\int_0^1 x^2\,dx=\frac{1}{3}$",
    r"$\sum_{n=1}^{\infty}\frac{1}{n^2}=\frac{\pi^2}{6}$",
    r"$\lim_{x\to 0}\frac{\sin x}{x}=1$",
]

FRACTIONS_ROOTS_INTEGRALS = [
    r"$\frac{a}{b}+\frac{c}{d}=\frac{ad+bc}{bd}$",
    r"$\sqrt{\frac{x^2+y^2}{2}}\geq\frac{x+y}{2}$",
    r"$\int_a^b f(x)\,dx=F(b)-F(a)$",
    r"$\iint_D f(x,y)\,dx\,dy$",
    r"$\oint_C \mathbf{F}\cdot d\mathbf{r}$",
    r"$\sum_{k=0}^n \binom{n}{k}=2^n$",
    r"$\prod_{i=1}^n i=n!$",
    r"$\sqrt[3]{x^3+y^3}\neq x+y$",
    r"$\frac{\partial^2 u}{\partial x^2}+\frac{\partial^2 u}{\partial y^2}=0$",
    r"$\int_{-\infty}^{\infty}e^{-x^2}dx=\sqrt{\pi}$",
]

MATRICES_CASES = [
    r"$\binom{n}{k}=\frac{n!}{k!(n-k)!}$",
    r"$f(x)=\sum_{n=0}^{\infty}a_n x^n$",
    r"$\max_{0\leq x\leq 1}f(x)$",
    r"$\limsup_{n\to\infty}\sqrt[n]{|a_n|}$",
    r"$\int_0^1\!\!\int_0^x f(x,y)\,dy\,dx$",
    r"$f:A\to B,\ g:B\to C\ \Rightarrow\ g\circ f:A\to C$",
    r"$\bigcap_{i=1}^{\infty}A_i=\emptyset$",
    r"$\bigoplus_{k=0}^{n-1}\mathbb{Z}_k$",
    r"$\left.\frac{df}{dx}\right|_{x=0}=0$",
    r"$\sum_{i=1}^{26}a_i=a_1+a_2+\cdots+a_{26}$",
]

LONG_COMPLEX = [
    r"$\frac{d}{dx}\left(\frac{f(x)}{g(x)}\right)=\frac{f'(x)g(x)-f(x)g'(x)}{[g(x)]^2}$",
    r"$\iiint_V \nabla\cdot\mathbf{F}\,dV=\oiint_S \mathbf{F}\cdot d\mathbf{S}$",
    r"$\sum_{n=0}^{\infty}\frac{f^{(n)}(a)}{n!}(x-a)^n$",
    r"$\det(A)=\varepsilon_{ijk}a_{1i}a_{2j}a_{3k}$",
    r"$\frac{\partial(f,g)}{\partial(x,y)}=f_x g_y-f_y g_x$",
    r"$\int_0^{2\pi}\sqrt{\left(\frac{dx}{dt}\right)^2+\left(\frac{dy}{dt}\right)^2}\,dt$",
    r"$\lim_{n\to\infty}\left(1+\frac{1}{n}\right)^n=e$",
    r"$\nabla\times\mathbf{E}=-\frac{\partial\mathbf{B}}{\partial t},\ \nabla\times\mathbf{B}=\mu_0\mathbf{J}+\mu_0\epsilon_0\frac{\partial\mathbf{E}}{\partial t}$",
    r"$\Phi(x)=\frac{1}{\sqrt{2\pi}}\int_{-\infty}^x e^{-t^2/2}\,dt$",
    r"$\Gamma(z)=\int_0^{\infty}t^{z-1}e^{-t}\,dt\quad(\Re(z)>0)$",
]

HANDWRITTEN_STYLE = [
    r"$y=mx+b$",
    r"$A=\pi r^2$",
    r"$V=\frac{4}{3}\pi r^3$",
    r"$d=\sqrt{(x_2-x_1)^2+(y_2-y_1)^2}$",
    r"$S=\frac{n}{2}(a_1+a_n)$",
    r"$P(A|B)=\frac{P(B|A)P(A)}{P(B)}$",
    r"$\bar{x}=\frac{1}{n}\sum_{i=1}^n x_i$",
    r"$z=\frac{x-\mu}{\sigma}$",
    r"$F=G\frac{m_1 m_2}{r^2}$",
    r"$E=\hbar\omega$",
]

ALL_FORMULAS = SIMPLE + FRACTIONS_ROOTS_INTEGRALS + MATRICES_CASES + LONG_COMPLEX + HANDWRITTEN_STYLE

# ═══════════════════════════════════════════════════════════
# Rendering
# ═══════════════════════════════════════════════════════════

def render_matplotlib(formula, out_path, dpi=150, font_size=18):
    """Render a LaTeX formula using matplotlib mathtext."""
    fig, ax = plt.subplots(figsize=(4, 1.2), dpi=dpi)
    ax.axis("off")
    ax.text(0.5, 0.5, formula, transform=ax.transAxes,
            fontsize=font_size, ha="center", va="center",
            usetex=False)
    fig.savefig(out_path, bbox_inches="tight", pad_inches=0.1,
                dpi=dpi, facecolor="white", edgecolor="none")
    plt.close(fig)

    # Convert to grayscale
    img = Image.open(out_path).convert("L")
    # Resize to a standard height while maintaining aspect ratio
    w, h = img.size
    target_h = 96
    ratio = target_h / h
    new_w = max(48, int(w * ratio))
    img = img.resize((new_w, target_h), Image.LANCZOS)
    # Pad to at least 128px wide
    if new_w < 128:
        padded = Image.new("L", (128, target_h), 255)
        padded.paste(img, ((128 - new_w) // 2, 0))
        img = padded
    img.save(out_path)
    return img


def add_handwriting_noise(img, intensity=0.03):
    """Add slight noise and rotation to simulate handwriting."""
    arr = np.array(img, dtype=np.float32)
    noise = np.random.randn(*arr.shape) * intensity * 255
    arr = np.clip(arr + noise, 0, 255).astype(np.uint8)
    img = Image.fromarray(arr)

    # Slight rotation (±3 degrees)
    angle = random.uniform(-3, 3)
    img = img.rotate(angle, expand=True, fillcolor=255)

    # Slight blur
    img = img.filter(ImageFilter.GaussianBlur(radius=0.5))

    return img


# ═══════════════════════════════════════════════════════════
# Generate
# ═══════════════════════════════════════════════════════════

def main():
    manifest = []

    for i, formula in enumerate(ALL_FORMULAS):
        category_idx = i // 10
        cat_names = ["simple", "fractions_roots", "matrices_cases", "long_complex", "handwritten"]
        cat = cat_names[category_idx] if category_idx < 5 else "extra"

        fname = f"{cat}_{i:03d}.png"
        out_path = os.path.join(OUT_DIR, fname)

        try:
            img = render_matplotlib(formula, out_path)

            # Add noise for handwritten category
            if cat == "handwritten":
                img = add_handwriting_noise(img)
                img.save(out_path)

            # Extract ground truth (strip $ signs)
            gt = formula.strip("$")
            manifest.append({
                "id": fname,
                "category": cat,
                "ground_truth": gt,
                "source": "matplotlib_mathtext",
            })
            print(f"  [{i+1:2d}/50] {fname}: {gt[:50]}...")

        except Exception as e:
            print(f"  [{i+1:2d}/50] SKIP {fname}: {e}")

    # Save manifest
    manifest_path = os.path.join(OUT_DIR, "ground_truth.json")
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)

    print(f"\nGenerated {len(manifest)} images to {OUT_DIR}")
    print(f"Manifest: {manifest_path}")


if __name__ == "__main__":
    main()
