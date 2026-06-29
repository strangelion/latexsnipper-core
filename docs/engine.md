# Engine Crate

> 核心引擎 — 组织所有 Capability 的宿主

## 职责

Engine 是整个 Core 的入口点：
- 组织 Runtime、Pipeline、Model 等所有 Capability
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
    pub async fn recognize(&self, image, mode) -> Result<Document>;
}
```

## RecognizeMode

```rust
pub enum RecognizeMode { Formula, Text, Mixed }
```

## 依赖关系

```
Engine
↑ 依赖所有 Capability
↓ 被 FFI/WASM/CLI 依赖
```
