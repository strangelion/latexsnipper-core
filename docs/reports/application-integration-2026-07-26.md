# Application integration completion report

Date: 2026-07-26  
Starting commit: `3edcd4c`

## 1. Existing capability audit

- `SnipperEngine` already owns the canonical `Arc<RuntimeRegistry>` and is
  reusable across requests.
- model packages are cached by the engine; ONNX Runtime owns its reusable
  inference-session cache.
- pipeline contexts are request-scoped and do not own an independent runtime.
- `EngineReadiness` already provides the authoritative read-only runtime/model
  probe.
- the conversion layer already derives all requested textual formats from
  `Document`.
- the one-shot synchronous SDK creates a thread and Tokio runtime per OCR call;
  it remains for compatibility, while applications now have a long-lived API.

## 2. Public API added

- stable `RecognitionProfile` with snake_case serde values and lossless
  `RecognizeMode` conversion;
- `RecognitionInput`, `RecognitionOptions`, and `RecognitionRequest`;
- `RecognitionResult`, `RecognitionMetadata`, and runtime metadata;
- `RecognitionSession` and `RecognitionSessionBuilder`;
- health, capability, model status, runtime status, and warmup reports;
- `ProgressSink`, `ProgressEvent`, and stable progress stages;
- cloneable `CancellationToken` and `RecognitionControl`;
- stable `ApplicationErrorCode` and source-preserving `ApplicationError`.

## 3. Lifecycle

Session construction creates the engine, registry, and one Tokio runtime once.
All recognition calls reuse those objects. Requests are serialized through
`&mut self`; the Session is `Send` and intentionally not `Sync`.

Warmup creates model executors and eager adapter runtime resources. Repeated
warmup for the same profile is constant-time and does not clear other profiles.
`close()` and `Drop` clear runtime-owned session caches and shut down the owned
Tokio runtime without panicking inside another async runtime.

## 4. CLI reuse

Raster `recognize`, `rec`, and job recognition build an application request,
call `RecognitionSession`, then convert/export `RecognitionResult.document`.
CLI arguments and stdout/stderr markers are covered by an integration test.

PDF OCR keeps the pre-existing SDK compatibility branch because its renderer
uses external `pdftoppm`/`mutool` processes. The new application API never
launches a child process.

## 5. Validation

The following completed successfully:

```text
python scripts/verify_v3_contract_freeze.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --no-fail-fast
```

The workspace test run reported zero failures. Two pre-existing opt-in WASM
performance tests remained ignored.

Application integration tests cover:

- engine/session reuse and recovery after failed input;
- path, bytes, image, Unicode path, missing, damaged, unsupported, bounded, and
  blank inputs;
- successful and missing-model warmup, idempotence, and profile switching;
- health and provider-unavailable state;
- ordered progress, callback isolation, cancellation, timeout, and reuse after
  control errors;
- nested-runtime-safe sync/async boundaries;
- Document-authoritative conversions and diagnostics;
- explicit close, cache release, and stable serialized error codes.

## 6. Adapter boundary

No JSONL worker, stdin/stdout service, HTTP/gRPC/WebSocket server, MCP server,
PyO3 module, Tauri command, C ABI addition, or application-specific response
schema was added.

## 7. Future adapters

Python, Tauri, C ABI, JSONL, and HTTP adapters should own one
`RecognitionSession`, translate their transport input to `RecognitionRequest`,
forward progress/cancellation, and expose `RecognitionResult.document` or call
its conversion helpers. Transport framing, authentication, compatibility
fields, and UI state remain outside Core.
