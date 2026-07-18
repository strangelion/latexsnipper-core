# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.1] - 2026-07-19

### Fixed

- Normalize PyTorch-exported `floor(...)` ONNX dimension annotations into
  Tract-compatible integer expressions, including nested feature-pyramid shapes.
- Read concrete input shapes from model-package `config.json` in the in-memory
  resolver, without coupling the runtime to a model category, variant, or size.
- Keep dynamic and partially unknown third-party model shapes dynamic.
- Decode browser ONNX artifacts without prematurely importing and optimizing
  their graphs before the package metadata has been committed.

## [3.0.0] - 2026-07-18

### Added

- Independent v3 API-envelope, plugin-manifest, and model-manifest contract types.
- Structured migration outcomes with machine-readable warnings and explicit manual-action status.
- Strict v2 plugin migration that rejects native ABI and ambiguous reserved WASI contracts.
- Strict v2 model migration that requires exact per-artifact SHA-256 values and leaves profiles unavailable until evidence is authored.
- Core 3 public API inventory, schema plan, breaking-change guide, migration guide, and compatibility policy.
- Fuzz coverage for v3 API envelopes, plugin/model manifests, and migration functions.
- Default-deny WASI Component host with WIT v1 lifecycle, typed brokers, hard interruption,
  bounded package verification, and structured diagnostics.
- Ed25519-threshold signed registries with rollback/freeze protection, HTTPS same-origin
  downloads, bounded archive validation, atomic remote WASI installation, and rollback.
- Registry and remote-plugin CLI commands with explicit trust-root confirmation and
  machine-readable trust, revocation, expiry, quarantine, and compatibility states.
- Browser table recognition with a bounded projection structure fallback, per-cell OCR
  confidence, merged-cell geometry preservation, and artifact-derived readiness.
- Browser handwriting recognition through TrOCR encoder/decoder/tokenizer artifacts,
  including production-model Tract execution and a bounded WASM decode profile.
- Browser-side model, image, queue, task-duration, table-element, result, cache, and
  download budgets with pre-allocation validation where possible.
- Version-aware plugin and model manifest loaders that reject unknown schemas and
  expose only runtime-safe v3 model profiles.
- CLI migration commands for plugin manifests, model manifests, and documents,
  including bounded input, source preservation, and explicit manual-action exits.
- Callable WASM v3 API-info, capability, and conversion exports with matching
  TypeScript envelope declarations while retaining explicit v2 adapters.
- Additive FFI response-version queries and independent contract-version metadata.
- Explicit enable/disable, verified capability registration, and a re-verifying
  execution bridge for signed remote WASI plugins. Activation binds the registry
  snapshot to the host-verified manifest and artifact; every invocation rejects
  plugins disabled, revoked, updated, or replaced after activation.
- Portable `RenderBundle` rendering APIs with vector/raster preference and SVG validation.
- Runtime validation for untrusted Web Worker request messages.
- Fidelity corpus and structural validation for DOCX, PPTX, and XLSX packages.
- Stable release validation, artifact verification, supply-chain metadata, and publication gates.

### Changed

- Workspace crates and the WASM npm package are staged at `3.0.0-alpha.1`.
- The unchanged `Document` schema version `1.0.0` is now a shared constant.
- WASI Component authority is enforced with capability-directory filesystem access,
  host-owned resource ceilings, exact runtime/manifest capability matching, and a
  RustSec-clean Wasmtime 36.0.12 runtime.
- Tract 0.21.10 is vendored with a narrowly scoped inference-to-typed compatibility
  fix for dimension tensors represented as typed `I64` values.
- The generated capability matrix now uses schema `3.0.0`; CLI JSON output defaults
  to the v3 envelope and requires `--api-version 2` for the legacy shape.
- Workspace crates and supported package surfaces are released as version `3.0.0`.
- Official GA binary targets are Windows x86_64, Linux x86_64, and Apple Silicon macOS.
- Office Open XML exporters use corrected package relationships, content types, themes,
  slide layouts, spreadsheet styles, and block-level OMML placement.

### Fixed

- WASM v3 envelopes serialize JSON maps as plain JavaScript objects, matching
  the TypeScript declarations and making fields such as `data.schemaVersion`
  available through normal property access in browsers and Node.js.
- Fixed DOCX block-level display OMML placement that could prevent Word from opening generated files.
- Fixed XLSX table/header/merged-cell structures that caused Excel repair prompts.
- Fixed PPTX theme content types, presentation properties, slide-layout IDs, master/layout
  structure, and related package validity issues that caused PowerPoint repair prompts.
