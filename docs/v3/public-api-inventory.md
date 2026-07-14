# Public API inventory for Core 3.0

Baseline: `main` at `ee1aebf9c274c5a4389d28bee61059436126a855`.
This inventory records the public surface before later Core 3 runtime changes.

| Surface | v2/current contract | PR 1 change | Runtime impact |
|---|---|---|---|
| Rust workspace crates | SemVer `2.0.0` | staged to `3.0.0-alpha.1` | Cargo consumers must opt into alpha |
| Rust SDK (`latexsnipper-core`, engine, pipeline) | recognition/import/export APIs | no behavioral API replacement | unchanged |
| Public API types | request/response and stream types | adds `ApiEnvelopeV3`, version set, structured v3 error | contract only |
| Foundation | common IDs/errors/configuration | adds generic migration report/outcome/warnings | additive |
| `Document` AST | serialized schema string `1.0.0` | centralizes `DOCUMENT_SCHEMA_VERSION` | wire compatible |
| Format/capability AST types | generated capability registry | v3 capability version reserved | runtime remains capability v2 |
| Model manifest | source/category/variant plus global checksum map | adds manifest/profile v3 and strict migration | loader remains legacy |
| Plugin manifest | plugin API/ABI v1 and legacy class enum | adds manifest v3, plugin API v2 contract, trust classes and migration | host/store remain legacy |
| Plugin execution | trusted in-process and reviewed isolated process | no v3 executor | unchanged |
| Process IPC | protocol v1 request/response | explicitly preserved as independent v1 | unchanged |
| WASM exports | response API v2, capability v2, AST `1.0.0` | AST version now reuses canonical constant | exports remain v2 |
| TypeScript package | Worker client/protocol v1 and model cache v2 | package version staged to alpha | declarations/behavior unchanged |
| CLI | import, recognize, convert, capabilities, model and plugin commands | no v3 command or flag | unchanged |
| Package layout | crates, WASM npm package, model/plugin artifacts | no layout change | unchanged |
| Cargo features | native/WASM/provider feature matrix | no feature removal | unchanged |
| FFI | current JSON response and pointer/length API | no ABI change | unchanged |

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

## Review obligations for later PRs

Any later PR that makes a v3 contract callable must update Rust exports,
serialized examples, TypeScript declarations, CLI behavior/help/exit codes,
package contents, compatibility adapters, fuzz targets, and this inventory in
the same change.
