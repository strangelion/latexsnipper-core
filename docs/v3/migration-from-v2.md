# Migrating from Core 2 to Core 3 contracts

Core 3 keeps compatibility adapters only where the old contract can be mapped
without widening permissions, inventing evidence, or changing field meaning.
Unknown future schema versions are rejected before legacy deserialization.

## CLI migration workflow

The migration commands preserve the source and choose a new sibling output by
default:

```bash
snipper migrate plugin-manifest plugin.json --json
snipper migrate model-manifest model-manifest.json --json
snipper migrate document document.json --json
snipper migrate inspect unknown.json --json
```

Plugin and model destinations use `*.v3.json`; Document migration uses
`*.migrated.json` because the Document schema remains `1.0.0`. Existing output
is rejected unless `--force` is present. The output path is always checked
against the canonical source path, so `--force` can never overwrite the source.

Exit code `11` means `requires_manual_action`. In that case the CLI emits the
structured report and writes no migrated contract. Examples include native ABI
plugins, ambiguous legacy WASI declarations, untyped network/signature fields,
missing artifact digests, and model profiles without authored evidence.

`migrate inspect` never writes and succeeds after reporting whether automatic
migration would require manual action.

## Version-aware runtime loaders

`LoadedPluginManifest::parse_json` and `LoadedModelManifest::parse/load` inspect
the explicit schema before selecting a reader. They do not attempt to parse
schema 4 or another future schema as v2.

The native model manager receives a compatibility view only from v3 profiles
whose evidence state is `experimental` or `validated`. An `unavailable` profile
cannot become a runtime model through the adapter. The original v3 manifest is
preserved on disk when downloaded.

Local process-plugin installation remains the legacy API 1 path. A manifest-v3
package is explicitly rejected there because that host cannot enforce every v3
permission. Manifest-v3 WASI packages are installed through the signed registry
path, remain disabled after installation, and are registered only after an
explicit enable operation and re-verification.

Runtime hosts activate an enabled registry package through
`ActivatedRemoteWasiPlugin`. This bridge re-verifies the canonical
`plugin.json` package with the WASI host policy, compiles the verified
Component, and preserves invocation-time manifest/declaration checks.

## API envelope and capability output

CLI capability JSON defaults to API envelope v3:

```bash
snipper capabilities --format json
snipper capabilities --format json --api-version 2
```

The second command is the explicit v2 compatibility adapter. `--api-version`
with a non-JSON format is rejected rather than ignored. The executable
capability matrix is schema `3.0.0`.

WASM callers can use `api_info_v3`, `capabilities_v3`, and `convert_v3`.
Existing `*_v2` exports remain callable. The TypeScript package declares the
discriminated `ApiEnvelopeV3<T>`, independent contract versions, capability-v3
document, and API-info types.

## Rust migration helpers

Rust callers may still use `PluginManifestV3::migrate_from_v2` and
`ModelManifestV3::migrate_from_v2` directly. Always inspect
`MigrationOutcome.report.status`; `RequiresManualAction` is not successful
installation or publication.

Never infer:

- trusted in-process execution from `native_abi`;
- a Component WIT version from a reserved legacy WASI enum;
- network scheme or port from a host-only grant;
- signature algorithm or key identity from an untyped signature;
- model quality/readiness from the presence of files.

## FFI

FFI recognition JSON now includes a `versions` object with
`ffiResponseVersion = 3`, diagnostic schema, Document schema, and Core version.
Legacy result fields (`done`, `latex`, `text`, `confidence`, `error`, `time_ms`)
remain unchanged. Native callers can query the numeric response version through
`latexsnipper_ffi_response_version` or Android `nativeResponseVersion`.
