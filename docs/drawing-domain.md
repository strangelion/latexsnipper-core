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
| TikZ / base TikZ | P0 | source available; compile blocked until pinned Tectonic and package lock exist | local sidecar to SVG/PDF/PNG |
| PGFPlots | P0 | allowed profile; package lock required | TikZ route |
| SVG source | P0 | enabled after strict sanitation | in-process canonical SVG |
| Drawing JSON | P0 | enabled, lossless structured contract | in-process |
| Mermaid | P1 | source enabled; compiler must be pinned | strict local/wasm sidecar to SVG |
| Graphviz DOT | P1 | source enabled; executable and output plugin must be probed | local/wasm sidecar |
| CircuitikZ, tikz-cd, forest | P1 | allowed profiles; package lock required | TikZ route |
| chemfig | P1 experimental | disabled until explicitly allowed and locked | TikZ route |
| PlantUML | P2 experimental | disabled by default | local SANDBOX profile only |
| Asymptote | P2 experimental | disabled by default | local sidecar only; remote render forbidden |
| MetaPost | P2 experimental | disabled by default | system TeX sidecar |
| PSTricks | blocked | blocked by default | no production route |

Missing executables, weak executable identities, absent package locks and absent Graphviz output plugins produce blocked/unavailable readiness. They are never converted to passed evidence.

## Security boundary

Compilation is local and network-free. Shell escape, absolute/parent-path access, remote includes, PlantUML include/environment access and Asymptote remote rendering are disabled. Sidecars require an absolute path, version and SHA-256. Plans contain controlled arguments plus time, memory, output-byte, file-count, AST-node, resource and source limits.

SVG sanitation rejects DTDs, scripts, event handlers, `foreignObject`, external references, unsafe CSS URLs and complexity bombs. Only local fragments and PNG data references are accepted by the current policy. The canonical SVG has a stable digest and bounded view box.

## Office routing

The preferred vector route is sanitized SVG. A controlled line/arrow/rectangle/ellipse/text/group subset may become native Office shapes when the host advertises support. Complex paths and unsupported objects fall back to SVG rather than silently losing content. OLE is selected only when a truthful host capability and a source payload are present. PNG is the compatibility fallback; PDF is reserved for export/print. EMF and EPS are not represented as generally available Core output routes.

The cache key includes the Drawing document, renderer identity, package-lock digest, target output and all resource digests. Drawing failure candidates store hashes and sanitized references rather than raw user source.
