# API stability and deprecation policy

The public Rust crates use semantic versioning. Stable APIs may gain additive
fields or methods in a minor release; source-breaking changes require a major
release unless needed to close a security vulnerability.

The WASM response contract is versioned independently by `apiVersion`. The
current contract is v2 and always returns `ok`, version metadata, diagnostics,
and either `data` or a stable error code. Capability documents have their own
`capabilityVersion` because readiness can change when models are loaded.

Plugin manifests use `pluginApiVersion`. Built-in Rust plugins are the only
executable class in this release. Native ABI and WASI Component classes are
reserved contracts, not production execution claims.

Deprecated APIs remain for at least one minor release when safe. The legacy
synchronous WASM recognition function is retained only to return a migration
error because blocking browser inference is unsafe; callers must use
`recognize_v2`.
