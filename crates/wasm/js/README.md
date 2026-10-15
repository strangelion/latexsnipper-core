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

## Structured formula completion

Full and `--no-default-features --features conversion-only` packages expose
`complete_formula_v3(requestJson)`, `apply_formula_completion_v3(requestJson)` and
`formula_completion_capabilities_v3()`. Feature-detect these additive exports on
older packages. They use the shared Core closed LaTeX request and snapshot, not
the conversion grammar or an AI model. The synchronous helpers `completeFormula`
and `applyFormulaCompletion` return Core v3 envelopes; interactive callers should
use the corresponding `WasmWorkerClient` methods.

```ts
import { WasmWorkerClient, utf16ToFormulaByteOffset } from "@latexsnipper/wasm-runtime";
const client = new WasmWorkerClient({ workerUrl: "/worker-entry.js", moduleUrl: "/pkg/core.js" });
await client.ready();
const source = String.raw`\fr`;
const input = { source, inputFormat: "latex" as const,
  cursor: utf16ToFormulaByteOffset(source, source.length), revision: "42" };
const suggestions = await client.completeFormula(input);
// Display candidates only. Explicit user acceptance must read CURRENT editor
// source/cursor/revision, echo suggestions.data.snapshot and select candidateId.
client.terminate();
```

All ranges and cursors use UTF-8 bytes. `utf16ToFormulaByteOffset` and
`formulaByteOffsetToUtf16` convert editor positions and reject offsets inside a
code point or unpaired UTF-16 surrogates; no replacement/normalization occurs.
Revisions are canonical decimal strings up to `"18446744073709551615"`, never JS
numbers. Query data contains snapshot and completion (typed tree/candidates);
acceptance data contains source, cursor and absolute tabStops. Empty slots and
null metadata preserve the JSONL/C/Python shape. Snapshot is not authorization.

The Worker validates both message and request, clones queued snapshots, and uses
the existing bounded serial queue, duplicate-ID guard, AbortSignal and active
execution deadline. Defaults are 32 queued tasks and 30 seconds active execution
(`maxFormulaDurationMillis`, or per-call `timeoutMillis`); queue time is excluded.
Queued abort removes only that task. Active abort/timeout terminates and rebuilds
the Worker, drops old replies, and never replays the active edit. Existing model
reload behavior is retained when sharing a recognition client.

Core requests/data are each capped at 1 MiB, source at 64 KiB, tree nodes at 4096,
depth below 64, candidates at 128. The JS Worker completion envelope has a separate
1 MiB + 64 KiB cap (`maxCompletionResultBytes` can lower it); old conversion limits
stay 256 KiB. Configured `maxFormulaInputBytes` also constrains completion source.
Core refusals remain envelopes; transport/validation/termination failures reject
with WorkerRuntimeError. Missing exports refuse explicitly, with no main-thread
fallback. The host still rechecks current editor state, owns explicit acceptance,
undo and writes, and must not treat structural closure as mathematical validity.
This is not an editor integration, mobile-host acceptance or learned ranking.

### Scoped command signatures

Both full and conversion-only packages add `complete_formula_scoped_v3`,
`apply_formula_completion_scoped_v3` and `formula_scoped_completion_capabilities_v3`.
These expose editing schema 2 separately from v1. The JS direct helpers are
`completeFormulaScoped`, `applyFormulaCompletionScoped` and
`formulaScopedCompletionCapabilities`; interactive hosts should use
`client.completeFormulaScoped` / `client.applyFormulaCompletionScoped` instead.
Worker actions are additive `complete-formula-scoped` and
`apply-formula-completion-scoped`, still on transport protocol 1.

Query input adds the current `catalog: {scope, definitions}`. Scope has opaque
`userSha256`, nullable `projectSha256/documentSha256`, and `generation`.
Definitions have `id/revision/userSha256/projectSha256/command/arguments`, with at
most 64 items and four arguments each (empty for symbols). Required kinds are
`math`/`text`; one leading `optional-math`/`optional-text` is also supported.
Feature-detect `argumentKinds` and `maxLeadingOptionalArguments` before sending
optional kinds to older builds. The usual `definition:<id>` candidate omits the
optional argument; `definition:<id>:optional` explicitly inserts `[]` and typed
tab stops. Existing bracket/braced lists are preserved. No defaults are expanded.
Commands are 1–64 ASCII letters without backslash/digits; IDs allow ASCII
letters/digits/`-_.` up to 64 bytes. All revisions/generations are exact canonical
decimal strings, including `"18446744073709551615"`. Scope hashes must be 64
lowercase hexadecimal characters. They are identities, not credentials.

