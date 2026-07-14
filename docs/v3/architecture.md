# Core 3.0 architecture and delivery status

This document describes the target Core 3.0 architecture and, separately, what
the first stacked change actually implements. The baseline audited for this
work is `ee1aebf9c274c5a4389d28bee61059436126a855` on `main`.

## Status vocabulary

- **Implemented** means code and tests are present in this change.
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

The `Document` remains the shared semantic representation. Its serialized
schema stays at `1.0.0` in this change because its shape did not change. The
crate release version, API envelope, capability schema, diagnostic schema,
plugin manifest/API, model manifest, process IPC, Worker protocol, and future
WIT interface are independent contracts.

Native process permissions describe only operations brokered by the host. They
do not prevent a native executable from invoking operating-system APIs. Only a
validated WASI Component host can form the default-deny boundary intended for
untrusted third-party plugins.

## PR 1 implementation boundary

Implemented here:

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

Contract only in this change:

- `ApiEnvelopeV3` and capability-schema version 3;
- plugin API version 2 and Component WIT version 1;
- model manifest schema version 3.

Still running on existing v2 paths:

- WASM exported functions and TypeScript declarations;
- capability generation and CLI capability output;
- plugin registry/store/execution host and process IPC v1;
- model manager/downloader and browser model cache;
- Worker protocol v1 and the current FFI surface.

Not implemented here:

- WASI Component execution, interruption, or permission enforcement;
- signed remote registry, provenance verification, rollback/freeze protection,
  or remote plugin installation;
- v3 WASM/CLI endpoints or a v3 model-runtime loader;
- OCR accuracy evidence, Office/PDF fidelity certification, or release
  artifacts.

## Stacked delivery sequence

1. v3 contracts and migration foundation (this change).
2. WASI Component host and WIT interfaces.
3. signed plugin registry and package verification.
4. browser model runtime/cache completion.
5. production OCR validation and benchmark evidence.
6. Office/PDF fidelity framework.
7. public API/CLI integration and compatibility adapters.
8. security hardening, packaging, and release-candidate audit.

Each later change must update this status based on executable evidence rather
than the presence of a type, enum, manifest field, or test double.
