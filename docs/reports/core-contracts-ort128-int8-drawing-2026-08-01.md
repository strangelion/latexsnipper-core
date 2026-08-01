# Core contracts, ORT 1.28, INT8 and Drawing evidence — 2026-08-01

## A. Contract inventory and execution

Five JSON-compatible YAML contract files define runtime, provider, quality, conversion and security rules. `scripts/run_agent_contracts.py` independently executes each rule and emits JSON, JUnit XML and a SHA-256 list. CI has distinct `contract-runtime`, `contract-provider`, `contract-ort128`, `contract-int8`, `contract-conversion`, `contract-office-output`, `contract-failure-corpus`, `contract-model-package-security` and `contract-drawing` jobs. A blocking rule without executable evidence is failed/notRun, never passed.

## B. Effective provider propagation

`SessionMetadata` now separates requested providers from the effective provider and records ordered attempts and diagnostics. ONNX and legacy adapters populate this after session creation. Runtime observations, PP-FormulaNet, TrOCR, formula backend provenance, readiness and quality lookup consume the observed provider. Unknown effective provider fails closed.

## C. Ephemeral and persistent validation

Process-local smoke evidence is keyed by process, runtime instance, session generation, provider and smoke model. It survives a readiness refresh but is cleared by session/runtime/model lifecycle resets. Persistent trust requires strong runtime binary, provider library, driver/device and smoke-model identity. Stale reports are platform-preferred, time-ordered and deterministically downgraded to probe evidence.

## D. ORT 1.28 matrix

The runtime uses `ort = 2.0.0-rc.13`, the binding release for ONNX Runtime 1.28, with API 27. CPU compilation and tests are blocking across the repository's Windows/Linux/macOS CI. DirectML, CUDA, TensorRT and CoreML execution cases remain `notRun` on unpinned hosted hardware; no CPU fallback is reported as their success. The versioned matrix covers ONNX/ORT, no/single/multiple external data, FP32/FP16/INT8 and explicit/fallback-disabled policy.

## E. External-data security

External data accepts only contained relative files with unique names, exact SHA-256 and exact sizes. Missing data, hash mismatch, path/symlink escape, excessive size and file-count limits have stable fail-closed error codes. Generation publication validates all files first, fsyncs a same-directory staging manifest and performs write-through atomic replacement on Windows.

## F. Model mapping and fallback

Model artifacts declare role, format and external-data mapping. Ambiguous graph roles and dynamic libraries are rejected. Existing session owners retain old mappings while a cache switch publishes a new generation; occupied mapped files can be listed. Requested fallback attempts remain diagnostic history, while effective provider is the sole runtime identity.

## G. INT8 tensor, AST and Office determinism

The deterministic comparator fixes model, tokenizer, dataset, normalization and AST-canonicalizer identities. It compares tensors, top-k/final tokens, raw/normalized/corrected LaTeX, AST, conversion, OMML, MathML, Typst, Office payload and Word read-back hashes against `quality/int8/thresholds.v1.json`. Unsupported devices and provider fallback cannot validate. No redistributable fixed INT8 formula model is approved in this repository, so hardware quality cases are blocked/notRun rather than claimed as passed.

## H. Failure-corpus intake and promotion

Candidates use a versioned schema and move through inbox, minimized, approved and regressions. Deduplication includes input/AST hashes, failure signature, provider, model and runtime. Promotion requires sanitization, redistribution permission/license, reproducibility, an expected result and an error classification. Raw user documents are prohibited.

## I. Cross-parser divergence

Structural comparison returns a deterministic first JSON/AST divergence path. Stable repository round-trips are blocking; unknown external inputs remain candidates until reviewed.

## J. Office output contract

`contracts/fixtures/office-output-v1` binds Document AST JSON, LaTeX, OMML, insertion payload and expected read-back semantics with per-file SHA-256. Office host hashes are deliberately null/notRun in Core and are populated by Office CI on a supported host.

## K. Security tests

Tests cover external-data traversal, symlink escape, mismatch, duplicate names and resource limits; model dynamic-library exclusion; Drawing shell/network denial; PlantUML and Asymptote restrictions; Graphviz plugin probing; SVG active-content and complexity rejection; and failure-corpus privacy gates.

