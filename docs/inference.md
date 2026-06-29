# Inference Crate

> 推理能力 — Detection, Recognition

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `formula_detector` | formula_detector.rs | YOLOv8 公式检测 |
| `formula_recognizer` | formula_recognizer.rs | TrOCR 编码器 + 解码（greedy/beam search） |
| `text_detector` | text_detector.rs | DBNet 文字检测 + Moore 轮廓 |
| `text_recognizer` | text_recognizer.rs | CRNN + CTC 解码 |
| `types` | types.rs | DetectionBox, RecognitionResult |

## 参数类型

所有推理函数接受配置参数，从 `config.json` 加载：

| 参数类型 | 说明 | 构造方法 |
|----------|------|---------|
| `DetectionParams` | 公式检测 | `DetectionParams::from_config(&config)` |
| `RecognitionParams` | 公式识别 | 默认值或从 config 读取 |
| `TextDetParams` | 文字检测 | 默认值或从 config 读取 |
| `TextRecParams` | 文字识别 | 默认值或从 config 读取 |

## 公式检测流程

```
Image → letterbox(target_size) → normalize(mean, std) → Tensor[1,3,H,W]
  → ONNX Session → YOLOv8 output
  → decode (row_major/col_major, apply_sigmoid)
  → confidence filter → NMS → DetectionBox[]
```

### DetectionParams

| 字段 | 默认值 | config.json 来源 | 说明 |
|------|--------|-----------------|------|
| `target_size` | 768 | preprocessing.resize.width/height | 输入尺寸 |
| `conf_threshold` | 0.25 | postprocessing.confidence_threshold | 置信度阈值 |
| `iou_threshold` | 0.45 | postprocessing.iou_threshold | NMS IoU 阈值 |
| `mean` | [0,0,0] | preprocessing.normalization.mean | 归一化均值 |
| `std` | [1,1,1] | preprocessing.normalization.std | 归一化标准差 |
| `apply_sigmoid` | true | postprocessing.apply_sigmoid | 是否对 class scores 做 sigmoid |
| `output_layout` | "row_major" | postprocessing.output_layout | 输出布局 row_major/col_major |

**apply_sigmoid 说明**：
- `true`：模型输出 raw logits，需要 sigmoid 转换为概率（默认）
- `false`：模型已内置 sigmoid 激活，输出直接是概率值

**output_layout 说明**：
- `"row_major"`：数据按 `[N, 6]` 行优先排列，每 6 个连续值为一个 anchor
- `"col_major"`：数据按 `[6, N]` 列优先排列，需要转置后读取

## 公式识别流程

```
Image → resize(img_size) → normalize(mean, std) → Tensor[1,3,H,W]
  → Encoder Session → hidden_states[1,577,384]
  → Greedy/Beam Search → Token Decode → LaTeX repair → String
```

### RecognitionParams

| 字段 | 默认值 | config.json 来源 | 说明 |
|------|--------|-----------------|------|
| `img_size` | 384 | encoder.input.shape | 输入尺寸 |
| `beam_width` | 3 | decoding.beam_width | 束搜索宽度 |
| `top_k` | 5 | decoding.top_k | 每步候选数 |
| `max_tokens` | 256 | decoder.max_length | 最大解码步数 |
| `greedy` | true | decoding.type=="greedy" | 使用 greedy 还是 beam search |
| `mean` | [0.5,0.5,0.5] | preprocessing.normalization.mean | 归一化均值 |
| `std` | [0.5,0.5,0.5] | preprocessing.normalization.std | 归一化标准差 |

## 文字检测流程

```
Image → resize(max_side) → pad(stride) → normalize(mean, std)
  → ONNX Session → probability map
  → Binary threshold → Contour tracing
  → Unclip expansion → Score filter
  → Merge overlapping → DetectionBox[]
```

### TextDetParams

| 字段 | 默认值 | config.json 来源 | 说明 |
|------|--------|-----------------|------|
| `max_side` | 960 | — | 最大边长 |
| `stride` | 32 | preprocessing.divisible_by | 填充步幅 |
| `det_threshold` | 0.3 | postprocessing.threshold | 二值化阈值 |
| `box_threshold` | 0.5 | postprocessing.box_threshold | 框置信度阈值 |
| `unclip_ratio` | 1.6 | postprocessing.unclip_ratio | 轮廓扩展系数 |
| `mean` | [0.5,0.5,0.5] | preprocessing.normalization.mean | 归一化均值 |
| `std` | [0.5,0.5,0.5] | preprocessing.normalization.std | 归一化标准差 |

## 文字识别流程

```
Image → resize(h=target_h) → pad(w=max_w) → normalize(mean, std)
  → ONNX Session → logits[1,seq,vocab]
  → CTC decode (blank_id) → String
```

### TextRecParams

| 字段 | 默认值 | config.json 来源 | 说明 |
|------|--------|-----------------|------|
| `target_h` | 48 | preprocessing.resize.height | 固定高度 |
| `max_w` | 320 | preprocessing.resize.width | 最大宽度 |
| `blank_id` | 0 | decoding.blank_id | CTC blank token |
| `mean` | [0.5,0.5,0.5] | preprocessing.normalization.mean | 归一化均值 |
| `std` | [0.5,0.5,0.5] | preprocessing.normalization.std | 归一化标准差 |

## 关键类型

### DetectionBox

```rust
pub struct DetectionBox {
    pub rect: Rect,
    pub confidence: f32,
    pub class_id: usize,
    pub class_name: String,
}
```

### RecognitionResult

```rust
pub struct RecognitionResult {
    pub text: String,
    pub confidence: f32,
}
```

## 依赖关系

```
Inference
↑ 依赖 Runtime (InferenceSession trait), Image, AST, Model
↓ 被 Pipeline, Mock 依赖
```
