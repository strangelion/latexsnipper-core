# Production capability and fidelity policy

The executable source of truth is `DocumentImporter::supported_formats()`,
`DocumentExportService::supported_formats()`, and
`DocumentExportService::capability_matrix()`. Use:

```bash
snipper capabilities --format json
snipper capabilities --format json --input docx --output png
```

Office/PDF rows include six independent fidelity measurements. The checked-in
[generated matrix](generated/fidelity-capabilities.md) is regenerated directly
from that executable registry in CI; drift fails the build.

## Stability classes

| Class | Contract |
|---|---|
| Stable | In-process implementation, public API/CLI path, semantic tests, and no known mandatory external service. |
| Best effort | Output is structurally valid and reopen-tested, but some source semantics or layout may be downgraded with diagnostics. |
| Visual fallback | Appearance is prioritized over editability; source metadata or assets are retained where possible. |
| Opaque preservation | Unknown OOXML parts are retained byte-for-byte when preservation mode is enabled, but generated core parts take precedence. |
| Experimental | API is callable and tested, but fidelity and format coverage are still intentionally conservative. |

Semantic LaTeX, Markdown, Typst, HTML, MathML, OMML, plain text, and JSON AST
paths are stable for the AST variants they natively represent. PDF/SVG/PNG and
DOCX/PPTX/XLSX package export are experimental/best-effort. A package reopening
successfully proves structural validity, not full Microsoft Office visual parity.

## Import policy

Format detection is signature/package-first, then MIME, then extension. Mismatched
hints return typed errors. OOXML imports enforce safe enclosed paths, entry-count,
decompressed-size, compression-ratio, XML depth/element, DTD/entity, external
relationship, page, and asset limits. Encrypted OOXML packages return `EncryptedFile`.
Raster headers are checked before decode/allocation for width, height, and total pixels.
OOXML imports also enforce distinct slide, sheet, and embedded-object budgets. Model
archives require a manifest SHA-256 and enforce download, entry, decompressed-size,
compression-ratio, enclosed-path, duplicate-output, and symlink limits.
Unsupported SVG elements retain the original SVG source asset and emit
`W_UNSUPPORTED_FEATURE`. Office unknown parts are retained only with
`ImportOptions::preserve_unknown_parts`.

Raster image import preserves the original scan asset in the AST; it does not
implicitly perform OCR. Consequently direct PNG/JPEG/BMP/TIFF/WebP/GIF to semantic
or document output pairs are reported unavailable. Use `snipper recognize` (or the
SDK recognition API) first. Raster to JSON AST remains available for asset-preserving
ingestion.

Native PDF extraction is inherently best effort because arbitrary font encodings,
missing `ToUnicode` maps, complex graphics-state transforms, and reading order can
be ambiguous. Scanned pages require OCR models and a rendering dependency.

## Format fidelity classification

| Capability | Status | Validity and expected loss |
|---|---|---|
| LaTeX, Typst, Markdown math | stable for represented AST | Semantic math is editable; unsupported source macros/packages, custom fonts, and exact line breaking may be lost. |
| MathML, OMML | stable/best effort | Formula structure is editable; application-specific properties and exact typography may be downgraded. |
| JSON AST | stable for matching schema | Highest semantic round trip; foreign schema versions require migration. |
| HTML | best effort | Text/math semantics retained; scripts, CSS layout, charts, and embedded application objects are not reproduced. |
| SVG | experimental/best effort | Supported shapes remain vector; filters, animation, external resources, fonts, complex text layout, and unsupported elements may fall back to the opaque source asset. |
| PNG | visual fallback | Pixels preserve appearance only; text, formulas, tables, fonts, and objects are not editable. Transparency/background depend on the selected renderer path. |
| PDF | experimental/best effort | Structural validity is tested. Reading order, fonts, international text without mappings, charts, complex transforms, editability, layout, and round trip are not guaranteed. Embedded scans require OCR. |
| DOCX | experimental/best effort | Package validity and core text/formula/table semantics are tested. Styles, fonts, pagination, floating layout, charts, SmartArt, OLE, and embedded objects may be retained opaquely or lost; visual and round-trip fidelity are not guaranteed. |
| PPTX | experimental/best effort | Package validity and supported slide text/shapes are tested. Masters, animations, charts, SmartArt, fonts, OLE, complex layouts, and editability may degrade. |
| XLSX | experimental/best effort | Package validity and supported cell/table content are tested. Formulas, charts, pivots, conditional formatting, macros, embedded objects, exact sizing, and round trip may degrade. |
| Tables | best effort | Logical rows/cells are preserved where parsed; merged cells, borders, widths, styles, and nested layouts vary by format. |
| Charts and SmartArt | unsupported semantically | They may survive only as opaque OOXML parts or visual assets; no editable AST conversion is promised. |
| OLE and embedded objects | unsupported semantically | No OLE activation or editable object round trip is provided; strict workflows must reject diagnostics. |