- Fixed malformed Worker messages being trusted as `WorkerRequest` values at runtime.

### Compatibility

- Existing WASM API v2, capability v2, plugin/model runtime, process IPC v1, Worker protocol v1, CLI, and FFI behavior remain unchanged. JSON AST text runs may now include an optional `confidence` field.
- The `3.0.0-alpha.1` source version is an internal development identifier only;
  no alpha, beta, or RC package is required or published on the path to 3.0.0 GA.
- Synchronous WASM recognition remains on its asynchronous v2 endpoint because its
  Worker progress/cancellation protocol is independently versioned and unchanged.
- Worker protocol remains version 1.
- Existing explicitly retained v2 WASM compatibility APIs remain available.
- Document schema version remains independently versioned.

## [2.0.0]

### Added

- Signature-first unified import for paths and buffers across raster images, SVG, PDF,
  Office OOXML, Markdown, HTML, LaTeX, Typst, MathML, OMML, JSON AST, and plain text.
- Binary-safe PDF/PNG/DOCX/PPTX/XLSX artifacts with MIME, SHA-256, and exact byte size.
- Reopen-tested OOXML package writers, append-only PDF overlay, generated capability registry,
  structured fidelity diagnostics, and runtime provider diagnostics.
- ONNX execution-provider registration for CUDA, DirectML, CoreML, and CPU fallback on
  Windows, Linux, and macOS.
- Cross-platform CI checks and a checksum-verified Windows real-model test job.

### Changed

- `Snipper::from_file` now routes through the unified importer instead of assuming an image.
- Synchronous OCR SDK APIs execute on a worker thread and are safe inside an existing Tokio runtime.
- Visual and Office exporters are documented as experimental/best-effort instead of being
  described as universally lossless.

### Compatibility

- `ExportArtifact.text` remains available for text callers; new code should use
  `ExportArtifact.content`, `as_bytes()`, or `write_to()`.

## [2.0.0] - 2026-07-08

### Migration Guide from 1.0.x

This release contains significant API changes. Key migration items:

#### Block Enum Expansion (18 new variants)
- `Block` grew from ~12 variants to **30 variants**: PageBreak, SectionBreak, HeaderFooter, Bibliography, FormField, Revision, ChemicalFormula, QrCode, Graph, etc.
- All match arms on `Block` must be updated. Use `cargo build` to find missing arms — each missing arm produces a compiler error.

#### Inline Enum Expansion (6 new variants)
- `Inline` added: Anchor, CrossReference, CitationGroup, NoteRef, LineBreak, SoftBreak, Span, Link, Code, Superscript, Subscript.
- Code with exhaustive `match inline` will fail to compile. Use `cargo build` to locate missing arms.

#### ListBlock / ListItem Breaking Changes
- `ListBlock.ordered: bool` → `ListBlock.style: Option<ListStyle>` with `is_ordered()` accessor.
  Migration: `if l.ordered` → `if l.is_ordered()`.
- `ListItem.inlines: Vec<Inline>` → `ListItem.content: Vec<Block>`.
  Migration: wrap inline content in `Block::Paragraph(...)` or use block-level content.

#### TableBlock Breaking Changes
- `TableBlock.rows: Vec<Vec<TableCell>>` → `Vec<TableRow>`.
  Each `TableRow` has `cells: Vec<TableCell>`, `height`, `is_header`.
  Migration: `for row in &table.rows { for cell in row { ... } }` → `for row in &table.rows { for cell in &row.cells { ... } }`.
- `TableCell.inlines: Vec<Inline>` → `TableCell.content: Vec<Block>`.
  Use `cell.collect_inlines()` helper for temporary compatibility.

#### StageRunner API Change
- `StageRunner::run(&self, spec)` → `StageRunner::run(&self, spec, job_root)`.
  All stage implementations must be updated to accept `job_root: &JobRoot`.

#### MediaAsset Field Rename
- `MediaAsset.checksum_sha256: Option<String>` renamed to `checksum: Option<String>`.
  This affects all construction sites (docx_reader, pptx_reader, html_parser, etc.).

#### ArtifactKind Enum Expansion
- `ArtifactKind::Source` → `ArtifactKind::SourceDocument`.
- New variants: `SourceImage`, `ExtractedAsset`, `ProviderRaw`, `Other`.

#### TextRun Struct Changes
- `TextRun` added `style: Option<TextStyle>` field alongside legacy `bold`/`italic`/`underline`/`strikethrough`.
  Struct literal constructions must add `style: None`.

