# Success component fixture

This source produces the deterministic core WebAssembly input used by the host
integration tests. Regenerate it from the repository root with:

```text
cargo build --locked --manifest-path crates/plugin-wasi/fixtures/success-component/Cargo.toml --target wasm32-unknown-unknown --release
```

Copy the resulting `latexsnipper_wasi_success_fixture.wasm` to
`crates/plugin-wasi/tests/fixtures/success-component.core.wasm`. Tests wrap the
embedded WIT metadata into a Component Model binary with the pinned
`wit-component` toolchain before execution.

Before copying, build with `--remap-path-prefix` for local checkout and user
dependency-cache roots, and scan the binary for machine-specific path markers.
The fixture lockfile is tracked; it is separate from the workspace lockfile.
The finite `application/vnd.fixture.power` importer/exporter exercises the
registered formula bridge with real Document JSON and refusal controls. It is
not a full formula-format converter or a production plugin.
