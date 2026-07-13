# Benchmarks

Core smoke benchmarks are executable on every native CI platform:

```bash
cargo bench -p latexsnipper-tests --bench core_bench
```

Coverage includes AST traversal, MathML/OMML/Typst conversion, pipeline nodes, SVG/PNG/PDF
rendering, DOCX/PPTX/XLSX generation and re-import, plus an eight-plugin chain. Every metric
emits a human-readable line and a stable `benchmark_json=` record containing name,
iterations, elapsed nanoseconds, and nanoseconds per iteration.

Normal CI runs the suite only as a smoke gate and does not compare timing thresholds.
The weekly `Scheduled hardening` workflow collects the JSON records into a versioned
artifact with the source commit for regression analysis. This avoids turning shared-runner
timing noise into flaky pull-request failures.

Windows real-model benchmarking remains separate:

```bash
cargo bench -p latexsnipper-tests --bench recognition_bench
```

It measures text recognition, formula detection, and formula recognition when the verified
model packages and fixtures are present. Model load/session construction and first/warm
inference diagnostics are also emitted by the runtime and real-model test paths; they are not
fabricated when model assets are unavailable.
