# Public API inventory for Core 3.0

Baseline: `main` at `ee1aebf9c274c5a4389d28bee61059436126a855`.
This inventory records the original surface and the current Core 3 runtime
integration state.

| Surface | v2/current contract | PR 1 change | Runtime impact |
|---|---|---|---|
| Rust workspace crates | SemVer `2.0.0` | unpublished `3.0.0-alpha.1` development identifier | no v3 package is available until `3.0.0` GA |
| Rust SDK (`latexsnipper-core`, engine, pipeline) | recognition/import/export APIs | no behavioral API replacement | unchanged |
| Public API types | request/response and stream types | adds `ApiEnvelopeV3`, version set, structured v3 error | contract only |
| Foundation | common IDs/errors/configuration | adds generic migration report/outcome/warnings | additive |
| `Document` AST | serialized schema string `1.0.0` | centralizes `DOCUMENT_SCHEMA_VERSION` | wire compatible |
| Format/capability AST types | generated capability registry | v3 capability version reserved | executable matrix schema `3.0.0`; CLI/WASM v3 projections callable |
| Model manifest | source/category/variant plus global checksum map | adds manifest/profile v3 and strict migration | version-aware loader; only evidenced v3 profiles adapt to native manager |
| Plugin manifest | plugin API/ABI v1 and legacy class enum | adds manifest v3, plugin API v2 contract, trust classes and migration | explicit version loader; signed registry/WASI path consumes v3 and joins verified installation to execution |
| Plugin execution | trusted in-process and reviewed isolated process | no v3 executor | unchanged |
| Process IPC | protocol v1 request/response | explicitly preserved as independent v1 | unchanged |
| WASM exports | response API v2, capability v2, AST `1.0.0` | AST version now reuses canonical constant | adds callable `api_info_v3`, `capabilities_v3`, `convert_v3`; v2 retained |
| TypeScript package | Worker client/protocol v1 and model cache v2 | package version is internal-only until GA | declares envelope/capability/API-info v3; Worker protocol remains v1 |
| CLI | import, recognize, convert, capabilities, model and plugin commands | no v3 command or flag | adds migration command family and capability envelope selection |
| Package layout | crates, WASM npm package, model/plugin artifacts | no layout change | unchanged |
| Cargo features | native/WASM/provider feature matrix | no feature removal | unchanged |
| FFI | current JSON response and pointer/length API | no ABI change | additive version object and numeric response-version exports |

## Serialized-contract audit notes

- `Document` already carries `schema_version`; its shape does not require a v3
  schema bump.
- WASM API v2 and capability v2 are distinct today and stay callable.
- Worker protocol v1 and browser cache schema v2 are separate from the WASM
  response envelope.
- The legacy plugin enum contains reserved/native variants that are not equal to
  a safe v3 trust class. Migration therefore rejects ambiguous cases.
- The legacy model manifest may omit per-artifact checksums and evidence fields.
  v3 migration fails on missing artifact digests and marks missing evidence for
  manual completion.

## Runtime integration completed in PR 7

- JSON capability output defaults to envelope v3; `--api-version 2` is an
  explicit JSON-only compatibility adapter.
- Plugin/model loaders select contracts from explicit schema fields and reject
  future versions rather than falling back to v2.
- Signed registry installation keeps v3 WASI plugins disabled; explicit enable
  re-verifies the package before its granted format capabilities can extend the
  executable matrix. `ActivatedRemoteWasiPlugin` then performs handle-relative
  host verification, binds the result to the registry snapshot, and compiles it
  before invocation. Each invocation rechecks enablement, revocation, active
  version, manifest identity, and package provenance.
- `snipper migrate` preserves sources, emits structured reports, uses exit code
  11 for manual action, and refuses unsafe output.
- Recognition/job output formats are no longer silently changed by file name
  extensions. The complete disposition audit is in `../cli-option-matrix.md`.

## Review obligations for later PRs

RC hardening must freeze Rust exports, serialized examples, TypeScript
declarations, CLI behavior/exit codes, package contents, compatibility adapters,
WIT, and this inventory together.
