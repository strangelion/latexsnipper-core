# Application integration API

`latexsnipper_engine::application` is the transport-neutral integration layer
for native applications. It gives CLI, desktop, Office, mobile, Python, FFI,
and future service adapters one lifecycle and request model without placing a
transport protocol in Core.

## Layer boundary

```text
Transport or UI
  JSONL / Python / C / Tauri / HTTP adapter (outside this module)
                    |
                    v
Application API
  RecognitionSession / request / result / health / progress / cancellation
                    |
                    v
SnipperEngine
                    |
                    v
Pipeline / RuntimeRegistry / ModelManager / Document conversion
```

The application API does not define JSON actions, read stdin, write stdout,
listen on a port, depend on a UI framework, or reproduce an upstream
application response schema. `Document` remains the authoritative recognition
result.

## Existing capability audit

The engine already had the ownership graph needed by a long-lived caller:

- `SnipperEngine` owns one canonical `Arc<RuntimeRegistry>`.
- model packages are cached by the engine;
- ONNX Runtime factories retain their own session cache;
- `SnipperEngine::recognize` creates only a request-scoped pipeline context;
- conversion already derives LaTeX, Markdown, Typst, MathML, and OMML from
  `Document`;
- `SnipperEngine::readiness()` already probes runtimes and resolves model
  manifests without running large inference.

The legacy one-shot `sdk::Snipper` creates a worker thread and Tokio runtime for
each OCR call so its synchronous API remains safe when called from an existing
runtime. `RecognitionSession` is the long-lived alternative: it creates the
engine, registry, and one Tokio runtime during construction and reuses them.

## Basic use

```rust,no_run
use latexsnipper_engine::application::{
    RecognitionProfile, RecognitionRequest, RecognitionSession, RuntimePreference,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = RecognitionSession::builder()
        .models_dir("models")
        .runtime_preference(RuntimePreference::Auto)
        .build()?;

    let warmup = session.warmup(RecognitionProfile::Formula)?;
    if !warmup.ready {
        for missing in warmup.missing_models {
            eprintln!("{}: {}", missing.task, missing.reason);
        }
    }

    let result = session.recognize(
        RecognitionRequest::from_path("formula.png")
            .with_profile(RecognitionProfile::Formula),
    )?;
    println!("{}", result.to_latex()?);
    Ok(())
}
```

`RecognitionInput` accepts an owned Unicode path, in-memory bytes with an
optional `InputFormat` hint, or a `SnipperImage`. Bytes are decoded in memory;
images are passed directly without a PNG encode/decode round trip. Path size,
decoder allocation, dimensions, decoded bytes, and pixel count use the shared
`ImportOptions` safety limits.

Raster PNG, JPEG, WebP, BMP, TIFF, and GIF are accepted. Other formats are
rejected with `UnsupportedFormat`; they are not silently imported as text.
In particular, the application API does not launch `pdftoppm` or `mutool`.
Existing CLI PDF OCR remains a compatibility path until an in-process PDF
renderer is available.

`include_source_asset` is currently rejected explicitly. It is not silently
ignored. `model_cache_hit` is `None` because the runtime does not expose a
trustworthy per-request cache-hit bit.

## Lifecycle and threading

`RecognitionSession`:

- constructs `SnipperEngine` and the Runtime Registry once;
- owns one Tokio runtime for all synchronous calls;
- keeps model packages and runtime-owned session caches between requests;
- serializes calls through `&mut self`;
- is `Send` and deliberately not `Sync`;
- never creates an unbounded queue;
- clears runtime session caches on `close()` or `Drop`;
- rejects calls after close.

Call synchronous `recognize` or `recognize_with_control` from non-async code.
They detect an existing Tokio context and return a structured error instead of
causing a nested-runtime panic. Async applications call `recognize_async` or
`recognize_async_with_control` on their existing executor.

## Warmup and status

`warmup(profile)` resolves every required task and creates its model executor.
Adapters with runtime sessions load them during executor creation, so a
successful report represents prepared execution resources. The report lists
loaded models, missing requirements, diagnostics, and elapsed time. Repeating
the same profile returns the cached report with `already_warm = true`; warming
another profile does not clear previous runtime caches.

Status methods are:

- `health_check()` for a real runtime/model/readiness snapshot;
- `capabilities()` for supported profiles;
- `model_status(profile)` for profile tasks;
- `runtime_status()` for registered provider probes;
- `warmup(profile)` for active execution-resource preparation.

Ordinary health checks never run large-model inference.

## Progress, cancellation, and timeout

`ProgressSink` receives stable `ProgressEvent` values. Pipeline node events are
mapped to UI-neutral stages and contain no model path, token, environment
variable, or credential. Missing sinks have no cost beyond a branch. Sink
panics are isolated and do not fail recognition.

`CancellationToken` is cloneable. The application layer checks it before and
after input decoding, and the pipeline checks it before and after every node.
Cancellation returns `ApplicationErrorCode::Cancelled` and does not clear
model sessions. Runtime calls are not force-terminated; cancellation takes
effect at the next safe boundary.

Request timeouts use the same node boundaries and return
`ApplicationErrorCode::Timeout`.

## Error contract

`ApplicationError` exposes:

- stable `ApplicationErrorCode`;
- display-safe `message`;
- optional developer `detail`;
- `retryable`;
- the original `SnipperError` through the standard error source chain and
  `source_error()`.

Adapters should serialize `code`, not Rust `Debug` output, and should decide
whether developer detail is appropriate for their trust boundary.

## CLI reuse

Raster `snipper recognize` and job recognition now follow:

```text
CLI arguments
  -> application::RecognitionRequest
  -> RecognitionSession
  -> RecognitionResult.document
  -> DocumentConverter / DocumentExportService
```

Arguments and stdout/stderr behavior are unchanged. The legacy PDF branch is
isolated because it currently depends on an external renderer.

## Future adapters

Adapters should be thin owners of one `RecognitionSession`:

- a Python or PyO3 adapter maps Python objects to `RecognitionRequest` and
  exposes `Document` or derived conversions;
- a Tauri adapter stores a session in managed state and maps events to its UI;
- a C ABI adapter owns an opaque session handle and maps stable error codes;
- a JSONL or HTTP adapter maps transport requests to the same API and keeps
  framing, authentication, and compatibility DTOs outside Core.

None of those adapters requires a second engine lifecycle. They are
intentionally not implemented by this module.
