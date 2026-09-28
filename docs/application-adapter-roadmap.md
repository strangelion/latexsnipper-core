# Application adapter persistence roadmap

## Persistence boundary

Core already provides process-lifetime persistence through
`latexsnipper_engine::application::RecognitionSession`. One session owns and
reuses its engine, Tokio runtime, verified model packages, warmup reports, and
runtime-owned inference session caches. `close()` and `Drop` release those
resources deterministically.

A live inference session cannot survive a process restart. Across restarts,
only verified model artifacts, strong provider-validation evidence, and other
explicit on-disk caches are reusable. Adapters must not serialize native
runtime pointers or claim that they are restart-persistent.

## Adapter status

| Adapter | Process-lifetime session | Restart-persistent evidence/cache | Current status |
|---|---|---|---|
| Rust application API | yes | model artifacts and qualified provider evidence | implemented and tested |
| CLI | only for the duration of one command | yes where configured | implemented; no daemon mode |
| Android JNI / iOS C FFI | global engine object | no adapter-owned session state | legacy bridge; uses `StubRuntime` and does not own `RecognitionSession` |
| Python / PyO3 | one `RecognitionSession` per Python `Session` object | process-local only | PyO3/maturin package implemented and smoke-tested |
| Generic C ABI | one bounded opaque handle per `RecognitionSession` | process-local only | implemented in `latexsnipper-ffi` ABI v1 |
| WASM | module/process lifetime, experimental recognition state | verified browser model cache | experimental |
| Tauri / Office host | host-specific | host-specific | implemented outside Core or pending host adoption |

## Delivery order

### 1. Stable opaque C session ABI — implemented

Add a platform-neutral C ABI in `latexsnipper-ffi` with versioned request and
response JSON and an opaque numeric handle registry. The minimum lifecycle is:

```text
session_create -> session_warmup -> session_recognize* -> session_reload_models
               -> session_health -> session_close
```

The v1 implementation rejects stale/double-closed handles, bounds concurrent
sessions, serializes calls per session, and never exposes Rust pointers or
struct layouts. Byte buffers carry explicit lengths and all returned JSON uses
one documented free function. Closing a session removes the handle before
clearing runtime caches exactly once. The public declaration is
`crates/ffi/include/latexsnipper_session.h`.

### 2. Python extension — implemented

The separate `latexsnipper-python` crate uses PyO3 and maturin. It owns
one `RecognitionSession` per Python `Session` object and exposes context-manager
and explicit `close()` semantics. Recognition must release the GIL while Core
runs, accept paths or Python buffer objects without an unnecessary image encode,
and return the authoritative Document JSON plus requested derived formats.

Python exceptions must map from stable `ApplicationErrorCode` values. A module
global singleton is not acceptable because it prevents independent model roots,
test isolation, and controlled shutdown.

The implementation is in `crates/python`. Core work, including construction,
warmup, recognition, conversion, status, reload, and close, runs with the GIL
released. `recognize_bytes` accepts Python buffer-protocol objects and performs
one ownership copy of the encoded bytes without a Python-side decode/re-encode.
Results are Python dictionaries containing the authoritative `document`, Core
metadata and diagnostics, and an `outputs` map for explicitly requested derived
formats. `LaTeXSnipperError` exposes stable `code`, `detail`, and `retryable`
attributes. The installed-extension smoke test covers independent model roots,
warmup reuse, failure recovery, idempotent close, closed-session errors, and
context-manager cleanup on Windows, Linux, and macOS CI runners. A cloneable
Python `CancellationToken` maps to Core's request control without introducing
an adapter-owned cancellation mechanism.

### 3. Long-running transport for non-native callers

After the C and Python contracts stabilize, an optional JSONL worker may own a
bounded set of sessions for applications that cannot load a native extension.
Authentication, framing, process supervision, and access control remain outside
the engine. The worker must use the same `RecognitionIntegrationApi` rather than
creating a second lifecycle.

## Acceptance gates

1. Two consecutive requests reuse the same engine/runtime session and the second
   warmup reports `alreadyWarm`.
2. Invalid input, cancellation, and timeout do not poison the session.
3. Two independent Python sessions may use different model roots without state
   leakage.
4. `close()` is idempotent, frees caches once, and all later calls return a
   stable closed-session error.
5. Repeated create/recognize/close loops pass leak checks and bounded-memory
   stress tests on Windows, Linux, macOS, and supported Python versions.
6. Wheel and shared-library builds contain no model files, credentials, or
   machine-specific paths.
