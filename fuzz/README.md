# Fuzzing

The `fuzz/` crate contains 12 libFuzzer targets for format detection, package
and parser attack surfaces. Inputs are capped in each harness, no target uses
the network, and all crashes can be reproduced with the artifact path printed
by cargo-fuzz.

```bash
cargo install cargo-fuzz --locked
cargo fuzz build
cargo fuzz run latex_parser fuzz/corpus/latex_parser -- -max_total_time=60
cargo fuzz tmin latex_parser fuzz/artifacts/latex_parser/<crash-file>
cargo fuzz cmin latex_parser fuzz/corpus/latex_parser
```

PR CI compiles every target. Scheduled CI runs each target briefly and uploads
`fuzz/artifacts/` on failure. Longer campaigns are intentionally manual or
nightly; deterministic malformed-input regression tests remain in the normal
Rust suite.
