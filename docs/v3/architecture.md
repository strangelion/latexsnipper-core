# Core 3.0 architecture and delivery status

This document describes the target Core 3.0 architecture and the implemented
boundaries of the first three stacked changes. Each merged stage records its
validated commit and CI evidence in the pull request.

## Status vocabulary

- **Implemented** means code and executable tests are present.
- **Contract only** means versioned data types or migration rules exist, but no
  runtime consumes them yet.
- **Existing v2 runtime** means the current production path is intentionally
  unchanged.
- **Planned** means a later stacked change must implement and validate it.

## Layers and trust boundaries

```text
Applications / CLI / Rust SDK / FFI / Browser adapter
                         |
              versioned public contracts
                         |
     import -> Document AST -> recognize -> convert/export
                         |
       model profiles and capability/readiness evidence
                         |
   plugin broker -> trusted Rust | native process | WASI component
                         |
      signed registry metadata and verified artifacts
```

The `Document` serialized schema stays at `1.0.0` because its shape did not
change. Crate release, API envelope, capability schema, diagnostic schema,
plugin manifest/API, model manifest, process IPC, Worker protocol, and WIT are
independent contracts.

Native process permissions describe only operations brokered by the host. They
do not prevent native executables from invoking operating-system APIs. The
Component host forms a default-deny execution boundary by linking only typed
WIT brokers granted by a verified manifest. Signed distribution and disabled
installation are implemented separately from public execution integration.

## PR 1 implementation boundary

Implemented in PR 1:

- `3.0.0-alpha.1` workspace/package staging;
- shared structured migration outcomes and warnings;
- explicit `Document` schema constant without changing its wire shape;
- v3 API-envelope and independent-version contract types;
- plugin manifest schema v3, exact execution-class vocabulary, and strict v2
  migration decisions;
- model manifest schema v3, exact artifact digests, evidence states, and strict
  v2 migration decisions;
- unit tests, CI coverage, and fuzz compilation for the introduced schemas and
  migrations.

Still running on existing v2 paths:

- WASM exported functions and TypeScript declarations;
- capability generation and CLI capability output;
- legacy plugin registry/store and native process IPC v1;
- model manager/downloader and browser model cache;
- Worker protocol v1 and the current FFI surface.

## PR 2 implementation boundary

Implemented in this stacked change:

- immutable `wit/plugin-v1` package `latexsnipper:plugin@1.0.0` with typed
  lifecycle, transform, importer, exporter, diagnostics, capability,
  cancellation/deadline, filesystem, environment, network, model, temporary
  storage, clock, and randomness contracts;
- the native-only `latexsnipper-plugin-wasi` host pinned to Wasmtime `36.0.12`,
  whose MSRV is the workspace MSRV of Rust `1.88.0`;
- no ambient WASI CLI, stdio, environment, filesystem, socket, or process
  imports; authority is available only through exact typed brokers;
- manifest-v3/core/API/WIT/artifact/digest/size/path/license/provenance-shape
  validation and post-verification digest rechecking;
- per-execution Store/instance isolation, fuel, a per-host epoch ticker with
  per-Store deadline/cancellation callbacks, memory/table/resource/input/output/
  diagnostic/model/temp limits, and cancellable concurrency admission;
- stable `PLUGIN_WASI_*` diagnostics and result/patch validation;
- real Component Model fixtures covering transform/import/export, broker grants
  and denial, trap, invalid patch, output/memory limits, hard timeout,
  in-flight cancellation, cleanup, and host reuse.

Still planned after PR 2:

- routing verified remote packages to the Component host for public execution;
- production network transport policy. PR 2 exposes an exact-destination
  broker contract and ships a deny-by-default implementation; trusted
  applications must supply the bounded transport;
- v3 WASM/CLI endpoints, a v3 model-runtime loader, OCR accuracy evidence,
  Office/PDF fidelity certification, and release artifacts.

## PR 3 implementation boundary

Implemented in this stacked change:

- strict root/timestamp/snapshot/targets schemas with independent versions,
  expiry, Ed25519 role thresholds, canonical signed bytes, sequential dual-
  threshold root rotation, and rollback/freeze detection;
- configured HTTPS origins, identity encoding, bounded same-origin redirects,
  timeouts, MIME separation, exact length/SHA-256, and bounded response reads;
- remote target policy that permits only `WasiComponent` and requires target,
  manifest, artifact kind, compatibility, digest, and validated Component bytes
  to agree;
- bounded ZIP extraction with traversal, duplicate, symlink, special-file,
  member-count, compressed-size, decompressed-size, and per-file rejection;
- a separate locked remote store with staging, re-verification, immutable
  version directories, durable index replacement, backup recovery, quarantine,
  revocation, and last-known-good rollback;
- plugin registry/search/install/update/rollback/verify/info/doctor/revoke CLI
  commands. Remote install is disabled and never executes plugin code;
- registry/package fuzz targets and the formal
  [registry threat model](plugin-registry-threat-model.md).

Still planned after PR 3:

- public runtime execution integration for verified remote packages;
- browser table and handwriting pipelines, production OCR evidence,
  Office/PDF fidelity certification, v3 public runtime migration, and release
  candidate/GA work.

## Stacked delivery sequence

1. v3 contracts and migration foundation (implemented).
2. WASI Component host and WIT interfaces (implemented and hardened).
3. signed plugin registry and cryptographic package verification (implemented).
4. browser table and handwriting recognition.
5. production OCR validation and benchmark evidence.
6. Office/PDF fidelity framework.
7. public API/CLI integration and compatibility adapters.
8. security hardening, packaging, and release-candidate audit.

Each later change must update this status based on executable evidence rather
than the presence of a type, enum, manifest field, or test double.