#### Deprecations
- `Document::normalize_assets()` replaces manual legacy image migration.
- `FigureBlock.caption` deprecated — use `caption_inlines` or `caption_inlines_or_legacy()` accessor.
- `Inline::Footnote` deprecated — use `Inline::NoteRef` + `Document.notes`.
- `TextBoxBlock.rotation_deg`/`z_index` deprecated — use `transform`/`layer` fields via `effective_transform()`/`effective_layer()` accessors.
- `LinkInline.target: String` read via `target_string()` accessor (respects new `link_target` field).

For full details, see the per-crate documentation and docs/ast.md.

### Added

- **Contract Deduplication (Phase A)**
  - Unified `ModelProviderKind`, `FidelityLevel`, `FormatCapability` — single canonical definition in `latexsnipper-ast`
  - Moved `SemanticFormat`/`ExportFormat`/`TargetFormat` to `latexsnipper-ast` as platform contracts
  - Removed duplicate `Exporter` trait from conversion crate (dead code)
  - Split `AssetResolver` into `AssetStore` + `AssetReferenceResolver` + `AssetExporter` three-layer traits
  - `ExportFormat`, `SemanticFormat`, `TargetFormat` now exported from `latexsnipper-ast::format`

- **Asset Normalization (Phase B)**
  - `Document::add_asset()`, `Document::get_asset()` — asset management convenience methods
  - `Document::validate_asset_refs()` — detects dangling `asset_id` references, returns diagnostics
  - `Document::migrate_legacy_image_data()` — promotes legacy `image_data` to `MediaAsset` entries
  - `Block::inlines_mut()` — mutable inline accessor for block traversal
  - DOCX/PPTX readers now create `MediaAsset` entries and use `asset_id` instead of raw `image_data`
  - `guess_image_format()` helper for file extension → `AssetFormat` resolution

- **Full Block Coverage (Phase C)**
  - `RenderNode::Unsupported { block_type, message }` — visible degradation instead of silent drop
  - Markdown/HTML/LaTeX/Typst converters now produce visible placeholders for:
    `ChartBlock`, `ShapeBlock`, `EmbeddedObjectBlock`, `AnnotationBlock`
  - HTML converter: type-aware `<div class="chart/shape/...">` with `title` attribute
  - RenderTree emits `Unsupported` for chart/shape/embedded/annotation (visible in SVG/PDF/Text)
  - SVG/PDF/Text export generators handle `RenderNode::Unsupported`

- **StageSpec Execution Loop (Phase D)**
  - `StageRunner` trait in `ast::traits` with `kind()` and `run()` methods
  - `PipelineManifest::from_stage_specs()` — bridges job contract specs to pipeline execution
  - `JobRoot::ensure_dirs()` — creates standard 11-directory job tree
  - `DecodeStage`, `RecognizeStage`, `ConvertStage`, `ExportStage` — concrete runner implementations

- **Office/PDF Utility Loop (Phase E)**
  - 12 diagnostic code constants: `W_SMARTART_NOT_SUPPORTED`, `W_OLE_NOT_SUPPORTED`,
    `W_CHART_DATA_SIMPLIFIED`, `W_MEDIA_NOT_SUPPORTED`, `W_ACTIVEX_NOT_SUPPORTED`,
    `W_FORM_FIELD_NOT_SUPPORTED`, `W_REVISION_NOT_FULLY_PRESERVED`, `W_BLOCK_DOWNGRADED`,
    `I_LEGACY_IMAGE_MIGRATED`, `W_MISSING_ASSET_REF`, `E_API_CALL_FAILED`, `E_SCHEMA_VALIDATION_FAILED`
  - DOCX reader detects SmartArt (`mc:AlternateContent`), OLE (`o:OLEObject`), Charts (`c:chartSpace`) — emits diagnostics

- **API/VLM Stabilization (Phase F)**
  - `RemoteApiResult::is_usable()` now includes `schema_valid` check
  - `is_usable_for_profile()` — per-profile usability considering optional schema
  - `send_request()` returns `ApiRawResponse` struct with content + token usage from raw body
  - HTTP status-aware error mapping: 401→`E_API_AUTH`, 429→`E_API_RATE_LIMIT`, timeout→`E_API_TIMEOUT`
  - `ProviderReport` always has populated `calls` (even on error paths — upload blocked, payload build fail)
  - `UploadScope` enum (`CroppedRegion`, `PageImage`, `WholeDocument`) with `UploadPolicy::allows()`
  - `ApiRawResponse` extracted to module scope for `cargo fmt` compliance

