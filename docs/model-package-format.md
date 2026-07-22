# Model Package Format

> `runtimeVariants` 的完整 JSON schema 和示例

## 包结构

每个模型包包含以下文件：

```text
trocr-deit/
├── config.json          ← 模型配置 + runtimeVariants（运行时自描述）
├── encoder_model.onnx   ← encoder ONNX 模型
├── decoder_model.onnx   ← decoder ONNX 模型
└── tokenizer.json       ← HuggingFace tokenizer
```

打包成 `.zip` 后通过 GitHub Releases 分发，SHA-256 校验和写入 catalog manifest。

## runtimeVariants 字段

```typescript
interface RuntimeVariant {
  id: string;            // 唯一标识，如 "onnx" / "paddle-native"
  runtime: string;       // RuntimeKind 标识：onnx-runtime / paddle-inference / ...
  status: string;        // "stable" | "experimental" | "deprecated" | "disabled" | "broken"
  priority: number;      // 解析优先级（越大越优先）
  artifacts: {           // 模型文件映射（键名是语义标签）
    [key: string]: string;
  };
  platforms?: string[];  // 平台约束（windows / linux-x86_64 / macos / apple / ...）
  capabilities?: string[]; // 要求的 RuntimeCapability（如 "full-inference-program"）
  fallbacks?: string[];  // 回退 variant IDs（按顺序尝试）
}
```

## 完整示例：TrOCR（encoder-decoder ONNX）

```json
{
  "runtimeVariants": [
    {
      "id": "onnx",
      "runtime": "onnx-runtime",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "encoder": "encoder_model.onnx",
        "decoder": "decoder_model.onnx",
        "tokenizer": "tokenizer.json",
        "config": "config.json"
      },
      "platforms": [],
      "capabilities": ["encoder-decoder"],
      "fallbacks": []
    }
  ]
}
```

## 完整示例：PP-FormulaNet-S（Paddle Native only）

```json
{
  "runtimeVariants": [
    {
      "id": "paddle-native",
      "runtime": "paddle-inference",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "model": "inference.json",
        "params": "inference.pdiparams",
        "tokenizer": "tokenizer.json",
        "config": "config.json"
      },
      "platforms": [],
      "capabilities": [
        "full-inference-program",
        "control-flow",
        "kv-cache",
        "parallel-generation"
      ],
      "fallbacks": []
    }
  ]
}
```

## 完整示例：YOLOv8（单文件 ONNX）

```json
{
  "runtimeVariants": [
    {
      "id": "onnx",
      "runtime": "onnx-runtime",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "model": "mathcraft-mfd.onnx",
        "config": "config.json"
      },
      "platforms": [],
      "capabilities": [],
      "fallbacks": []
    }
  ]
}
```

## 完整示例：TATR（带外部数据 ONNX）

```json
{
  "runtimeVariants": [
    {
      "id": "onnx",
      "runtime": "onnx-runtime",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "model": "model.onnx",
        "externalData": "model.onnx.data",
        "config": "config.json"
      },
      "platforms": [],
      "capabilities": [],
      "fallbacks": []
    }
  ]
}
```

## 完整示例：DBnet（带 inference.yml）

```json
{
  "runtimeVariants": [
    {
      "id": "onnx",
      "runtime": "onnx-runtime",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "model": "inference.onnx",
        "inferenceConfig": "inference.yml",
        "config": "config.json"
      },
      "platforms": [],
      "capabilities": [],
      "fallbacks": []
    }
  ]
}
```

## Artifact 键名语义

| 键名 | 含义 | 示例 |
|---|---|---|
| `model` | 单文件模型（检测、CTC、SLANet 等） | `model.onnx` |
| `encoder` | Encoder 模型（encoder-decoder 架构） | `encoder_model.onnx` |
| `decoder` | Decoder 模型 | `decoder_model.onnx` |
| `params` | 参数文件（Paddle `.pdiparams`） | `inference.pdiparams` |
| `tokenizer` | Tokenizer 配置文件 | `tokenizer.json` |
| `config` | 模型配置文件 | `config.json` |
| `inferenceConfig` | 预处理/后处理配置 | `inference.yml` |
| `externalData` | 外部权重数据（>2GB ONNX） | `model.onnx.data` |

## Status 语义

| Status | `is_selectable()` | 含义 |
|---|---|---|
| `stable` | ✅ | 生产就绪 |
| `experimental` | ✅ | 可用但有风险 |
| `deprecated` | ❌ | 将被移除 |
| `disabled` | ❌ | 主动关闭 |
| `broken` | ❌ | 已知不可用 |

## 回退策略

`fallbacks` 是模型级别的回退——当某个 Runtime variant 无法解析时，
系统尝试 `fallbacks` 中列出的 variant id。

注意：**不是** Runtime 级别的回退。如果 PP-FormulaNet-S 的 Paddle Native
不可用，系统回退到 TrOCR（另一个模型），而不是同一个模型的错误 ONNX 版本。

## 相关文档

- [Runtime Architecture](runtime-architecture.md) — Runtime 系统的整体架构
- [Runtime Development](runtime-development.md) — 如何编写新的 Runtime
- [Custom Runtime Plugin](custom-runtime-plugin.md) — 第三方插件 ABI
