# Runtime and recognition quality report — 2026-07-26

## A. Current baseline and existing capabilities

The work started from clean `main` at
`ba29c27f800ba5dbd53954923be236b3cedb24c2`. The workspace initially had 32
members; it now has 33 after adding the benchmark crate. There are no Git
submodules.

The public dependency boundary remains:

```text
api-types -> ast, image
runtime -> foundation, model, tensor, ast
inference -> runtime, model, image, ast
pipeline -> inference, runtime, ast
engine -> pipeline, runtime, model, api-types, conversion/export
CLI / FFI / WASM -> public engine/API contracts
```

`PreparedModel`, `ModelExecutionContext`, role-aware artifact selection,
context-created text/formula/table executors, and PipelineContext diagnostics
were already present and were not reimplemented.

`cargo tree -d` still reports dependency-version duplication including
`base64`, `digest`, `getrandom`, `imagesize`, `itertools`, `ndarray`, `nom`,
`rand`, `sha2`, `ureq`, `webpki-roots`, and `weezl`. This task did not attempt a
high-risk dependency convergence.

## B. Root causes and design decisions

- The checked-in PP-FormulaNet-S package is a Paddle full program. Its internal
  while loop owns KV state. The experimental `decoder_step.onnx` and
  reconstructed full-sequence ONNX graphs were intentionally removed earlier
  because their semantics drifted.
- No `decoder_step.onnx`, Paddle-to-ONNX loop body, or captured 29-variable
  state dump exists in the repository or supplied files. Therefore a concrete
  29-state mapping and Add.34 producer diagnosis cannot be established from
  evidence.
- Provider selection is modeled as descriptors, probes, options, and traces;
  it does not bind an unstable provider ABI. Native libraries remain owned by
  trusted runtime installations, never model packages.
- ONNX and ORT-format are artifact alternatives owned by the runtime. mmap is
  rejected until the binding can prove mapping lifetime and Windows locking.
- Recognition correction is rule-gated. Raw, normalized, corrected, diff,
  validation, and transformation hashes remain separate from render text.
- Readiness is a stable API DTO. Office and other clients do not inspect model
  directories or Rust session types.

## C. Modified crates and files

- `latexsnipper-benchmark`: versioned dataset contract, metrics, JSON/CSV runner.
- `latexsnipper-inference`: decoder schema validation, evidence-aware
  postprocessing, runtime provenance, formula repair fixes.
- `latexsnipper-runtime`: provider descriptors/trace, artifact hardening,
  ONNX/ORT selection, fail-closed mmap option.
- `latexsnipper-model`: ORT artifacts recognized by the secured importer.
- `latexsnipper-ast`: provenance and postprocess DTOs attached to formulas.
- `latexsnipper-pipeline`: propagation of recognition evidence.
- `latexsnipper-api-types` and `latexsnipper-engine`: readiness DTO and snapshot.
- `.github/workflows/scheduled.yml`: scheduled dataset/runner validation and
  checksum-verified manual evaluation of real prediction bundles.
- Decoder, runtime, readiness, provenance, and benchmark documentation/tools.

## D. Decoder 29-state mapping

`DecoderStateSchema` is implemented with stable roles, dtype/rank, axis
semantics, growth axis, layer index, attention kind, and transition validation.
It emits the requested cache error families and tests monotonic self-cache
growth and static cross-cache behavior.

A 29-entry fixture is deliberately not checked in: the required graph and
runtime state names/shapes are absent. Creating 29 guessed positional entries
would violate the fail-closed requirement. The exact evidence capture procedure
is in `docs/decoder/pp-formulanet-kv-cache-contract.md`.

## E. Add.34 root-cause evidence

`trace_onnx_node.py`, `dump_step_state.py`, and `compare_incremental.py` emit
machine-readable graph/state/difference evidence and stop at the first
divergence. They cover T=1/2/3/6/9/15/30, logits, top-k, and named state arrays.

No Add.34 root cause is claimed. The graph containing Add.34 is unavailable, so
its two producers and step-0/step-1 runtime shapes cannot be inspected.

## F. Formula benchmark

The v1 dataset has exactly 50 hash-verified CC0 synthetic images and meets every
required category minimum. Normalization and regression thresholds are
versioned. The runner computes exact/normalized exact match, CER, TER, parse and
structure validity, delimiter/repetition/EOS/truncation, top-1/top-5, cold/warm
and p50/p95, grouped by category, quality, and sequence length. Run metadata
requires commit, model/hash, runtime/provider, machine, threads, warmup, seed,
load time, and peak RSS.