- **P0 Type Additions**
  - `TextDirection` (`Ltr`/`Rtl`/`Auto`) and `UnderlineStyle` (`Single`/`Double`/`Dotted`/`Dashed`/`Wavy`) enums in style.rs
  - `Transform2D` struct with rotation/scale/translate/skew fields in style.rs
  - `LayerInfo` struct with z_index/locked/hidden/group_id fields in style.rs
  - `TextRun.style: Option<TextStyle>` — style field alongside legacy bold/italic/underline/strikethrough
  - `NoteKind` (`Footnote`/`Endnote`), `NoteRefInline`, `NoteDefinition` for structured footnotes
  - `FigureBlock::caption_inlines_or_legacy()` and `caption_plain_text()` accessor methods
  - All converters (Markdown/HTML/LaTeX/Typst/RenderTree) updated to use caption accessors
  - `PipelineDiagnosticEvent → ast::Diagnostic` From impl for cross-system diagnostic mapping
  - `DocumentReport::with_stage_reports()` / `with_provider_reports()` chaining
  - `CapabilityMatrix::query(input, output)` and `explain_loss(input, output)` methods
  - `effective_text_style()` with `merge_style()` — style inheritance rules
  - `ParagraphBlock.style: Option<ParagraphStyle>` — paragraph-level style support

- **P0 Page Layout / List / Table Types**
  - `PageLayout`, `PageMargin`, `PageOrientation`, `ColumnLayout` — page layout descriptors
  - `PageBreakBlock`, `SectionBreakBlock`/`SectionBreakKind` — page/section break Block variants
  - `HeaderFooterBlock`/`HeaderFooterKind`/`HeaderFooterScope` — header/footer Block variant
  - `ListStyle`/`BulletStyle`/`NumberingStyle` — structured list styling (disc/circle/square/decimal/roman etc.)
  - `ListBlock` now uses `style: Option<ListStyle>` + `start: Option<u32>` replacing `ordered: bool`
  - `ListItem.inlines: Vec<Inline>` → `content: Vec<Block>` — multi-block list item support
  - `TableBlock.rows`: `Vec<Vec<TableCell>>` → `Vec<TableRow>` with per-row `height`/`is_header`
  - `TableCell.inlines: Vec<Inline>` → `content: Vec<Block>` — table cells support block-level content
  - `TableColumn`, `CellDataType` (`Text`/`Number`/`Boolean`/`Date`/`Formula`), `TableStyle`, `TableCellStyle`
  - `Document.notes: Vec<NoteDefinition>` — footnote/endnote storage on Document
  - Updated 18+ downstream converter/engine/example files for new type layouts

- **P1 Type Additions**
  - `AnchorInline` — inline bookmark/anchor for Office/HTML/PDF cross-references
  - `CrossReferenceInline`/`CrossReferenceKind` — structured cross-references
  - `CitationGroupInline`/`CitationItem` — multi-citation support with prefix/suffix/locator
  - `FormFieldBlock`/`FormFieldKind` — PDF/Word form field support (text/checkbox/radio/dropdown etc.)
  - `BibliographyBlock`/`BibliographyEntry` — structured bibliography with entry types
  - `Revision`/`RevisionKind` — tracked changes support (inserted/deleted/moved/format)
  - `AccessibilityInfo` — alt_text/title/description/decorative/reading_order
  - `LinkTarget` enum (`Url`/`InternalAnchor`/`Email`/`File`/`Custom`)
  - `DocumentOutline`/`TocEntry` — table-of-contents hierarchy
  - All converters updated with visible placeholders for new Block/Inline variants

- **P2 Type Additions**
  - `ChemicalFormulaBlock` — chemical formula (mhchem-style) support
  - `QrCodeBlock` — QR code / barcode block
  - `GraphBlock`/`DataPoint`/`GraphType` — data graph (bar/line/pie/scatter/area)
  - `VectorPath`/`PathCommand`/`ShapeGroup` — vector path operations
  - `AudioAsset`/`AudioFormat` — embedded audio support
  - `VideoAsset`/`VideoFormat` — embedded video support

