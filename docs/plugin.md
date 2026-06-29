# Plugin Crate

> 插件 API — 扩展 Core 能力的标准接口

> **状态：规划中（Planned）**

## 核心原则

1. **Plugin 通过标准接口扩展 Engine，不修改 Core 代码**
2. **Plugin 只能访问 Engine 暴露的 Public API**
3. **Plugin 之间互相隔离，一个 Plugin 崩溃不影响其他**

## 规划架构

```
Engine
  ├── Core Capability (OCR / Pipeline / Export)
  └── Plugin Registry
       ├── Plugin A (Table Detection)
       ├── Plugin B (Chemistry Recognition)
       └── Plugin C (Custom Export)
```

## 规划 Trait

### Plugin

```rust
pub trait Plugin: Send + Sync {
    /// 插件名称
    fn name(&self) -> &str;

    /// 插件版本
    fn version(&self) -> &str;

    /// 初始化插件
    fn init(&mut self, engine: &dyn EngineApi) -> Result<()>;

    /// 处理请求
    fn handle(&self, request: &PluginRequest) -> Result<PluginResponse>;

    /// 清理资源
    fn cleanup(&mut self) -> Result<()>;
}
```

### PluginRequest / PluginResponse

```rust
pub struct PluginRequest {
    pub action: String,
    pub document: Document,
    pub metadata: HashMap<String, serde_json::Value>,
}

pub struct PluginResponse {
    pub document: Document,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

## 使用场景

| 场景 | 说明 |
|------|------|
| Table Detection | 识别表格结构并插入 TableBlock |
| Chemistry | 识别化学式并转为 AST |
| Custom Export | 新增 PDF / DOCX 导出格式 |
| Post-processing | OCR 结果后处理（校正、补全） |

## 依赖关系

```
Plugin
↑ 依赖 Engine（通过 EngineApi trait）
↓ 被 Engine 管理
```
