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
| Portable visual rendering | Single-format `ExportArtifact` output | Adds `RenderPreference`, `RenderBundle`, SVG validation/normalization, and SVG-to-PNG fallback composition | additive |

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

### Additive registered formula bindings (2026-10-12)

The semantic registry is shared by Rust, explicit CLI selection, the independent
C registry ABI, the formal Python `FormulaRegistry`, and the WASM/JS registry.
The native WASI profile path retains verified activation and host-only sensitive
action approval. WASM/JS supports trusted synchronous callbacks, not a browser
WASI loader or permission sandbox; its contract is in
[`conversion-registry-wasm.md`](../formats/conversion-registry-wasm.md).
Existing enum formats and serialized layouts are retained. Registry JSON ingestion
is closed/versioned; `CapabilityTarget` and registry declarations now deserialize
without changing their serialized shape.

`create_formula_registry_v1`/`FormulaRegistryWasm` are additive exports, and
`FormulaRegistry`/`WorkerFormulaRegistry` are additive SDK classes. Worker protocol
v1 adds closed `formula-registry-create` and `formula-registry-command` actions.
Worker termination invalidates old instances and queued registry work without
replay; an explicit new instance is required. The new SDK contract files are
included in the freeze manifest. This does not close Office, binary/document
routes, browser untrusted WASI, persistent consent or mobile-host acceptance.

GA hardening freezes Rust exports, serialized examples, TypeScript
declarations, including `formula.ts` and its additive linear target union. The
formula-only `FormulaOutputTarget`/`convert_formula_to_target` API leaves the
legacy document `OutputFormat` enum and nine-output document conversion intact.
Formula capability rows add finite UnicodeMath/AsciiMath best-effort routes;
their canonical input/output labels are unchanged from the reserved input IDs.
No WIT or C function signature is replaced.
Native WASI profiles accept the two canonical linear format IDs as finite
importer targets/exporter sources. `WasiFormulaBackend::importer_to_target`
is additive; `importer(OutputFormat)` remains callable. Guest data still travels
as the existing single-formula Document JSON/LaTeX projection, with the same
verified activation, explicit enablement, candidate validation and execution
budgets. These routes do not grant filesystem/network consent or browser WASI.

`formula_layout::latex_fixed_delimiter_size` is an additive finite Rust helper for
the 16 fixed delimiter commands. Existing command/argument layouts and serialized
AST shapes remain unchanged. Its em tiers are target projections, not proof of
TeX/Word/SVG font equivalence; strict OMML still refuses unresolved fixed sizing.

GA hardening also freezes CLI behavior/exit codes, package contents, compatibility adapters,
WIT, and this inventory together. `contracts/v3-contract-freeze.json` is checked
in CI and covers exact contract files plus aggregate hashes for the public Rust
source trees. An intentional change requires an explicit manifest refresh and
renewed review.
