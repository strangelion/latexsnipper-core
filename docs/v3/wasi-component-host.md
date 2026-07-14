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

Unknown imports fail instantiation. Paths must be relative and are opened from
capability directory handles with no-follow semantics on Linux, macOS, and
Windows. Traversal, absolute/prefix bypass, symlink escape, non-regular files,
and hard-link aliases are rejected. The host repeats the artifact digest check
through the verified package directory handle immediately before compilation.
The broker never resolves an untrusted relative path into an ambient host path.

Runtime capability declarations must exactly match the verified manifest.
Execution entrypoints additionally require the matching registration grant,
hook, and importer/exporter format declaration. Broker calls require both a
runtime declaration and a concrete manifest permission; only actually granted
capabilities are included in the initialization context.

## Execution and cleanup

Every invocation receives a fresh Store, ResourceTable, and component instance.
Fuel supplies a deterministic CPU budget. A single 5 ms epoch ticker belongs to
the host, while each Store has its own callback that checks only that execution's
deadline and cancellation token. One timeout therefore cannot interrupt another
concurrent Store. Component calls, broker calls, output validation, and shutdown
must complete before output is published; any failure drops the Store and all
guest resources.

Manifest limits are requests, not authority. The host applies explicit
minimum/default/maximum policy, rejects zero, below-minimum, and integer-width
overflow values, and clamps requests to host ceilings. Limits cover memory,
tables, memories, core instances, resources, input, output, diagnostics, model
bytes, temporary bytes, fuel, deadline, and concurrent executions. Waiting for
a concurrency permit is itself cancellable and included in the deadline.

Guest errors from initialize, invocation, and shutdown retain bounded structured
diagnostics. Shutdown runs after every successfully initialized invocation and a
shutdown failure prevents otherwise successful output from being published.

## Package boundary

The host accepts only unpacked directories containing `plugin.json` and the
declared component artifact. Traversal is deterministic and bounded by entry,
file, directory, recursion-depth, total-byte, metadata-byte, path-length, and
wall-clock limits. It validates manifest/core/API/WIT compatibility, semantic
versions, artifact kind/path/size/SHA-256, license, configuration-schema shape,
normalized path uniqueness, file/link types, declared payload roots, and
signature metadata shape. Archives and compression are rejected rather than
extracted.

Cryptographic signature/provenance verification and rollback/freeze protection
belong to the signed registry workstream. Until that lands, the Component host
is a local Rust API and must not be presented as remote plugin installation.

## Stable diagnostics

- `PLUGIN_WASI_TRAP`
- `PLUGIN_WASI_TIMEOUT`
- `PLUGIN_WASI_CANCELLED`
- `PLUGIN_WASI_MEMORY_LIMIT`
- `PLUGIN_WASI_OUTPUT_LIMIT`
- `PLUGIN_WASI_RESOURCE_POLICY`
- `PLUGIN_WASI_PERMISSION_DENIED`
- `PLUGIN_WASI_CAPABILITY_MISMATCH`
- `PLUGIN_WASI_INVOCATION_NOT_DECLARED`
- `PLUGIN_WASI_PROTOCOL_MISMATCH`
- `PLUGIN_WASI_INVALID_INPUT`
- `PLUGIN_WASI_INVALID_PATCH`
- `PLUGIN_WASI_HOST_FAILURE`
