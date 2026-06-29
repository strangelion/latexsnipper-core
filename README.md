<div align="center">

# LaTeXSnipper Core

**A composable Rust engine for mathematical OCR, document understanding, and multi-format document processing.**
[![About](docs/About.png)]()
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)]()
[![License](https://img.shields.io/badge/License-MIT-blue.svg)]()
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
- HTML
- SVG
- PDF

The entire architecture is designed around a single principle:

> **OCR is only one way to build a document. The document itself is the source of truth.**

---

# Why LaTeXSnipper Core?

Most OCR libraries follow this workflow:

```

Image
│
▼
OCR
│
▼
LaTeX

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
┌─────────┼──────────────┐
▼         ▼              ▼
LaTeX    OMML         MathML
▼         ▼              ▼
Typst   Markdown       HTML

```

This architecture makes every output format a renderer instead of a dedicated OCR implementation.

---

# Features

## Core

- Unified Document AST
- Node Graph Pipeline
- Engine-based execution model
- Pluggable Runtime Backend
- Platform-independent architecture

## OCR

- Formula detection
- Formula recognition
- Text detection
- Text recognition
- Mixed document pipeline

## Runtime

- Stub Runtime (implemented)
- ONNX Runtime (planned)
- Runtime abstraction layer
- Session-based execution

## Image

- Zero-copy ImageView
- Image Operations
- Image Pipeline
- Geometry abstraction

## Conversion

- LaTeX
- OMML
- MathML
- Typst
- Markdown

## Export

- SVG
- PNG
- HTML
- PDF

## Cross Platform

- Desktop
- Android
- iOS
- Office Add-ins
- WebAssembly
- CLI

---

# Architecture

```

```
             SnipperEngine
                   │
     ┌─────────────┼─────────────┐
     │             │             │
 Pipeline      Model Manager   Runtime
     │                           │
     ▼                           ▼
```

Inference Layer            Runtime Backend
│                           │
▼                           ▼
Tensor                 ONNX / Stub
│
▼
Image + Document AST
│
▼
Foundation

```

---

# Workspace

```

crates/

foundation/
ast/
tensor/
image/
runtime/
model/
inference/
pipeline/
syntax/
conversion/
export/
engine/
plugin/
mock/
ffi/
wasm/
cli/

```

---

# Current Status

| Module | Status |
|---------|--------|
| Foundation | ✅ Stable |
| AST | ✅ Stable |
| Tensor | ✅ Stable |
| Image | ✅ Stable |
| Runtime Abstraction | ✅ Stable |
| Model Manager | ✅ Stable |
| Inference | 🚧 In Progress |
| Pipeline | 🚧 In Progress |
| Engine | 🚧 Integration |
| ONNX Runtime | ⏳ Planned |
| WASM | ⏳ Planned |
| Export | ⏳ Planned |

---

# Development Roadmap

## Alpha

- [x] Workspace
- [x] Foundation
- [x] AST
- [x] Tensor
- [x] Image
- [x] Runtime abstraction
- [x] Stub Runtime
- [x] Engine
- [x] Node Graph

## Beta

- [ ] ONNX Runtime Backend
- [ ] End-to-End OCR Pipeline
- [ ] WASM Runtime
- [ ] CLI

## Stable

- [ ] Office Integration
- [ ] Mobile Integration
- [ ] Plugin SDK
- [ ] Multi-model Runtime
- [ ] Performance Optimization

---

# Design Philosophy

LaTeXSnipper Core is built around several core principles.

## Document First

The document is the source of truth.

Not LaTeX.

Not OCR.

Not Office.

Everything is converted into a unified AST.

---

## Composable

Everything is a Node.

Everything is a Pipeline.

Everything is replaceable.

---

## Platform Independent

Business logic lives in Rust.

UI lives outside.

Desktop.

Mobile.

Office.

Web.

All share the same engine.

---

## Pluggable Runtime

Inference backends are interchangeable.

Current:

- Stub Runtime

Future:

- ONNX Runtime
- TensorRT
- NCNN
- WebGPU

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

- LaTeXSnipper Mobile
- LaTeXSnipper Office
- Future Desktop Edition
- Future Web Edition

All of them share the same Rust Core.

---

# Contributing

Contributions are welcome.

Please open an Issue before submitting large changes.

---

# License

GNU AGPL-3.0。允许学习和个人使用，禁止闭源商业化分发。修改后分发或网络服务必须公开全部源码。
