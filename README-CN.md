<div align="center">

# LaTeXSnipper Core

**面向 OCR、统一文档 AST 与多格式转换的 Rust 文档理解核心。**

[![CI](https://github.com/strangelion/latexsnipper-core/actions/workflows/ci.yml/badge.svg)](https://github.com/strangelion/latexsnipper-core/actions/workflows/ci.yml)
[![WASM](https://github.com/strangelion/latexsnipper-core/actions/workflows/wasm.yml/badge.svg)](https://github.com/strangelion/latexsnipper-core/actions/workflows/wasm.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.88.0-orange?logo=rust)](Cargo.toml)
[![Workspace](https://img.shields.io/badge/workspace-2.0.0-blue)](Cargo.toml)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20WASM-lightgrey)]()

**原生 ONNX 推理 · Tract/WASM · CLI · Rust SDK · 已校验本地插件**

[![About](assets/About.png)](assets/About.png)

[English](README.md) · [中文](README-CN.md)

</div>

---

## 项目状态

`main` 分支当前面向 **2.0.0 workspace 代码基线**。核心已经完成发布候选级硬化，覆盖原生多平台 CI、浏览器/WASM 打包、真实模型测试、fuzz、依赖审计、插件进程隔离、能力注册表和 CLI 行为。

这并不表示所有格式、模型和插件边界具有相同成熟度。

| 领域 | 状态 | 能力约定 |
|---|---|---|
| 统一 `Document` AST 与 JSON schema | **稳定** | AST 是导入、识别、转换和导出的共同事实来源。 |
| LaTeX、Markdown、Typst、纯文本、JSON AST | **对已表示 AST 稳定** | 语义内容可编辑；源格式专属宏、字体和精确换行可能丢失。 |
| MathML 与 OMML | **稳定 / best effort** | 公式结构可编辑；应用专属属性与精确排版可能降级。 |
| 原生 OCR 流水线 | **已实现并通过 CI 验证** | 需要兼容的本地模型 profile；准确率和 readiness 取决于模型与模式。 |
| HTML | **Best effort** | 保留文本与数学语义；不会复现任意脚本、CSS 布局和应用对象。 |
| SVG、PNG、PDF | **实验性 / best effort** | 已测试结构或视觉输出，但不保证完整布局、字体、可编辑性和 round-trip。 |
| DOCX、PPTX、XLSX | **实验性 / best effort** | 已测试包结构与受支持语义，但不保证 Microsoft Office 视觉一致性。 |
| WASM 语义转换 | **稳定** | 浏览器 target 有意排除原生二进制 exporter。 |
| WASM Tract 识别 | **实验性** | 模型驱动、异步执行，并在 Chrome/Firefox 测试；浏览器表格和手写流水线尚不可用。 |
| 内置 Rust 插件 | **Host 行为稳定** | 已实现确定性排序、typed hook、事务 patch、失败策略、soft deadline 和 quarantine。 |
| 隔离 native process 插件 | **仅限经过审核的本地代码** | 支持 hard timeout 和资源控制，但不是操作系统文件/网络沙箱。 |
| WASI Component host、native 动态库 ABI、远程插件安装 | **不可用** | 不应将这些能力宣传为已可执行。 |

可执行能力注册表是事实来源：

```bash
snipper capabilities --format json
snipper capabilities --format json --input docx --output png
```

精确稳定性定义与损失模型见[生产能力与保真策略](docs/production-capabilities.md)。

<!-- capability-inputs: PNG,JPEG,WebP,BMP,TIFF,GIF,SVG,PDF,DOCX,PPTX,XLSX,HTML,Markdown,LaTeX,Typst,MathML,OMML,JSON AST,Plain text -->
<!-- capability-outputs: JSON AST,Plain text,Markdown,LaTeX,Typst,HTML,MathML,OMML,SVG,PDF,PNG,DOCX,PPTX,XLSX -->

---

## LaTeXSnipper Core 提供什么

LaTeXSnipper Core 不只是一个“图片转 LaTeX”封装。它为桌面应用、浏览器应用、Office 集成、移动端适配器和命令行工作流提供共同的文档模型与执行层。

- **统一文档 AST** — 页面、块、inline、公式、表格、资产、几何、样式、诊断、脚注、修订、引用和无障碍元数据。
- **模型驱动识别** — 通过模型 profile 和流水线模式组合公式、文本、混合文档、版式、方向、表格和手写组件。
- **多格式导入与转换** — 语义格式、栅格资产、PDF、SVG 与 Office Open XML 包。
- **二进制安全导出** — 文本与二进制产物使用不同表示，并携带 MIME、SHA-256、字节长度、资产和诊断。
- **原生与浏览器执行** — 支持桌面 target 的 ONNX Runtime，以及 WebAssembly 中的 Tract。
- **完整操作工具** — CLI、SDK、能力查询、模型管理、插件包管理、诊断、批处理报告、shell 补全和 man page。
- **安全导向解析** — 签名优先检测、有界 archive/XML 处理、安全包路径、checksum 校验和结构化失败。

---

## 架构

```text
应用与适配器
├── Desktop / Office / 移动端集成
├── CLI
├── Rust SDK / FFI
└── Browser Worker + WASM
                │
                ▼
引擎与流水线
输入字节或像素
→ 解码 / 导入 / PDF 渲染
→ 版式与区域候选
→ 按模式选择识别器
→ 区域消解与阅读顺序
→ 统一 Document AST
                │
                ▼
转换与导出
├── 语义文本：LaTeX / Markdown / Typst / HTML / MathML / OMML
├── 结构化数据：JSON AST / 纯文本
├── 视觉输出：SVG / PNG / PDF
└── 文档包：DOCX / PPTX / XLSX
```

核心业务逻辑与平台无关。UI、相机、浏览器 API、Office 自动化和操作系统集成应位于 adapter 或具体应用中。

---

## 输入、识别与转换行为

### 栅格图片

栅格导入与 OCR 是两个明确分离的概念：

- `snipper import image.png`：将原始扫描图作为 AST 资产保留。
- `snipper recognize -i image.png ...`：运行已配置的 OCR 流水线。
- 未显式执行 recognition 时，栅格图到语义文本的直接转换会报告不可用。

这样可以避免普通 importer 在未声明的情况下静默执行模型推理。

### PDF

PDF 有两条不同路径：

1. **原生 PDF 提取** — 从内容流进行 best-effort 文本提取。
2. **页面渲染 OCR** — 通过 `pdftoppm` 或 `mutool` 渲染页面，再运行识别。

任意字体编码、缺失 `ToUnicode` 映射、阅读顺序、复杂图形状态变换与扫描页都会影响保真度。

### Office 文档包

DOCX、PPTX 和 XLSX reader/writer 会保留受支持的文本、公式、表格、资产和包结构。不支持的对象可能降级、在启用 preservation 时作为 opaque part 保留，或通过诊断报告。

文档包能够重新打开，只能证明**结构有效**，不能证明与 Microsoft Office 完全视觉一致。

### 保真维度

项目将以下能力视为相互独立：

- 包结构有效性；
- 语义保留；
- 布局保留；
- 视觉保真；
- 可编辑性；
- round-trip 保真。

不能由其中一项推导另一项。

---

## CLI

### 构建

```bash
git clone https://github.com/strangelion/latexsnipper-core.git
cd latexsnipper-core

cargo build --release -p latexsnipper-cli
```

二进制输出位置：

```text
target/release/snipper       # Linux/macOS
target/release/snipper.exe   # Windows
```

已发布的二进制和模型包请查看 [GitHub Releases](https://github.com/strangelion/latexsnipper-core/releases)。

### 先检查环境

```bash
snipper version
snipper doctor
snipper capabilities
snipper capabilities --format json
```

### 模型与识别

```bash
snipper models download
snipper models verify

snipper recognize -i scan.png -f latex -o result.tex
snipper recognize -i page.png -f markdown --recognize-mode mixed -o page.md
```

识别需要兼容的本地模型资产。`snipper doctor` 和 `snipper capabilities` 会报告真实 readiness，而不是假设模型已经存在。

### 导入、转换与导出

```bash
# 导入为统一 JSON AST
snipper import report.docx -o report.ast.json

# 通过统一 AST 转换
snipper convert report.docx --to markdown -o report.md
snipper convert notes.md --to typst -o notes.typ

# 渲染/导出 AST 或受支持文档
snipper export report.ast.json --to pdf -o report.pdf

# 检查或验证输入
snipper inspect report.pdf --json
snipper validate report.docx
```

### 批量转换

```bash
snipper convert documents \
  --to markdown \
  --recursive \
  --output-dir converted \
  --jobs 4 \
  --continue-on-error \
  --report conversion-report.json
```

CLI 还支持原子写入、no-clobber/force 策略、strict diagnostics、warning 失败、页码范围、JSON/SARIF 诊断、稳定退出码、shell 补全和 roff man page。

### 插件包

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

远程 URL 安装仍保持禁用。

---

## Rust SDK

在本 workspace 中使用 native engine feature：

```toml
[dependencies]
latexsnipper-engine = { path = "crates/engine", features = ["native"] }
```

### 一行完成图片识别

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

为保持向后兼容，栅格图片传入 `Snipper::from_file` 时会执行 OCR。配置的模型目录中必须存在兼容模型。

### 不执行 OCR 的文档导入

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

内存字节导入使用 `Snipper::from_bytes`；应用已经持有 `Document` AST 时使用 `Snipper::from_document`。

---

## WebAssembly 与浏览器执行

WASM 构建使用 **Tract**，不会链接 ONNX Runtime、原生 PDF/PNG/Office exporter、原生文件系统 downloader 或 native Tokio runtime。

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

v2 浏览器 API 包括：

- `api_info_v2()` 与 `capabilities_v2()`；
- 带校验的模型加载、卸载、清空和事务更新；
- 异步 `recognize_v2()` 与进度报告；
- 直接调用的 stage-boundary cooperative cancellation；
- binary-safe `convert_v2()` envelope。

推荐使用 `WasmWorkerClient` 作为浏览器入口：

- 单个 active recognition + 有界队列；
- public request ID 与 internal wire ID 分离；
- progress event 与 stale-response suppression；
- 通过 terminate Worker 实现 hard cancellation；
- Worker 重启并重新加载已校验模型；
- RPC timeout 与 `AbortSignal`；
- recovery 失败时返回结构化错误，不会无限重启。

JavaScript 包还提供：

- 仅存储 SHA-256 已校验模型的 IndexedDB cache；
- schema migration 与 LRU budget eviction；
- 可中止流式下载；
- 最大体积限制；
- 下载进度；
- mirror fallback；
- best-effort 或 required cache policy。

浏览器表格和手写流水线尚不可用。Production-derived WASM 模型测试证明模型/运行时兼容性，不证明 OCR 准确率。

详见 [WASM adapter](docs/wasm.md)。

---

## 插件系统与安全边界

### 可信进程内插件

可信 Rust 插件支持：

- typed hook；
- 确定性排序与依赖检查；
- 事务 `DocumentPatch`；
- `Stop`、`Continue`、`DisablePlugin`、`Rollback` 策略；
- panic containment；
- cooperative cancellation 与 deadline；
- 单插件并发上限；
- soft timeout 后 quarantine。

任意进程内 Rust 线程无法安全强杀。Soft timeout 返回后，插件代码可能仍在协作退出或继续运行。

### 隔离进程插件

版本化 process host 提供：

- JSON request/response IPC；
- ABI/protocol 兼容性检查；
- SHA-256 已校验本地包；
- hard deadline termination；
- Unix session/process-group containment；
- Windows Job Object containment；
- memory 与 response-file observation limit；
- 私有临时工作目录；
- 严格 success/error 响应验证；
- 跨进程 registry lock 与抗崩溃原子替换。

> **安全边界：** native process plugin 不是操作系统文件/网络沙箱。Manifest permission 只约束 brokered host operation；任意 native 代码仍可直接调用操作系统 API。仅运行经过审核的本地 process plugin。

仍不可用：

- WASI Component 执行 host；
- 稳定 native 动态库插件 ABI；
- 远程插件 registry/install/update 信任模型；
- 完整 native 文件系统/网络沙箱。

详见 [Plugin system](docs/plugin.md)。

---

## 模型

模型 readiness 由 profile 驱动。Manifest、checksum、配置、tokenizer、key 文件和 runtime compatibility 必须完整，能力才会报告 ready。

| 模型系列 | 主要用途 | 状态 |
|---|---|---|
| YOLOv8-MFD | 公式检测 | 默认原生 profile |
| TrOCR-DeiT | 公式识别 | 默认原生 profile |
| PP-OCRv6 Det / Rec | 多语言文本检测与识别 | 默认原生 profile |
| OpenOCR Mobile Det / Rec | 替代 DBNet/CTC 文本流水线 | 实验性 |
| PP-DocLayout v3 | 文档版式分析 | 实验性 |
| TATR Detection / Structure | 表格检测与结构识别 | 实验性 |
| SLANet Plus | 替代表格结构后端 | 实验性 |
| PP-LCNet 方向模型 | 文档/文本行方向和兼容性测试 | 实验性 / test profile |

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

请使用已校验 downloader，不要直接复制未经验证的模型文件：

```bash
snipper models download
snipper models list
snipper models verify
```

模型包仍受各自上游许可证约束。精确 source revision 和 checksum 请以模型 manifest 与 release asset 为准。

---

## Workspace

```text
crates/
├── foundation/   错误、诊断、配置、事件
├── ast/          统一 Document AST 与公开文档类型
├── tensor/       与运行时无关的张量类型
├── image/        解码、变换、归一化、PDF 页面渲染
├── runtime/      RuntimeBackend、ONNX Runtime provider、模型解析
├── model/        manifest、profile、校验、downloader 元数据
├── inference/    detector 与 recognizer adapter
├── pipeline/     stage graph、区域消解、阅读顺序
├── syntax/       LaTeX、Typst、Markdown parser/renderer
├── conversion/   importer、语义转换、Office package 处理
├── export/       SVG、PNG、PDF 与文本渲染
├── engine/       编排、SDK、job、metrics
├── api-types/    版本化公开 request/response 类型
├── tract/        基于 Tract 的 WASM runtime backend
├── plugin/       内置与 isolated-process 插件基础设施
├── ffi/          Android JNI 与 iOS C-facing adapter
├── wasm/         wasm-bindgen API
├── cli/          `snipper` 命令行应用
├── mock/         确定性测试替身
└── tests/        集成测试与 benchmark

fuzz/             cargo-fuzz target 与 seed corpus
docs/             架构、能力、安全与发布文档
```

---

## 验证与 CI

### 原生检查

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --doc --workspace --all-features
```

### Fuzz

仓库包含真实 `cargo-fuzz` target，覆盖格式检测、ZIP/OOXML 包、XML、SVG、PDF、JSON AST、LaTeX、Typst、Markdown math、模型 manifest 和插件 manifest。

```bash
cargo install cargo-fuzz
cargo +nightly fuzz list
cargo +nightly fuzz run format_signature
```

PR CI 会编译 fuzz target；Scheduled hardening 会运行有界 libFuzzer campaign，并在失败时上传 crash artifact。

### 持续集成覆盖

CI 覆盖：

- Windows、Linux、macOS workspace 检查；
- formatting、strict Clippy、MSRV、测试、doc test、feature matrix；
- 原生真实模型流水线；
- parser security 与 round-trip corpus；
- Chrome 与 Firefox 浏览器测试；
- web、bundler、Node WASM package；
- TypeScript、Worker、IndexedDB、download、ESM、Vite 测试；
- 依赖审计、模型 URL 校验、benchmark 与 scheduled fuzz。

Benchmark smoke 用于验证执行路径，不会在共享 runner 上设置脆弱的时间阈值。详见 [Benchmarks](docs/benchmark.md)。

---

## 安全与信任模型

主要控制包括：

- 签名/包结构优先的格式检测；
- 对 hint 不匹配和加密包返回 typed error；
- archive entry 数量、解压体积、压缩比和路径穿越限制；
- XML 深度/元素、DTD/entity 与 external relationship 限制；
- 分配前检查栅格尺寸和总像素；
- 模型 archive SHA-256 与解压限制；
- 插件包 checksum、ABI、路径、symlink、文件数量和体积校验；
- 使用结构化诊断，而不是静默丢失保真度。

这些控制会降低风险，但不会把 native process plugin 变成不可信代码沙箱。

---

## 已知边界与 GA 工作

2.0.0 代码基线适合进行 release-candidate 评估，但仍有以下明确边界：

- 在宣传执行不可信第三方插件前，实现并验证 WASI Component host；
- 在启用远程插件安装前，完成 registry、signature、provenance 与 update policy；
- 补充超出浏览器方向模型兼容性 smoke 的生产 OCR 兼容性和准确率证据；
- 基于代表性 corpus 与平台定义 Office/PDF 保真保证；
- 继续更长时间 fuzz、benchmark 趋势存储、更多浏览器覆盖和移动端内存 profile。

详见 [v2.0.0-rc.1 release checklist](docs/release-checklist.md)。

---

## 文档

| 文档 | 用途 |
|---|---|
| [生产能力](docs/production-capabilities.md) | 稳定性、保真和不支持能力策略 |
| [WASM adapter](docs/wasm.md) | 浏览器 API、Worker、cache、下载与模型验证 |
| [插件系统](docs/plugin.md) | 执行类别、包校验、权限和安全边界 |
| [CLI option matrix](docs/cli-option-matrix.md) | 参数传播与支持组合 |
| [Export](docs/export.md) | 视觉和文档包导出行为 |
| [Benchmarks](docs/benchmark.md) | 原生与浏览器 benchmark 方法 |
| [Release checklist](docs/release-checklist.md) | RC 条件、GA blocker 与后续工作 |
| [Architecture](docs/architecture.md) | 核心架构概览 |
| [Pipeline](docs/pipeline.md) | 识别与处理流水线 |
| [Testing](docs/testing.md) | 测试策略与 fixture |

---

## 相关项目

- [LaTeXSnipper Office](https://github.com/strangelion/LaTeXSnipper-Office) — 桌面应用与 Office/WPS 集成。
- [LaTeXSnipper Mobile](https://github.com/strangelion/LaTeXSnipper_mobile) — 移动端集成工作。
- [SakuraMathcraft/LaTeXSnipper](https://github.com/SakuraMathcraft/LaTeXSnipper) — 原始 Python 桌面项目，也是模型与后处理思路的重要来源。

---

## 许可证

LaTeXSnipper Core 采用 **GNU Affero General Public License v3.0**。

该许可证并不笼统禁止商业使用。分发受许可证覆盖的程序，或通过网络向用户提供修改版本时，可能需要按照 AGPL-3.0 提供对应源码。本段仅为实用摘要，不构成法律意见；具体以许可证正文为准。

第三方模型和依赖保留各自许可证。重新分发前请检查模型 manifest 与 release metadata。

---

## 致谢

LaTeXSnipper Core 建立在 Rust、ONNX、OCR、文档处理和浏览器生态的众多开源项目之上，包括 PaddleOCR、OpenOCR、Microsoft TrOCR、Table Transformer、RapidAI、Ultralytics、ONNX Runtime、Tract、`image`、`tokio`、`serde`、`clap`、`wasm-bindgen` 等。

贡献代码时应保持能力描述准确：不能因为存在代码路径就将功能标记为稳定，也不能将文档包结构有效等同于语义、视觉、可编辑或 round-trip 保真。
