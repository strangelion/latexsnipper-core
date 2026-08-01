# Failure corpus lifecycle

Raw user documents are never committed. Intake records contain hashes and optional sanitized references only. Candidates move through `inbox`, `minimized`, and `approved`; only a reviewed, sanitized, reproducible, licensed candidate with an expected result and an error classification can be added to `regressions/index.json`.

Deduplication uses the input semantic hash, AST hash, failure signature, provider, model, and runtime. Office failures retain the smallest OMML, Flat OPC, payload, or host metadata needed to reproduce the issue—not the source document.
