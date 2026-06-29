# Engine Crate

> 核心引擎 — 组织所有 Capability 的宿主

## 职责

Engine 是整个 Core 的入口点：
- 组织 Runtime、Model 等所有 Capability
- 提供统一的 `recognize()` API
- 管理配置和生命周期

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `engine` | engine.rs | SnipperEngine（主编排器） |
| `config` | config.rs | EngineConfig |

## SnipperEngine

```rust
pub struct SnipperEngine {
    config: EngineConfig,
    runtime: Box<dyn RuntimeBackend>,
    model_manager: ModelManager,
}

impl SnipperEngine {
    pub fn new(config, runtime) -> Self;
    pub fn runtime(&self) -> &dyn RuntimeBackend;
    pub fn model_manager(&self) -> &ModelManager;
    pub fn config(&self) -> &EngineConfig;
    pub async fn recognize(&self, image, mode) -> Result<Document>;
}
```

## RecognizeMode

```rust
pub enum RecognizeMode { Formula, Text, Mixed }
```

`recognize()` 根据 mode 分发到：
- `recognize_formula()` — 公式识别（占位，待 Inference 集成）
- `recognize_text()` — 文字识别（占位）
- `recognize_mixed()` — 混合识别（占位）

## EngineConfig

```rust
pub struct EngineConfig {
    pub models_dir: PathBuf,
    pub acceleration: AccelerationMode,
    pub max_threads: usize,
}
// Default: models/, Auto, 4 threads
```

## 依赖关系

```
Engine
↑ 依赖所有 Core crate
↓ 被 FFI/WASM/CLI 依赖
```
