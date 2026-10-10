# LaTeXSnipper JSONL worker

`latexsnipper-worker` is a long-running, local process adapter for callers that
cannot load the Rust, C, or Python library directly. It owns a bounded set of
Core `RecognitionSession` objects and reuses each session's model/runtime state
until `session.close`, `worker.shutdown`, EOF, or process exit.

The worker is a **trusted local transport**, not a network service or sandbox.
It performs no authentication or path authorization. The supervising
application must control process access, choose permitted model/input roots,
apply an OS sandbox when required, and terminate an unresponsive child.

## Start

```bash
cargo run -p latexsnipper-worker --release
```

Write one UTF-8 JSON request per line to stdin and read exactly one flushed JSON
response per accepted line from stdout. Diagnostic logs, if any, go to stderr.
The protocol version is independent of the Core API envelope version:

```json
{"version":1,"id":"create-1","action":"session.create","params":{"modelsDir":"models","runtimePreference":"auto","parseMode":"specialized","maxThreads":4}}
{"version":1,"id":"warm-1","action":"session.warmup","params":{"sessionId":1,"profile":"croppedFormula"}}
{"version":1,"id":"ocr-1","action":"session.recognizePath","params":{"sessionId":1,"path":"formula.png","profile":"croppedFormula","formats":["latex","mathml","omml"],"timeoutMs":30000}}
{"version":1,"id":"close-1","action":"session.close","params":{"sessionId":1}}
{"version":1,"id":"stop-1","action":"worker.shutdown","params":{}}
```

Every response echoes a string or numeric `id`, reports `protocolVersion: 1`,
and embeds the stable Core v3 envelope (`ok`, `versions`, `diagnostics`, `data`
or `error`). A successful recognition keeps the authoritative `document` in
`data` and adds requested derived strings under `data.outputs`.

## Actions

| Action | Purpose |
|---|---|
| `session.create` | Create an independent persistent Core session. |
| `session.health` | Read runtime/model readiness without large-model inference. |
| `session.capabilities` | Read profile capability projection. |
| `session.warmup` | Prepare one profile; repeated calls report `alreadyWarm`. |
| `session.recognizePath` | Recognize a local raster path and optionally derive text formats. |
| `session.reloadModels` | Reload verified model manifests and clear warmup reports. |
| `session.close` | Close one session; duplicate close is a successful no-op. |
| `formula.capabilities` | Query model-free native conversion capabilities and projection limits. |
| `formula.convert` | Convert a declared formula string without creating or changing recognition sessions. |
| `worker.status` | Read live and maximum session counts. |
| `worker.shutdown` | Close all sessions, flush the response, and exit successfully. |

Supported profiles are `formula`, `croppedFormula`, `text`, `mixed`, `table`,
`handwriting`, and `formulaLayout`. Supported derived formats are `latex`,
`latex_display`, `latex_equation`, `typst`, `markdown_inline`, `markdown_block`,
`mathml`, `omml`, and `html`.

## Model-free formula conversion

These additive actions retain protocol version 1. They require no model directory,
recognition session, OCR warmup or external conversion service:

```json
{"version":1,"id":"formats","action":"formula.capabilities","params":{}}
{"version":1,"id":"bare","action":"formula.convert","params":{"content":"frac(a,b)","inputFormat":"typst","outputFormat":"latex-fragment","mode":"best-effort"}}
{"version":1,"id":"native-math","action":"formula.convert","params":{"content":"x^2","inputFormat":"latex","outputFormat":"omml"}}
```

Capability data has `schemaVersion: 1`, `target: "native"`, the shared Core
`conversions` array, `projections` and `limits`. A projection is not a new semantic
output format: `latex-fragment` uses the underlying `latex_display` capability.
Check each route's `available`, `mode`, limitations and unavailable reason before
offering it. Input labels include unsupported formats for diagnostic purposes;
their presence does not enable MTEF, UnicodeMath or AsciiMath conversion.

Conversion parameters are `content`, `inputFormat`, `outputFormat`, and optional
`mode` (`strict` by default, or explicit `best-effort`). Unknown fields, including
`backend` and `timeoutMs`, are rejected. This action uses builtin Core routes,
not third-party plugin selection or a document/binary/OLE interface.

Success data contains `content` and the actual native `capability`. Bare projection
also contains `contentKind: "latex-fragment"`. Legacy `latex` exports a document;
do not insert it directly into a math editor or between `$` delimiters. Single-formula
`markdown_inline` uses `$...$`; full Markdown document conversion has separate
semantics. Bare shape checks do not prove renderer support or visual fidelity.

`UNSUPPORTED_FORMAT`, `INVALID_ARGUMENT`, `INVALID_PARAMS`, `CONVERSION_FAILED`,
`INPUT_TOO_LARGE` and `OUTPUT_TOO_LARGE` are explicit failure envelopes, not
fallback results. A failed request does not stop the process or clear recognition
sessions. No implicit best-effort, retry, output file or document replacement occurs.

## Limits and concurrency

- at most 32 live sessions by default;
- at most 1 MiB per request line;
- at most 100 MiB per recognized input file;
- at most 10 minutes for a requested recognition timeout;
- formula source is at most 64 KiB UTF-8, with Core depth/token/reconstruction guards;
- formula result `data` is at most 256 KiB after JSON serialization (not including
  protocol fields, echoed ID or error envelope); this is not a peak allocation limit;
- requests on one stdin/stdout stream execute serially.

Run multiple supervised worker processes when parallel inference or hard
cancellation is required. A request timeout is cooperative at Core pipeline
boundaries; the supervisor remains responsible for a hard process deadline.
Formula conversion is synchronous and has no in-process interruption or deadline
parameter. Hard cancellation requires terminating the child; all process-local
recognition sessions then disappear. The supervisor must correlate IDs, discard
stale replies and explicitly recreate any sessions; it must not automatically
repeat a document mutation. Mobile integrations should use the WASM Worker route.
Image recognition remains path-only. Binary-buffer callers should use the C or
Python adapter instead of expanding binary data into JSON.
