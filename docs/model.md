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
| `manager` | manager.rs | ModelManager（文件系统管理） |

## ModelConfig

从 `config.json` 解析模型元数据，支持当前和未来模型架构：

```rust
pub struct ModelConfig {
    // 基础信息
    pub model_type: String,           // "yolov8", "trocr", "dbnet", "crnn_ctc"
    pub model_family: Option<String>,
    pub license: Option<String>,
    pub task_type: Option<String>,    // detection/ocr/classification/segmentation
    pub num_classes: Option<u32>,
    pub dynamic_shapes: Option<bool>,

    // 输入输出
    pub input: Option<InputConfig>,
    pub output: Option<OutputConfig>,
    pub outputs: Option<Vec<OutputConfig>>,  // 多输出模型

    // TrOCR 嵌套结构
    pub encoder: Option<EncoderConfig>,
    pub decoder: Option<DecoderConfig>,

    // 处理配置
    pub preprocessing: Option<PreprocessConfig>,
    pub postprocessing: Option<PostprocessConfig>,
    pub decoding: Option<DecodingConfig>,

    // 扩展
    pub quantization: Option<QuantizationConfig>,
    pub extra: Option<serde_json::Value>,  // 模型特定参数
}
```

### 支持的 model_type

| model_type | task_type | 输入格式 | 说明 |
|------------|-----------|---------|------|
| `yolov8` | detection | flat input/output | YOLO 公式检测 |
| `trocr` | ocr | nested encoder/decoder | TrOCR 公式识别 |
| `dbnet` | detection | flat input/output | PaddleOCR 文字检测 |
| `crnn_ctc` | ocr | flat input/output | PaddleOCR 文字识别 |
| `pplcnet` | classification | flat input/output | 方向分类器 |
| `detr` | detection | flat input/output | DETR 检测器（未来） |
| `sam` | segmentation | 多子网络 | Segment Anything（未来） |

### PostprocessConfig 扩展字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `anchors` | `Vec<Vec<[f32; 2]>>` | Anchor-based 检测器的锚框尺寸 |
| `strides` | `Vec<u32>` | 各特征图下采样倍率 |
| `reg_max` | `Option<u32>` | YOLOv8 DFL 回归上限 |
| `num_queries` | `Option<u32>` | DETR object query 数量 |

### QuantizationConfig

```rust
pub struct QuantizationConfig {
    pub quant_type: Option<String>,      // "int8", "fp16", "bf16", "mixed"
    pub is_quantized: Option<bool>,
    pub accuracy_loss_pct: Option<f32>,  // 相比 FP32 的精度损失
    pub original_size_bytes: Option<u64>,
    pub quantized_size_bytes: Option<u64>,
}
```

### 文件发现方法

| 方法 | 说明 |
|------|------|
| `find_model_file(dir)` | 查找主 .onnx 文件 |
| `find_encoder_file(dir)` | 查找 encoder_model.onnx |
| `find_decoder_file(dir)` | 查找 decoder_model.onnx |
| `find_tokenizer_file(dir)` | 查找 tokenizer.json / ppocrv5_keys.txt |

### 参数提取方法

| 方法 | 返回 | 说明 |
|------|------|------|
| `normalization_mean()` | `Vec<f32>` | 归一化均值 |
| `normalization_std()` | `Vec<f32>` | 归一化标准差 |
| `resize_dimensions()` | `(Option<u32>, Option<u32>)` | 缩放尺寸 |
| `color_format()` | `&str` | "RGB" 或 "BGR" |
| `task_type()` | `&str` | 自动推断或显式指定的任务类型 |
| `has_dynamic_shapes()` | `bool` | 是否支持动态输入尺寸 |

## ModelManifest

清单结构（匹配 `model-manifest.json`）：

```rust
pub struct ModelManifest {
    pub source_id: String,
    pub source_label: String,
    pub version: String,
    pub base_url: String,
    pub mirrors: Vec<String>,
    pub checksums: HashMap<String, String>,
    pub categories: HashMap<String, CategoryInfo>,
}
```

### 模型目录结构

```
models/
├── formula-det/
│   └── yolov8-mfd/
│       ├── mathcraft-mfd.onnx
│       └── config.json
├── formula-rec/
│   └── trocr-deit/
│       ├── encoder_model.onnx
│       ├── decoder_model.onnx
│       ├── tokenizer.json
│       └── config.json
├── text-det/
│   └── ppocrv5-mobile/
│       ├── ppocrv5_mobile_det.onnx
│       └── config.json
└── text-rec/
    └── ppocrv5-mobile/
        ├── ppocrv5_mobile_rec.onnx
        ├── ppocrv5_keys.txt
        └── config.json
```

## ModelManager

- category_dir / variant_dir 路径管理
- is_installed / list_installed / delete_variant

## 依赖关系

```
Model
↑ 不依赖 Pipeline
↓ 被 Engine, Inference 依赖
```