Package validity, semantic preservation, layout preservation, visual fidelity,
editability, and round-trip fidelity are independent claims. Passing a ZIP/package
reopen test proves only structural validity unless the relevant row states more.

The executable golden-corpus gate is:

```bash
cargo run -p latexsnipper-fidelity --bin fidelity-check -- \
  run --index fidelity/corpora/index.json --repository-root . \
  --source-commit local --generated-at-utc unspecified \
  --output target/fidelity/report.json
```

It reports `structuralValidity`, `semanticPreservation`, `layoutPreservation`,
`visualFidelity`, `editability`, and `roundTripFidelity` separately. Its seven
layers cover reopen, semantic AST, expected diagnostics, assets, opaque parts,
optional rendering, and application-specific smoke. A missing renderer or Office
application is recorded as `skipped`; it never upgrades visual fidelity to passed.
The repository-generated DOCX/PPTX/XLSX/PDF corpora are checksum-pinned and cover
the representative features listed in `fidelity/corpora/index.json`.

## Export policy

Binary artifacts use `GeneratedContent::Binary(Vec<u8>)`; no PDF, PNG, DOCX,
PPTX, or XLSX bytes pass through a UTF-8 string. `ExportArtifact` reports MIME,
SHA-256, byte size, assets, and diagnostics. PDF overlay appends content streams
and merges resources instead of replacing original page contents.

Unsupported AST nodes must produce a structured diagnostic and a visible,
structured, visual, or opaque fallback. Consumers using strict fidelity should
reject warning/error diagnostics or set strict import options.

## Compatibility and migration

`ExportArtifact.text` remains as a compatibility adapter for text exporters.
New callers should read `ExportArtifact.content` or use `as_bytes()`/`write_to()`.
Synchronous OCR SDK entry points execute work on a dedicated OS thread so callers
inside an existing Tokio runtime do not trigger nested-runtime panics.

WASM semantic conversion is stable. WASM recognition, progress/cancellation, and
model-memory management remain experimental. Chrome and Firefox execute deterministic
text, table, and handwriting fixtures through Tract and the document pipeline. The
table profile uses a built-in projection structure backend when no compatible structure
model is loaded and requires a validated text recognizer. The handwriting profile
requires a TrOCR encoder, decoder, tokenizer, preprocessing metadata, and output schema.
A production TrOCR encoder/decoder also compiles and executes through Tract in the
opt-in real-model test. These tests prove runtime compatibility, not OCR accuracy.
TATR is Tract-compatible but exceeds the default browser hard-timeout budget, while
current SLANet exports require an unsupported ONNX Loop; neither is a default browser
profile. Native binary exporters are not linked into the browser target.

Built-in Rust plugin ordering, failure policies, transactional patches, cooperative
soft deadlines, quarantine, and bounded concurrency are stable. The versioned
isolated-process host provides process-group or Job Object termination, memory
limits, and a response-file observation limit for reviewed local process plugins.
Its permission model governs brokered host operations and is not a native OS
filesystem/network sandbox. Windows still has a pre-Job-assignment race, and total
workspace disk use is not quota-enforced. Native dynamic-library ABI and WASI
Component hosts remain unavailable. Remote plugin installation remains disabled
until its registry, signature, provenance, and update-channel trust model is
complete.