- **v5 Field & Type Completion**
  - `Length`/`LengthUnit` — typed measurement with Pt/Px/Emu/Mm/Cm/Inch/Percent/Em/Ex units
  - `TextStyle` expanded: `underline_style: Option<UnderlineStyle>`, `language`, `direction`, `letter_spacing`
  - `TableStyle`/`TableCellStyle` filled with border/alignment/banding/background fields
  - `TableBorder`/`BorderSide` — per-side border configuration
  - `Page.layout: Option<PageLayout>` + `Page.background_asset_id` — page layout linked to Page
  - `FormulaBlock` — `label`/`number`/`environment` fields + `FormulaEnvironment` enum
  - `EmbeddedObjectBlock` — `prog_id`/`class_id`/`storage_ref`/`display_as_icon` Office fields
  - `Transform2D`/`LayerInfo`/`AccessibilityInfo` applied to 8+ visual Block types
  - `RegionKind` expanded from 13 to 42 variants (Photo/Diagram/Chart/CodeBlock/FormField/QrCode etc.)
  - `Inline::NoteRef(NoteRefInline)` — new inline variant for footnote/endnote references
  - `LinkInline.link_target: Option<LinkTarget>` — typed link target alongside legacy `target`
  - `Document.outline: Option<DocumentOutline>` — document outline/toc hierarchy on Document
  - `SemanticFormat`/`ExportFormat`/`TargetFormat` — contract deduplicated via ast re-export
  - Markdown parser (`![alt](src)`) and HTML parser (`<img>`) now create `MediaAsset` entries
  - `RenderTree.diagnostics: Vec<Diagnostic>` — unsupported blocks emit `W_BLOCK_DOWNGRADED`

- **v6 Accessor & Orchestration (PR 1–5)**
  - **PR1**: `ShapeBlock`/`AnnotationBlock`/`ChemicalFormulaBlock` — unified transform/layer/accessibility fields
  - **PR2**: `TableCell::effective_style()` + `legacy_style_as_table_cell_style()`, `LinkInline::target_string()` + `effective_target()`, `Document::migrate_inline_footnotes_to_notes()` — old/new field accessor methods
  - **PR3**: `DocumentConverter::convert_artifact()` — semantic converters return `ExportArtifact` (with text + diagnostics + assets)
  - **PR4**: `NormalizeAssetOptions` + `Document::normalize_assets()` — 5-stage asset normalization (migrate/infer/dedup/checksum/validate)
  - **PR5**: `StageOrchestrator` — registers runners, writes `reports/<id>.report.json`, appends `logs/events.jsonl`, updates `artifacts/artifacts.json`

- **v7 StageRuntime & Asset Ref Closure**
  - `ArtifactKind` expanded to 14 variants with `from_stage_kind()` helper (Decode→DecodedImage, Export→ExportedFile, etc.)
  - `StageOrchestrator::run_stage()` now calls `job_root.ensure_dirs()`, propagates write errors, sets dynamic artifact kind
  - `run_spec_file()` delegates to `run_stage()` — consistent manifest/event/report writing
  - `normalize_assets()` dedup now tracks `old_id→kept_id` remap and calls `rewrite_asset_refs()`
  - `Document::rewrite_asset_refs()` — recursive walk of 9 Block types + Inlines to rewrite dangling asset references
  - `ConversionOutput` struct (`text`/`diagnostics`/`exported_assets`) — structured converter return type
  - `FigureBlock` converter output: `asset_id` priority over legacy `image_data` (Markdown/HTML/LaTeX/Typst)
  - `TextBoxBlock::effective_transform()`/`effective_layer()` — legacy-to-new field accessors
  - `AnnotationBlock` + `accessibility: Option<AccessibilityInfo>` — field consistency
  - `ConvertStage`/`ExportStage` — now record real elapsed time and input source diagnostics

- **v8 Asset Visitor & Runtime Closure (PR 1–6)**
  - **PR1**: `Document::visit_asset_refs()`/`visit_asset_refs_mut()`/`collect_asset_refs()` — unified asset reference visitor covering Page/Chart/EmbeddedObject/SourceInfo; `validate_asset_refs()` and `rewrite_asset_refs()` refactored to use visitor (eliminates duplicate block-walk code)
  - **PR2**: `MediaAsset.checksum_sha256` renamed to `checksum` — now computed via SHA-256 with base64 decode; updated 5 reader/parser files (docx/pptx/html/markdown/asset_resolver)
  - **PR3**: `ExportService::export()` now returns `tree.diagnostics` + `doc.diagnostics` instead of `Vec::new()` — `W_BLOCK_DOWNGRADED` warnings reach final `ExportArtifact`
  - **PR4**: `ConvertStage` real execution — reads Document AST JSON from `spec.input.source`, calls `DocumentConverter::convert_artifact()` with target format from `spec.options`, writes converted output file
  - **PR5**: `ExportStage` real execution — reads Document AST JSON, calls `ExportService::export()` with visual format from `spec.options`, writes exported output file
  - **PR6**: `collect_converter_diagnostics()` — recursively walks Document identifying 10 placeholder-rendered block types (Chart/Shape/EmbeddedObject/Annotation/FormField/ChemicalFormula/Graph/QrCode/Bibliography/HeaderFooter), emits `W_BLOCK_DOWNGRADED`; `convert_artifact()` now merges 3 diagnostic sources (doc + RenderTree + converter)