Query data is `{snapshot, context, candidates}` (not the v1 nested completion
shape); each candidate has `edit` and nullable `definition` (ID/revision). To
accept, provide CURRENT source/cursor/revision/catalog, the returned scoped
snapshot and candidateId. Core binds effective signature content even if a host
forgot to advance its definition version, plus active user/project/document and
reset generation. The host must advance generation on clear/reset and owns
authorized retrieval, acceptance, writes and undo. Other user/project definitions
are filtered before private semantic validation; effective collisions refuse
explicitly rather than guessing precedence. There is no macro body, expansion,
global registry, preference/history store or learning.

Both host and Worker check the closed transport. The Worker wrapper additionally
caps catalog JSON at 64 KiB; raw WASM retains the shared 1 MiB request cap.
Core validates active declaration semantics (`INVALID_CATALOG`), and uses the
same tree/source/result budgets. Queued requests deep-copy the catalog and scoped
snapshot, then share the existing queue, abort/deadline, result limits and
generation-based stale-response dropping. Active edits are never retried after a
Worker restart. Missing scoped exports refuse explicitly, with no v1 or
main-thread downgrade. Full/slim generated Node packages and real slim browser
Worker checks cover scoped acceptance, reset rejection, hard abort/timeout and
post-restart Core recovery; these are not mobile/editor or math-fidelity claims.

### Explicit private fragments

Full and conversion-only WASM add `complete_formula_fragments_v3`,
`apply_formula_completion_fragments_v3` and
`formula_fragment_completion_capabilities_v3`. These use editing profile 3;
v1/v2 capability and snapshot shapes remain unchanged. JS exposes typed direct
helpers `completeFormulaFragments` / `applyFormulaCompletionFragments` and
`formulaFragmentCompletionCapabilities`. Interactive hosts use
`client.completeFormulaFragments` / `client.applyFormulaCompletionFragments`.
Worker actions are `complete-formula-fragments` and
`apply-formula-completion-fragments`, still on transport protocol 1.

Pass `catalog: {commands, policy, fragments}`: `commands` is the existing scoped
catalog, `policy` requires boolean `retrievalEnabled`, `sensitiveDocument` and
`allowInSensitiveDocument`. Each fragment has `id`, exact decimal `revision`,
`userSha256`, nullable `projectSha256/documentSha256`, ASCII-letter `trigger`,
single-line `label` and bare LaTeX `source`. Core accepts at most 64 fragments,
8 KiB UTF-8 per active source, and matches an explicit control-word prefix.
Disabled, unapproved sensitive, or foreign-scope fragments are filtered before
their private semantics are inspected. Built-in candidates remain available.

Query data is `{snapshot, context, candidates}`. Candidates include `edit`,
nullable `definition`, and nullable `fragment` (ID/revision); accept by
`fragment:<id>`. Profile-3 snapshots contain `catalogSha256`, not
`definitionsSha256`. Acceptance rechecks CURRENT editor state, scope/reset epoch,
effective fragment contents and all policy flags. A failed acceptance must never
be downgraded to another profile, automatically retried, or applied from cached
candidate text. The host owns explicit acceptance and undo.

Fragment request JSON is capped at 1 MiB of actual UTF-8 (not ASCII-escaped JSON);
this includes the source, catalog and acceptance snapshot. The old scoped
catalog retains its separate 64 KiB cap. Result/source/tree limits are unchanged.
These are transport budgets, not peak JS-object/serialization-memory promises.
Both host and Worker validate closed shapes; Core validates active semantics.
Queued requests own a deep copy of the catalog, policy and snapshot, and retain
the same cancellation/deadline/restart/result guards. Missing exports refuse
explicitly without a main-thread or v1/v2 fallback.

