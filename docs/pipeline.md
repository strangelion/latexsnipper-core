# Pipeline Crate

> 节点化流水线 — 可组合的计算图

## 核心原则

1. **Pipeline 是 Node Graph，不是 if/else**
2. **每个 Node 独立处理 Context**
3. **支持并行/条件/缓存（未来）**

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `node` | node.rs | PipelineNode trait + TransformNode |
| `context` | context.rs | PipelineContext（image + document + metadata） |
| `graph` | graph.rs | PipelineGraph（节点编排） |
| `formula_pipeline` | formula_pipeline.rs | 公式格式化 |
| `text_pipeline` | text_pipeline.rs | 文字格式化 |
| `mixed_pipeline` | mixed_pipeline.rs | 混合格式化 |

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
    pub document: Document,
    pub metadata: HashMap<String, serde_json::Value>,
    pub cancelled: bool,
}
```

### PipelineGraph
```rust
pub struct PipelineGraph { name, nodes: Vec<Box<dyn PipelineNode>> }
// add_node() → run() → 所有节点顺序执行
```

## 依赖关系

```
Pipeline
↑ 依赖 AST, Image
↓ 被 Engine 依赖
```
