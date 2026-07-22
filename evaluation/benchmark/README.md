# PP-FormulaNet Paddle validation

This directory contains the fixed PP-FormulaNet-S validation corpus and the
reports used to gate the native Paddle Inference runtime.

- `p08_formulas/` contains 50 deterministic formula images and
  `ground_truth.json`. Regenerate the corpus with
  `python scripts/generate_ppfn_benchmark.py`.
- `paddle_parity.json` records the required 20-case runtime parity gate.
- `paddle_parity_50.json` records the complete 50-case regression run.

Both parity reports compare the same preprocessed `f32` tensors through the
official Paddle Python predictor and the Rust `runtime-paddle` implementation.
A case passes only when token IDs, EOS position, and tokenizer-decoded LaTeX
all match exactly. The reports validate runtime execution parity; they do not
claim independent OCR-quality validation or pixel-exact parity between Pillow
and the Rust image preprocessing implementation.

Regenerate a report in a development environment that has Paddle Python and
the tokenizer package installed:

```text
python scripts/validate_ppfn_paddle_parity.py \
  --runtime-home <packaged-paddle-runtime> \
  --model-home models/formula-rec/pp-formulanet-s \
  --count 20 \
  --output evaluation/benchmark/paddle_parity.json
```

Use `--count 50` and `paddle_parity_50.json` for the full regression gate.
The shipped application does not require Python.
