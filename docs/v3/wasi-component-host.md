# WASI Component host v1

`latexsnipper-plugin-wasi` executes Component Model packages described by a
plugin manifest v3. Its stable guest contract is
`wit/plugin-v1` (`latexsnipper:plugin@1.0.0`); this version is independent of
the Rust crate and manifest schema versions.

## Authority model

The linker does not add WASI CLI, filesystem, sockets, environment, stdio, or
process interfaces. A guest can use only these typed brokers:

- execution cancellation and remaining deadline;
- grant-ID plus relative-path filesystem reads and writes;
- allowlisted environment values;
- exact scheme/host/port network destinations and path-only requests;
- named model artifacts;
- bounded in-memory temporary files;
- separately granted monotonic clock and secure randomness.

Unknown imports fail instantiation. Paths must be relative, are canonicalized
inside canonical grant roots, and reject traversal, absolute/prefix bypass,
prefix confusion, and symlink escape. The host repeats the artifact digest
check immediately before compilation to detect replacement after package
verification. Filesystem replacement races that require platform-specific
handle-relative APIs remain a defense-in-depth follow-up; the broker never
passes a host path to the component.

## Execution and cleanup

Every invocation receives a fresh Store, ResourceTable, and component instance.
Fuel supplies a deterministic CPU budget. A single 5 ms epoch ticker belongs to
the host, while each Store has its own callback that checks only that execution's
deadline and cancellation token. One timeout therefore cannot interrupt another
concurrent Store. Component calls, broker calls, output validation, and shutdown
must complete before output is published; any failure drops the Store and all
guest resources.

Manifest limits cover memory, tables, core instances, resources, input, output,
diagnostics, model bytes, temporary bytes, fuel, deadline, and concurrent
executions. Waiting for a concurrency permit is itself cancellable and included
in the deadline.

## Package boundary

PR 2 deliberately accepts only unpacked directories containing `plugin.json`
and the declared component artifact. It validates manifest/core/API/WIT
compatibility, semantic versions, artifact kind/path/size/SHA-256, license,
configuration-schema shape, symlinks, and signature metadata shape. Archives
and compression are rejected rather than extracted.

Cryptographic signature/provenance verification and rollback/freeze protection
belong to the signed registry workstream. Until that lands, the Component host
is a local Rust API and must not be presented as remote plugin installation.

## Stable diagnostics

- `PLUGIN_WASI_TRAP`
- `PLUGIN_WASI_TIMEOUT`
- `PLUGIN_WASI_CANCELLED`
- `PLUGIN_WASI_MEMORY_LIMIT`
- `PLUGIN_WASI_OUTPUT_LIMIT`
- `PLUGIN_WASI_PERMISSION_DENIED`
- `PLUGIN_WASI_PROTOCOL_MISMATCH`
- `PLUGIN_WASI_INVALID_PATCH`
- `PLUGIN_WASI_HOST_FAILURE`