- **v9 Stage Runtime & Asset Final Mile (PR 1–5)**
  - **PR1**: `StageRunner::run(spec, job_root)` — added `JobRoot` parameter; ConvertStage/ExportStage write to `job_root.converted_dir`/`job_root.exported_dir`; write failures propagate errors; manifest path uses real file path
  - **PR2**: `StageProducedArtifact` struct (kind/path/mime/format/checksum/size); `StageReport.produced_artifacts` field; Orchestrator writes manifest from produced artifacts
  - **PR3**: `simple_base64_decode()` — std-only base64 decoder; `resolve_asset_bytes()` now decodes InlineBase64 before hashing; `SimpleAssetResolver::get_bytes()` returns decoded bytes (fixes .png/.jpg export corruption)
  - **PR4**: AssetRef visitor full coverage — added `Table.rows[].cells[].content[]`, `List.items[].content[]`, `HeaderFooter.content[]`, `Shape.text`, `Chart.title`, `Figure.caption_inlines`, `Annotation.content`, `FormField.label` to both immutable and mutable visitors
  - **PR5**: `DecodeStage` MVP — reads source → writes `job_root.decoded_dir/source.ext`; `RecognizeStage` MVP — reads Document JSON → writes `job_root.ast_dir/document.ast.json`

- **v10 Stage Data Plane Final Mile (PR 6–10)**
  - **PR6**: All 4 StageRunners now fill `produced_artifacts` with real kind/path/mime/format/checksum/size — Orchestrator manifest no longer relies on legacy fallback
  - **PR7**: Stage status semantics — no output = `Failed`, passthrough unsupported = `Failed` with diagnostic
  - **PR8**: `ArtifactKind::from_output_or_stage()` — `spec.output.artifact_kind` string takes priority over `from_stage_kind()` mapping
  - **PR9**: `visit_inline_asset_refs()`/`_mut()` — recursive Spans/Links/Footnotes/Superscripts/Subscripts; block visitors delegate to inline visitor for all inline collections
  - **PR10**: `simple_base64_decode()` — data URI prefix stripping, whitespace filtering, URL-safe base64 (`-`/`_`→`+`/`/`), invalid char → `Err`; applied to both `ast` and `SimpleAssetResolver`

- **Office/PDF Import Deepening**
  - `docx_reader` — heading detection via `w:pStyle` → `Block::Heading` with parsed level; list detection via `w:numPr` → `Block::List` with accumulated `ListItem`s
  - `pptx_reader` — table parsing via `a:tbl`/`a:tr`/`a:tc` → `TableBlock` with cell text extraction
  - `xlsx_reader` — `CellDataType` population (Boolean/Date/Text/Formula), formula extraction via `<f>` tag, column width parsing via `<col>` tags

### Changed
- `FigureBlock.caption` deprecated in favor of `caption_inlines: Option<Vec<Inline>>`
- `RecognizeInput::Image(String)` → `RecognizeInput::Image(SnipperImageDescriptor)`
- `ParagraphBlock` now has `style: Option<ParagraphStyle>` field
- `RemoteApiProvider` returns specific error codes based on HTTP status
- DOCX reader extracts images as `MediaAsset` entries with `asset_id`
- PPTX reader extracts images as `MediaAsset` entries with `asset_id`
- `ChartBlock`/`ShapeBlock`/`EmbeddedObjectBlock`/`AnnotationBlock` no longer silently dropped — produce visible placeholders with type info

### Fixed
- Bin (various): `ParagraphBlock` missing `style` field in 20+ files across workspace
- Clipboard `block_to_clipboard_html` dropped 10+ block types silently
- RenderTree filtered out Chart/Shape/EmbeddedObject/Annotation to `None`

