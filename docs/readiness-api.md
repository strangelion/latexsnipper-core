# Engine readiness API v2

`SnipperEngine::readiness()` is the public capability boundary for desktop,
Office, SDK, FFI, and WASM adapters. Hosts must not scan model manifests,
search for provider DLLs, parse `RuntimeRegistry`, or access factories and
sessions.

The returned `EngineReadiness` uses `schemaVersion: 2`. It deliberately splits:

- `technicalReady`: executor, session, and successful inference have actually
  been observed for the selected model;
- `qualityReady`: trusted release evidence passes at least an experimental
  baseline;
- `productionRecommended`: every required model has both technical readiness
  and a `validated` real+hard-negative quality baseline.

Resolving a manifest or artifact alone never sets `technicalReady`. Readiness
does not perform expensive work: it probes runtimes and returns cached provider
validation, but never creates a session, runs smoke inference, or benchmarks.
Call warmup or explicit provider validation separately.

Each model exposes the individual facts `manifestValid`, `artifactsValid`,
`runtimeResolved`, `executorCreated`, `sessionCreated`, and
`smokeInferencePassed`. Its quality status is one of `unknown`,
`baselineMissing`, `baselineFailed`, `experimental`, or `validated`.

The three runtime facts are event driven. Executor construction records only
the selected model, successful runtime session creation records that model's
session, and a completed call records inference. A successful mixed/table graph
does not promote conditional or fallback models that were never executed.

Provider validation keys are collected by the single
`ProviderEnvironmentFingerprint` implementation. Unknown or unavailable
library/device/smoke observations are explicit and cannot cache session or
smoke evidence. `SmokeInference` loads the configured versioned tensor fixture,
creates a provider session, executes it, and validates output names, dtypes,
shapes, and optional SHA. `Benchmark` performs additional measured inference
runs instead of treating model loading as a benchmark.

Trusted model quality data should use
`EngineConfig::with_quality_baselines_dir(...)`; the legacy
`<models parent>/quality/baselines` derivation is used only when no explicit
directory is configured. Provider smoke validation similarly uses
`EngineConfig::with_provider_smoke_fixture(...)`.

## Compatibility

Consumer DTOs ignore unknown fields. Missing v2 fields use safe defaults, and
v1 `ready` is accepted as an input alias for `technicalReady`; v2 never emits
the ambiguous field. Producers and strict same-version validators remain
covered by contract snapshots and schema checks. Explicit `null` for newly added readiness
collections and booleans is treated as the same fail-closed default as omission;
non-null values with the wrong type remain errors.

## Stable error codes

Consumers branch on `code`, never `message`. v2 adds:

```text
MODEL_BASELINE_MISSING
MODEL_BASELINE_FAILED
MODEL_QUALITY_NOT_VALIDATED
AUTO_ACCEPT_NOT_RECOMMENDED
CROPPED_FORMULA_MODEL_MISSING
PROVIDER_VALIDATION_STALE
PROVIDER_VALIDATION_REQUIRED
REAL_DATASET_MISSING
TABLE_QUALITY_BASELINE_MISSING
DECODER_ARTIFACT_MISSING
DECODER_STATE_CAPTURE_UNAVAILABLE
```

Existing model, runtime, provider, decoder, input, output, and postprocess
codes retain their exact wire spelling.
