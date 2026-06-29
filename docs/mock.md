# Mock Crate

> 测试用 Fake 实现 — 无需真实模型即可验证全流程

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `fake_detector` | fake_detector.rs | 返回固定 DetectionBox 的 Mock 检测器 |
| `fake_recognizer` | fake_recognizer.rs | 返回固定 RecognitionResult 的 Mock 识别器 |
| `fake_pipeline` | fake_pipeline.rs | 组合 FakeDetector + FakeRecognizer 的完整 Pipeline |
| `fake_document` | fake_document.rs | 预置的混合 Document（公式+文字） |

## FakeDetector

```rust
pub struct FakeDetector { boxes: Vec<DetectionBox> }
```

| 构造方法 | 说明 |
|----------|------|
| `FakeDetector::new(boxes)` | 自定义检测结果 |
| `FakeDetector::single_formula(conf)` | 单个公式框 |
| `FakeDetector::single_text(conf)` | 单个文字框 |
| `FakeDetector::empty()` | 空结果 |

## FakeRecognizer

```rust
pub struct FakeRecognizer { results: Vec<RecognitionResult> }
```

| 构造方法 | 说明 |
|----------|------|
| `FakeRecognizer::new(results)` | 自定义识别结果 |
| `FakeRecognizer::formula(latex, conf)` | 单个公式 |
| `FakeRecognizer::text(text, conf)` | 单行文字 |
| `FakeRecognizer::from_detections(detections, texts)` | 按检测框配对 |

## FakePipeline

组合 FakeDetector + FakeRecognizer，模拟完整 OCR 流程：

```rust
pub struct FakePipeline {
    detector: FakeDetector,
    recognizer: FakeRecognizer,
}
```

| 构造方法 | 说明 |
|----------|------|
| `FakePipeline::formula(latex, conf)` | 公式识别 Pipeline |
| `FakePipeline::text(text, conf)` | 文字识别 Pipeline |
| `FakePipeline::mixed(formula, text, conf)` | 混合识别 Pipeline |

`run(&self, image)` 返回完整 `Document`：
- `isolated`/`embedding` 类 → `Block::Formula`
- `text` 类 → `Block::Paragraph`

## fake_document()

返回预置 Document：
```
"Given the equation " (text)
"E=mc^2" (inline formula)
", we can derive the following:" (text)
"\frac{a+b}{c}" (display formula)
```

## 依赖关系

```
Mock
↑ 依赖 AST, Image, Inference (类型)
↓ 被 Engine, CLI, Tests 使用
```
