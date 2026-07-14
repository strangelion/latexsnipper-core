# LaTeXSnipper plugin WIT v1

This directory is the immutable source contract for
`latexsnipper:plugin@1.0.0`. The package version is independent of the Rust
crate release version and of plugin manifest schema versions.

The world intentionally imports only typed host brokers. It does not import
WASI CLI, sockets, filesystem, environment, or stdio worlds. A component has
no ambient host authority: every operation is checked against the verified
manifest and the execution-scoped limits before the host performs it.

Breaking changes require a new sibling directory and WIT package version.
