# OCR evaluation and evidence

LaTeXSnipper separates evaluator correctness, model compatibility, and model accuracy. A model
must not be described as accuracy-validated merely because it loads, executes, or passes the
non-production evaluator contract.

## Evidence levels

| Level | Purpose | Permitted claim |
|---|---|---|
| Pull request contract | Validate corpus integrity, metric code, serialization, and gate behavior | The evaluation contract is executable and deterministic |
| Scheduled model run | Execute checksum-pinned models against licensed scheduled corpora | The named model/runtime/provider combination produced the attached metrics |
| Release evidence | Execute the frozen release model set and release corpora | The exact model and corpus digests passed the release thresholds |

The checked-in `evaluation-contract-oracle-v1` is explicitly marked `productionModel: false`.
It returns references by sample ID to exercise the evaluator. Its perfect scores are not OCR
accuracy and must never be cited as model validation.

## Corpus contract

`evaluation/corpora/index.json` references one manifest for each required task:

- printed formula;
- handwritten formula;
- Latin text;
- Simplified Chinese text;
- mixed CJK and Latin text;
- mixed formula and text document blocks;
- document layout;
- table structure;
- orientation.

Every manifest records source, revision, SPDX license, attribution, redistribution policy,
annotation format, preprocessing assumptions, asset SHA-256, and a deterministic content
SHA-256 over sample identity, asset bytes, and canonical annotation JSON. Paths are relative,
forward-slash-only, and traversal is rejected. Tracked assets may not use a prohibited
redistribution policy.

The repository-authored fixtures are intentionally small regression samples. They are useful
for pull request determinism but are not statistically representative production corpora.
Production claims require broader, independently licensed scheduled and release inputs.

## Metrics

| Task | Metrics |
|---|---|
| Latin, Simplified Chinese, mixed CJK/Latin | Character error rate (CER), word error rate (WER) |
| Printed and handwritten formula | Normalized exact match, token edit structural similarity |
| Layout | Macro F1 after class-aware IoU 0.5 matching |
| Table | Logical cell/span F1, ordered cell-tree token similarity |
| Orientation | Exact 0/90/180/270 accuracy |
| Mixed formula/text | Block kind-and-content F1, reading-order LCS similarity |

Score metrics are finite and bounded to `[0, 1]`. CER and WER are finite non-negative ratios and
can exceed `1.0` when insertions outnumber reference units. Threshold files must name exactly the
metric contract for every selected task. Missing predictions, duplicate IDs, unknown samples,
digest mismatches, schema mismatches, and failed thresholds produce a non-zero exit code.

## Local contract run

```bash
cargo run -p latexsnipper-evaluation --bin ocr-eval -- \
  validate --index evaluation/corpora/index.json

cargo run -p latexsnipper-evaluation --bin ocr-eval -- \
  contract-predictions \
  --index evaluation/corpora/index.json \
  --tier pull-request \
  --model-spec evaluation/contract-oracle-v1.json \
  --output target/evaluation/contract-predictions.json

cargo run -p latexsnipper-evaluation --bin ocr-eval -- \
  evaluate \
  --index evaluation/corpora/index.json \
  --predictions target/evaluation/contract-predictions.json \
  --gates evaluation/gates/pull-request.json \
  --tier pull-request \
  --source-commit "$(git rev-parse HEAD)" \
  --generated-at-utc "$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
  --output target/evaluation/pull-request-evidence.json
```

Prediction bundles bind every run to a corpus digest, model digest, runtime, provider, platform,
preprocessing version, and postprocessing version. Evidence reports repeat those identities and
record every metric, threshold, rationale, and pass result.

## Model manifest binding

A model manifest v3 profile with `evidence.status = validated` must provide:

- corpus and benchmark IDs;
- a safe relative evidence report path;
- the report SHA-256;
- supported modes and runtime compatibility;
- memory estimate;
- preprocessing, postprocessing, and output schemas.

This prevents a descriptive benchmark label from being treated as executable evidence. Release
automation must verify the report bytes against the manifest digest before changing a profile to
`validated`.
