# Official WASM worker runtime

`WasmWorkerClient` is the recommended browser entrypoint. It serializes recognition with a
bounded queue and assigns every request an ID. Cancelling the active request terminates the
entire Worker, suppresses stale progress/results, creates a new Worker, and reloads every
previously verified model artifact. This is hard cancellation of the browser execution unit;
the direct Rust `cancel_recognition_v2()` API remains cooperative and cannot interrupt active
Tract inference.

`IndexedDbModelCache` stores only SHA-256-verified artifacts, uses schema version 2, tracks
profile/source/size/last-use metadata, supports explicit deletion and clearing, and evicts by
LRU within a configurable byte budget. `downloadVerifiedModel` streams responses, reports
known or unknown total length, enforces a maximum size, supports `AbortSignal` and mirror
fallback, verifies SHA-256 before activation/cache write, and never persists partial bytes.

Build and test:

```text
npm ci
npm run typecheck
npm test
npm run build:example
```
