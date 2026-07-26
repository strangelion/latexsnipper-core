# Formula recognition benchmark v1

This directory is the reproducible quality contract for formula recognition.
It contains 50 deterministic synthetic fixtures, conservative versioned
normalization rules, configurable regression thresholds, and machine-readable
result locations.

The dataset contains no user data. Every fixture is generated in-repository
under `CC0-1.0`; image hashes are frozen in `manifest.json`. The six
matrix/piecewise fixtures are rendered with Typst. The remaining fixtures
retain the existing deterministic PP-FormulaNet benchmark images.

Run an implementation-produced prediction bundle with:

```text
cargo run --locked -p latexsnipper-benchmark -- \
  --formula-manifest benchmarks/formula-recognition/v1/manifest.json \
  --normalization benchmarks/formula-recognition/v1/normalization.json \
  --predictions <predictions.json> \
  --output benchmarks/formula-recognition/v1/results/<commit>/<runtime>.json \
  --csv-output benchmarks/formula-recognition/v1/results/<commit>/<runtime>.csv
```

Prediction bundles must include the Core commit, model identity and checksum,
runtime/provider versions, OS/CPU/GPU, thread count, warmup count, seed,
timestamp, model load time, and peak RSS when available. Missing samples,
unknown samples, stale normalized ground truth, duplicate IDs, and invalid
latencies fail closed.

`predictions-trocr-deit-ort-cpu.json` and its JSON/CSV reports were produced by
the real TrOCR encoder/decoder through ONNX Runtime CPU; predictions are not
copied from ground truth. This establishes an honest model/runtime baseline on
the synthetic v1 distribution. Its normalized exact match is 0 and its CER is
about 1.302, so it is evidence of the current quality gap, not a quality claim.

`thresholds.json` defines comparison policy. Real screenshot, scan, mobile, and
hard-negative claims remain blocked until licensed inputs pass the intake
contract in `docs/quality/real-dataset-intake-status.md`.
