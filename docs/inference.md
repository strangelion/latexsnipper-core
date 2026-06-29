# Inference Crate

> 推理能力 — Detection, Recognition

## 模块

| 模块 | 文件 | 说明 | 对应 Java |
|---|---|---|---|
| `formula_detector` | formula_detector.rs | YOLOv8 公式检测 | DetPreProcess + FormulaDetPostProcess |
| `formula_recognizer` | formula_recognizer.rs | TrOCR 编码器 + Beam Search 解码 | FormulaRecPreProcess + FormulaRecPostProcess |
| `text_detector` | text_detector.rs | DBNet 文字检测 + Moore 轮廓 | TextDetProcessor |
| `text_recognizer` | text_recognizer.rs | CRNN + CTC 解码 + T2S | TextRecPreProcess + TextRecPostProcess |
| `types` | types.rs | DetectionBox, RecognitionResult | — |

## 公式检测流程

```
Image → letterbox(768) → normalize → Tensor[1,3,768,768]
  → ONNX Session → YOLOv8 output
  → decode (center→corner, confidence filter)
  → NMS (IoU=0.45) → DetectionBox[]
```

## 公式识别流程

```
Image → resize(384) → normalize[-1,1] → Tensor[1,3,384,384]
  → Encoder Session → hidden_states[1,577,384]
  → Beam Search (width=3, top-k=5, max=512 tokens)
  → Token Decode → LaTeX repair → String
```

## 文字检测流程

```
Image → resize(max=960) → pad(stride=32) → normalize
  → ONNX Session → probability map
  → Binary threshold(0.3) → Contour tracing
  → Unclip expansion → Score filter(0.5)
  → Merge overlapping → DetectionBox[]
```

## 文字识别流程

```
Image → resize(h=48) → pad(w=320) → normalize[-1,1]
  → ONNX Session → logits[1,seq,vocab]
  → CTC decode (argmax + blank collapse)
  → Traditional→Simplified Chinese → String
```

## 关键参数

| 参数 | 值 | 说明 |
|---|---|---|
| DET_TARGET_SIZE | 768 | YOLOv8 输入尺寸 |
| CONF_THRESHOLD | 0.25 | 检测置信度阈值 |
| IOU_THRESHOLD | 0.45 | NMS IoU 阈值 |
| BEAM_WIDTH | 3 | 束搜索宽度 |
| TOP_K | 5 | 每步候选数 |
| MAX_TOKENS | 512 | 最大解码步数 |
| TARGET_H | 48 | CRNN 固定高度 |
| MAX_W | 320 | CRNN 最大宽度 |
| DET_THRESH | 0.3 | DBNet 二值化阈值 |
| BOX_THRESH | 0.5 | 文字框最低置信度 |
| UNCLIP_RATIO | 1.6 | 轮廓扩展系数 |

## 依赖关系

```
Inference
↑ 依赖 Runtime, Image, AST
↓ 被 Pipeline 依赖
```
