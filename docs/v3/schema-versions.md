# Core 3.0 contract version plan

Contract versions are intentionally independent. A crate release does not
silently advance every serialized protocol.

| Surface | Existing runtime | Core 3 contract target | Stacked status |
|---|---:|---:|---|
| Workspace crates | `2.0.0` baseline | `3.0.0-alpha.1` | Implemented |
| `Document` JSON schema | `1.0.0` | `1.0.0` | Preserved; constant centralized |
| WASM API envelope | `2` | `3` | `api_info_v3`, `capabilities_v3`, and `convert_v3` callable; v2 adapters retained |
| Capability schema | `2` | `3` (`3.0.0` matrix) | Executable registry, CLI, and WASM projection integrated |
| Diagnostic schema | implicit/current | `1` | Version identified in v3 envelope contract |
| Plugin manifest schema | legacy manifest | `3` | Types/migration implemented; consumed by the Component host |
| Plugin API | `1` | `2` for manifest v3 | Component host validates v2; legacy host remains API 1 |
| Native process IPC | `1` | `1` | Preserved |
| Component WIT | unavailable | `1` (`latexsnipper:plugin@1.0.0`) | WIT package, typed host, and real fixtures implemented |
| Model manifest/profile | legacy v2 shape | `3` | Version-aware loader integrated; only evidenced v3 profiles enter runtime adapter |
| `.lsmodel` ZIP transport | application-specific | `1` | Root `manifest.toml`, bounded inspection, safe deterministic packager |
| Browser model cache | `2` | TBD by cache implementation | Unchanged |
| Browser Worker protocol | `1` | `1` unless behavior requires change | Unchanged |
| FFI response contract | unversioned v2-era surface | `3` | Self-describing JSON plus native numeric version query |
| Benchmark evidence | current ad hoc output | independent version required | Implemented by OCR evidence schema; freeze review remains |
| Registry metadata | unavailable | signed schema `1.0`; independent root/timestamp/snapshot/targets counters | Implemented |
| Remote plugin store index | unavailable | `1` | Implemented; separate from the legacy local store |

`ApiContractVersionsV3` carries the relevant versions explicitly. Consumers
must validate each field they depend on; they must not infer compatibility from
the Core crate version alone.

## Version-change rules

- Change the `Document` schema only when its serialized shape or semantics
  change.
- Change the API envelope only when response framing changes.
- Change capability, diagnostic, model, plugin, registry, cache, Worker, and
  benchmark versions independently.
- Reject an unsupported version with a structured error; never reinterpret a
  field whose semantics changed.
- Add compatibility readers only where they do not weaken the v3 security
  model.
