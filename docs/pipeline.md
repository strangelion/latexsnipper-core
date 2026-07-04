# Pipeline Crate

> 节点化流水线 — 可组合的计算图

## 核心原则

1. **Pipeline 是 Node Graph，不是 if/else**
2. **每个 Node 独立处理 Context**
3. **支持取消（cancelled flag）**
4. **节点表达能力（ModelTask），不表达实现**

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `node` | node.rs | PipelineNode trait + TransformNode |
| `context` | context.rs | PipelineContext（image + artifacts + metadata + sessions） |
| `graph` | graph.rs | PipelineGraph（拓扑排序 + 显式依赖） |
| `artifacts` | artifacts.rs | PipelineArtifacts 强类型数据 |
| `reading_order` | reading_order.rs | 阅读顺序排序（y-bucket + x tie-breaker） |

## 关键类型

### PipelineNode

```rust
#[async_trait]
pub trait PipelineNode: Send + Sync {
    fn name(&self) -> &str;
    async fn process(&self, ctx: &mut PipelineContext) -> Result<()>;
}
```

### PipelineContext

```rust
pub struct PipelineContext {
    pub image: Option<SnipperImage>,
    pub artifacts: PipelineArtifacts,
    pub metadata: HashMap<String, serde_json::Value>,
    pub backend: Option<Arc<dyn RuntimeBackend>>,
    pub model_resolver: Option<SharedModelResolver>,
    pub sessions: HashMap<String, CachedSession>,
    pub diagnostics: Vec<DiagnosticEvent>,
    pub cancelled: bool,
    pub models_dir: Option<PathBuf>,
}
```

### PipelineArtifacts

```rust
pub struct PipelineArtifacts {
    pub formula_detections: Vec<DetectionBox>,
    pub text_detections: Vec<DetectionBox>,
    pub handwriting_detections: Vec<DetectionBox>,
    pub table_detections: Vec<DetectionBox>,
    pub formula_blocks: Vec<Block>,
    pub text_blocks: Vec<Block>,
    pub handwriting_blocks: Vec<Block>,
    pub table_blocks: Vec<Block>,
}
```

### PipelineGraph

```rust
pub struct PipelineGraph { name, entries: Vec<NodeEntry> }
// methods: new(), add_node(), add_node_with_deps(), run(), len()
```

`run()` 按拓扑排序执行节点，显式依赖保证正确顺序。

## 依赖关系

```
Pipeline
↑ 依赖 AST, Image, Runtime
↓ 被 Engine 依赖
```
