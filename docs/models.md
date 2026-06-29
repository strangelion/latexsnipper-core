# Models

> 模型下载与管理指南

## 模型清单

模型清单位于 `models-download/model-manifest.json`，定义了所有可用模型的元数据。

## 可用模型

| 类别 | 模型 ID | 类型 | 大小 | 说明 |
|------|---------|------|------|------|
| formula-det | yolov8-mfd | YOLOv8 | ~77 MB | 公式检测 |
| formula-rec | trocr-deit | TrOCR | ~112 MB | 公式识别 |
| text-det | ppocrv5-mobile | DBNet | ~4.5 MB | 文字检测 |
| text-rec | ppocrv5-mobile | CRNN+CTC | ~16 MB | 文字识别 |

## 模型文件结构

每个模型类别包含以下文件：

```
formula-det/
├── mathcraft-mfd.onnx    # 模型文件
└── config.json           # 模型配置

formula-rec/
├── encoder_model.onnx    # 编码器
├── decoder_model.onnx    # 解码器
├── tokenizer.json        # 分词器
└── config.json           # 模型配置

text-det/
├── ppocrv5_mobile_det.onnx
└── config.json

text-rec/
├── ppocrv5_mobile_rec.onnx
├── ppocrv5_keys.txt      # 字符集
└── config.json
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

模型从 GitHub Releases 下载：

```
https://raw.githubusercontent.com/strangelion/LaTeXSnipper_mobile/main/dist-models/
```

每个模型打包为 ZIP 文件：

| ZIP 文件 | 内容 |
|----------|------|
| latexsnipper-formula-det.zip | 公式检测模型 |
| latexsnipper-formula-rec.zip | 公式识别模型 |
| latexsnipper-text-det.zip | 文字检测模型 |
| latexsnipper-text-rec.zip | 文字识别模型 |

## 本地模型路径

默认模型目录：`~/.latexsnipper/models/`

可通过 `EngineConfig.models_dir` 自定义路径。

## SHA256 校验

ModelManifest 支持 SHA256 校验，确保模型文件完整性。校验失败时 `ModelManager` 会拒绝加载。