No production accuracy number is reported. A real prediction bundle has not
been generated for all 50 images, and an oracle bundle is explicitly forbidden
from establishing the baseline.

## G. Runtime/provider trace

CPU, DirectML, CUDA, TensorRT, and CoreML descriptors are implemented. The
resolver returns stable accept/reject codes, requested and selected providers,
fallback status, and reasons. CPU fallback must be explicitly allowed.
Security tests reject model-packaged DLL/SO/dylib and scripts.

`.ort` alternatives are opt-in through `preferOrtFormat` and
`artifact-format:ort`; `.onnx` remains default. `memoryMapModel: true` returns
`ONNX_MODEL_MMAP_UNSUPPORTED`.

The Windows I/O-only baseline on the 87,496,990-byte TrOCR encoder measured:

```text
buffered read median 22.3220 ms, p95 23.0452 ms
mmap open median      0.0319 ms, p95 0.0545 ms
mmap page touch       19.7864 ms, p95 24.4050 ms
process peak working set during run 111,685,632 bytes
replace while mapped failed with Windows error 5
replace after close succeeded
```

These figures do not include ORT session creation and are not presented as an
ORT performance improvement.

## H. Table results

The existing table path already creates formula detection/recognition executors
through `PipelineContext::create_model_executor`, reuses
`TextRecognitionService` from context, continues after per-cell failures, and
preserves cell geometry, spans, text/formula confidence, formula provenance,
and diagnostics. Workspace `table_e2e` and table round-trip tests pass.

No new TEDS/structure-accuracy baseline or durable raw-crop asset reference was
added. Ambiguous-cell dual-candidate scoring remains a follow-up; the current
path selects formula when filtered formula detections exist and otherwise uses
text recognition.

## I. Provenance and postprocess

Formula outputs carry model ID/version, runtime, provider, source region slot,
raw/normalized confidence, and hashed transformation evidence. The AST keeps
this evidence separate from `FormulaSource`.

The rule processor triggers on low confidence, empty output, group/environment
or left/right imbalance, duplicate runs, dangling commands, EOS/truncation, and
matrix shape. It performs only evidence-recorded conservative fixes and emits
`POSTPROCESS_REVIEW_REQUIRED` when validity cannot be established. Valid
high-confidence formulas skip transformation. Legacy repair no longer corrupts
valid `\frac` or matrix `&`.

## J. API and contract changes

`EngineReadiness` schema v1 exposes mode/task, runtime, model, and diagnostic
readiness with stable `CoreErrorCode` values. `SnipperEngine::readiness()` is
read-only and does not expose `Arc`, factories, or sessions.

Formula AST gained optional, serde-default evidence fields. New evidence fields
are boxed to keep `Inline` compact while preserving JSON shape. Every intentional
public-tree change is recorded in `CHANGELOG.md`, and
`contracts/v3-contract-freeze.json` was updated and verified.

## K. Test commands and results

Passed on Windows 11 x86_64:

```text
python scripts/verify_v3_contract_freeze.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --no-fail-fast
cargo test -p latexsnipper-runtime -p latexsnipper-model
cargo test -p latexsnipper-ast -p latexsnipper-inference
           -p latexsnipper-pipeline -p latexsnipper-conversion
           -p latexsnipper-mock
python -m py_compile tools/runtime/benchmark_model_loading.py
```

The full workspace test completed with zero failures. Two documented
large-model WASM performance tests remained ignored by their pre-existing
opt-in policy.

## L. Incomplete and blocked items

- Decoder incremental equivalence, the 29-state fixture, and Add.34 diagnosis
  are blocked on the missing `decoder_step.onnx`/loop-body artifact and captured
  runtime state.
- A 50-image real-model formula accuracy baseline is not yet checked in.
- True model mmap is not enabled; the binding/lifecycle and Windows replacement
  contract are not proven.
- DirectML/CUDA benchmark results require runners with those installed provider
  stacks. The workflow accepts only checksum-verified real prediction bundles
  and does not invent those environments.
- Table TEDS, raw-crop persistence, and ambiguous-cell candidate comparison
  remain outside this completed change set.

## M. Commit SHAs

```text
592e155 test(benchmark): add versioned formula dataset schema
3d14783 feat(benchmark): add reproducible formula metrics runner
9cd5e7a docs(decoder): freeze decoder state schema
462b935 feat(runtime): add provider descriptors and trace
aa6fd40 feat(recognition): preserve correction provenance
d1b8b28 feat(readiness): expose mode-specific readiness
20a307b feat(runtime): add optional ORT artifact selection
2df9c0f ci(bench): validate scheduled formula benchmark contract
baeac14 fix(quality): keep public result types compact
```
