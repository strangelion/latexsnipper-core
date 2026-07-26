# Engine readiness API

`SnipperEngine::readiness()` is the public capability boundary for desktop,
Office, SDK, and FFI adapters. Callers must not scan the Core model directory or
infer capability from private Rust types.

The returned `EngineReadiness` JSON uses `schemaVersion: 1` and contains:

- every public recognition mode and the model task selected for each;
- every registered runtime and its probe result;
- every parsed model manifest and its resolved runtime/provider preference;
- scan and runtime diagnostics using stable error codes.

The method is read-only. It probes and resolves configuration but does not
create inference sessions. A `ready: true` task means a package and runtime
variant are resolvable with the current installation; session creation can
still fail later and must report `SESSION_CREATE_FAILED`.

Consumers should branch on `code`, never on `message`. `message` is diagnostic
text and may change. The stable codes are represented by `CoreErrorCode`:

```text
MODEL_NOT_FOUND
MODEL_MANIFEST_INVALID
MODEL_ARTIFACT_MISSING
MODEL_ARTIFACT_HASH_MISMATCH
RUNTIME_NOT_FOUND
PROVIDER_UNAVAILABLE
PROVIDER_LIBRARY_MISSING
SESSION_CREATE_FAILED
INPUT_SHAPE_MISMATCH
DECODER_CACHE_SCHEMA_MISMATCH
DECODER_INCREMENTAL_DIVERGENCE
OUTPUT_VALIDATION_FAILED
POSTPROCESS_REVIEW_REQUIRED
```

Unknown fields are rejected when deserializing this schema. Consumers must
negotiate `schemaVersion` before decoding a newer contract.
