# Models

> 模型下载与管理指南

## 模型清单

模型清单位于 `models/model-manifest.json`，定义了所有可用模型的元数据。

## 可用模型

| 类别 | 模型 ID | 类型 | 大小 | 说明 |
|------|---------|------|------|------|
| formula-det | yolov8-mfd | YOLOv8 | ~77 MB | 公式检测 |
| formula-rec | trocr-deit | TrOCR | ~112 MB | 公式识别 |
| text-det | v6-small | DBNet | ~4.5 MB | 文字检测 |
| text-rec | v6-small | CRNN+CTC | ~16 MB | 文字识别 |
| table-det | tatr-detection | DETR | ~2 MB | 表格区域检测（Microsoft Table Transformer） |
| table-struct | slanet-plus | SLANet | ~10 MB | 表格结构识别（推荐，中文表格友好，PP-Structure v2） |
| table-struct | tatr-structure | DETR | ~2.2 MB | 表格结构识别（学术PDF友好，PubTables-1M） |

## 模型文件结构

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
│   └── v6-small/
│       ├── inference.onnx
│       └── config.json
├── text-rec/
│   └── v6-small/
│       ├── inference.onnx
│       ├── inference.yml
│       └── config.json
├── table-det/
│   └── tatr-detection/
│       ├── model.onnx
│       └── model.onnx.data
└── table-struct/
    ├── tatr-structure/
    │   ├── model.onnx
    │   └── model.onnx.data
    └── slanet-plus/          # 可选，需手动转换
        ├── model.onnx
        ├── config.json
        └── dict.txt
```

## config.json 示例

```json
{
  "modelType": "yolov8",
  "inputShape": [1, 3, 768, 768],
  "preprocessing": {
    "resize": 768,
    "padStride": 32,
    "mean": [0.0, 0.0, 0.0],
    "std": [255.0, 255.0, 255.0]
  },
  "postprocessing": {
    "confThreshold": 0.25,
    "iouThreshold": 0.45
  }
}
```

## 模型下载

模型通过 `model-manifest.json` 中定义的 URL 下载，支持多镜像源。

## 本地模型路径

默认模型目录：项目根目录下的 `models/`

## SHA256 校验

ModelManifest 支持 SHA256 校验，确保模型文件完整性。校验失败时 `ModelManager` 会拒绝加载。

## 后端选择

表格结构识别支持两个后端，通过 `STRUCT_BACKEND` 环境变量或 `TableStructureNode::with_backend()` 切换：

- `tatr` — Microsoft Table Transformer（学术PDF友好）
- `slanet` — SLANet_plus（中文表格推荐）
- `projection` — 投影法 fallback（无需模型）
