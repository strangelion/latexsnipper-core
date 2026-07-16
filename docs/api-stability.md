# API stability and deprecation policy

The public Rust crates use semantic versioning. Stable APIs may gain additive
fields or methods in a minor release; source-breaking changes require a major
release unless needed to close a security vulnerability.

The WASM response contract is versioned independently. Envelope v3 returns
`ok`, an independent `versions` set, diagnostics, and exactly one of `data` or
`error`. The explicit v2 exports remain compatibility adapters. Capability
schema v3 is independent because readiness can change when models are loaded.

The current plugin runtime uses plugin API 1. Trusted in-process plugins and
reviewed isolated native-process plugins are executable; native-process
permissions cover brokered host operations and are not an operating-system
sandbox. The `latexsnipper-plugin-wasi` crate executes verified manifest-v3
Component packages against WIT v1. Signed-registry installation, explicit
enable/disable, and verified capability registration consume manifest v3; the
legacy local process store rejects v3 packages it cannot safely enforce. Native
dynamic-library ABI remains unavailable.

Core 3 adds callable API envelope v3, capability schema v3, plugin manifest
schema 3/plugin API 2, and model manifest schema 3. CLI migration and
version-aware loaders reject unknown schemas instead of falling back to v2.
Contract versions evolve independently; see `v3/schema-versions.md`.

Deprecated APIs remain for at least one minor release when safe. The legacy
synchronous WASM recognition function is retained only to return a migration
error because blocking browser inference is unsafe; callers must use
`recognize_v2`.