## L. Artifact SHA

Every contract run writes hashes for its JSON and JUnit artifacts. Office bundle files and INT8 comparison bundles carry SHA-256. Provider persistence refuses descriptive/weak fingerprints.

## M. Blocked items

Real DirectML/CUDA/TensorRT/CoreML INT8 validation, real-model quality deltas, GPU driver/device fingerprints and Office read-back require approved models and pinned hardware/Office runners. They remain blocked or notRun. PSTricks is disabled; PlantUML, Asymptote, MetaPost and chemfig remain experimental.

## N. Commit SHA

CI evidence records `GITHUB_SHA`; local evidence records `git rev-parse HEAD`. The final delivery message identifies the pushed Core and Office commits after both workflows pass.

## O. BenchmarkMeasured/Validated semantics

Three timed samples set `benchmarkMeasured` and `BenchmarkMeasured` only. `benchmarkValidated` remains false unless a separate versioned benchmark policy, fixed model/dataset, warmup/sample floor, machine/driver identity, performance and quality thresholds, and artifact hashes are supplied.

## P. Unknown provider fail-closed result

No session/effective provider yields unknown or missing quality, `qualityReady=false` and `productionRecommended=false`; a configured first provider or CPU baseline cannot bypass the check.

## Q. Ephemeral invalidation matrix

Runtime/session clear, runtime rescan/reload and resolver changes clear process-local evidence and advance generation. Ordinary readiness lookup does not erase a valid current-session result.

## R. Stale-report determinism

Exact strong keys win. Otherwise stale candidates first prefer matching OS/architecture, then newest validation time, then stable runtime-instance ordering; all are downgraded and non-reusable.

## S. Baseline error classification

Missing deployment directory, invalid index, hash mismatch, missing model baseline, provider mismatch and dataset mismatch remain distinct codes and diagnostics.

## T. Fallback provenance consistency

Session metadata, observation, recognition provenance, readiness, baseline lookup and downstream evidence use the effective provider. Requested order and failure reasons remain in fallback history only.

## U. External-data atomic update

Invalid staged files leave the live manifest unchanged. A valid manifest is staged and synced before atomic activation. Active old sessions own their old mapping until released; new sessions obtain the new cache entry, and occupied files are observable for upgrade diagnostics.

## Drawing supplement A–P

- A. Taxonomy: source languages, TikZ profiles, interchange, general outputs and Office-only outputs are distinct enums.
- B. Domain: `DrawingDocument` preserves source, raw nodes, structured objects, resources, datasets and provenance.
- C. Adapters: editing, parse, emit, round-trip, native/wasm compile and per-output capabilities are separate flags.
- D. Routing: P0 TikZ/SVG/Drawing JSON, P1 Mermaid/Graphviz/package profiles and P2 experimental adapters are explicit.
- E. SVG/Office: strict sanitized SVG is preferred; native shapes use a controlled subset and fall back to SVG.
- F. Security: no shell/network by default, pinned sidecars, controlled arguments and bounded resources.
- G. Package locks: every TikZ-family compile plan requires a 64-hex package-lock digest.
- H. Readiness: missing engines and output plugins are blocked, never promoted.
- I. Cache: renderer, package, resources, document and output participate in the digest.
- J. Artifact graph: source, AST/scene, sanitized SVG and raster/export products use distinct artifact kinds.
- K. Office payload: hash-addressed source/SVG/PNG/PDF/native-scene references are forward-compatible.
- L. Native shapes: only basic geometry/text/group objects qualify; advanced paths retain vector fallback.
- M. Golden coverage: unit fixtures cover taxonomy, source preservation, cache, SVG attacks and Office routing.
- N. Failure corpus: Drawing candidates store sanitized metadata and hashes, not raw private source.
- O. Experimental boundaries: chemfig, PlantUML, Asymptote and MetaPost are disabled until explicitly pinned; PSTricks is blocked.
- P. Unsupported claims: no local compiler, EMF fidelity, native-shape breadth or Office host result is claimed without host evidence.
