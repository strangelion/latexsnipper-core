# Drawing domain and adapter policy

The Drawing domain separates five concepts that must not be flattened into one “format” list:

| Layer | Examples | Contract |
|---|---|---|
| Source language | TikZ, Mermaid, Graphviz DOT, PlantUML, Asymptote | Editable source and adapter capabilities |
| TikZ package profile | base TikZ, PGFPlots, CircuitikZ, tikz-cd, forest, chemfig | Versioned package lock and allow-list |
| Structured interchange | Drawing JSON, canonical SVG, Graphviz JSON, Office shape scene | Stable schema or sanitizer contract |
| General output | SVG, PDF, PNG, WebP, EPS | Immutable artifact plus SHA-256 |
| Office-only output | native shapes, OLE, host insertion metadata | Host capability and round-trip contract |

`latexsnipper-drawing` is the initial single-crate boundary. `DrawingDocument` preserves source, canvas, layers, objects, resources, datasets, raw nodes, compatibility and provenance. Advanced or unknown syntax is retained in `RawDrawingNode`; a source-preserving adapter never claims lossless structured round-trip.

## Adapter support

| Adapter/profile | Priority | Default status | Compile route |
|---|---:|---|---|
| TikZ / base TikZ | P0 | source available; compile blocked until pinned Tectonic, dvisvgm and package lock exist | supervised local Tectonic → dvisvgm → sanitized SVG |
| PGFPlots | P0 | allowed profile; package lock required | TikZ route |
| SVG source | P0 | enabled after strict sanitation | in-process canonical SVG |
| Drawing JSON | P0 | enabled, lossless structured contract | renderer not yet provided; readiness remains unavailable |
| Mermaid | P1 | source enabled; compiler must be pinned | strict local/wasm sidecar to SVG |
| Graphviz DOT | P1 | source enabled; executable and SVG output plugin must be probed | supervised local sidecar to sanitized SVG |
| CircuitikZ, tikz-cd, forest | P1 | allowed profiles; package lock required | TikZ route |
| chemfig | P1 experimental | disabled until explicitly allowed and locked | TikZ route |
| PlantUML | P2 experimental | disabled by default | local SANDBOX profile only |
| Asymptote | P2 experimental | disabled by default | local sidecar only; remote render forbidden |
| MetaPost | P2 experimental | disabled by default | system TeX sidecar |
| PSTricks | blocked | blocked by default | no production route |

Missing executables, mismatched executable bytes, absent package locks and absent Graphviz output plugins produce blocked/unavailable readiness. Output readiness is reported per adapter and output format; it is not a global “SVG/PDF/PNG available” flag.

## Security boundary

Compilation is local and network-free. Shell escape, TeX file-reading primitives, absolute/parent-path access, remote includes, PlantUML include/environment access and Asymptote remote rendering are disabled. Sidecars require an absolute path, version and SHA-256, and the executable bytes are hashed again before planning and execution. Package profiles, resource roots/count/size/hash, AST size, timeout, output bytes and generated-file count are enforced before or around execution. `memoryLimitBytes` is passed to the local supervisor boundary but is not represented as OS-enforced evidence by Core alone.

SVG sanitation rejects DTDs, scripts, event handlers, `foreignObject`, external references, unsafe attribute URLs, unsafe `<style>` content and complexity bombs. Only local fragments and PNG data references are accepted by the current policy. The sanitized XML has a stable digest and bounded view box; it is not advertised as a geometry-level canonicalization.

## Office routing

The preferred vector route is sanitized SVG. A controlled line/arrow/rectangle/ellipse/text/group subset may become native Office shapes when the host advertises support. Complex paths and unsupported objects fall back to SVG rather than silently losing content. OLE is selected only when a truthful host capability and a source payload are present. PNG is the compatibility fallback; PDF is reserved for export/print. EMF and EPS are not represented as generally available Core output routes.

The cache key includes the Drawing document, renderer identity, package-lock digest, target output and all resource digests. Drawing failure candidates store hashes and sanitized references rather than raw user source.

## Cross-repository contract

Core owns and generates the Drawing payload/readiness contract consumed by Office:

- `contracts/schema/drawing-office-payload-v1.schema.json`
- `contracts/schema/drawing-readiness-v1.schema.json`
- `contracts/fixtures/drawing-office-payload-v1.json`
- `contracts/fixtures/drawing-readiness-v1.json`

Regenerate them with `cargo run -p latexsnipper-drawing --features contract-schema --example export_drawing_contracts`. CI uses the same command with `-- --check`, so a Rust DTO change cannot silently drift from the committed cross-repository fixtures.