### Changed
- Block enum variants expanded: TextBox, Chart, Shape, EmbeddedObject, Annotation
- Inline variants expanded: Span, Link, Code, Superscript, Subscript, LineBreak, SoftBreak
- Converters (Markdown/HTML/LaTeX/Typst) now use `doc.assets` for image resolution
- `RenderNode` now includes `Image` and `Figure` variants
- `RecognizerNode` fills SourceInfo with `confidence` and `region` from detections
- `Diagnostic` struct now has `recoverable: bool` and `data: serde_json::Value`
- `JobQueue::complete/fail` write structured `EventRecord` and `Diagnostic`
- **Clippy Fixes**: `derivable_impls`, `unnecessary_unwrap`, `match_like_matches_macro`, `upper_case_acronyms`, `option_as_ref_deref`, `map_or`→`is_none_or`, `needless_range_loop`
- **Model Packaging**: Removed duplicate `openocr-mobile` variants; updated manifest file lists to use `config.json`; added `layout_cdla/config.json`
- **Model Release Workflow**: Per-variant `sourceUrl` support replaces hardcoded `BASE_TAG`
- PipelineGraph execution order: nodes now execute in insertion order, not alphabetical
- recognize_pdf() missing RuntimeBackend injection
- Test paths hardcoded to non-existent model directories
- SDK duplicate inference logic (from_pdf returns clear NotImplemented error)
- CropNode now actually performs image cropping instead be a no-op
- ModelPackage executor `run()` methods now properly implement inference logic
- `\tableofcontents` command not parsed correctly (only handled as environment, not standalone command)
  - `\underline{text}` — underline text formatting, output to OMML/LaTeX/HTML/Typst
  - `\begin{description}` environment — definition lists with optional labels
  - `\footnote{text}` — footnote (OMML outputs `[^content]` placeholder)
  - `\label{key}`, `\ref{key}`, `\eqref{key}` — cross-references (OMML outputs `(?key)` placeholder)
  - `\cite{key}`, `\citep{key}`, `\citet{key}` — citations (OMML outputs `[key]` placeholder)
  - `\bibliography{file}` — bibliography reference (placeholder)
  - `\tableofcontents` — table of contents (outputs "目录" placeholder)
  - `\begin{theorem}`, `\begin{lemma}`, `\begin{proof}` etc. — theorem-like environments
  - `\begin{minipage}{width}` — minipage layout
  - `\begin{figure}`, `\begin{table}` — float environments

- **AST Extensions**
  - `TextRun.underline` and `TextRun.strikethrough` fields
  - `Inline::Footnote`, `Inline::Label`, `Inline::Reference`, `Inline::Citation` variants
  - `CiteStyle` enum (Plain/Author/Parenthetical)
  - `Block::DescriptionList`, `Block::TableOfContents`, `Block::Theorem`, `Block::Proof`, `Block::Minipage`, `Block::Float` variants
  - `DescriptionListBlock`, `DescriptionItem`, `TheoremBlock`, `ProofBlock`, `MinipageBlock`, `FloatBlock` structs

- **Input Parsers**
  - Enhanced Markdown parser: headings, paragraphs, bold/italic, code, lists, blockquotes, horizontal rules, display/inline math
  - New HTML parser (`html_parser.rs`): headings, paragraphs, bold/italic/underline, code, lists, blockquotes, horizontal rules, math

- **MathML Parser Enhancement**
  - `<menclose>` tag support with notation attribute (strikethrough, box)

- **Testing**
  - 4 new integration tests for LaTeX commands pipeline, OMML conversion, Markdown parser, HTML parser

### Changed
- TextRun now has 6 fields: text, bold, italic, underline, strikethrough, source
- Block enum expanded from 10 to 15 variants

### Added
- **Pipeline Architecture**
  - `PipelineArtifacts` strong-typed struct replacing string-keyed metadata
  - `ModelResolver` trait with `FsModelResolver` (native) and `MemoryModelResolver` (WASM)
  - `ReadingOrder` module with y-bucket + x tie-breaker sorting
  - `DiagnosticEvent` and `DiagnosticLevel` for pipeline diagnostics
  - `PostprocessNode` now implements reading order sorting
  - `PipelineContext.model_packages` field for registering ModelPackage instances
  - `utils.rs` shared module with `get_backend()`, `load_config()`, `resolve_model_handle()`, `get_or_create_session()`

- **Model Package Architecture**
  - `ModelPackage` and `ModelExecutor` traits for model abstraction
  - `ModelTask` enum (FormulaDetection, TextRecognition, etc.)
  - `ModelDescriptor`, `ModelInput`, `ModelOutput` types
  - `ModelRegistry` with TOML manifest support
  - `ValidationReport` for model integrity checking
  - `YoloV8DetectorPackage` with full `run()` implementation
  - `TrOcrFormulaPackage` with full `run()` implementation
  - `CrnnTextRecognizerPackage` with full `run()` implementation
  - Lazy session loading in executors (sessions created on first `run()` call)

