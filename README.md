<div align="center">

# LaTeXSnipper Core

**A composable Rust engine for mathematical OCR, document understanding, and multi-format document processing.**

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)]()
[![License](https://img.shields.io/badge/License-AGPL--3.0-blue)]()
[![Status](https://img.shields.io/badge/Status-v1.0.0-brightgreen)]()
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20WASM-lightgrey)]()

**Build once. Run everywhere.**

A single Rust core powering Desktop, Mobile, Office Add-ins and Web applications.

[![About](assets/About.png)]()

[English](README.md) · [中文](README-CN.md)

</div>

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
Image → Preprocess → Detection → Crop → Recognition → Document AST → Output
           │              │              │              │
       letterbox       YOLOv8        TrOCR/CRNN     LaTeX/OMML
        normalize       DBNet         Beam Search    MathML/Typst
```

---

## Features

| Capability | Status | Details |
|-----------|--------|---------|
| **AST** | ✅ | Document → Page → Block → Inline → Formula |
| **Image** | ✅ | SnipperImage, ImageView, decode, resize, normalize |
| **Inference** | ✅ | YOLOv8 detection, TrOCR recognition, CRNN+CTC |
| **Pipeline** | ✅ | DAG Node Graph, YAML/JSON Manifest, async with cancellation |
| **Conversion** | ✅ | 12 formats: LaTeX, OMML, MathML, Typst, Markdown, HTML |
| **Export** | ✅ | SVG, Text, PDF generators |
| **Runtime** | ✅ | ONNX Runtime (with session caching) + Stub |
| **Model** | ✅ | Manifest, Config, SHA256 verification |
| **Syntax** | ✅ | LaTeX/Typst/Markdown Parser + Renderer |
| **Plugin** | ✅ | Plugin trait, Registry, Request/Response |
| **Engine** | ✅ | JobQueue, Service trait, Request/Response Builder, Streaming API |
| **FFI** | ✅ | Android JNI, iOS C FFI (OnnxRuntimeBackend) |
| **WASM** | ✅ | parse/render/convert/recognize bindings |
| **CLI** | ✅ | recognize/parse/render/version |

---

## Workspace

```
crates/
├── foundation/     Error, Result, Logger, Config, EventBus
├── ast/            Document AST — single source of truth
├── tensor/         Inference I/O tensors
├── image/          Platform-independent image processing
├── runtime/        RuntimeBackend + InferenceSession traits
├── model/          Model manifest, config, management
├── inference/      Detection + Recognition pipelines
├── pipeline/       Node-based async pipeline
├── syntax/         LaTeX/Typst/Markdown Parser + Renderer
├── conversion/     AST → LaTeX/OMML/MathML/Typst/Markdown/HTML
├── export/         RenderTree → SVG/Text/PDF
├── engine/         SnipperEngine + JobQueue + Service
├── plugin/         Plugin API (Plugin trait, Registry)
├── mock/           Fake implementations for testing
├── ffi/            Android JNI + iOS C FFI
├── wasm/           WebAssembly bindings
├── cli/            CLI tool
└── tests/          Integration tests (150+ tests)
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

| Document | Description |
|----------|-------------|
| [architecture.md](docs/architecture.md) | Four-layer architecture overview |
| [inference.md](docs/inference.md) | Detection + Recognition parameters |
| [model.md](docs/model.md) | Model config, manifest, directory structure |
| [conversion.md](docs/conversion.md) | 12 output formats |
| [plugin.md](docs/plugin.md) | Plugin system |
| [getting-started.md](docs/getting-started.md) | Developer guide |

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

GNU AGPL-3.0. 学习和个人使用允许，禁止闭源商业化分发。
