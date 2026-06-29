# Model Crate

> 模型管理 — Manifest, Config, Download, Cache

## 核心原则

1. **Model 只管模型文件，不管 OCR**
2. **config.json 解析 + 模型文件发现**
3. **Manifest 验证 + SHA256 校验**

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `config` | config.rs | ModelConfig（config.json 解析） |
| `manifest` | manifest.rs | ModelManifest（清单解析 + 校验） |
| `manager` | model.rs | ModelManager（文件系统管理） |

## ModelConfig

从 `config.json` 解析模型元数据：
- model_type, input/output shape, dtype
- preprocessing（resize, pad, stride）
- postprocessing（threshold, NMS）
- 文件发现：find_model_file / find_encoder_file / find_decoder_file / find_tokenizer_file

## ModelManifest

清单结构：
- source_id, source_label, version
- categories → variants → files
- checksums（SHA256）
- mirrors（镜像源）

## ModelManager

- category_dir / variant_dir 路径管理
- is_installed / list_installed / delete_variant

## 测试

7 项测试覆盖：config 解析（含 preprocessing）、manifest 解析/验证、checksum 校验、路径管理。

## 依赖关系

```
Model
↑ 不依赖 Pipeline
↓ 被 Engine 依赖
```
