# API stability and deprecation policy

The public Rust crates use semantic versioning. Stable APIs may gain additive
fields or methods in a minor release; source-breaking changes require a major
release unless needed to close a security vulnerability.

The WASM response contract is versioned independently by `apiVersion`. The
current contract is v2 and always returns `ok`, version metadata, diagnostics,
and either `data` or a stable error code. Capability documents have their own
`capabilityVersion` because readiness can change when models are loaded.

The current plugin runtime uses plugin API 1. Trusted in-process plugins and
reviewed isolated native-process plugins are executable; native-process
permissions cover brokered host operations and are not an operating-system
sandbox. Native dynamic-library ABI and WASI Component execution are not
available.

Core `3.0.0-alpha.1` adds contract-only API envelope v3, capability schema v3,
plugin manifest schema 3/plugin API 2, and model manifest schema 3 types. The
callable WASM, capability, plugin-host, and model-loader paths remain on their
existing contracts until later stacked changes provide real implementations.
Contract versions evolve independently; see `v3/schema-versions.md`.

Deprecated APIs remain for at least one minor release when safe. The legacy
synchronous WASM recognition function is retained only to return a migration
error because blocking browser inference is unsafe; callers must use
`recognize_v2`.
