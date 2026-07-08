<div align="center">

# LaTeXSnipper Core

**One-stop solution from images to multi-format documents**

[![Rust](https://img.shields.io/badge/Rust-1.96+-orange?logo=rust)]()
[![License](https://img.shields.io/badge/License-AGPL--3.0-blue)]()
[![Status](https://img.shields.io/badge/Status-Core%20Pipeline%20Working-brightgreen)]()
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey)]()
[![Clippy](https://img.shields.io/badge/Clippy-0%20warnings-brightgreen)]()

**One line of code, images to LaTeX/Markdown/Typst**

[![About](assets/About.png)]()

[English](README.md) · [中文](README-CN.md)

</div>

---

## Quick Start

```rust
use latexsnipper_engine::sdk::Snipper;

// One line: image → detect → recognize → AST → export
let snipper = Snipper::from_file("input.png")?;

// Export to any format
let latex = snipper.to_latex()?;
let markdown = snipper.to_markdown()?;
let typst = snipper.to_typst()?;
let html = snipper.to_html()?;
let json = snipper.to_json()?;
```

### Output Examples

**Input**: Image containing formulas

**Output (LaTeX)**:
```latex
$$ E = m c ^ { 2 } $$

$$ \int _ { 0 } ^ { \infty } e ^ { - x ^ { 2 } } d x = \frac { \sqrt { \pi } } { 2 } $$
```

**Output (Markdown)**:
```markdown
$$ E = m c ^ { 2 } $$

$$ \int _ { 0 } ^ { \infty } e ^ { - x ^ { 2 } } d x = \frac { \sqrt { \pi } } { 2 } $$
```

**Output (Typst)**:
```typst
$ E = m c ^ { 2 } $

$ integral _ 0 ^ infinity e ^ - x ^ 2 d x = frac sqrt pi 2 $
```

---

## Core Capabilities

| Capability | Status | Details |
|------------|--------|---------|
| **Image → AST** | ✅ | YOLOv8 detection + TrOCR recognition |
| **AST → LaTeX** | ✅ | Full formula, table, list support |
| **AST → Markdown** | ✅ | MathJax compatible, image asset support |
| **AST → Typst** | ✅ | Native Typst syntax |
| **AST → HTML** | ✅ | MathJax rendering |
| **AST → MathML** | ✅ | Office compatible |
| **AST → OMML** | ✅ | Word compatible |
| **AST → SVG** | ✅ | Visual render via ExportService |
| **AST → PDF** | ✅ | Visual render via printpdf |
| **AST → XML** | ✅ | Fully typed JSON AST |
| **Markdown → AST** | ✅ | Headings, bold/italic, code, lists, math |
| **HTML → AST** | ✅ | Full tag support, MathJax compatible |
| **DOCX → AST** | ✅ | Word paragraphs, runs, images, tables |
| **PPTX → AST** | ✅ | PowerPoint slides, text, shapes, images |
| **XLSX → AST** | ✅ | Excel sheets, tables, strings |
| **PDF native → AST** | ✅ | Native text extraction via lopdf |
| **PDF overlay** | ✅ | Overlay AST text onto source PDF |
| **SVG → ShapeBlock** | ✅ | Parse SVG primitives to AST shapes |
| **Chart → ChartBlock** | ✅ | VLM-powered chart data extraction |
| **Diagram → Shape/Graph** | ✅ | VLM-powered diagram understanding |
| **Document→Report** | ✅ | `DocumentReport::from_document()` with block/confidence/asset summaries |
| **Capability query** | ✅ | `CapabilityMatrix::query()` / `explain_loss()` |
| **Asset normalization** | ✅ | `migrate_legacy_image_data()` promotes legacy data, `validate_asset_refs()` |
| **StageRunner trait** | ✅ | `DecodeStage`/`RecognizeStage`/`ConvertStage`/`ExportStage` implementations |
| **Job persistence** | ✅ | `JobRoot::ensure_dirs()` creates 11-directory job tree |
| **OOXML fragment** | ✅ | `write_ooxml_fragment()` — AST → Word body XML |
| **Clipboard bundle** | ✅ | HTML+RTF+PlainText+PNG multi-format |
| **Office insertion** | ✅ | Auto-select OMath/SVG/HTML per app |
| **FigureBlock caption** | ✅ | `caption_inlines_or_legacy()` / `caption_plain_text()` accessors |
| **Upload policy** | ✅ | `UploadScope` granular control via `UploadPolicy::allows()` |
| **Diagnostic codes** | ✅ | 12 standardized codes (SmartArt/OLE/Chart/media/API warnings) |
| **API error codes** | ✅ | 8 codes (auth, timeout, rate limit, schema, etc.) |
| **Schema validation** | ✅ | Two-stage (lightweight + feature-gated full) |
| **Pipeline diagnostics** | ✅ | `From<DiagnosticEvent> for ast::Diagnostic` mapping |
| **Capability matrix** | ✅ | `snipper capabilities` |
| **TextRun.style** | ✅ | `Option<TextStyle>` alongside legacy bold/italic/underline |
| **TextDirection/UnderlineStyle** | ✅ | Style enums for LTR/RTL/underline variants |
| **Transform2D/LayerInfo** | ✅ | 2D transforms and z-order for blocks |
| **NoteDefinition** | ✅ | Structured footnote/endnote with multi-block content |
| **PageLayout/PageMargin/PageOrientation** | ✅ | Page layout descriptors |
| **ListStyle/BulletStyle/NumberingStyle** | ✅ | Structured list styling |
| **ListItem.content: Vec\<Block\>** | ✅ | Multi-block list items |
| **TableRow/TableColumn/TableStyle** | ✅ | Enhanced table model |
| **TableCell.content: Vec\<Block\>** | ✅ | Block-level cell content |
| **Anchor/CrossReference Inline** | ✅ | Bookmark and cross-reference inlines |
| **FormFieldBlock** | ✅ | Form field support (text/checkbox/dropdown) |
| **BibliographyBlock** | ✅ | Structured bibliography entries |
| **Revision/TrackedChange** | ✅ | Revision tracking support |
| **ChemicalFormula/QrCode/Graph Block** | ✅ | Domain-specific block types |
| **DocumentOutline** | ✅ | Table-of-contents hierarchy |

---

## LaTeX Syntax Support

| Feature | LaTeX | OMML | HTML | Typst | Markdown |
|---------|-------|------|------|-------|----------|
| Bold `\textbf{}` | ✅ | ✅ | ✅ | ✅ | ✅ |
| Italic `\textit{}` | ✅ | ✅ | ✅ | ✅ | ✅ |
| Underline `\underline{}` | ✅ | ✅ | ✅ | ✅ | — |
| Footnote `\footnote{}` | ✅ | ⚡ | ✅ | ✅ | ✅ |
| Cross-ref `\ref{}` | ✅ | ⚡ | ✅ | ✅ | ✅ |
| Citation `\cite{}` | ✅ | ⚡ | ✅ | ✅ | ✅ |
| Description list | ✅ | ✅ | ✅ | ✅ | ✅ |
| Theorem/proof | ✅ | ✅ | ✅ | ✅ | ✅ |
| Minipage | ✅ | ✅ | ✅ | ✅ | — |
| Float (figure/table) | ✅ | ✅ | ✅ | ✅ | — |

⚡ = 占位符输出，需要 Word API 插入实际内容

---

## Why LaTeXSnipper Core?

**Not just another OCR engine.**

The core value of LaTeXSnipper Core is a **unified document AST**:

1. **Any input** → Images, clipboard, Office, PDF
2. **Unified AST** → Document / Block / Inline / Formula
3. **Any output** → LaTeX, Typst, Markdown, Office, Web

OCR is just one input source. Future Office plugins, clipboard listeners, and PDF parsers will all connect to the same AST.

> Take a photo of a math problem, and simultaneously output LaTeX, Typst, Markdown, and Word-compatible formats — all from the same API.

---

## Architecture

LaTeXSnipper Core follows a strict **four-layer architecture**:

| Layer | Responsibility |
|-------|---------------|
| **Platform** | UI, Camera, Permissions — belongs to each app |
| **Adapter** | JNI, WASM, Office.js, CLI — translates platform types to Core types |
| **Core** | AST, Inference, Pipeline, Conversion, Export — all business logic |
| **Runtime** | ONNX Runtime, Stub — interchangeable inference backends |

> Core never knows which platform is calling it. It only cares about input, processing, and output.

---

## Module Dependencies

```
Engine
  ├── Conversion (LaTeX/OMML/MathML/Typst/Markdown/HTML)
  ├── Export (SVG/Text/PDF)
  ├── Syntax (Parser + Renderer)
  ├── Pipeline (Node Graph)
  │     ├── Inference (Detection + Recognition)
  │     │     ├── Runtime (ONNX/Stub)
  │     │     └── Image (Decode/Resize/Normalize)
  │     └── AST (Document Data Model)
  └── Model (Manifest + Config)
        └── Foundation (Error/Log/Event/Config)
```

---

## Recognition Pipeline

![Pipeline](assets/pipeline.svg)

```
┌───────────────────────────────────────────────────────────────────────────┐
│                         Recognition Pipeline                              │
├───────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐             │
│  │  Decode  │──> │Normalize │──> │  Layout  │──> │  Region  │             │
│  │          │    │          │    │Detection │    │ Proposal │             │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘             │
│        │                                             │                    │
│        │            ┌────────────────────────────────┘                    │
│        │            │                                                     │
│        │            v                                                     │
│        │         ┌─────────────────────────────────────┐                  │
│        │         │                                     │                  │
│        │         │  ┌─────────────┐  ┌─────────────┐   │                  │
│        │         │  │   Formula   │  │    Text     │   │                  │
│        │         │  │ Recognition │  │ Recognition │   │                  │
│        │         │  │  (TrOCR)    │  │  (CRNN)     │   │                  │
│        │         │  └──────┬──────┘  └──────┬──────┘   │                  │
│        │         │         │                │          │                  │
│        │         │         └────────┬───────┘          │                  │
│        │         │                  │                  │                  │
│        │         └──────────────────┼──────────────────┘                  │
│        │                            │                                     │
│        │                            v                                     │
│        │                     ┌──────────┐                                 │
│        │                     │  Merge   │                                 │
│        │                     └────┬─────┘                                 │
│        │                          │                                       │
│        │                          v                                       │
│        │                  ┌──────────────┐                                │
│        └────────────────> │ Document AST │                                │
│                           └──────┬───────┘                                │
│                                  │                                        │
│                                  v                                        │
│                           ┌──────────┐    ┌──────────┐                    │
│                           │Conversion│──> │  Export  │                    │
│                           └──────────┘    └──────────┘                    │
│                                 │              │                          │
│                          LaTeX/OMML        SVG/Text/PDF                   │
│                          MathML/Typst                                     │
│                          Markdown/HTML                                    │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

---

## Features

### Stable

| Capability | Status | Details |
|-----------|--------|---------|
| **AST** | ✅ | Document → Page → Block → Inline → Formula |
| **Image** | ✅ | SnipperImage, ImageView, decode, resize, normalize |
| **Conversion** | ✅ | 6 formats: LaTeX, OMML, MathML, Typst, Markdown, HTML |
| **Syntax** | ✅ | LaTeX/Typst/Markdown Parser + Renderer |
| **Pipeline** | ✅ | DAG Node Graph, YAML/JSON Manifest, async with cancellation |
| **Region Graph** | ✅ | Conflict resolution, ArtifactRef routing, layout→recognition routing |
| **CJK/Latin Normalization** | ✅ | Recursive text normalization across all block types |

### Experimental

| Capability | Status | Details |
|-----------|--------|---------|
| **Inference** | ✅ | YOLOv8 detection + TrOCR formula recognition + CRNN+CTC text recognition |
| **Runtime** | ✅ | ONNX Runtime (session caching, GPU auto-detect) + Tract (WASM) + Stub |
| **Engine** | ✅ | SnipperEngine + JobQueue + Model hot-reload + SHA-256 validation + Metrics + ModelPackage |
| **Model** | ✅ | Manifest (TOML), Config, SHA256 verification, ModelRegistry |
| **ModelPackage** | ✅ | `ModelPackage`/`ModelExecutor` traits, lazy session loading, pipeline integration |
| **Pipeline** | ✅ | Node-based async pipeline with ModelTask abstraction + ReadingOrder + ModelPackage fallback |
| **Plugin** | ✅ | Plugin trait, Registry, TransformPlugin |
| **FFI** | ✅ | Android JNI + iOS C FFI |
| **WASM** | ✅ | Full parse/render/convert/recognize bindings with Tract backend |
| **CLI** | ✅ | recognize/parse/render/version commands, file export, format hints, minigame |
| **Export** | ✅ | SVG/Text/PDF with printpdf, headings, tables, lists, code, formulas, page selection |
| **Table Recognition** | ✅ | SLANet+ / TATR table structure + PP-DocLayout v3 layout detection |
| **Handwriting** | ✅ | Handwriting detection + TrOCR recognition + postprocessing |
| **Formula Layout** | ✅ | LaTeX AST parsing + symbol-level detection |
| **Multi-page** | ✅ | PDF decoding + multi-page pipeline; PDF rendering via pdftoppm/mutool |
| **PDF Rendering** | ✅ | Page rendering via pdftoppm (poppler) or mutool (MuPDF) |

---

## Workspace

```
crates/
├── foundation/     ✅ Error, Result, Logger, Config, EventBus
├── ast/            ✅ Document AST — single source of truth (incl. report, format, traits)
├── tensor/         ✅ Inference I/O tensors
├── image/          ✅ Platform-independent image processing + PDF rendering
├── runtime/        ✅ RuntimeBackend + InferenceSession + ModelResolver + ModelPackage + ModelRegistry + Validation
├── model/          ✅ Model manifest, config, SHA256 verification
├── inference/      ✅ Detection + Recognition pipelines + ModelPackage adapters
├── pipeline/       ✅ Node-based async pipeline + PipelineArtifacts + ReadingOrder
├── syntax/         ✅ LaTeX/Typst/Markdown Parser + Renderer
├── conversion/     ✅ AST → LaTeX/OMML/MathML/Typst/Markdown/HTML + DOCX/PPTX/XLSX readers
├── export/         ✅ RenderTree → SVG/Text/PDF (printpdf), page selection
├── engine/         ✅ SnipperEngine + JobQueue + Metrics + Hot-reload + SDK
├── api-types/      ✅ Public API types (RecognizeMode, Request, Response, StreamItem)
├── tract/          ✅ Tract-based WASM RuntimeBackend
├── plugin/         ✅ Plugin trait, Registry
├── mock/           ✅ Fake implementations for testing
├── ffi/            ✅ Android JNI + iOS C FFI
├── wasm/           ✅ WebAssembly bindings with Tract backend
├── cli/            ✅ CLI tool (recognize/parse/render/version/play) with job management
└── tests/          ✅ Integration tests (15+ tests with real models)
```

---

## Getting Started

### Install CLI

```bash
# From git (recommended)
cargo install --git https://github.com/strangelion/latexsnipper-core snipper

# Or build from source
git clone https://github.com/strangelion/latexsnipper-core
cd latexsnipper-core
cargo build --release -p latexsnipper-cli
```

### PDF Support

PDF page rendering requires one of these external tools:

```bash
# Linux
sudo apt install poppler-utils    # provides pdftoppm
# or
sudo apt install mupdf-tools      # provides mutool

# macOS
brew install poppler
# or
brew install mupdf

# Windows
choco install poppler
# or
choco install mupdf
```

### Use as Library

```toml
[dependencies]
latexsnipper-engine = "1.0"
```

```rust
use latexsnipper_engine::sdk::Snipper;

let snipper = Snipper::from_file("input.png")?;
let latex = snipper.to_latex()?;
```

### Custom Model Integration

```rust
use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
use latexsnipper_inference::adapters::YoloV8DetectorPackage;
use latexsnipper_runtime::{ModelId, ModelTask};
use std::sync::Arc;

// Create engine
let config = EngineConfig::with_models_dir("models".into());
let backend = OnnxRuntimeBackend::new("models".into())?;
let mut engine = SnipperEngine::new(config, Box::new(backend));

// Register custom model package (optional — falls back to built-in inference)
let package = YoloV8DetectorPackage::from_config(&config, ModelId::new("formula-det", "yolov8"))
    .with_model_path("models/formula-det/yolov8-mfd/mathcraft-mfd.onnx".into());
engine.register_model_package(ModelTask::FormulaDetection, Arc::new(package));

// Recognize — will use ModelPackage if registered, otherwise direct function calls
let doc = engine.recognize(image, RecognizeMode::Formula).await?;
```

### Run Examples

```bash
# Parse LaTeX
snipper parse --latex '$\frac{a+b}{c}$'

# Recognize from image
snipper recognize -i image.png -f latex -o output.tex

# Run all tests
cargo test --workspace
```

See [docs/getting-started.md](docs/getting-started.md) for details.

---

## Documentation

### Architecture

| Document | Description |
|----------|-------------|
| [architecture.md](docs/architecture.md) | Four-layer architecture overview |
| [pipeline.md](docs/pipeline.md) | Recognition pipeline design |
| [runtime.md](docs/runtime.md) | Runtime backend system |
| [engine.md](docs/engine.md) | Engine and job queue |

### Developer Guide

| Document | Description |
|----------|-------------|
| [getting-started.md](docs/getting-started.md) | Developer guide |
| [plugin.md](docs/plugin.md) | Plugin system |
| [testing.md](docs/testing.md) | Testing strategies |

### Reference

| Document | Description |
|----------|-------------|
| [ast.md](docs/ast.md) | Document AST specification |
| [syntax.md](docs/syntax.md) | LaTeX/Typst/Markdown parser |
| [conversion.md](docs/conversion.md) | 12 output formats |
| [conversion_guide.md](docs/conversion_guide.md) | Conversion guide with examples |

### Roadmap

| Document | Description |
|----------|-------------|
| [dual-track.md](docs/dual-track.md) | Development roadmap |

---

## Design Principles

- **Document First** — The document is the source of truth, not LaTeX or OCR
- **Composable** — Everything is a Node, everything is a Pipeline
- **Platform Independent** — Business logic in Rust, UI outside
- **Pluggable Runtime** — ONNX, TensorRT, NCNN — all interchangeable
- **Pluggable Models** — `ModelPackage` trait allows custom model integrations without changing pipeline code

---

## Models

LaTeXSnipper Core uses ONNX models for formula detection/recognition and text detection/recognition.

### Supported Models

| Model | Size | Purpose | Source | License |
|-------|------|---------|--------|---------|
| YOLOv8-MFD | ~66 MB | Formula detection | [Mathcraft](https://github.com/SakuraMathcraft/LaTeXSnipper) | MIT |
| TrOCR-DeiT | ~104 MB | Formula recognition (encoder+decoder) | [Microsoft TrOCR](https://huggingface.co/microsoft/trocr-base-handwritten) | MIT |
| PP-OCRv6 Det | ~10 MB | Text detection | [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | Apache-2.0 |
| PP-OCRv6 Rec | ~21 MB | Text recognition (18709 chars: CN/EN/math/greek) | [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | Apache-2.0 |
| OpenOCR Mobile Det | ~9 MB | Text detection (OpenOCR mobile DBNet) | [OpenOCR](https://github.com/Topdu/OpenOCR) | Apache-2.0 |
| OpenOCR Mobile Rec | ~21 MB | Text recognition (OpenOCR mobile CTC) | [OpenOCR](https://github.com/Topdu/OpenOCR) | Apache-2.0 |
| PP-DocLayout v3 | ~13 MB | Document layout analysis (10 categories) | [RapidAI/RapidLayout](https://github.com/RapidAI/RapidLayout) | Apache-2.0 |
| TATR Detection | ~34 MB | Table region detection (DETR-based) | [Microsoft Table Transformer](https://github.com/microsoft/table-transformer) | MIT |
| TATR Structure | ~34 MB | Table structure recognition (rows/cols/cells) | [Microsoft Table Transformer](https://github.com/microsoft/table-transformer) | MIT |
| SLANet Plus | ~7 MB | Table structure recognition (alternative backend) | [RapidAI/RapidTable](https://github.com/RapidAI/RapidTable) | Apache-2.0 |

### Model Directory Structure

```
models/
├── formula-det/yolov8-mfd/     # Formula detection (YOLOv8) — Stable
├── formula-rec/trocr-deit/     # Formula recognition (TrOCR) — Stable
├── text-det/v6-small/          # Text detection (PP-OCRv6) — Stable
├── text-det/openocr-mobile/    # Text detection (OpenOCR mobile) — Experimental
├── text-rec/v6-small/          # Text recognition (PP-OCRv6) — Stable
├── text-rec/openocr-mobile/    # Text recognition (OpenOCR mobile) — Experimental
├── layout/
│   └── pp-layout-cdla/         # Document layout analysis (CDLA) — Stable
├── table-det/
│   ├── tatr-detection/         # Table detection (TATR) — Experimental
│   └── doclayout-v3/           # Document layout analysis (PP-DocLayout) — Experimental
├── table-struct/
│   ├── tatr-structure/         # Table structure (TATR) — Experimental
│   └── slanet-plus/            # Table structure (SLANet) — Experimental
└── doc-ori/                    # Document orientation classification — Experimental
```

### Model Support Status

| Model | Status | Default | Release |
|---|---|---|---|
| YOLOv8-MFD | Stable | Yes | models-v2.0.0 |
| TrOCR-DeiT | Stable | Yes | models-v2.0.0 |
| PP-OCRv6 Det (v6-small) | Stable | Yes | models-v2.0.0 |
| PP-OCRv6 Rec (v6-small) | Stable | Yes | models-v2.0.0 |
| OpenOCR Mobile Det | Experimental | No | models-v2.0.0 |
| OpenOCR Mobile Rec | Experimental | No | models-v2.0.0 |
| PP-DocLayout v3 | Experimental | No | models-v2.0.0 |
| TATR Detection | Experimental | No | models-v2.0.0 |
| TATR Structure | Experimental | No | models-v2.0.0 |
| SLANet Plus | Experimental | No | models-v2.0.0 |
| PP-LCNet (doc/textline ori) | Experimental | No | test-models only |

> Note: `test-models/` directory contains models under active testing and should not be modified.

---

## Benchmark

See [docs/benchmark.md](docs/benchmark.md) for detailed comparison with LaTeXSnipper Desktop.

| Metric | LaTeXSnipper (Python) | Core (Rust) | Winner |
|--------|----------------------|-------------|--------|
| Text Recognition | ~50 ms | **8.8 ms** | Core 5.7x faster |
| Formula Detection | ~300 ms | **293.9 ms** | Core 1.0x faster |
| Formula Recognition | ~400 ms | **213.3 ms** | Core 1.9x faster |
| Formula Output | `$$ E = m c ^ { 2 } $$` | `$$ E = m c ^ { 2 } $$` | Same |
| Text Accuracy | 100% | ~95% | LaTeXSnipper (v5 vs v6 model) |

---

## Related Projects

- [LaTeXSnipper Mobile](https://github.com/strangelion/LaTeXSnipper_mobile) — Android app
- LaTeXSnipper Office — Office Add-in
- [LaTeXSnipper Desktop](https://github.com/SakuraMathcraft/LaTeXSnipper) — Desktop app
- LaTeXSnipper Web — Web app (planned)

All share the same Rust Core.

---

## Acknowledgements

This project builds on the work of these open-source projects:

### Models & Algorithms

| Project | Usage |
|---------|-------|
| [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | PP-OCRv6 text detection & recognition models |
| [Ultralytics YOLOv8](https://github.com/ultralytics/ultralytics) | YOLOv8-MFD formula detection model |
| [TrOCR](https://huggingface.co/microsoft/trocr-base-handwritten) | Transformer-based formula recognition |
| [LaTeXSnipper Desktop](https://github.com/SakuraMathcraft/LaTeXSnipper) | Original Python implementation, post-processing algorithms |

### Rust Ecosystem

| Crate | Usage |
|-------|-------|
| [ort](https://github.com/pyke/ort) | ONNX Runtime Rust bindings |
| [image](https://github.com/image-rs/image) | Image decoding and processing |
| [imageproc](https://github.com/image-rs/imageproc) | Image processing primitives |
| [tokio](https://github.com/tokio-rs/tokio) | Async runtime |
| [clap](https://github.com/clap-rs/clap) | CLI argument parsing |
| [serde](https://github.com/serde-rs/serde) | Serialization framework |
| [ndarray](https://github.com/rust-ndarray/ndarray) | N-dimensional array operations |
| [wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) | WebAssembly bindings |
| [jni](https://github.com/jni-rs/jni) | Android JNI bindings |

---

## License

GNU AGPL-3.0. Allowed for learning and personal use. Closed-source commercial distribution is prohibited.

---

## Model Sources & Licenses

This project uses third-party models. Their licenses are listed below for compliance.

| Model | Source Repository | License | Notes |
|-------|-------------------|---------|-------|
| YOLOv8-MFD | [SakuraMathcraft/LaTeXSnipper](https://github.com/SakuraMathcraft/LaTeXSnipper) | MIT | Formula detection model, trained on Mathcraft dataset |
| TrOCR-DeiT | [microsoft/trocr-base-handwritten](https://huggingface.co/microsoft/trocr-base-handwritten) | MIT | Transformer OCR encoder+decoder |
| PP-OCRv6 | [PaddlePaddle/PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | Apache-2.0 | Text detection & recognition (v6-small variant) |
| PP-DocLayout v3 | [RapidAI/RapidLayout](https://github.com/RapidAI/RapidLayout) | Apache-2.0 | PicoDet-based document layout analysis, 10 categories |
| TATR Detection | [microsoft/table-transformer](https://github.com/microsoft/table-transformer) | MIT | DETR-based table region detection |
| TATR Structure | [microsoft/table-transformer](https://github.com/microsoft/table-transformer) | MIT | DETR-based table structure recognition |
| SLANet Plus | [RapidAI/RapidTable](https://github.com/RapidAI/RapidTable) | Apache-2.0 | Table structure recognition, 95.89% TEDS |

**PaddleOCR / PP-Structure** models are developed by [Baidu PaddlePaddle](https://github.com/PaddlePaddle/PaddleOCR) under the Apache-2.0 license.

**RapidAI** models ([RapidLayout](https://github.com/RapidAI/RapidLayout), [RapidTable](https://github.com/RapidAI/RapidTable)) are converted from PaddleOCR to ONNX format and distributed under the Apache-2.0 license. ONNX models are downloaded from [ModelScope](https://www.modelscope.cn/models/RapidAI).
