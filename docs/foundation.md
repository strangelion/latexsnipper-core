# Foundation Crate

> 基础设施层 — Error, Result, Logging, Config, Event

## 职责

Foundation 只提供基础设施，不提供业务能力：
- 统一错误类型 `SnipperError`
- 类型别名 `Result<T>`
- 错误转换 trait `IntoSnipper`
- 日志系统 `CoreLogger`
- 配置管理 `CoreConfig`
- 事件总线 `EventBus`

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `error` | error.rs | SnipperError 枚举 + Result 别名 + IntoSnipper trait |
| `logging` | logging.rs | CoreLogger（内存收集） + init_logger() |
| `config` | config.rs | CoreConfig（models_dir, log_level, acceleration, max_threads） |
| `event` | event.rs | EventBus + EventType + Event |

## SnipperError

```rust
#[derive(Error, Debug, Clone)]
pub enum SnipperError {
    Io(String),
    Model(String),
    Runtime(String),
    Inference(String),
    Pipeline(String),
    Image(String),
    Conversion(String),
    Export(String),
    Plugin(String),
    Config(String),
    Timeout(u64),
    Cancelled,
    Other(String),
}
```

## CoreConfig

```rust
pub struct CoreConfig {
    pub models_dir: PathBuf,
    pub log_level: String,
    pub acceleration: AccelerationMode,
    pub max_threads: usize,
}
```

| 方法 | 说明 |
|------|------|
| `CoreConfig::default()` | 默认配置（models/, info, Auto, 4 threads） |
| `CoreConfig::from_file(path)` | 从 JSON 文件加载 |
| `CoreConfig::save(path)` | 保存为 JSON 文件 |

## AccelerationMode

```rust
pub enum AccelerationMode { Cpu, Gpu, Auto }
```

注意：此类型定义在 Foundation 中，Runtime crate 重新导出。

## EventType

```rust
pub enum EventType {
    ModelLoadProgress,
    RecognitionStarted,
    RecognitionCompleted,
    RecognitionFailed,
    ExportStarted,
    ExportCompleted,
    PipelineNodeExecuted,
    Custom(String),
}
```

## EventBus

```rust
pub struct EventBus { /* ... */ }
impl EventBus {
    pub fn new() -> Self;
    pub fn subscribe(&self, event_type: EventType, listener: EventListener);
    pub fn emit(&self, event: Event);
}
```

## IntoSnipper

```rust
pub trait IntoSnipper<T> {
    fn into_snipper(self) -> Result<T>;
}
// blanket impl for Result<T, E: Display>
```

## 约束

- 不允许放图像处理
- 不允许放 OCR
- 不允许放格式转换
- 不允许放任何业务逻辑

## 依赖关系

```
Foundation
↑
所有其他 crate 都依赖 Foundation
```
