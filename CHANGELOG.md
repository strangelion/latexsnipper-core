# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

## [0.1.0] - 2026-06-28

### Added
- Initial project structure
- Cargo workspace setup
