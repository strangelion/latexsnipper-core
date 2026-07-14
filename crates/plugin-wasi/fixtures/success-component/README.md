# Success component fixture

This source produces the deterministic core WebAssembly input used by the host
integration tests. Regenerate it from the repository root with:

```text
cargo build --manifest-path crates/plugin-wasi/fixtures/success-component/Cargo.toml --target wasm32-unknown-unknown --release
```

Copy the resulting `latexsnipper_wasi_success_fixture.wasm` to
`crates/plugin-wasi/tests/fixtures/success-component.core.wasm`. Tests wrap the
embedded WIT metadata into a Component Model binary with the pinned
`wit-component` toolchain before execution.
