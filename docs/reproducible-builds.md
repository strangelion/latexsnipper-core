# Reproducible build guidance

Release metadata records the Git commit, sorted tracked-source manifest, SPDX
SBOM, third-party license report, artifact list, and SHA-256 checksums.

To reproduce a native CLI build:

```bash
git checkout <release-commit>
rustup toolchain install 1.88.0
cargo build --locked --release -p latexsnipper-cli --target <target-triple>
```

The Intel macOS release target is the only exception to the default `ort`
binary download path. `ort` no longer publishes an Intel macOS bundle, so the
release workflows download Microsoft ONNX Runtime `1.23.2` for
`x86_64-apple-darwin`, verify SHA-256
`d10359e16347b57d9959f7e80a225a5b4a66ed7d7e007274a15cae86836485a6`, set
`ORT_LIB_LOCATION` to its `lib` directory, request dynamic linking, add an
`@executable_path` runtime search path, and package the dylib with its license
and third-party notices. The extracted CLI smoke test verifies that this
bundled runtime is loadable.

To reproduce the browser package:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.13.1 --locked
wasm-pack build crates/wasm --target web --release --out-dir ../../target/wasm-web --locked
```

Exact byte reproducibility can still vary with linker, operating-system SDK,
archive timestamps, and dependencies not yet built in a hermetic container.
Checksums therefore attest published artifacts; they are not a claim that every
developer machine will emit identical bytes.
