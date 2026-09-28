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

## OCR and Incremental Document Contracts

`latexsnipper-benchmark` complements `latexsnipper-evaluation`: evaluation measures
prediction quality against OCR corpora, while benchmark cases measure execution behavior
and incremental equivalence.

Run the included incremental performance case:

```powershell
cargo run -p latexsnipper-benchmark -- --case benchmark/cases/formula-incremental.json
```

Run the checked-in incremental golden case:

```powershell
cargo run -p latexsnipper-benchmark -- --golden-case benchmark/golden/incremental/formula-edit-v1
```

The versioned benchmark contract may link to an existing `corpusTask` from
`latexsnipper-evaluation`. Its runners include `formula_conversion` (P50/P95
conversion latency), `formula_incremental` (local edits with full-parse
equivalence), and `incremental_scale`. The scale runner executes: last-formula
fast-path edit, beginning-of-document insertion plus reconcile, then 100 edits
of the same formula. Checked-in cases cover 10, 100, 1,000, and 10,000 formulas:

```powershell
cargo run -p latexsnipper-benchmark -- --case benchmark/cases/incremental-scale-1000.json
```

It records touched/reparsed/converted/rendered nodes, cache hits/misses/
evictions/bytes, and reconciliation matched/replaced node counts. No fixed
latency threshold is asserted. Golden cases additionally assert the final
serialized `Document` and expected touched-node metrics. OCR, PDF, and Office
runners will reuse these contracts with the existing `evaluation/` and
`fidelity/` corpora.

`incremental_formula_edit_scale` isolates one last-formula fast-path edit from
the structural reconcile workload. Its 10/100/1,000/10,000 cases assert
`reparsed_nodes == 1`, making the touched-node contract independently visible:

The runner reports `setup_latency_ns`, `edit_latency_ns`, and
`verify_latency_ns` separately. Its latency percentiles use only the edit
window; full-equivalence verification runs afterwards as a correctness oracle.

```powershell
cargo run -p latexsnipper-benchmark -- --case benchmark/cases/incremental-formula-edit-scale-10000.json
```

## Formula and Office corpus contract

The formula/Office closeout corpus is frozen independently from the OCR image
corpora. Its versioned plan records a 10,000-formula category allocation, 500
compound Office document scenarios, a deterministic seed, cross-cutting wrapper
and Markdown coverage, expected outcomes, output targets, and insertion modes.

Validate the plan and generate the small deterministic pilot that freezes the
schema and digest:

```powershell
cargo run --locked -p latexsnipper-evaluation --bin formula-corpus -- `
  validate-plan --plan evaluation/formula-corpus/plan.json

cargo run --locked -p latexsnipper-evaluation --bin formula-corpus -- `
  generate-pilot `
  --plan evaluation/formula-corpus/plan.json `
  --output target/evaluation/formula-pilot.json

cargo run --locked -p latexsnipper-evaluation --bin formula-corpus -- `
  validate-corpus `
  --plan evaluation/formula-corpus/plan.json `
  --corpus target/evaluation/formula-pilot.json
```

The pilot proves schema, coverage, ordering, context materialization, and digest
determinism. The pull-request corpus expands this to 360 deterministic records
(300 valid and 60 malformed) while staying small enough for regular CI:

```powershell
cargo run --locked -p latexsnipper-evaluation --bin formula-corpus -- `
  generate-pull-request `
  --plan evaluation/formula-corpus/plan.json `
  --output target/evaluation/formula-pr.json

cargo run --locked -p latexsnipper-evaluation --bin formula-corpus -- `
  validate-corpus `
  --plan evaluation/formula-corpus/plan.json `
  --corpus target/evaluation/formula-pr.json
```

Neither tier claims Microsoft Office application fidelity. Those measurements
require the full corpus and installed-Office evidence layers defined in
[closing-plan.md](closing-plan.md).

Generate the digest-frozen 10,000-record corpus for nightly or release evidence:

```powershell
cargo run --release --locked -p latexsnipper-evaluation --bin formula-corpus -- `
  generate-full `
  --plan evaluation/formula-corpus/plan.json `
  --output target/evaluation/formula-full.json
```

The plan freezes separate SHA-256 digests for pilot, pull-request, and full
tiers. Generator or template changes therefore require an explicit corpus
contract review instead of silently changing historical baselines.

Run the full corpus once through the benchmark entry point to produce both the
record-level evaluation evidence and a compact performance summary:

```powershell
$formulaCommit = (git rev-parse HEAD).Trim()
$formulaGeneratedAt = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
cargo run --release --locked -p latexsnipper-benchmark `
  --bin latexsnipper-formula-corpus-benchmark -- `
  --plan evaluation/formula-corpus/plan.json `
  --corpus target/evaluation/formula-full.json `
  --source-commit $formulaCommit `
  --generated-at-utc $formulaGeneratedAt `
  --output target/evaluation/formula-full-benchmark.json `
  --evaluation-output target/evaluation/formula-full-report.json `
  --require-expected-outcomes
```

The compact report records expected-outcome, parse, conversion, and round-trip
rates; total elapsed time; throughput; and P50/P95/P99 for each measured stage.
Peak process memory remains explicitly unmeasured until a platform-specific
external harness supplies a reliable value.

The full tier is a synthetic contract-scale corpus produced by the reviewed
deterministic compositional generator. Every valid record has a distinct
normalized source, every valid category covers at least six grammar families,
and the drawing category includes both TikZ and PGFPlots families. It verifies
scale, quotas, grammar-family coverage, diagnostics, pipeline stability, and
evidence generation. It must not be presented as accuracy on 10,000
independent real-world formulas; publishing that claim still requires a
licensed external corpus with representative real-world distributions.

Generate the PR semantic baseline and enforce the expected-outcome diagnostic
gate after creating the PR corpus:

```powershell
$formulaCommit = (git rev-parse HEAD).Trim()
$formulaGeneratedAt = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
cargo run --locked -p latexsnipper-evaluation --bin formula-corpus -- `
  evaluate `
  --plan evaluation/formula-corpus/plan.json `
  --corpus target/evaluation/formula-pr.json `
  --source-commit $formulaCommit `
  --generated-at-utc $formulaGeneratedAt `
  --output target/evaluation/formula-pr-report.json `
  --require-expected-outcomes
```

The report records parse outcomes, expected-outcome matches, semantic conversion
and round-trip success, per-stage P50/P95/P99 latency, output sizes and hashes,
and explicitly deferred SVG/PNG/Office targets. Structural validation distinguishes recoverable
group or delimiter problems from fatal group, environment, and delimiter
mismatches. CI requires every valid, recoverable, and rejected record to match
its declared outcome; conversion and round-trip rates remain reported evidence
until representative quality thresholds are frozen.
