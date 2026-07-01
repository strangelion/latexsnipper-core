<div align="center">

# LaTeXSnipper Core

**可组合的 Rust 数学 OCR 引擎，支持文档理解和多格式处理。**

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)]()
[![License](https://img.shields.io/badge/License-AGPL--3.0-blue)]()
[![Status](https://img.shields.io/badge/Status-架构稳定-yellow)]()
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20WASM-lightgrey)]()

**一次构建，处处运行。**

单一 Rust 核心驱动桌面端、移动端、Office 插件和 Web 应用。

[![About](assets/About.png)]()

[English](README.md) · [中文](README-CN.md)

</div>

---

## 为什么选择 LaTeXSnipper Core？

| 特性 | 说明 |
|------|------|
| **平台无关** | 纯 Rust 架构，无 UI 依赖 — 可在桌面、移动端、Office 或 Web 上运行 |
| **统一文档 AST** | 文档结构的唯一数据源，独立于任何输出格式 |
| **多 OCR 运行时** | 可互换后端：ONNX Runtime、TensorRT、NCNN — 选择适合你平台的方案 |
| **多格式转换** | 从一个 AST 生成 12 种输出格式：LaTeX、OMML、MathML、Typst、Markdown、HTML |
| **流式流水线** | 异步节点图，支持取消、进度跟踪和并行执行 |
| **为各平台设计** | 桌面端（Windows/macOS/Linux）、移动端（Android/iOS）、Office 插件、Web（WASM） |

> 架构和 crate 边界已稳定。大部分实现仍在积极开发中。

---

## 架构

LaTeXSnipper Core 采用严格的**四层架构**：

| 层级 | 职责 |
|------|------|
| **Platform** | UI、相机、权限 — 属于各平台应用 |
| **Adapter** | JNI、WASM、Office.js、CLI — 将平台类型转换为 Core 类型 |
| **Core** | AST、推理、流水线、转换、导出 — 全部业务逻辑 |
| **Runtime** | ONNX Runtime、Stub — 可替换的推理后端 |

> Core 永远不知道是谁在调用它。它只关心输入、处理和输出。

---

## 模块依赖关系

```
Engine
  ├── Conversion (LaTeX/OMML/MathML/Typst/Markdown/HTML)
  ├── Export (SVG/Text)
  ├── Syntax (解析器 + 渲染器)
  ├── Pipeline (节点图)
  │     ├── Inference (检测 + 识别)
  │     │     ├── Runtime (ONNX/Stub)
  │     │     └── Image (解码/缩放/归一化)
  │     └── AST (文档数据模型)
  └── Model (清单 + 配置)
        └── Foundation (错误/日志/事件/配置)
```

---

## 识别流水线

![流水线](assets/pipeline.svg)

```
图像 → 预处理 → 检测 → 裁切 → 识别 → 文档 AST → 输出
         │          │          │          │
     letterbox    YOLOv8    TrOCR     LaTeX/OMML
      normalize    DBNet    Beam Search MathML/Typst
```

---

## 功能列表

### 稳定版

| 能力 | 状态 | 说明 |
|------|------|------|
| **AST** | ✅ | Document → Page → Block → Inline → Formula |
| **图像** | ✅ | SnipperImage、ImageView、解码、缩放、归一化 |
| **转换** | ✅ | 12 种格式：LaTeX、OMML、MathML、Typst、Markdown、HTML |
| **语法** | ✅ | LaTeX/Typst/Markdown 解析器 + 渲染器 |
| **流水线** | ✅ | 异步节点流水线，支持取消 |

### 实验版

| 能力 | 状态 | 说明 |
|------|------|------|
| **推理** | 🚧 | YOLOv8 检测、TrOCR 识别、CRNN+CTC |
| **运行时** | 🚧 | ONNX Runtime（会话缓存）+ Stub |
| **引擎** | 🚧 | JobQueue、Service trait、Request/Response Builder、Streaming API |
| **模型** | 🚧 | 清单、配置、SHA256 校验 |
| **插件** | 🚧 | Plugin trait、Registry、Request/Response |
| **FFI** | 🚧 | Android JNI、iOS C FFI |
| **WASM** | 🚧 | parse/render/convert 绑定 |
| **CLI** | 🚧 | recognize/parse/render/version |
| **导出** | 🚧 | SVG、Text 生成器 |

### 规划中

| 能力 | 状态 | 说明 |
|------|------|------|
| **表格识别** | ○ | 表格结构检测和解析 |
| **手写识别** | ○ | 手写文本识别 |
| **公式布局** | ○ | 复杂公式布局分析 |
| **多页处理** | ○ | 多页文档处理 |

---

## 工作空间

```
crates/
├── foundation/     ✅ 错误、Result、日志、配置、事件总线
├── ast/            ✅ 文档 AST — 唯一数据源
├── tensor/         ✅ 推理 I/O 张量
├── image/          ✅ 平台无关图像处理
├── runtime/        🚧 RuntimeBackend + InferenceSession trait
├── model/          🚧 模型清单、配置、管理
├── inference/      🚧 检测 + 识别管线
├── pipeline/       ✅ 节点化异步流水线
├── syntax/         ✅ LaTeX/Typst/Markdown 解析器 + 渲染器
├── conversion/     ✅ AST → LaTeX/OMML/MathML/Typst/Markdown/HTML
├── export/         🚧 RenderTree → SVG/Text
├── engine/         🚧 SnipperEngine — 主入口
├── plugin/         🚧 插件 API（Plugin trait、Registry）
├── mock/           ✅ 测试用 Fake 实现
├── ffi/            🚧 Android JNI + iOS C FFI
├── wasm/           🚧 WebAssembly 绑定
├── cli/            🚧 命令行工具
└── tests/          ✅ 集成测试（70 个测试）
```

---

## 快速开始

```bash
# 构建
cargo build

# 运行 CLI
cargo run -p latexsnipper-cli -- parse --latex '$\frac{a+b}{c}$'

# 运行全部测试
cargo test --workspace
```

详见 [docs/getting-started.md](docs/getting-started.md)。

---

## 文档

### 架构

| 文档 | 说明 |
|------|------|
| [architecture.md](docs/architecture.md) | 四层架构总览 |
| [pipeline.md](docs/pipeline.md) | 识别流水线设计 |
| [runtime.md](docs/runtime.md) | 运行时后端系统 |
| [engine.md](docs/engine.md) | 引擎和任务队列 |

### 开发者指南

| 文档 | 说明 |
|------|------|
| [getting-started.md](docs/getting-started.md) | 开发者入门指南 |
| [plugin.md](docs/plugin.md) | 插件系统 |
| [testing.md](docs/testing.md) | 测试策略 |

### 参考

| 文档 | 说明 |
|------|------|
| [ast.md](docs/ast.md) | 文档 AST 规范 |
| [syntax.md](docs/syntax.md) | LaTeX/Typst/Markdown 解析器 |
| [conversion.md](docs/conversion.md) | 12 种输出格式 |

### 路线图

| 文档 | 说明 |
|------|------|
| [dual-track.md](docs/dual-track.md) | 开发路线图 |

---

## 设计原则

- **文档优先** — 文档是数据源，不是 LaTeX 或 OCR
- **可组合** — 一切都是节点，一切都是流水线
- **平台无关** — 业务逻辑在 Rust，UI 在外部
- **运行时可插拔** — ONNX、TensorRT、NCNN 全部可替换

---

## 相关项目

- [LaTeXSnipper Mobile](https://github.com/strangelion/LaTeXSnipper_mobile) — Android 应用
- LaTeXSnipper Office — Office 插件
- [LaTeXSnipper 桌面端](https://github.com/SakuraMathcraft/LaTeXSnipper)
- LaTeXSnipper Web — Web 端（规划中）

所有项目共享同一个 Rust Core。

---

## 许可证

GNU AGPL-3.0。允许学习和个人使用，禁止闭源商业化分发。修改后分发或网络服务必须公开全部源码。
