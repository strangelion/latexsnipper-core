# Conversion-only WASM profile

Locally verified on 2026-10-10. This is a conversion package and browser Worker
boundary, not Obsidian desktop/mobile or complete formula-fidelity acceptance.

## Contract and compatibility

- `crates/wasm` defaults to `recognition`, preserving previous function exports.
  `--no-default-features --features conversion-only` selects the isolated profile.
  Cargo features remain additive; `--all-features` deliberately selects the full profile.
- Both builds share `formula_api.rs`, Core capability dispatch, reconstruction,
  source/output guards, error envelopes and bare-formula projection. No second parser
  or conversion implementation is maintained for the slim package.
- Slim exports only initialization, v3 API information, formula capabilities and
  formula/bare-formula conversion. It reports no v2 compatibility exports.
  Existing default API metadata and 39 previous generated JS function exports were
  compared with the pre-refactor package and preserved.
- `api-types` image request builders are default-enabled but optional. Slim uses
  protocol/AST types without image input builders; existing default users retain them.
- The normal dependency tree has 53 packages and excludes Core image, engine,
  inference, model, runtime, tensor, pipeline, export, tract, ONNX and image codecs.
  `verify_conversion_wasm_dependencies.py` enforces this on an isolated Cargo selection;
  a feature-unified full-workspace build is not evidence of a slim dependency tree.
- The official Worker accepts either package. Missing recognition methods return
  `WORKER_RECOGNITION_UNAVAILABLE` without modifying model state or falling back.

## Measurements

Same local source/toolchain, release builds optimized by wasm-pack. Rust 1.96.0,
wasm-pack 0.13.1 and wasm-bindgen 0.2.126. Byte counts describe generated web files,
not npm archives, HTTP compression or a future published version.

| Measurement | Full | Conversion-only |
| --- | ---: | ---: |
| Optimized WASM bytes | 15,932,543 | 1,752,764 |
| Generated JS bytes | 34,835 | 11,238 |
| Linear memory bytes immediately after initialization | 2,555,904 | 1,572,864 |
| Web ESM initialization under Node, one observation (ms) | 110.39 | 2.99 |
| Chrome Worker ready wall time, one observation (ms) | 587.70 | 74.30 |

The WASM byte reduction is about 89%. Timings are single observations in a local
environment, can be affected by compilation/cache/order and are not a speed guarantee.
Worker readiness includes module loading/initialization. Linear memory is one
initial snapshot, not allocator peak, browser memory, OCR model memory or process RSS.

WASM SHA-256:

- Full: `24e27fac6d1e13e88c80dd31d8da2c0467776749c2597fb94d9969fdcf989da3`.
- Conversion: `9c6ce4143bee2558ffa254777bdebb081f555fd629af6165c9bffd1b155352fe`.

## Executed checks

Generated Node packages compared all 144 capability rows, four bare-formula
directions and selected source/budget rejection cases with identical results.
Full/slim web packages both executed in real Chrome Workers: eight projections,
document-splice and unavailable-format refusal, worker result limits, deliberate
Wasm infinite-loop hard cancellation/timeout and two post-restart Core conversions.
Slim also explicitly refused an image recognition request. Both successful browser
runs had zero console errors/warnings; no OCR model was downloaded.

Default recognition unit tests, the actual tiny Tract detector, API type tests with
and without image input, slim/full checks and Clippy also passed locally. The new
browser profile unit test is included in CI; only actually executed test results
may be treated as acceptance. Firefox and mobile/Obsidian remain separate gates.

The separate local wasm-pack/WebDriver run failed with HTTP 404 after automatically
selecting ChromeDriver 155 while installed Chrome was 154.0.8037.98. That harness
attempt is not a passing test and is distinct from the successful Playwright-driven
real Worker runs above. New Chrome/Firefox CI profile results remain to be checked;
no system browser or driver configuration was changed to suppress the failure.

Reproduce byte/output comparison with
`node crates/wasm/js/scripts/conversion-profile-smoke.mjs`, after generating both
Node/web packages in `target/wasm-full-*` and `target/wasm-conversion-*`.
Build commands and Worker usage are in [the JS runtime guide](../../crates/wasm/js/README.md).

Native JSONL formula RPC is documented separately in [the worker guide](../../crates/worker/README.md).
Host asset loading, package selection, lifecycle and insertion remain separate
acceptance boundaries; see [application adapters](../application-adapters.md).
