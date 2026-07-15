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

The client rejects invalid or oversized work before cloning bytes or posting to the Worker.
Defaults are a queue of 32, a 30 second RPC timeout, a 120 second recognition timeout,
128 MiB per model, 256 MiB total model bytes, 8192 x 8192 and 40 million image pixels,
16 MiB serialized results, and a 512 MiB IndexedDB cache. Every limit is configurable with a
positive safe integer. A task timeout follows the same terminate, recover, verified-model
reload, and stale-response suppression path as an explicit hard cancellation.

Browser table mode uses the built-in projection structure profile when no validated structure
model is loaded, then runs cell text recognition and emits geometry, merges, confidence, and
diagnostics in the `TableBlock`. Browser handwriting mode requires a validated TrOCR encoder,
decoder, tokenizer, preprocessing metadata, decoding metadata, and I/O schema before readiness
is reported. These pipelines are experimental until OCR accuracy gates are available.

The wasm-pack `bundler` target uses the Wasm ESM integration proposal. Vite consumers must add
`vite-plugin-wasm` and target `esnext`; the checked-in `vite.config.ts` is the canonical example.
Run `npm run smoke:packages` after generating the web, bundler, and nodejs packages under
`target/wasm-web`, `target/wasm-bundler`, and `target/wasm-nodejs`.

Build and test:

```text
npm ci
npm run typecheck
npm test
npm run smoke:packages
npm run build:example
```
