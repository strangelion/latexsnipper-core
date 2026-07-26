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

`thresholds.json` defines comparison policy. It deliberately requires a real
baseline rather than allowing a synthetic or oracle run to establish quality.
No accuracy claim is made until a checked-in result was produced by the real
model/runtime path.
