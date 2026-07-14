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

The browser production-model job writes stable schema version 1 JSON with separate
model byte size, Tract session creation, first inference, warm inference, estimated
working set, input/output shapes, and scores. Package jobs separately record WASM
binary and JavaScript glue size. Worker unit tests measure startup and restart
semantics; timing thresholds are intentionally not enforced on pull requests.

Archive download, verification, and extraction remain downloader metrics rather than
inference metrics. Native recognition benchmarks distinguish session construction,
first/warm inference, pipeline postprocessing, AST construction, and conversion where
the relevant layer exposes timing. Scheduled artifacts are comparable by schema and
commit, but regression decisions require repeated samples rather than one shared-runner
observation.
