# Reproducible build guidance

Release metadata records the Git commit, sorted tracked-source manifest, SPDX
SBOM, third-party license report, artifact list, and SHA-256 checksums.

To reproduce a native CLI build:

```bash
git checkout <release-commit>
rustup toolchain install 1.88.0
cargo build --locked --release -p latexsnipper-cli --target <target-triple>
```

## Supported native release targets

Core 3.0 GA publishes and validates native CLI archives for:

- `x86_64-unknown-linux-gnu`
- `x86_64-pc-windows-msvc`
- `aarch64-apple-darwin`

Intel macOS (`x86_64-apple-darwin`) is not part of the official GA binary
artifact matrix.

Current upstream ONNX Runtime releases no longer provide prebuilt macOS
x86_64 binaries. Maintaining a release-only dependency on an older runtime
would create a separate unsupported runtime path, so the official Core 3
release matrix targets Apple Silicon macOS.

This does not define a source-level portability guarantee for custom Intel
macOS builds using a separately supplied compatible runtime.

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
