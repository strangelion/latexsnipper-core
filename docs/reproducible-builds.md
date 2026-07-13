# Reproducible build guidance

Release metadata records the Git commit, sorted tracked-source manifest, SPDX
SBOM, third-party license report, artifact list, and SHA-256 checksums.

To reproduce a native CLI build:

```bash
git checkout <release-commit>
rustup toolchain install 1.88.0
cargo build --locked --release -p latexsnipper-cli --target <target-triple>
```

To reproduce the browser package:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.13.1 --locked
wasm-pack build crates/wasm --target web --release --out-dir ../../target/wasm-web
```

Exact byte reproducibility can still vary with linker, operating-system SDK,
archive timestamps, and dependencies not yet built in a hermetic container.
Checksums therefore attest published artifacts; they are not a claim that every
developer machine will emit identical bytes.
