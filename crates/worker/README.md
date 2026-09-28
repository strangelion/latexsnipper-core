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
| `worker.status` | Read live and maximum session counts. |
| `worker.shutdown` | Close all sessions, flush the response, and exit successfully. |

Supported profiles are `formula`, `croppedFormula`, `text`, `mixed`, `table`,
`handwriting`, and `formulaLayout`. Supported derived formats are `latex`,
`latex_display`, `latex_equation`, `typst`, `markdown_inline`, `markdown_block`,
`mathml`, `omml`, and `html`.

## Limits and concurrency

- at most 32 live sessions by default;
- at most 1 MiB per request line;
- at most 100 MiB per recognized input file;
- at most 10 minutes for a requested recognition timeout;
- requests on one stdin/stdout stream execute serially.

Run multiple supervised worker processes when parallel inference or hard
cancellation is required. A request timeout is cooperative at Core pipeline
boundaries; the supervisor remains responsible for a hard process deadline.
The v1 JSONL protocol intentionally accepts paths only. Binary-buffer callers
should use the C or Python adapter instead of expanding binary data into JSON.
