# Browser formula Worker conformance

Verified locally on 2026-10-10 using an isolated headless Chrome session on Windows.
This is browser conformance, not Obsidian, Android or iOS/WebKit acceptance.

## Executed scope

- Production `worker-entry.js` and `WasmWorkerClient.convertFormula` invoked the
  generated Core web WASM package without recognition models.
- LaTeX, Typst, MathML and OMML each produced the expected bare fraction and inline
  Markdown: eight actual worker conversions, not mock output comparisons.
- Document-splicing input and unavailable MTEF direction returned explicit Core
  errors without silently choosing another converter.
- The fixture's oversized envelope was refused inside the worker before posting.
- A separate fixture forwarded normal requests to real Core. Its exceptional
  `fixture:infinite` request entered a real, deliberately infinite Wasm loop.
  AbortSignal and execution timeout both terminated that worker, created a new
  worker and successfully executed a subsequent Core LaTeX-to-OMML conversion.
- The successful run had zero console errors/warnings. Test assets returned HTTP
  200, and no OCR model was requested. Earlier Python static-server resets and a
  stalled development server were failed harness attempts, not passing evidence.
  The final run used the bounded loopback-only Node conformance server.

Client/request-validation unit tests additionally cover queue order/full state,
UTF-8 budgets, result limits, duplicate identity, stale responses, queued abort,
pre-aborted signals, timeout recovery and existing recognition behavior. A regression
test reproduced an enqueue/cancel microtask race; the shared pump now checks worker
generation after awaiting readiness, so stale pumps cannot dispatch before replacement
worker initialization.

## Reproduction

From the repository root:

```text
wasm-pack build crates/wasm --target web --release --out-dir ../../target/wasm-fragment-web --locked
npm --prefix crates/wasm/js run build
node crates/wasm/js/scripts/formula-worker-server.mjs
```

Open the URL printed by the server and await `window.formulaWorkerProof`. The
result must have `ok: true`, eight projections, two refusal cases, hard cancellation,
Wasm timeout and two post-restart Core conversions. Close the test server/browser
after validation. All fixture stalling behavior is confined to test scripts.

The 64 KiB input and 256 KiB serialized result ceiling are transport guards,
not a complete parser/renderer safety or lossless fidelity proof. Timeout applies
to active execution, not queue waiting; renderer/editor support must be checked
separately. Package/module assets remain constrained to the worker origin.

Next work: conversion-only WASM, native JSONL conversion actions, and real
[Obsidian integration acceptance](../application-adapter-roadmap.md).
