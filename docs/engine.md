# Engine Crate

> 核心引擎 — 组织所有 Capability 的宿主

## 职责

Engine 是整个 Core 的入口点：
- 组织 Runtime、Model 等所有 Capability
- 提供统一的 `recognize()` API
- 管理配置和生命周期
- 模型热加载和版本管理
- 性能监控和诊断

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `engine` | engine.rs | SnipperEngine（主编排器） |
| `config` | config.rs | EngineConfig |
| `metrics` | metrics.rs | RecognitionMetrics + MetricsBuilder |
| `job` | job.rs | JobQueue |
| `service` | service.rs | Service trait |

## SnipperEngine

```rust
pub struct SnipperEngine {
    config: EngineConfig,
    runtime: Arc<dyn RuntimeBackend>,
    model_resolver: Option<SharedModelResolver>,
    model_manager: ModelManager,
    job_queue: JobQueue,
    _sessions: Mutex<HashMap<String, CachedSession>>,
}

impl SnipperEngine {
    pub fn new(config, runtime) -> Self;
    pub fn with_model_resolver(config, runtime, resolver) -> Self;
    pub fn runtime(&self) -> &dyn RuntimeBackend;
    pub fn model_manager(&self) -> &ModelManager;
    pub fn config(&self) -> &EngineConfig;
    pub fn model_resolver(&self) -> Option<&SharedModelResolver>;
    pub fn set_model_resolver(&mut self, resolver);
    pub fn has_model(&self, category, variant) -> bool;
    pub fn register_model_package(&mut self, task, package);
    pub fn reload_model(&self, session_key) -> Result<()>;
    pub fn reload_all_models(&self) -> Result<()>;
    pub async fn recognize(&self, image, mode) -> Result<Document>;
    pub async fn recognize_pdf(&self, pdf_path, mode) -> Result<Document>;
}
```

## RecognizeMode

```rust
pub enum RecognizeMode { Formula, Text, Mixed, Handwriting, Table, FormulaLayout }
```

## DocumentParseMode

```rust
pub enum DocumentParseMode {
    SpecializedStable,  // 默认：专用模型链（PP-OCR + TrOCR + TATR）
    OpenOcrText,        // 文本检测/识别使用 OpenOCR mobile 变体
    OpenDocHybrid,      // Layout分析 + 区域路由 + 专用识别器
}
```

通过 `EngineConfig::set_parse_mode()` 设置，`build_pipeline()` 根据模式自动构建不同图结构。

`OpenDocHybrid` 模式自动从 `model-manifest.json` 注册 layout 包。`OpenOcrText` 模式自动将 `text-det`/`text-rec` 变体设为 `openocr-mobile`。

## 模型热加载

```rust
// 重载指定模型
engine.reload_model("formula_det")?;

// 重载所有模型
engine.reload_all_models()?;

// 检查模型可用性
if engine.has_model("formula-det", "yolov8-mfd") {
    // 模型可用
}
```

## 性能监控

```rust
use latexsnipper_engine::{MetricsBuilder, RecognitionMetrics};

let metrics = MetricsBuilder::new()
    .runtime("onnxruntime")
    .model_version("formula-det", "1.0.0")
    .build();

// 管道执行后
println!("{}", metrics.summary());
```

## 依赖关系

```
Engine
↑ 依赖所有 Core crate
↓ 被 FFI/WASM/CLI 依赖
```
