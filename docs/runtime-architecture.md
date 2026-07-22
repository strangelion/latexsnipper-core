# Runtime Architecture

> 模型 → RuntimeVariant → RuntimeResolver → RuntimeRegistry → RuntimeFactory → RuntimeSession

## 核心流程

```
ModelManifest.runtimeVariants[]
        │
        ▼
RuntimeResolver.resolve()
        │  遍历 variants（按 priority 降序）
        │  检查 platforms / capabilities / RuntimeRegistry
        │
        ▼
ResolvedRuntimeVariant { runtime_kind, artifacts, ... }
        │
        ▼
RuntimeRegistry.create_resolved_session()
        │  找到对应的 RuntimeFactory
        │  加载 artifacts（model, encoder, decoder, tokenizer...）
        │
        ▼
RuntimeSession { run(), metadata(), ... }
```

## RuntimeKind 枚举

```text
RuntimeKind::OnnxRuntime      ← ONNX Runtime（CPU / CUDA / DirectML / TensorRT EP / ...）
RuntimeKind::PaddleInference  ← Paddle Inference Native
RuntimeKind::ExecuTorch       ← ExecuTorch（XNNPACK / Core ML delegate / ...）
RuntimeKind::TensorRt         ← TensorRT standalone（原生 ONNX 解析 + engine build）
RuntimeKind::TensorRtRtx      ← TensorRT-RTX（AOT/JIT，RTX GPU 专用）
RuntimeKind::CoreMl           ← Apple Core ML（.mlpackage / .mlmodelc）
RuntimeKind::Custom(...)      ← 第三方 Runtime 插件（WASM / dlopen）
```

## 关键设计决策

### 1. Runtime 只管 Session/Tensor，不管 Pipeline

Core engine 只认识 `RuntimeFactory` 和 `RuntimeSession`，不直接引用 ORT/Paddle/ExecuTorch API。
这允许同一个模型在不同 Runtime 间切换而无需修改上层代码。

### 2. 多 Runtime 不是"所有模型强制 ONNX"

PP-FormulaNet-S 拥有完整的 Paddle 原生控制流（while、KV cache、parallel generation），
强制 ONNX 导出会丢失语义。其正式路径是 Paddle Native only，没有 ONNX fallback。

ONNX 模型（TrOCR、DBNet、CTC、YOLOv8、TATR、SLANet、PicoDet）继续以 ONNX Runtime 作为首选。

### 3. Fallback 是模型级别，不是 Runtime 级别

当检测到 Paddle Runtime 不可用时，系统回退到**不同的模型**（如 TrOCR），而不是
同一个模型的错误 ONNX 版本。这使得每个 Runtime 路径都可以保持语义正确性。

### 4. RuntimeVariant 是模型包的自描述

每个模型包携带自己的 `runtimeVariants`（在 `config.json` 中），因此即使没有
远程 catalog，系统也能知道如何加载模型。

## 分层图

```
┌──────────────────────────────────────────────────┐
│                   Engine                         │
│  (runtime_registry.rs, model selection policy)   │
├──────────────────────────────────────────────────┤
│                 Pipeline                         │
│  (formula recognizer, text detector, ...)        │
├──────────────────────────────────────────────────┤
│                Inference                         │
│  (adapters: PPFormulaNet, TrOCR, DBNet, ...)     │
├──────────────────────────────────────────────────┤
│                  Runtime                         │
│  ┌──────────┬───────────┬──────────┬──────────┐  │
│  │  ONNX    │  Paddle   │ExecuTorch│ TensorRT │  │
│  │ Runtime  │ Inference │          │          │  │
│  ├──────────┼───────────┼──────────┼──────────┤  │
│  │ Core ML  │  Custom   │  Remote   │          │  │
│  │          │  Plugin   │  API      │          │  │
│  └──────────┴───────────┴──────────┴──────────┘  │
├──────────────────────────────────────────────────┤
│               Model Registry                     │
│  (model-manifest.json, ModelResolver, checksums) │
└──────────────────────────────────────────────────┘
```

## 相关文档

- [Model Package Format](model-package-format.md) — `runtimeVariants` 的完整 JSON schema
- [Runtime Development](runtime-development.md) — 如何实现新的 Runtime
- [Custom Runtime Plugin](custom-runtime-plugin.md) — 第三方 Runtime 插件 ABI
