# Remaining defects and Office contract report

Date: 2026-07-27  
Baseline: `b2267a029f4dd69f256057682497bb7bcdecfb80`  
Tested implementation: `5acc3e9cec52773ae31226847b5db7c3b5fbb2fd`

## A. Baseline and commit chain

The implementation starts from the requested `b2267a0` CI/WASM baseline. The tested implementation is `5acc3e9`, containing Readiness v2, quality and provider registries, CroppedFormula, acceptance decisions, the experimental mmap owner, decoder capture tooling, and the Office-facing facade. This report and its machine-readable companion are committed separately so the evidence can name the exact tested implementation.

## B. Readiness v2

`READINESS_SCHEMA_VERSION` is 2. Model readiness now exposes manifest, artifact, runtime, executor, session, and smoke facts separately. Mode readiness separates `technicalReady`, `qualityReady`, and `productionRecommended`; a single broad `ready` value is no longer emitted by v2. Warmup and recognition update only facts actually observed.

## C. DTO compatibility

The public readiness consumer DTOs tolerate unknown fields and use defaults for missing or null added fields. Tests cover v1 input, v2 output, unknown fields, missing additions, null values, reordered fields, and an Office consumer fixture. The v1 `ready` field is accepted only as a compatibility alias and is not emitted by v2.

## D. CroppedFormula

The stable wire name is `croppedFormula`; the CLI spelling is `cropped-formula`. Its plan contains recognition and postprocessing only, requires FormulaRecognition but not FormulaDetection, and skips detector/crop nodes. CLI, iOS C, Android JNI, and WASM entry paths expose the mode. The structural regression test proves less pipeline work than the complete Formula mode; no fabricated latency number is claimed.

## E. Model quality registry

A release-owned `ModelQualityRegistry` validates model identity, dataset/runtime/provider identity, generated commit, evidence hash syntax, thresholds, and the baseline file hash against a trusted index. The current TrOCR evidence is faithfully classified `BaselineFailed`: normalized exact `0.0`, CER `1.3022407503908286`, TER `0.6284916201117319`; it is not production recommended. Synthetic-only passing evidence can reach only `Experimental`; `Validated` requires both real and hard-negative evidence.

## F. Acceptance decision

Core now returns `AutoAccept`, `RequireReview`, or `Reject` from a shared `RecognitionAcceptance` decision. Formula output retains raw, normalized, corrected, confidence, quality status, and acceptance. Missing or non-validated quality evidence fails closed instead of being auto-accepted by an Office client.

## G. Provider validation

`ProviderValidationStore` persists exact-key reports for the process. Reuse requires matching Core/runtime/provider/library/OS/architecture/device-driver/smoke-model identity. Runtime or provider-library drift marks cached evidence stale and downgrades it to `ProbePassed`, clearing session/smoke/benchmark claims. Explicit validation supports ProbeOnly and session creation; smoke/benchmark remain required unless real keyed evidence is available.

## H. Decoder

The repository now contains pinned Paddle requirements, isolated PowerShell/Bash runners, and a capture script that records real names, dtypes, LoD, token prefixes, and step 0/1/2/3/6/9 shapes. The available Paddle PIR API still cannot provide the required while arguments in this environment, so the 29-state mapping and Add.34 producer are explicitly blocked with `DECODER_STATE_CAPTURE_UNAVAILABLE`. No positional guess, fake state fixture, or fake Add.34 result was committed.

## I. Real formula data

No redistributable, reviewed real screenshots/scans/mobile photos/hard negatives were supplied. Admitted counts remain zero and `REAL_DATASET_MISSING` is machine readable. The intake contract requires a redistributable license, redaction review, source SHA-256, annotation reviewer, and ground truth independent from predictions. Consequently hard-negative FPR and real-data calibration metrics are not claimed.

## J. Table metrics

No licensed real table image set was supplied. Admitted count remains zero, status is `TABLE_QUALITY_BASELINE_MISSING`, and no real TEDS result is claimed. Existing synthetic infrastructure is not relabeled as real evidence.

## K. mmap owner

The opt-in feature is `memory-map-model-experimental`. `RuntimeSessionOwnerCache` binds an optional `ModelMemoryOwner` to the session entry, drops the session before the mapping, supports atomic replacement, permits old/new `Arc` entries to coexist during hot reload, and releases mappings on clear. Production ORT in-memory construction remains fail-closed because ORT rc.12 would require an unsafe self-reference for a borrowed model buffer; the experimental owner is not default-enabled and no unmeasured performance benefit is claimed.

## L. Office facade

`RecognitionIntegrationApi` provides readiness, warmup, recognition, provider validation, and model reload with owned public DTOs. Office consumers no longer need manifest scanning, provider DLL decisions, runtime-registry parsing, or access to internal `Arc`/session/factory objects. CLI uses `RecognitionSession`; FFI and WASM share public modes and engine contracts, but their transports are not yet generated mechanically from this Rust trait.

## M. Test evidence

On Windows x86_64 with Rust/Cargo 1.96.0:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo test --workspace --all-features`: passed; all executed unit, integration, and doc tests passed. Two existing opt-in WASM performance tests remained explicitly ignored.
- `cargo fmt --all -- --check`: passed.
- v3 contract freeze: 10 contract files and 17 public Rust source trees verified.
- locked Cargo metadata: passed.
- engine WASM32 check and WASM package all-target check: passed.
- Python, PowerShell, and Bash decoder capture syntax checks: passed.
- privacy-path and common-secret-pattern scans: passed.

The machine-readable evidence file names the exact tested commit and commands.

## N. Incomplete items

The following require real external artifacts or a later runtime integration and remain honestly incomplete:

1. Real formula dataset and its FPR/FNR/calibration/correction metrics.
2. Real table dataset and real TEDS/structure/span/cell metrics.
3. Evidence-backed decoder 29-state mapping, Add.34 analysis, and full-vs-incremental differential fixture.
4. Actual smoke/benchmark validation for a fully keyed provider tensor fixture or signed release report.
5. Direct production ORT mmap session construction and measured Windows update/uninstall behavior.
6. Mechanical generation of Tauri/FFI/WASM/CLI transports from one interface definition.

These gaps are surfaced through blocked/error states; none is presented as complete.

## O. Commit SHA

Tested implementation: `5acc3e9cec52773ae31226847b5db7c3b5fbb2fd`.

The evidence/report commit is the commit that contains this file; it is intentionally separate and has the tested implementation as its parent.