This API does no history lookup, storage, learning, network IO or rendering.
Policy flags and identity hashes bind snapshots; they are not permission tokens.
Trusted hosts must authorize retrieval BEFORE supplying private data and pass
the genuine current policy/catalog when accepting. Clear/reset must advance the
scope generation. Full/slim generated Node packages and a real slim browser
Worker cover private filtering, sensitive allow, Unicode edits, stale refusal,
hard abort/timeout, no acceptance replay and recovery; actual editors/mobile
hosts and persistent preference storage remain separate work. Explicit in-memory
preference ranking is available through the separate opt-in API below.

## Owned preference feedback

Both full and conversion-only WASM packages export
`create_formula_preferences_v1` and `formula_preference_capabilities_v3`.
`FormulaPreferences` owns a synchronous WASM object; use
`WasmWorkerClient.createFormulaPreferences(configuration)` for the bounded
asynchronous `WorkerFormulaPreferences` route. Call `close()` explicitly.

Configuration is `{schemaVersion:1,scope,keyHex}`; the host supplies the exact
completion scope and a fresh private random 32-byte key encoded as lowercase hex.
Web Crypto is also required for instance-isolated feedback identities.
Ranking and feedback default to off. There is no automatic capture, persistence,
network IO or model dependency. Explicit commands support capture/discard,
validated acceptance/undo, deterministic ranking and clear with a newer scope
generation and different key. All u64 values remain decimal strings.

Each module allows 64 open owners, each owner 128 pending captures and 1024 events.
Metadata is capped at 4 KiB, editing commands/data at 1 MiB. Worker requests own
their input; hard cancellation, timeout or lost oversized responses invalidate
old owners and queued feedback without recreation or replay. Queued cancellation
does not restart active work. These identities are not permission credentials.
See [the lifecycle and command contract](../../../docs/formula-preferences-wasm.md).

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
`ascii-math` and `unicode-math` now have finite formula-only input/output routes;
they are separate grammars, not full Word buildup or document exporters. See
[the linear conversion contract](../../../docs/formats/linear-formula-conversion.md)
for supported syntax, normalization and rejection boundaries. An available route
does not mean every expression or style in that language is representable.

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

Capabilities contain 176 WASM input/output/mode rows from the shared Core registry.
Successful data contains the converted `content` and executed `capability`, including
the reconstruction path and limitations. UnicodeMath/AsciiMath have finite
formula-only routes; MTEF remains unsupported. OLE is a host object, not a string format. Reconstruction does not
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

## Registered formula conversion

`FormulaRegistry` wraps the additive `create_formula_registry_v1` export and exposes
trusted synchronous JS format/backend registration, explicit enablement, capabilities,
conversion reports and deterministic close/free. Registered backends start disabled.
This is not a browser WASI sandbox or a third-party code loader. Synchronous callbacks
must not return Promises or start unmanaged asynchronous work.

The official Worker supports the same registry via an app-owned factory:

```ts
const registry = await client.createFormulaRegistry(); // Built-in routes only.
try {
  const report = await registry.convert({
    schemaVersion: 1, input: "latex", output: "omml", mode: "strict",
    backend: null, content: "x^2", contextSha256: actualHostContextSha256,
  }, { schemaVersion: 1, backends: [] }, { signal: controller.signal });
  if (report.ok && report.data?.outcome.kind === "converted") {
    const candidate = report.data.outcome.result.candidate;
    // Continue through the host's own preview/write validation.
  }
} finally { await registry.close(); }
```

For extensions, explicitly load a trusted same-origin bridge at client initialization;
its factory maps a finite configuration key to app-owned callbacks. The built-in factory
rejects supplied keys. Requests cannot supply module URLs, callbacks or permissions.
Active abort/timeout terminates the Worker and invalidates all old registry handles and
queued registry operations without replay. Create a new registry explicitly after recovery;
an old handle's `close()` may reject as stale. Direct synchronous calls cannot forcibly
interrupt callbacks. See [the contract and runnable fixtures](../../../docs/formats/conversion-registry-wasm.md)
for budgets, lifecycle and remaining browser/WASI boundaries.

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
