# Foundation Crate

> 基础设施层 — Error, Result, Logging, Config, Event

## 职责

Foundation 只提供基础设施，不提供业务能力：
- 统一错误类型 `SnipperError`
- 类型别名 `Result<T>`
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
