# WASI Component plugin guide

Core 3 remote plugins are WebAssembly Components implementing
`latexsnipper:plugin@1.0.0` from `wit/plugin-v1/plugin.wit`. They do not receive
ambient WASI CLI, filesystem, environment, socket, stdio, or process access.

## Package layout

```text
plugin-package/
├── plugin.json
├── component.wasm
├── LICENSE
└── provenance.json
```

`plugin.json` must use manifest schema v3, execution class `wasi_component`,
plugin API v2, WIT version 1, an exact component path, SHA-256, byte length,
capabilities, permissions, resource limits, license, and provenance fields.
Unknown future schemas are rejected; ambiguous legacy native classes are not
silently migrated.

Use the checked-in WIT package as the source of truth. A component built against
a different world, package name, or version will fail verification or linking.

## Capabilities and permissions

Declare only exported operations the component actually implements. The host
binds callable transform/import/export capabilities to the verified manifest;
guest self-description cannot expand authority.

All brokers are default-deny:

- filesystem grants identify package-relative directories and read/write mode;
- environment grants name exact variables;
- network grants bind scheme, normalized host, and port;
- model grants identify exact host-owned artifacts;
- temporary storage, clocks, and randomness are independent grants.

Filesystem access is handle-relative with no-follow opens. Network access is an
exact-destination broker contract and remains denied unless the embedding
application supplies an approved bounded transport.

## Resource and error contract

The host owns fuel, epoch deadline, memory, table, resource, input, output,
diagnostic, model, temporary-storage, and concurrency ceilings. Cancellation and
deadline interruption are hard host controls. A plugin must still check the
typed cancellation/deadline context at useful work boundaries.

Return bounded structured diagnostics. Do not place secrets, full input
documents, or unbounded guest error text in diagnostics. Invalid patches,
oversized output, traps, protocol mismatch, and broker denial are converted to
stable `PLUGIN_WASI_*` diagnostics.

## Local validation

```bash
cargo test -p latexsnipper-plugin-wasi
cargo clippy -p latexsnipper-plugin-wasi --all-targets --all-features -- -D warnings
cargo +nightly fuzz build wasi_component_package
snipper plugin verify ./plugin-package
```

Installation from a signed registry is disabled after download. An operator or
application must explicitly enable the verified package. Every invocation then
revalidates store state and package trust; changing or replacing package bytes
invalidates activation.

See [wasi-component-host.md](wasi-component-host.md) for the host boundary and
[registry-operator-guide.md](registry-operator-guide.md) for distribution.
