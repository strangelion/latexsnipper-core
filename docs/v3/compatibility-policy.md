# Core 3 compatibility and deprecation policy

## SemVer and pre-GA stability

Stable Rust crates follow semantic versioning. The source tree keeps its
pre-GA version until every RC-grade gate is complete; intermediate alpha, beta,
or RC packages are not required. Every contract change must be recorded in the
changelog and migration guide. API/schema freeze is a validation state, not a
reason to publish an intermediate artifact.

## Independent serialized contracts

Consumers must negotiate the specific contract they use. Core version alone is
not proof of API-envelope, capability, Document, plugin, model, registry,
benchmark, Worker, cache, IPC, WIT, or FFI compatibility. See
`schema-versions.md` for the current map.

Unknown or unsupported versions must fail with structured diagnostics. A v2
field whose semantics changed must never be silently interpreted as its v3
counterpart.

## Compatibility window

- Existing v2 runtime entry points remain usable through explicit adapters;
  callable v3 CLI/WASM replacements are now available.
- Legacy plugin API 1 manifests are checked against both the current crate
  version and the preserved Core 2 contract version; this adapter does not
  enable any new execution class or broaden permissions.
- A v2 surface may be deprecated only after its tested v3 replacement and
  migration path are available.
- Safe deprecations remain through the unpublished development cycle and for at
  least one minor release after GA unless a security issue requires faster
  removal.
- Removal dates belong in the changelog, migration guide, and API docs.
- Security-sensitive compatibility is not promised when it would broaden a
  permission, accept unverifiable artifacts, weaken signature/provenance checks,
  or re-enable native dynamic-library trust.

## Document schema

The `Document` schema remains `1.0.0`. A Core major-version bump does
not by itself change serialized documents. A future Document change requires a
separate schema version, reader/migration tests where feasible, golden fixtures,
and an explicit fidelity statement.

## Plugin and model migration policy

Migration results contain source/target identifiers, versions, a status, and
structured warnings. `RequiresManualAction` is not success. Callers must not
install, enable, or publish the migrated result until the warnings are resolved
and the v3 contract validates.

The CLI therefore writes no output for a manual-action result. Version-aware
loaders reject unknown future schemas before legacy parsing. Manifest-v3 local
process packages are rejected because the legacy process host cannot enforce
the complete v3 permission model; verified WASI registry packages use the v3
path and remain disabled until explicitly enabled.

External plugin compatibility ends where the v3 trust model would be weakened:
legacy native ABI and ambiguous reserved WASI manifests are rejected. Model
compatibility ends where an artifact lacks an exact digest or a profile lacks
the evidence needed to claim readiness.

## Claims policy

A type, enum variant, manifest field, mock, or document is not runtime support.
Documentation must label contract-only and planned behavior. A capability may
be called implemented only after the real path, negative cases, security
boundaries, and supported platforms are exercised in CI or documented manual
validation.
