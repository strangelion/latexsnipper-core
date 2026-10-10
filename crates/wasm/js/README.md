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

The default Rust feature set preserves recognition and every previous export.
For a conversion-only package, build explicitly with default features disabled:

```text
wasm-pack build crates/wasm --target web --release --out-dir ../../target/wasm-conversion-web --locked --no-default-features --features conversion-only
```

Cargo features are additive: `--features conversion-only` alone still includes
default recognition. The isolated profile exports `init`, `api_info_v3`,
`formula_capabilities_v3`, `convert_formula_v3` and `convert_formula_fragment_v3`.
It reports `v2CompatibilityExports: false` and does not export recognition,
model loading or document-conversion APIs. Existing default packages remain compatible.
`latexsnipper-api-types` keeps image request builders enabled by default; this leaf
uses only its protocol types and does not pull image decoders into the slim build.

`WasmWorkerClient` works with either package. Recognition requests against the slim
package return `WORKER_RECOGNITION_UNAVAILABLE`, not a missing-function exception
or silent fallback. No models are loaded. Conversion capabilities and output
envelopes are shared between profiles, including strict/best-effort limitations.
See [the measured profile report](../../../docs/formats/conversion-only-wasm.md).

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

For a standalone formula, feature-detect the additive
`convert_formula_fragment_v3(content, inputFormat, mode?)` export or use
`convertFormulaFragment`. Success includes `contentKind: "latex-fragment"`;
the capability describes the underlying `latex_display` route, followed by a
bounded, exactly-one-display shape check. This does not imply complete MathLive
syntax support, TeX execution safety or lossless reconstruction.

```ts
import { convertFormulaFragment } from "@latexsnipper/wasm-runtime";

const bare = convertFormulaFragment(initializedWasmModule, "frac(a,b)", {
  inputFormat: "typst", mode: "best-effort",
});
// Check bare.ok before inserting bare.data.content into a formula editor.
```

LaTeX/Typst/MathML/OMML single-formula `markdown_inline` conversion now emits
`$...$`, while `markdown_block` emits `$$...$$`. Full Markdown document conversion
preserves source display modes; it is not an unconditional inline projection.
Fragment projection refuses extra blocks, preambles, nested math-mode boundaries
and document environments, and retains newline termination for trailing comments.

Source and reconstructed LaTeX are bounded to 64 KiB, 64 lexical nesting levels and
512 structural tokens per call; XML DTDs are rejected. These conservative budgets
are not grammar validation. Over-limit input returns `INPUT_TOO_LARGE`, unknown
labels/modes return `INVALID_ARGUMENT`, unavailable routes return `UNSUPPORTED_FORMAT`,
and source/export failures return `CONVERSION_FAILED`.

The direct helpers remain synchronous. `WasmWorkerClient.convertFormula` now uses
the additive `convert-formula` worker action; existing actions and protocol version
1 remain compatible. Prefer a dedicated client instance for conversion-only work.
No recognition model is required or automatically loaded. If models were explicitly
loaded into a shared client, hard restart restores them as before.

```ts
const controller = new AbortController();
const result = await client.convertFormula({
  requestId: "editor-revision-1",
  content: "frac(a,b)", inputFormat: "typst",
  outputFormat: "latex-fragment", mode: "best-effort",
}, { signal: controller.signal, timeoutMillis: 5000 });
// result is the raw Core v3 envelope: check ok before using its content.
// controller.abort() cancels queued work or terminates/rebuilds the active Worker.
```

Conversion and recognition share the bounded queue. Formula defaults are 64 KiB
UTF-8 input, 256 KiB serialized result envelope and 30 seconds of active execution;
input/result limits may be lowered but not raised above these ceilings. Timeout
does not include queue waiting. Queued abort removes only that task; active abort
or timeout terminates the Worker, suppresses stale responses and reinitializes it
before continuing queued tasks. This is not merely a Promise timeout. The worker
checks formula input before invoking WASM and result size before posting it.
Missing exports on an older WASM package return `WORKER_FORMULA_UNAVAILABLE`; there
is no silent main-thread fallback. An older worker rejects the new action explicitly.

Feature-detect additive exports on older generated packages. Recognition behavior
and v2/v3 document-conversion APIs remain compatible. Package smoke executes all
routes in Node and initialized Web ESM
packages under Node, calls the built TS helper, and checks bundler builds; this is
not a real-browser visual test. Browser unit tests cover the same new API.

For actual browser Worker conformance, build the web package at
`target/wasm-fragment-web`, run `npm run build` here and then start
`node crates/wasm/js/scripts/formula-worker-server.mjs` from the repository root.
Open the printed loopback page. It executes Core conversion through the production
worker and uses a separate clearly named fixture with a deliberately infinite
Wasm loop to verify hard cancellation, timeout and post-restart Core conversion.
This does not claim Obsidian desktop/mobile, WebKit or Android acceptance; see
[the application adapter boundaries](../../../docs/application-adapters.md).

Use `?profile=full` or `?profile=conversion` on that page to test freshly built
packages under `target/wasm-full-web` or `target/wasm-conversion-web`. The default
test profile still uses `target/wasm-fragment-web`. `smoke:conversion-profile`
compares generated Node package output for every capability row and records web
package bytes, initialization wall time and the initialized linear memory size;
the latter is not whole-process or peak memory.

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
