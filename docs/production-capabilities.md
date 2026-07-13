# Production capability and fidelity policy

The executable source of truth is `DocumentImporter::supported_formats()`,
`DocumentExportService::supported_formats()`, and
`DocumentExportService::capability_matrix()`. Use:

```bash
snipper capabilities --format json
snipper capabilities --format json --input docx --output png
```

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
hints return typed errors. OOXML imports enforce safe enclosed paths and total
decompressed-size limits. Encrypted OOXML packages return `EncryptedFile`.
Unsupported SVG elements retain the original SVG source asset and emit
`W_UNSUPPORTED_FEATURE`. Office unknown parts are retained only with
`ImportOptions::preserve_unknown_parts`.

Native PDF extraction is inherently best effort because arbitrary font encodings,
missing `ToUnicode` maps, complex graphics-state transforms, and reading order can
be ambiguous. Scanned pages require OCR models and a rendering dependency.

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
