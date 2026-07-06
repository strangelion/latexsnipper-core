# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **Region Graph Routing**: Only `Independent` regions are routed to top-level recognizers; `Child` and `Discarded` regions excluded to prevent duplicate output in table cells
- **Region Graph Projection**: `ArtifactRef` replaces array-index projection for layout/formula/text/table candidates
- **Region Graph Import**: `RegionResolveNode` imports existing `region_candidates` from `LayoutNode` before adding detector results
- **TextRecognitionService**: `recognize_via_package()` delegates to shared `TextRecognitionService`, eliminating dual session paths
- **Recursive CJK/Latin Normalization**: `normalize_block_inlines()` handles Paragraph, Heading, Table cells, List items, Quote, DescriptionList, and Handwriting
- **Engine Pipeline Activation**: `build_pipeline()` constructs OpenDocHybrid pipeline when `parse_mode == OpenDocHybrid`
- **Engine Layout Registration**: `try_register_layout_package()` discovers layout variant from manifest automatically
- **Engine OpenOcrText Mode**: `configure_context()` auto-selects `openocr-mobile` variant when mode is `OpenOcrText`
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

## [0.1.0] 07.01

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
