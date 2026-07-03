<div align="center">

# LaTeXSnipper Core

**One-stop solution from images to multi-format documents**

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)]()
[![License](https://img.shields.io/badge/License-AGPL--3.0-blue)]()
[![Status](https://img.shields.io/badge/Status-Core%20Pipeline%20Working-brightgreen)]()
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20WASM-lightgrey)]()

**One line of code, images to LaTeX/Markdown/Typst**

[![About](assets/About.png)]()

[English](README.md) · [中文](README-CN.md)

</div>

---

## Quick Start

```rust
use latexsnipper_pipeline::sdk::Snipper;

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
| **AST → Markdown** | ✅ | MathJax compatible |
| **AST → Typst** | ✅ | Native Typst syntax |
| **AST → HTML** | ✅ | MathJax rendering |
| **AST → MathML** | ✅ | Office compatible |
| **AST → OMML** | ✅ | Word compatible |

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
  ├── Export (SVG/Text)
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
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Recognition Pipeline                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐             │
│   │  Decode   │───▶│Normalize │───▶│  Layout  │───▶│  Region  │             │
│   │          │    │          │    │Detection │    │ Proposal │             │
│   └──────────┘    └──────────┘    └──────────┘    └──────────┘             │
│        │                                               │                    │
│        │              ┌────────────────────────────────┘                    │
│        │              │                                                     │
│        │              ▼                                                     │
│        │         ┌──────────────────────────────────────┐                  │
│        │         │                                      │                  │
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
│        │                            ▼                                     │
│        │                     ┌──────────┐                                 │
│        │                     │  Merge   │                                 │
│        │                     └────┬─────┘                                 │
│        │                          │                                       │
│        │                          ▼                                       │
│        │                  ┌──────────────┐                                │
│        └─────────────────▶│ Document AST │                                │
│                           └──────┬───────┘                                │
│                                  │                                        │
│                                  ▼                                        │
│                           ┌──────────┐    ┌──────────┐                    │
│                           │Conversion│───▶│  Export  │                    │
│                           └──────────┘    └──────────┘                    │
│                                  │              │                          │
│                           LaTeX/OMML        SVG/Text/PDF                   │
│                           MathML/Typst                                     │
│                           Markdown/HTML                                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Features

### Stable

| Capability | Status | Details |
|-----------|--------|---------|
| **AST** | ✅ | Document → Page → Block → Inline → Formula |
| **Image** | ✅ | SnipperImage, ImageView, decode, resize, normalize |
| **Conversion** | ✅ | 12 formats: LaTeX, OMML, MathML, Typst, Markdown, HTML |
| **Syntax** | ✅ | LaTeX/Typst/Markdown Parser + Renderer |
| **Pipeline** | ✅ | DAG Node Graph, YAML/JSON Manifest, async with cancellation |

### Experimental

| Capability | Status | Details |
|-----------|--------|---------|
| **Inference** | ✅ | YOLOv8 detection + TrOCR formula recognition + CRNN+CTC text recognition |
| **Runtime** | ✅ | ONNX Runtime (session caching, GPU auto-detect) + Stub |
| **Engine** | ✅ | JobQueue, Service trait, Request/Response Builder, Streaming API |
| **Model** | ✅ | Manifest, Config, SHA256 verification |
| **Plugin** | ✅ | Plugin trait, Registry, TransformPlugin |
| **FFI** | ✅ | Android JNI + iOS C FFI |
| **WASM** | ✅ | Full parse/render/convert/recognize bindings |
| **CLI** | ✅ | recognize/parse/render/version commands |
| **Export** | 🟡 | SVG/Text available, PDF basic implementation |

### Planned

| Capability | Status | Details |
|-----------|--------|---------|
| **Table Recognition** | ○ | Table structure detection and parsing |
| **Handwriting** | ○ | Handwritten text recognition |
| **Formula Layout** | ○ | Complex formula layout analysis |
| **Multi-page** | ○ | Multi-page document processing |

---

## Workspace

```
crates/
├── foundation/     ✅ Error, Result, Logger, Config, EventBus
├── ast/            ✅ Document AST — single source of truth
├── tensor/         ✅ Inference I/O tensors
├── image/          ✅ Platform-independent image processing
├── runtime/        ✅ RuntimeBackend + InferenceSession traits (ONNX + Stub)
├── model/          ✅ Model manifest, config, SHA256 verification
├── inference/      ✅ Detection + Recognition pipelines (YOLOv8/TrOCR/CRNN)
├── pipeline/       ✅ Node-based async pipeline
├── syntax/         ✅ LaTeX/Typst/Markdown Parser + Renderer
├── conversion/     ✅ AST → LaTeX/OMML/MathML/Typst/Markdown/HTML
├── export/         🟡 RenderTree → SVG/Text/PDF (PDF basic impl)
├── engine/         ✅ SnipperEngine + JobQueue + Service
├── plugin/         ✅ Plugin trait, Registry
├── mock/           ✅ Fake implementations for testing
├── ffi/            ✅ Android JNI + iOS C FFI
├── wasm/           ✅ WebAssembly bindings
├── cli/            ✅ CLI tool (recognize/parse/render/version)
└── tests/          ✅ Integration tests (150+ tests)
```

---

## Getting Started

```bash
# Build
cargo build

# Run CLI
cargo run -p latexsnipper-cli -- parse --latex '$\frac{a+b}{c}$'

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

---

## Models

LaTeXSnipper Core uses PaddleOCR v6 ONNX models for text recognition.

### Supported Models

| Model | Size | Purpose | Download |
|-------|------|---------|----------|
| PP-OCRv6 Medium Det | 32 MB | Text detection (high accuracy) | `latexsnipper-text-det-v6-medium.zip` |
| PP-OCRv6 Medium Rec | 64 MB | Text recognition (18708 chars: CN/EN/math/greek) | `latexsnipper-text-rec-v6-medium.zip` |
| PP-OCRv6 Small Det | 7 MB | Text detection (lightweight) | `latexsnipper-text-det-v6-small.zip` |
| YOLOv8-MFD | 66 MB | Formula detection | `latexsnipper-formula-det.zip` |
| TrOCR | 104 MB | Formula recognition (encoder+decoder) | `latexsnipper-formula-rec.zip` |
| PP-LCNet Doc Ori | 6 MB | Document orientation (0/90/180/270) | `latexsnipper-doc-ori-v1.zip` |
| PP-LCNet Textline Ori | 6 MB | Textline orientation (0/180) | `latexsnipper-textline-ori-v1.zip` |
| UVDoc | 28 MB | Document unwarping | `latexsnipper-uvdoc-v1.zip` |

### Quick Start

Download `latexsnipper-models-all.zip` (193 MB) for all models:

```bash
curl -LO https://github.com/strangelion/latexsnipper-core/releases/download/models-v2.0.0/latexsnipper-models-all.zip
unzip latexsnipper-models-all.zip -d models/
```

### Model Directory Structure

```
models/
├── formula-det/yolov8-mfd/     # Formula detection
├── formula-rec/trocr-deit/     # Formula recognition
├── text-det/v6-medium/         # Text detection (recommended)
├── text-rec/v6-medium/         # Text recognition (recommended)
├── doc-ori/pp-lcnet-v1/        # Document orientation
├── textline-ori/pp-lcnet-v1/   # Textline orientation
└── uvdoc/uvdoc-v1/             # Document unwarping
```

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
