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

## Model-free formula conversion

After initializing a generated WASM module, its additive exports are
`formula_capabilities_v3()` and
`convert_formula_v3(content, inputFormat, outputFormat, mode?)`. They return the
existing v3 envelope without requiring or changing recognition models. Omitted
mode defaults to `strict`; only the supported LaTeX-to-OMML source subset currently
supports strict conversion. Other available routes require explicit `best-effort`.

The wrapper also exports typed helpers and format/mode types:

```ts
import { convertFormula, formulaConversionCapabilities } from "@latexsnipper/wasm-runtime";

const routes = formulaConversionCapabilities(initializedWasmModule);
const result = convertFormula(initializedWasmModule, "frac(a, b)", {
  inputFormat: "typst", outputFormat: "latex", mode: "best-effort",
});
if (result.ok) console.log(result.data.content, result.data.capability.limitations);
else console.error(result.error.code, result.error.details);
```

Capabilities contain 144 WASM input/output/mode rows from the shared Core registry.
Successful data contains the converted `content` and executed `capability`, including
the reconstruction path and limitations. UnicodeMath, AsciiMath and MTEF remain
unsupported; OLE is a host object, not a string format. Reconstruction does not
recover the author's original source, and best-effort can lose syntax/style.
LaTeX output uses the existing document exporter, not a promise of bare-source identity.

Source and reconstructed LaTeX are bounded to 64 KiB, 64 lexical nesting levels and
512 structural tokens per call; XML DTDs are rejected. These conservative budgets
are not grammar validation. Over-limit input returns `INPUT_TOO_LARGE`, unknown
labels/modes return `INVALID_ARGUMENT`, unavailable routes return `UNSUPPORTED_FORMAT`,
and source/export failures return `CONVERSION_FAILED`.

The helpers are synchronous direct-module adapters, not new `WasmWorkerClient`
RPC methods. Run them in a caller-owned Worker for UI isolation; they do not supply
hard cancellation or conversion queue management. Feature-detect the additive exports
on older WASM packages. Existing recognition/worker and v2/v3 document-conversion APIs
remain unchanged. Package smoke executes all routes in Node and initialized Web ESM
packages under Node, calls the built TS helper, and checks bundler builds; this is
not a real-browser visual test. Browser unit tests cover the same new API.

The wasm-pack `bundler` target uses the Wasm ESM integration proposal. Vite consumers must add
`vite-plugin-wasm` and target `esnext`; the checked-in `vite.config.ts` is the canonical example.
Run `npm run smoke:packages` after generating the web, bundler, and nodejs packages under
`target/wasm-web`, `target/wasm-bundler`, and `target/wasm-nodejs`.

Build and test:

```text
npm ci
npm run typecheck
npm test
npm run build
npm run smoke:packages
npm run build:example
```
