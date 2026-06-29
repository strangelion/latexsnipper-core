<div align="center">

# LaTeXSnipper Core

**A composable Rust engine for mathematical OCR, document understanding, and multi-format document processing.**
[![About](docs/About.png)]()
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)]()
[![License](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)]()
[![Status](https://img.shields.io/badge/Status-Alpha-yellow.svg)]()
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20iOS-lightgrey.svg)]()

**Build once. Run everywhere.**

A single Rust core powering Desktop, Mobile, Office Add-ins and Web applications.

</div>

---

# Overview

LaTeXSnipper Core is **not a traditional OCR library**.

It is a modular document computation engine that transforms images and structured documents into a unified Abstract Syntax Tree (AST), enabling multiple output formats including:

- LaTeX
- OMML (Microsoft Office Math)
- MathML
- Typst
- Markdown
- SVG
- Plain Text

The entire architecture is designed around a single principle:

> **OCR is only one way to build a document. The document itself is the source of truth.**

---

# Why LaTeXSnipper Core?

Most OCR libraries follow this workflow:

```
Image → OCR → LaTeX
```

LaTeXSnipper Core instead builds a structured document representation:

```
Image / PDF / Clipboard
        │
        ▼
  Detection Pipeline
        │
        ▼
    Document AST
        │
  ┌─────┼──────────┐
  ▼     ▼          ▼
LaTeX  OMML     MathML
  ▼     ▼          ▼
Typst Markdown    HTML
```

This architecture makes every output format a renderer instead of a dedicated OCR implementation.

---

# Architecture

```
┌──────────────────────────────────────────┐
│              Platform                    │
│   Android   │  Office  │  Browser  │ ... │
└──────────────────────────────────────────┘
                      │
┌──────────────────────────────────────────┐
│              Adapter                     │
│    JNI    │  WASM  │  Office.js  │ CLI  │
└──────────────────────────────────────────┘
                      │
┌──────────────────────────────────────────┐
│                Core                      │
│  AST │ Image │ Inference │ Pipeline      │
│  Conversion │ Export │ Model │ Syntax     │
└──────────────────────────────────────────┘
                      │
┌──────────────────────────────────────────┐
│              Runtime                     │
│   ONNX Runtime  │  Stub  │  Future ...   │
└──────────────────────────────────────────┘
```

Four-layer architecture with strict unidirectional dependencies.

---

# Features

## Core

- Unified Document AST (Document → Page → Block → Inline → Formula)
- Node Graph Pipeline (async, composable, cancellable)
- Engine-based execution model
- Pluggable Runtime Backend (ONNX / Stub / Future)
- Platform-independent architecture
- Event Bus for lifecycle events
- Configuration management

## OCR

- Formula detection (YOLOv8, NMS)
- Formula recognition (TrOCR encoder + Beam Search decoder)
- Text detection (DBNet + Moore contour tracing)
- Text recognition (CRNN + CTC decode)
- Mixed document pipeline

## Runtime

- Stub Runtime (testing)
- ONNX Runtime (ort 2.0.0-rc.12, session caching)
- Platform auto-detection (Windows/Linux/macOS)
- GPU acceleration detection (CUDA/DirectML/TensorRT)
- RuntimeBackend trait abstraction

## Image

- Platform-independent SnipperImage
- Zero-copy ImageView
- Operations: resize, crop, letterbox, normalize, pad, channel convert
- Decode/Encode (PNG)

## Syntax

- LaTeX Parser + Renderer
- Typst Renderer (200+ symbol mappings)
- Markdown Renderer

## Conversion

- AST → LaTeX (with Table rendering)
- AST → OMML (Office Math ML, with frac/sqrt/superscript parsing)
- AST → MathML (with symbol mapping)
- AST → Typst (with LaTeX→Typst conversion)
- AST → Markdown (with Table rendering)
- Converter trait (pluggable)

## Plugin

- Plugin trait (name, version, init, handle, cleanup)
- TransformPlugin (document transformation)
- PluginRegistry (register, unregister, handle, handle_all, handle_filtered)
- PluginRequest/PluginResponse types

## Export

- RenderTree intermediate representation
- SVG Generator
- Plain Text Generator
- Generator trait (pluggable)

## Mock

- FakeDetector, FakeRecognizer
- FakePipeline (formula/text/mixed)
- fake_document() for testing

## Cross Platform

- Android (JNI FFI)
- iOS (C FFI)
- CLI tool (`snipper`)
- WebAssembly (parse/render/convert)
- Plugin SDK

---

# Workspace

```
crates/
├── foundation/     # Error, Result, Logger, Config, EventBus
├── ast/            # Document AST — single source of truth
├── tensor/         # Inference I/O tensors
├── image/          # Platform-independent image processing
├── runtime/        # RuntimeBackend + InferenceSession traits
├── model/          # Model manifest, config, management
├── inference/      # Detection + Recognition pipelines
├── pipeline/       # Node Graph Pipeline
├── syntax/         # LaTeX/Typst/Markdown Parser + Renderer
├── conversion/     # AST → LaTeX/OMML/MathML/Typst/Markdown
├── export/         # RenderTree → SVG/Text
├── engine/         # SnipperEngine — main entry point
├── plugin/         # Plugin API (Plugin trait, Registry)
├── mock/           # Fake implementations for testing
├── ffi/            # Android JNI + iOS C FFI
├── wasm/           # WebAssembly bindings (planned)
├── cli/            # CLI tool
└── tests/          # Integration tests
```

