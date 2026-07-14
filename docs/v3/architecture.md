# Core 3.0 architecture and delivery status

This document describes the target Core 3.0 architecture and the implemented
boundaries of the first two stacked changes. The baseline audited for this work
is `ee1aebf9c274c5a4389d28bee61059436126a855` on `main`.

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
WIT brokers granted by a verified manifest. Signed distribution and legacy
registry/CLI integration remain separate work.

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
- the native-only `latexsnipper-plugin-wasi` host pinned to Wasmtime `38.0.4`,
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

- cryptographic signature and provenance verification against a signed remote
  registry, including rollback/freeze protection and update transactions;
- routing legacy plugin CLI/registry commands to the Component host;
- production network transport policy. PR 2 exposes an exact-destination
  broker contract and ships a deny-by-default implementation; trusted
  applications must supply the bounded transport;
- archive installation. PR 2 accepts only already-unpacked directory packages
  and rejects archives/compression rather than extracting attacker-controlled
  data;
- v3 WASM/CLI endpoints, a v3 model-runtime loader, OCR accuracy evidence,
  Office/PDF fidelity certification, and release artifacts.

## Stacked delivery sequence

1. v3 contracts and migration foundation (PR 1).
2. WASI Component host and WIT interfaces (this change).
3. signed plugin registry and cryptographic package verification.
4. browser model runtime/cache completion.
5. production OCR validation and benchmark evidence.
6. Office/PDF fidelity framework.
7. public API/CLI integration and compatibility adapters.
8. security hardening, packaging, and release-candidate audit.

Each later change must update this status based on executable evidence rather
than the presence of a type, enum, manifest field, or test double.
