# Architecture

> LaTeXSnipper Core 全局架构总览

## 设计目标

| 目标 | 说明 |
|------|------|
| 平台无关 | Core 永远不知道 Android / Office / Browser |
| 运行时可替换 | ONNX → TensorRT → NCNN，Pipeline 不变 |
| 数据模型统一 | 所有格式转换都经过 Document AST |
| 能力可组合 | 新增 Table / Handwriting / Chemistry 不修改已有 Pipeline |

---

## 四层架构

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

**依赖方向严格单向：Platform → Adapter → Core → Runtime。**
禁止反向依赖。

---

## Core 内部模块关系

```
                Foundation
                    ↑
        ┌───────────┼───────────┐
        │           │           │
     Tensor       AST        Image
        │           │           │
        └─────┬─────┘     ┌─────┘
              │           │
           Runtime      Model
              │
          Inference
              │
          Pipeline
           ↗    ↖
     Syntax    Export
           ↖    ↗
           Conversion
              │
           Engine
              ↑
       FFI / WASM / CLI
```

---

## Crate 职责速查

| Crate | 职责 | 依赖 |
|-------|------|------|
| `foundation` | Error, Result, Logger, Config, EventBus | — |
| `ast` | Document AST 唯一数据源 | foundation |
| `tensor` | 推理 I/O 的多维数组 | foundation |
| `image` | 平台无关图像处理 | ast |
| `runtime` | RuntimeBackend trait + InferenceSession | tensor |
| `model` | 模型 Manifest / Config / 管理 | foundation |
| `inference` | Detection + Recognition 推理能力 | runtime, image, ast |
| `pipeline` | 节点化计算图 | ast, image |
| `syntax` | LaTeX / Typst / Markdown Parser + Renderer | ast |
| `conversion` | AST → 多格式中间表示 | ast, syntax |
| `export` | RenderTree → SVG / Text / PDF | ast, syntax |
| `engine` | 统一入口，组织所有 Capability | 所有 Core crate |
| `plugin` | 插件 API | engine |
| `mock` | 测试用 Fake 实现 | ast, image, inference |
| `ffi` | Android JNI / iOS C FFI | engine |
| `wasm` | WebAssembly 绑定 | engine |
| `cli` | 命令行工具 | engine |

---

## 核心数据流

```
Image
  ↓  Image Crate (decode, resize, normalize)
Processed Image
  ↓  Inference (Detection → Recognition)
DetectionBox[]
  ↓  Inference (Recognition → LaTeX/Text)
Document AST
  ↓  Syntax (Parser / Renderer)
Document AST (enriched)
  ↓  Conversion (AST → 格式化输出)
Formatted Result
  ↓  Export (RenderTree → Generator)
Final Output (SVG / Text / PDF / ...)
```

**关键约束：OCR 永远输出 Document，不直接输出 LaTeX。**

---

## 关键设计决策

### Decision 1：围绕 Capability 建模，不围绕平台

传统项目按平台拆分（Android 项目 / Office 项目 / Web 项目），每个平台独立实现 OCR。
LaTeXSnipper 按能力拆分（AST / Image / Inference / Pipeline），所有平台共享同一套 Core。

### Decision 2：Adapter 统一输入格式

不同平台的输入类型不同（Bitmap / Canvas / Clipboard），Adapter 负责将它们统一为 `SnipperImage`。
Core 永远不直接处理平台类型。

### Decision 3：Runtime 通过 trait 抽象

```rust
trait RuntimeBackend {
    fn create_session(&self, ...) -> Result<Box<dyn InferenceSession>>;
}
```

Core 只认识 trait，不认识具体 ONNX / TensorRT 实现。替换 Runtime 时，Pipeline 和 Inference 代码不需要修改。

### Decision 4：所有格式转换经过 AST

禁止 LaTeX → OMML 直接转换。必须：LaTeX → AST → OMML。
将 N² 复杂度降为 2N，新增格式只需新增一个 Parser + Renderer。

### Decision 5：Engine 是唯一对外入口

平台永远不直接访问 Pipeline / Runtime / Scheduler。
所有外部调用通过 Engine 的 Public API。
