# Core 3 compatibility and deprecation policy

## SemVer and alpha stability

Stable Rust crates follow semantic versioning. `3.0.0-alpha.*` is an explicit
feedback period: contract adjustments may occur between alpha releases, but
every change must be recorded in the changelog and migration guide. The API and
schema freeze is deferred to the release-candidate stage.

## Independent serialized contracts

Consumers must negotiate the specific contract they use. Core version alone is
not proof of API-envelope, capability, Document, plugin, model, registry,
benchmark, Worker, cache, IPC, WIT, or FFI compatibility. See
`schema-versions.md` for the current map.

Unknown or unsupported versions must fail with structured diagnostics. A v2
field whose semantics changed must never be silently interpreted as its v3
counterpart.

## Compatibility window

- Existing v2 runtime entry points remain usable during the contract-only alpha.
- Legacy plugin API 1 manifests are checked against both the current crate
  version and the preserved Core 2 contract version; this adapter does not
  enable any new execution class or broaden permissions.
- A v2 surface may be deprecated only after its tested v3 replacement and
  migration path are available.
- Safe deprecations remain for at least one documented prerelease phase and,
  after GA, at least one minor release unless a security issue requires faster
  removal.
- Removal dates belong in the changelog, migration guide, and API docs.
- Security-sensitive compatibility is not promised when it would broaden a
  permission, accept unverifiable artifacts, weaken signature/provenance checks,
  or re-enable native dynamic-library trust.

## Document schema

The `Document` schema remains `1.0.0` for PR 1. A Core major-version bump does
not by itself change serialized documents. A future Document change requires a
separate schema version, reader/migration tests where feasible, golden fixtures,
and an explicit fidelity statement.

## Plugin and model migration policy

Migration results contain source/target identifiers, versions, a status, and
structured warnings. `RequiresManualAction` is not success. Callers must not
install, enable, or publish the migrated result until the warnings are resolved
and the v3 contract validates.

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
