# LaTeXSnipper Core

A high-performance, composable document processing engine for OCR, LaTeX, and structured document understanding.

LaTeXSnipper Core is not a simple OCR toolkit.
It is a **document computation engine** that converts images and documents into a unified intermediate representation (AST), and supports multiple output formats such as LaTeX, MathML, OMML, Markdown, and HTML.

It is designed for:

- Cross-platform OCR systems
- Office document processing (Word / WPS / Add-ins)
- Web-based document understanding
- Math formula recognition engines
- Extensible AI document pipelines

---

## 🚀 Key Features

- 🧠 Unified Document AST (single source of truth)
- 🔌 Pluggable inference backend (Stub / ONNX / future TensorRT)
- 🌐 Cross-platform runtime (Rust core + FFI + WASM)
- 🧩 Node-based computation graph pipeline
- 📄 Multi-format export (LaTeX / OMML / HTML / SVG)
- ⚡ Zero-copy image view pipeline design
- 🔄 Fully decoupled inference and execution engine

---

## 🏗 Architecture Overview

LaTeXSnipper Core is built around five layers:

- Foundation: error, logging, runtime context
- AST: unified document representation
- Runtime: backend abstraction layer
- Inference: detection / recognition / layout models
- Pipeline: node-based execution graph
- Engine: system orchestrator

---

## 🔄 Design Philosophy

- OCR is not the core — **Document is**
- Models are replaceable — **AST is stable**
- Pipeline is composable — **not hardcoded**
- Runtime is pluggable — **not coupled**
- UI is external — **core is platform-agnostic**

---

## 📦 Current Status

- Core architecture: ✅ Stable
- AST system: ✅ Completed
- Pipeline system: 🚧 In progress
- Runtime: ⚠ Stub backend (ONNX integration pending)
- Engine: 🚧 Integration testing
- FFI / WASM: 🚧 planned

---

## 🎯 Target Use Cases

- LaTeX OCR tools (like original LaTeXSnipper)
- Office document plugins (Word / WPS)
- Desktop OCR applications
- Web-based math recognition systems
- AI document processing pipelines

---

## ⚠️ Note

This project is still under active development.
The ONNX Runtime backend is not yet enabled; a stub backend is used for testing and architecture validation.

---

## 📜 License

TBD