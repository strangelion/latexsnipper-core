# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
