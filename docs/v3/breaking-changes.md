# Core 3.0 breaking changes

This file covers only changes introduced by the v3 contract foundation. The
existing v2 runtime remains available in PR 1.

## Workspace crate version

1. **v2 behavior:** workspace crates identify as `2.0.0`.
2. **Insufficiency:** Core 3 needs an explicit prerelease boundary for contract
   feedback without claiming release-candidate readiness.
3. **v3 replacement:** all workspace crates use `3.0.0-alpha.1` and internal
   dependency requirements match it.
4. **Migration:** depend on `3.0.0-alpha.1` only when evaluating v3 contracts;
   production callers can remain on v2.
5. **Adapter:** the v2 runtime paths remain in the alpha source tree. Legacy
   plugin API 1 Core requirements may match the preserved `2.0.0` contract.
6. **Removal:** adapter removal is not scheduled before a documented Core 3 RC
   deprecation review.

## Plugin execution classes and plugin API

1. **v2 behavior:** `built_in_rust`, `isolated_process`, `native_abi`, and a
   reserved `wasi_component` share one legacy manifest enum under plugin API 1.
2. **Insufficiency:** these names mix trust, packaging, and implementation; in
   particular, manifest permissions cannot sandbox arbitrary native code.
3. **v3 replacement:** manifest schema 3 uses exactly
   `trusted_in_process`, `isolated_native_process`, and `wasi_component`, with
   plugin API 2 and independent process-IPC/Component-WIT versions.
4. **Migration:** call `PluginManifestV3::migrate_from_v2` and inspect its
   structured report. Native dynamic-library and reserved v2 WASI manifests are
   rejected rather than reinterpreted.
5. **Adapter:** reviewed built-in and isolated-process manifests can be mapped;
   runtime execution still uses the legacy host in PR 1.
6. **Removal:** no v2 executor removal date is set; it requires a later host
   integration PR and at least the documented alpha/beta deprecation window.

## Plugin permissions, artifacts, and identity

1. **v2 behavior:** network grants may contain host names without scheme/port;
   signatures are untyped; external license metadata and artifact digests may
   be absent.
2. **Insufficiency:** the host cannot safely infer destination scope, signature
   algorithm/key identity, or artifact provenance.
3. **v3 replacement:** typed path access, typed network destinations, resource
   limits, exact artifact SHA-256, typed signature fields, license and optional
   provenance metadata.
4. **Migration:** ambiguous network and signature fields are omitted with
   `RequiresManualAction`; external artifacts without a digest fail migration;
   missing licenses require manual completion.
5. **Adapter:** only fields with unchanged semantics are copied.
6. **Removal:** unsafe compatibility that broadens permissions will not be
   added. Legacy loading remains only while the legacy host is supported.

## Model manifest/profile schema

1. **v2 behavior:** a global checksum map and optional profile metadata are
   accepted; runtime support and quality evidence are not modeled explicitly.
2. **Insufficiency:** artifacts can be ambiguous, and a profile can appear
   usable without modes, language/runtime compatibility, schemas, or evidence.
3. **v3 replacement:** every file/package artifact has its own SHA-256 and kind;
   profiles carry adapter/model/source/license identity, runtime metadata,
   preprocessing/postprocessing/output schemas, and evidence state.
4. **Migration:** call `ModelManifestV3::migrate_from_v2`. Missing or malformed
   artifact digests fail. Missing metadata produces structured warnings, and
   migrated profiles stay `unavailable` until evidence is authored.
5. **Adapter:** a strict in-memory migration helper exists; the model manager
   does not consume v3 manifests in PR 1.
6. **Removal:** the legacy reader remains until a later runtime PR documents
   real-package conversion and an RC removal decision.

## API response envelope

1. **v2 behavior:** WASM API responses carry API/capability/Core/AST versions.
2. **Insufficiency:** future diagnostic and independently evolving schema
   contracts need an explicit version set and an invariant between `ok`, `data`,
   and `error`.
3. **v3 replacement:** `ApiEnvelopeV3<T>` and `ApiContractVersionsV3` model the
   strict success/failure shape and independent versions.
4. **Migration:** construct `success`/`failure`, validate `has_valid_shape`, and
   gate each version independently. See `migration-from-v2.md`.
5. **Adapter:** no callable endpoint or TypeScript adapter is provided in PR 1;
   v2 WASM endpoints remain unchanged.
6. **Removal:** v2 endpoint deprecation begins only after a tested v3 endpoint
   and TypeScript migration path exist.

## Intentionally non-breaking surfaces

- `Document` JSON stays at schema `1.0.0`.
- native process IPC stays at version 1.
- Worker protocol stays at version 1.
- model cache stays at schema 2.
- CLI commands, FFI, Cargo features, and package layout are unchanged.
