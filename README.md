<div align="center">

# LaTeXSnipper Core

**A Rust document-understanding core for OCR, a unified document AST, and multi-format conversion.**

[![CI](https://github.com/strangelion/latexsnipper-core/actions/workflows/ci.yml/badge.svg)](https://github.com/strangelion/latexsnipper-core/actions/workflows/ci.yml)
[![WASM](https://github.com/strangelion/latexsnipper-core/actions/workflows/wasm.yml/badge.svg)](https://github.com/strangelion/latexsnipper-core/actions/workflows/wasm.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.88.0-orange?logo=rust)](Cargo.toml)
[![Workspace](https://img.shields.io/badge/workspace-3.0.0--alpha.1-blue)](Cargo.toml)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20WASM-lightgrey)]()

**Native ONNX inference · Tract/WASM · CLI · Rust SDK · verified local plugins**

[![About](assets/About.png)](assets/About.png)

[English](README.md) · [中文](README-CN.md)

</div>

---

## Project status

The workspace is staged as **3.0.0-alpha.1** for the stacked Core 3 changes. The contract foundation, default-deny WASI Component host, signed registry, and disabled-by-default remote WASI installation CLI are implemented; callable WASM, capability, model-loader, Worker, and FFI paths remain on their existing v2-era contracts. See the [Core 3 architecture and delivery status](docs/v3/architecture.md).

This does **not** mean that every format, model, or plugin boundary has the same maturity.

| Area | Status | Contract |
|---|---|---|
| Unified `Document` AST and JSON schema | **Stable** | The AST is the source of truth shared by import, recognition, conversion, and export. |
| LaTeX, Markdown, Typst, plain text, JSON AST | **Stable for represented AST** | Semantic content is editable; source-specific macros, typography, and exact line breaking may be lost. |
| MathML and OMML | **Stable / best effort** | Formula structure is editable; application-specific properties and exact typography may be downgraded. |
| Native OCR pipelines | **Implemented and CI-validated** | Requires compatible local model profiles. Accuracy and readiness depend on the selected models and mode. |
| HTML | **Best effort** | Text and math semantics are retained; arbitrary scripts, CSS layout, and application objects are not reproduced. |
| SVG, PNG, PDF | **Experimental / best effort** | Structural or visual output is tested, but full layout, font, editability, and round-trip fidelity are not guaranteed. |
| DOCX, PPTX, XLSX | **Experimental / best effort** | Package validity and supported semantics are tested; Microsoft Office visual parity is not guaranteed. |
| WASM semantic conversion | **Stable** | Native binary exporters are intentionally excluded from the browser target. |
| WASM Tract recognition | **Experimental** | Model-gated and asynchronous. Text, projection-structured tables, and TrOCR handwriting execute through the browser pipeline; readiness follows validated loaded artifacts. |
| Built-in Rust plugins | **Stable host behavior** | Deterministic ordering, typed hooks, transactional patches, failure policies, soft deadlines, and quarantine are implemented. |
| Isolated native process plugins | **Reviewed local code only** | Hard timeout and resource controls exist, but this is not an OS filesystem/network sandbox. |
| WASI Component host | **Implemented as a Rust host crate** | WIT v1, manifest/digest verification, typed brokers, hard interruption, and resource limits are tested. Public execution integration remains pending. |
| Signed remote WASI registry/install | **Implemented, disabled after install** | Ed25519 thresholds, expiry/rollback/freeze checks, bounded HTTPS/ZIP handling, provenance, update, revoke, and rollback are tested. Install never executes or enables code. |
| Native dynamic-library ABI | **Unavailable** | Remote/native substitution is rejected; reviewed local process plugins use a separate path. |

The executable source of truth is the capability registry:

```bash
snipper capabilities --format json
snipper capabilities --format json --input docx --output png
```

See [Production capability and fidelity policy](docs/production-capabilities.md) for the precise stability definitions and loss model.

<!-- capability-inputs: PNG,JPEG,WebP,BMP,TIFF,GIF,SVG,PDF,DOCX,PPTX,XLSX,HTML,Markdown,LaTeX,Typst,MathML,OMML,JSON AST,Plain text -->
<!-- capability-outputs: JSON AST,Plain text,Markdown,LaTeX,Typst,HTML,MathML,OMML,SVG,PDF,PNG,DOCX,PPTX,XLSX -->

---

## What LaTeXSnipper Core provides

LaTeXSnipper Core is more than an image-to-LaTeX wrapper. It provides a shared document model and execution layer for desktop applications, browser applications, Office integrations, mobile adapters, and command-line workflows.

- **Unified document AST** — pages, blocks, inlines, formulas, tables, assets, geometry, styles, diagnostics, notes, revisions, references, and accessibility metadata.
- **Model-driven recognition** — formula, text, mixed-document, layout, orientation, table, and handwriting building blocks selected through model profiles and pipeline modes.
- **Multi-format import and conversion** — semantic formats, raster assets, PDF, SVG, and Office Open XML packages.
- **Binary-safe export** — text and binary artifacts use distinct representations with MIME type, SHA-256, byte length, assets, and diagnostics.
- **Native and browser execution** — ONNX Runtime on supported desktop targets and Tract in WebAssembly.
- **Operational tooling** — CLI, SDK, capability inspection, model management, plugin package management, diagnostics, batch reports, shell completions, and man pages.
- **Security-oriented parsing** — signature-first detection, bounded archive/XML processing, safe package paths, checksum verification, and structured failures.

---

## Architecture

```text
Applications and adapters
├── Desktop / Office / mobile integrations
├── CLI
├── Rust SDK / FFI
└── Browser Worker + WASM
                │
                ▼
Engine and pipeline
Input bytes or pixels
→ decode / import / PDF render
→ layout and region proposal
→ mode-specific recognition
→ region resolution and reading order
→ unified Document AST
                │
                ▼
Conversion and export
├── semantic text: LaTeX / Markdown / Typst / HTML / MathML / OMML
├── structured data: JSON AST / plain text
├── visual output: SVG / PNG / PDF
└── packages: DOCX / PPTX / XLSX
```

Core business logic is platform-independent. UI, camera access, browser APIs, Office automation, and operating-system integration belong in adapters or applications.

---

## Input, recognition, and conversion behavior

### Raster images

Raster import and OCR are deliberately separate concepts:

- `snipper import image.png` preserves the scan as an AST asset.
- `snipper recognize -i image.png ...` runs the configured OCR pipeline.
- Direct raster-to-semantic conversion is reported unavailable unless recognition is explicitly invoked.

This prevents a plain importer from silently performing model inference.

### PDF

Two distinct paths exist:

1. **Native PDF extraction** — best-effort extraction from content streams.
2. **Rendered-page OCR** — renders pages through `pdftoppm` or `mutool`, then runs recognition.

Arbitrary PDF font encodings, missing `ToUnicode` maps, reading order, complex graphics-state transforms, and scanned pages can reduce fidelity.

### Office packages

DOCX, PPTX, and XLSX readers and writers preserve supported text, formulas, tables, assets, and package structure. Unsupported objects may be downgraded, retained as opaque parts when preservation is enabled, or reported through diagnostics.

A package reopening successfully proves **structural validity**, not exact Microsoft Office visual parity.

### Fidelity dimensions

The project treats these as separate claims:

- package validity;
- semantic preservation;
- layout preservation;
- visual fidelity;
- editability;
- round-trip fidelity.

Do not infer one from another.

---

## CLI

### Build

```bash
git clone https://github.com/strangelion/latexsnipper-core.git
cd latexsnipper-core

cargo build --release -p latexsnipper-cli
```

The binary is written to:

```text
target/release/snipper       # Linux/macOS
target/release/snipper.exe   # Windows
```

Check [GitHub Releases](https://github.com/strangelion/latexsnipper-core/releases) for published binaries and model packages.

### Inspect the environment first

```bash
snipper version
snipper doctor
snipper capabilities
snipper capabilities --format json
```

### Models and recognition

```bash
snipper models download
snipper models verify

snipper recognize -i scan.png -f latex -o result.tex
snipper recognize -i page.png -f markdown --recognize-mode mixed -o page.md
```

Recognition requires compatible local model artifacts. `snipper doctor` and `snipper capabilities` report readiness rather than assuming that model files exist.

### Import, convert, and export

```bash
# Import to the unified JSON AST
snipper import report.docx -o report.ast.json

# Convert through the unified AST
snipper convert report.docx --to markdown -o report.md
snipper convert notes.md --to typst -o notes.typ

# Render/export an AST or supported document
snipper export report.ast.json --to pdf -o report.pdf

# Inspect or validate an input
snipper inspect report.pdf --json
snipper validate report.docx
```

### Batch conversion

```bash
snipper convert documents \
  --to markdown \
  --recursive \
  --output-dir converted \
  --jobs 4 \
  --continue-on-error \
  --report conversion-report.json
```

The CLI also supports atomic output, no-clobber/force policies, strict diagnostics, warning failures, page ranges, JSON/SARIF diagnostics, stable exit codes, shell completions, and roff man pages.

### Plugin packages

```bash
snipper plugin verify ./plugin-package
snipper plugin install ./plugin-package
snipper plugin list
snipper plugin info example.plugin
snipper plugin enable example.plugin
snipper plugin disable example.plugin
snipper plugin doctor
snipper plugin uninstall example.plugin
```

Remote URL installation remains disabled.

---

## Rust SDK

Inside this workspace, enable the native engine feature:

```toml
[dependencies]
latexsnipper-engine = { path = "crates/engine", features = ["native"] }
```

### One-line image recognition

```rust
use latexsnipper_engine::sdk::Snipper;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snipper = Snipper::from_file("input.png")?;

    println!("{}", snipper.to_latex()?);
    println!("{}", snipper.to_markdown()?);
    println!("{}", snipper.to_typst()?);

    Ok(())
}
```

For raster images, `Snipper::from_file` invokes OCR for backwards compatibility. Compatible model files must be available through the configured model directory.

### Import without OCR

```rust
use latexsnipper_ast::ImportOptions;
use latexsnipper_engine::sdk::Snipper;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snipper = Snipper::import_path(
        "document.docx",
        ImportOptions::default(),
    )?;

    println!("{}", snipper.to_markdown()?);
    Ok(())
}
```

Use `Snipper::from_bytes` for in-memory import and `Snipper::from_document` when an application already owns a `Document` AST.

---

## WebAssembly and browser execution

The WASM build uses **Tract** and does not link ONNX Runtime, native PDF/PNG/Office exporters, native filesystem downloaders, or a native Tokio runtime.

```bash
rustup target add wasm32-unknown-unknown

wasm-pack build crates/wasm \
  --target web \
  --release \
  --out-dir ../../target/wasm-web

cd crates/wasm/js
npm ci
npm audit
npm run typecheck
npm test
npm run build
npm run build:example
```

The v2 browser API includes:

- `api_info_v2()` and `capabilities_v2()`;
- verified model load, unload, clear, and transactional update APIs;
- asynchronous `recognize_v2()` and progress reporting;
- stage-boundary cooperative cancellation for direct calls;
- binary-safe `convert_v2()` envelopes.

`WasmWorkerClient` is the recommended browser entrypoint:

- one active recognition request with a bounded queue;
- public request IDs separated from internal wire IDs;
- progress events and stale-response suppression;
- hard cancellation by terminating the Worker;
- Worker restart and verified-model reload;
- RPC timeout and `AbortSignal`;
- structured recovery failure instead of infinite restart loops.

The JavaScript package also provides:

- SHA-256-verified IndexedDB model caching;
- schema migration and LRU budget eviction;
- abortable streaming downloads;
- maximum-size enforcement;
- progress reporting;
- mirror fallback;
- best-effort or required cache policy.

Browser table mode uses a bounded built-in projection structure backend plus a loaded text recognizer; merged-cell metadata from compatible structure models is preserved. Handwriting mode uses a loaded TrOCR encoder, decoder, tokenizer, and preprocessing/output metadata, with a browser decode cap. Production-model tests prove Tract/runtime compatibility and execution, not OCR accuracy. TATR and SLANet remain opt-in experimental structure backends because current upstream exports do not meet the default browser runtime budget.

See [WASM adapter](docs/wasm.md).

---

## Plugin system and security boundary

### Trusted in-process plugins

Trusted Rust plugins support:

- typed hooks;
- deterministic ordering and dependency checks;
- transactional `DocumentPatch`;
- `Stop`, `Continue`, `DisablePlugin`, and `Rollback` policies;
- panic containment;
- cooperative cancellation and deadlines;
- per-plugin concurrency limits;
- quarantine after soft timeout.

An in-process Rust thread cannot be safely force-killed. A soft timeout may return while plugin code is still unwinding or running cooperatively.

### Isolated-process plugins

The versioned process host provides:

- JSON request/response IPC;
- ABI/protocol compatibility checks;
- SHA-256-verified local packages;
- hard deadline termination;
- Unix session/process-group containment;
- Windows Job Object containment;
- memory and response-file observation limits;
- private temporary workspaces;
- strict success/error response validation;
- cross-process registry locking and crash-resistant atomic replacement.

> **Security boundary:** a native process plugin is not an operating-system filesystem/network sandbox. Manifest permissions govern brokered host operations; arbitrary native code can still call operating-system APIs directly. Only run reviewed local process plugins.

Still unavailable:

- public execution of remotely installed WASI Components;
- stable native dynamic-library plugin ABI;
- complete native filesystem/network sandboxing.

See [Plugin system](docs/plugin.md).

---

## Models

Model readiness is profile-driven. The manifest, checksum, configuration, tokenizer, key files, and runtime compatibility must all be present before a capability is reported ready.

| Model family | Primary role | Status |
|---|---|---|
| YOLOv8-MFD | Formula detection | Default native profile |
| TrOCR-DeiT | Formula recognition | Default native profile |
| PP-OCRv6 Det / Rec | Multilingual text detection and recognition | Default native profile |
| OpenOCR Mobile Det / Rec | Alternative DBNet/CTC text pipeline | Experimental |
| PP-DocLayout v3 | Document layout analysis | Experimental |
| TATR Detection / Structure | Table detection and structure | Experimental |
| SLANet Plus | Alternative table structure backend | Experimental |
| PP-LCNet orientation models | Document/text-line orientation and compatibility testing | Experimental / test profile |

```text
models/
├── formula-det/
├── formula-rec/
├── text-det/
├── text-rec/
├── layout/
├── table-det/
├── table-struct/
└── doc-ori/
```

Use the verified downloader rather than copying unverified model files:

```bash
snipper models download
snipper models list
snipper models verify
```

Model packages remain subject to their upstream licenses. See the model manifests and release assets for exact source revisions and checksums.

---

## Workspace

```text
crates/
├── foundation/   errors, diagnostics, configuration, events
├── ast/          unified Document AST and public document types
├── tensor/       runtime-independent tensor types
├── image/        decoding, transforms, normalization, PDF page rendering
├── runtime/      RuntimeBackend, ONNX Runtime provider, model resolution
├── model/        manifests, profiles, validation, downloader metadata
├── inference/    detector and recognizer adapters
├── pipeline/     stage graph, region resolution, reading order
├── syntax/       LaTeX, Typst, and Markdown parsers/renderers
├── conversion/   importers, semantic conversion, Office package handling
├── export/       SVG, PNG, PDF, and text rendering
├── engine/       orchestration, SDK, jobs, metrics
├── api-types/    versioned public request/response types
├── tract/        Tract-based WASM runtime backend
├── plugin/       built-in and isolated-process plugin infrastructure
├── ffi/          Android JNI and iOS C-facing adapters
├── wasm/         wasm-bindgen API
├── cli/          `snipper` command-line application
├── mock/         deterministic test doubles
└── tests/        integration tests and benchmarks

fuzz/             cargo-fuzz targets and seed corpora
docs/             architecture, capability, security, and release documents
```

---

## Verification and CI

### Native checks

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --doc --workspace --all-features
```

### Fuzzing

The repository includes real `cargo-fuzz` targets for format detection, ZIP/OOXML packages, XML, SVG, PDF, JSON AST, LaTeX, Typst, Markdown math, model manifests, and plugin manifests.

```bash
cargo install cargo-fuzz
cargo +nightly fuzz list
cargo +nightly fuzz run format_signature
```

Pull-request CI compiles fuzz targets. Scheduled hardening runs bounded libFuzzer campaigns and uploads crash artifacts on failure.

### Continuous integration coverage

CI covers:

- Windows, Linux, and macOS workspace checks;
- formatting, strict Clippy, MSRV, tests, doc tests, and feature matrices;
- real native model pipelines;
- parser security and round-trip corpora;
- Chrome and Firefox browser tests;
- web, bundler, and Node WASM packages;
- TypeScript, Worker, IndexedDB, download, ESM, and Vite tests;
- dependency audits, model URL verification, benchmarks, and scheduled fuzzing.

Benchmark smoke tests validate execution but do not enforce fragile timing thresholds on shared runners. See [Benchmarks](docs/benchmark.md).

---

## Security and trust model

Important controls include:

- signature/package-first format detection;
- typed errors for mismatched hints and encrypted packages;
- bounded archive entry count, decompressed size, compression ratio, and path traversal checks;
- XML depth/element, DTD/entity, and external relationship restrictions;
- raster dimension and total-pixel checks before allocation;
- verified model archive SHA-256 and extraction limits;
- plugin package checksum, ABI, path, symlink, count, and size validation;
- structured diagnostics instead of silent fidelity loss.

These controls reduce risk but do not turn native process plugins into untrusted-code sandboxes.

---

## Known boundaries and GA work

The 3.0 alpha is a contract-feedback build, not a release candidate. The following remain explicit boundaries:

- integrate verified remote packages with the public WASI execution runtime before advertising untrusted third-party execution;
- complete independent security review and longer fuzzing of registry/update policy before GA;
- collect production OCR compatibility and accuracy evidence beyond browser orientation-model compatibility smoke tests;
- define supported Office/PDF fidelity guarantees against representative corpora and platforms;
- continue longer fuzzing, benchmark trend storage, browser coverage, and mobile memory profiling.

See the [Core 3 release checklist](docs/release-checklist.md) and [schema version plan](docs/v3/schema-versions.md).

---

## Documentation

| Document | Purpose |
|---|---|
| [Core 3 architecture](docs/v3/architecture.md) | Stacked delivery status, version boundaries, and trust model |
| [WASI Component host](docs/v3/wasi-component-host.md) | WIT v1 authority, limits, package boundary, and diagnostics |
| [Core 3 migration](docs/v3/migration-from-v2.md) | Strict v2-to-v3 contract migration guidance |
| [Production capabilities](docs/production-capabilities.md) | Stability, fidelity, and unsupported-feature policy |
| [WASM adapter](docs/wasm.md) | Browser API, Worker runtime, cache, downloads, and model validation |
| [Plugin system](docs/plugin.md) | Execution classes, package verification, permissions, and security boundaries |
| [CLI option matrix](docs/cli-option-matrix.md) | Option propagation and supported combinations |
| [Export](docs/export.md) | Visual and package export behavior |
| [Benchmarks](docs/benchmark.md) | Native and browser performance benchmark methodology |
| [OCR evaluation](docs/ocr-evaluation.md) | Licensed corpora, accuracy metrics, gates, and evidence identity |
| [Release checklist](docs/release-checklist.md) | RC requirements, GA blockers, and future work |
| [Architecture](docs/architecture.md) | Core architecture overview |
| [Pipeline](docs/pipeline.md) | Recognition and processing pipeline |
| [Testing](docs/testing.md) | Test strategy and fixtures |

---

## Related projects

- [LaTeXSnipper Office](https://github.com/strangelion/LaTeXSnipper-Office) — desktop application and Office/WPS integrations.
- [LaTeXSnipper Mobile](https://github.com/strangelion/LaTeXSnipper_mobile) — mobile integration work.
- [SakuraMathcraft/LaTeXSnipper](https://github.com/SakuraMathcraft/LaTeXSnipper) — the original Python desktop project and an important source of models and post-processing ideas.

---

## License

LaTeXSnipper Core is licensed under the **GNU Affero General Public License v3.0**.

Commercial use is not categorically prohibited by the license. Distribution of covered works, and operation of modified versions for users over a network, can require providing the corresponding source code under AGPL-3.0. This paragraph is a practical summary, not legal advice; the license text is authoritative.

Third-party models and dependencies retain their own licenses. Review model manifests and release metadata before redistribution.

---

## Acknowledgements

LaTeXSnipper Core builds on work from the Rust, ONNX, OCR, document-processing, and browser ecosystems, including PaddleOCR, OpenOCR, Microsoft TrOCR, Table Transformer, RapidAI, Ultralytics, ONNX Runtime, Tract, `image`, `tokio`, `serde`, `clap`, `wasm-bindgen`, and many other open-source projects.

Contributions should preserve capability accuracy: do not mark a feature stable merely because a code path exists, and do not equate package validity with semantic, visual, editable, or round-trip fidelity.