---

# Current Status

| Module | Status | Docs |
|--------|--------|------|
| Foundation | ✅ Implemented | [foundation.md](docs/foundation.md) |
| AST | ✅ Implemented | [ast.md](docs/ast.md) |
| Tensor | ✅ Implemented | [tensor.md](docs/tensor.md) |
| Image | ✅ Implemented | [image.md](docs/image.md) |
| Runtime | ✅ Implemented | [runtime.md](docs/runtime.md) |
| Model | ✅ Implemented | [model.md](docs/model.md) |
| Inference | ✅ Implemented | [inference.md](docs/inference.md) |
| Pipeline | ✅ Implemented | [pipeline.md](docs/pipeline.md) |
| Syntax | ✅ Implemented | [syntax.md](docs/syntax.md) |
| Export | ✅ Implemented | [export.md](docs/export.md) |
| Engine | ✅ Implemented | [engine.md](docs/engine.md) |
| Mock | ✅ Implemented | [mock.md](docs/mock.md) |
| FFI | ✅ Implemented | [ffi.md](docs/ffi.md) |
| CLI | ✅ Implemented | [cli.md](docs/cli.md) |
| Conversion | ✅ Implemented | [conversion.md](docs/conversion.md) |
| Plugin | ✅ Implemented | [plugin.md](docs/plugin.md) |
| WASM | ✅ Implemented | [wasm.md](docs/wasm.md) |

---

# Development Roadmap

## Alpha

- [x] Workspace setup
- [x] Foundation (Error, Config, Event, Logger)
- [x] AST (Document, Block, Inline, Formula, Geometry, Operation, Visitor)
- [x] Tensor (Float32/Int64/Int32/UInt8)
- [x] Image (SnipperImage, ImageView, Operations, Decode)
- [x] Runtime abstraction (RuntimeBackend, InferenceSession, AccelerationMode)
- [x] Stub Runtime
- [x] ONNX Runtime backend (session caching, platform detection)
- [x] Model Manager (Manifest, Config, SHA256)
- [x] Inference (Formula Detector, Formula Recognizer, Text Detector, Text Recognizer)
- [x] Node Graph Pipeline
- [x] Engine (recognize API)
- [x] Syntax (LaTeX Parser/Renderer, Typst Renderer, Markdown Renderer)
- [x] Export (RenderTree, SVG Generator, Text Generator)
- [x] Mock (FakeDetector, FakeRecognizer, FakePipeline)
- [x] FFI (Android JNI, iOS C FFI)
- [x] CLI (recognize, parse, render, version)
- [x] Dual-track testing (Mock vs ONNX)

## Beta

- [x] Conversion crate (12 formats: LaTeX/OMML/MathML/Typst/Markdown/HTML)
- [x] Plugin SDK (Plugin trait, TransformPlugin, PluginRegistry)
- [x] WASM bindings (parse/render/convert)
- [ ] End-to-End OCR Pipeline (Engine ↔ Inference integration)
- [ ] WASM Runtime (onnxruntime-web)
- [ ] PDF Export

## Stable

- [ ] Office Integration
- [ ] Mobile Integration
- [ ] Multi-model Runtime
- [ ] Performance Optimization

---

# Getting Started

```bash
# Clone
git clone https://github.com/strangelion/latexsnipper-core.git
cd latexsnipper-core

# Build
cargo build

# Test
cargo test

# Run CLI
cargo run -p latexsnipper-cli -- parse --latex '$\frac{a+b}{c}$'
```

See [docs/getting-started.md](docs/getting-started.md) for details.

---

# Design Philosophy

## Document First

The document is the source of truth. Not LaTeX. Not OCR. Not Office.
Everything is converted into a unified AST.

## Composable

Everything is a Node. Everything is a Pipeline. Everything is replaceable.

## Platform Independent

Business logic lives in Rust. UI lives outside.
Desktop, Mobile, Office, Web — all share the same engine.

## Pluggable Runtime

Inference backends are interchangeable.
Current: Stub, ONNX Runtime. Future: TensorRT, NCNN, WebGPU.

---

# Project Goals

The goal of this project is **not** to build another OCR library.

The goal is to provide a reusable document engine for:

- Mathematical OCR
- Office automation
- Structured document understanding
- Cross-platform applications
- AI-powered document workflows

---

# Related Projects

- [LaTeXSnipper Mobile](https://github.com/strangelion/LaTeXSnipper_mobile)
- LaTeXSnipper Office
- Future Desktop Edition
- Future Web Edition

All of them share the same Rust Core.

---

# Contributing

Contributions are welcome. Please open an Issue before submitting large changes.

---

# License

GNU AGPL-3.0。允许学习和个人使用，禁止闭源商业化分发。修改后分发或网络服务必须公开全部源码。