- **Model Package Pipeline Integration**
  - `DetectorNode.detect_via_package()` — uses ModelPackage when registered
  - `RecognizerNode.recognize_via_package()` — uses ModelPackage when registered
  - Fallback to direct function calls when no package is registered
  - Engine `register_model_package()` and `get_model_package()` methods
  - Engine automatically registers packages with PipelineContext

- **api-types Crate**
  - New `latexsnipper-api-types` crate for cross-crate shared types
  - `RecognizeMode`, `RecognizeRequest`, `RecognizeResponse`, `StreamItem` types

- **Model Hot-Reload**
  - `CachedSession` version tracking and timestamps
  - `invalidate_session()` and `invalidate_all_sessions()` methods
  - Engine `reload_model()`, `reload_all_models()`, `has_model()` API

- **Model Validation**
  - SHA-256 checksum computation and verification
  - `CHECKSUMS.sha256` file loading
  - Batch model validation

- **Performance Metrics**
  - `RecognitionMetrics` with timing, detections, blocks, failures
  - `MetricsBuilder` for fluent metric construction
  - `SerializableMetrics` for JSON output
  - `NodeTimer` for per-node timing

- **PDF Rendering**
  - `decode_pdf()` and `decode_pdf_page()` now render pages
  - Support for `pdftoppm` (poppler-utils) and `mutool` (MuPDF)
  - Clear error messages with installation instructions

- **CLI Improvements**
  - `-v` / `--version` flag
  - `rec` alias for `recognize`
  - Workspace dependency inheritance for version management
  - Updated README with PDF support status

- **Testing**
  - Fixed test paths to match actual model locations
  - All 15 integration tests now pass with real models

### Changed
- `DetectorNode` and `RecognizerNode` use `ModelTask` instead of type-specific enums
- Pipeline nodes read from `PipelineArtifacts` instead of string-keyed metadata
- `collect_blocks_from_context()` simplified to use `artifacts.all_blocks()`
- Engine sorts blocks via `PostprocessNode` instead of inline sorting
- SDK moved from `latexsnipper_pipeline::sdk` to `latexsnipper_engine::sdk`
- `simple.rs` types renamed to `SimpleContext`, `SimpleRegion`, `SimpleCrop` to avoid naming conflicts
- Pipeline lib.rs re-exports reduced to only commonly used AST types

### Deprecated
- `latexsnipper_pipeline::sdk::Snipper` (use `latexsnipper_engine::sdk::Snipper` instead)
- `DetectorType` enum (use `ModelTask` instead)
- `RecognizerType` enum (use `ModelTask` instead)

---

## [1.0.0] 07.01

### Export
- PDF generator using printpdf 0.7 (replaces minimal hand-rolled PDF)
- Support for all document elements: headings, tables, lists, code, quotes, horizontal rules
- Page selection API: `filter_pages()`, `filter_page_numbers()`, `parse_page_range()`
- `DocumentConverter::convert_pages()` for partial document export

### CLI
- Added `--output` / `-o` flag for file export (e.g., `-o output.tex`, `-o output.typ`)
- Format auto-detection from file extension
- Helpful error messages with format suggestions (Levenshtein distance)
- Hint to use `-h` for help on invalid input

### AST
- Added `Document::filter_pages()` — filter by 0-based indices
- Added `Document::filter_page_numbers()` — filter by 1-based page numbers
- Added `Document::parse_page_range()` — parse "1-3,5,8-10" strings

### Architecture
- Stabilized four-layer architecture (Platform → Adapter → Core → Runtime)
- Established crate boundaries and module dependencies
- Defined Document AST as single source of truth

### Core
- Implemented Document AST data model
- Added platform-independent image processing
- Built async node-based pipeline with cancellation support

### Conversion
- LaTeX output format
- OMML output format
- MathML output format
- Typst output format
- Markdown output format
- HTML output format

### Syntax
- LaTeX parser and renderer
- Typst parser and renderer
- Markdown parser and renderer

### Inference (Experimental)
- YOLOv8 formula detection
- TrOCR formula recognition
- CRNN text recognition

### Runtime (Experimental)
- ONNX Runtime backend with session caching
- Stub backend for testing

### FFI (Experimental)
- Android JNI bindings
- iOS C FFI bindings

### WASM (Experimental)
- WebAssembly bindings for parse/render/convert

### CLI (Experimental)
- recognize command
- parse command
- render command
- version command

### Added
- Initial project structure
- Cargo workspace setup
