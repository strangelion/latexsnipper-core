# Model validation for Core 3

Model compatibility, readiness, and accuracy are separate claims. A model is
not production-validated merely because ONNX Runtime or Tract can load it.

## Required identity

Evidence must bind the model family, upstream source and immutable revision,
license, artifact names, byte lengths, SHA-256 digests, runtime/backend, input
and output names, shapes/dtypes, preprocessing, postprocessing, tokenizer or key
files, corpus index digest, metric schema, thresholds, source commit, toolchain,
and generation time.

Do not invent measurements when a licensed corpus or model artifact is absent.
Mark the profile `compatible`, `experimental`, or `requires_manual_action`
instead of `validated`.

## Evidence tiers

1. Pull-request contract fixtures validate schema, deterministic metrics, and
   gate behavior. They are not production accuracy evidence.
2. Scheduled compatibility executes checksum-pinned production-derived models
   in native and WASM runtimes and records timing/memory metadata.
3. Release evidence evaluates the frozen release models against licensed,
   checksum-pinned corpora using explicit accuracy gates.

Shared-runner timing is recorded as evidence but is not a fragile pass/fail
performance threshold. Accuracy thresholds must be justified per task and
corpus.

## Commands

```bash
cargo run -p latexsnipper-evaluation --bin ocr-eval -- \
  validate --index evaluation/corpora/index.json

cargo run -p latexsnipper-evaluation --bin ocr-eval -- \
  evaluate --index evaluation/corpora/index.json \
  --predictions <predictions.json> \
  --gates <gates.json> --tier <tier> \
  --source-commit <commit> --generated-at-utc <timestamp> \
  --output <evidence.json>
```

The release reviewer must inspect corpus licensing and provenance, not only the
numeric summary. See [../ocr-evaluation.md](../ocr-evaluation.md).
