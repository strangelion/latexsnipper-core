# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

## [0.1.0] - 2026-06-28

### Added
- Initial project structure
- Cargo workspace setup
