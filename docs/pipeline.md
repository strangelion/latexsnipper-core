# Pipeline Crate

> 节点化流水线 — 可组合的计算图

## 核心原则

1. **Pipeline 是 Node Graph，不是 if/else**
2. **每个 Node 独立处理 Context**
3. **支持取消（cancelled flag）**

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `node` | node.rs | PipelineNode trait + TransformNode |
| `context` | context.rs | PipelineContext（image + document + metadata） |
| `graph` | graph.rs | PipelineGraph（节点编排） |
| `formula_pipeline` | formula_pipeline.rs | 公式格式化 + Document 构建 |
| `text_pipeline` | text_pipeline.rs | 文字格式化 + Document 构建 |
| `mixed_pipeline` | mixed_pipeline.rs | 混合格式化 + Document 构建 |

## 关键类型

### PipelineNode

```rust
#[async_trait]
pub trait PipelineNode: Send + Sync {
    fn name(&self) -> &str;
    async fn process(&self, ctx: &mut PipelineContext) -> Result<()>;
}
```

### TransformNode

```rust
pub struct TransformNode {
    name: String,
    transform: Box<dyn Fn(&mut PipelineContext) -> Result<()> + Send + Sync>,
}
// 便捷构造：TransformNode::new("name", |ctx| { ... })
```

### PipelineContext

```rust
pub struct PipelineContext {
    pub image: Option<SnipperImage>,
    pub document: Document,
    pub metadata: HashMap<String, serde_json::Value>,
    pub cancelled: bool,
}
// methods: new(), with_image(), set(), get(), cancel()
```

### PipelineGraph

```rust
pub struct PipelineGraph { name, nodes: Vec<Box<dyn PipelineNode>> }
// methods: new(), add_node(), run(), len(), is_empty(), name()
```

`run()` 按顺序执行所有节点，遇 `cancelled` 则中断。

## Pipeline 函数

### formula_pipeline

| 函数 | 说明 |
|------|------|
| `format_formula_output(ctx)` | 从 Document 提取所有公式，存入 metadata |
| `build_document(formulas, texts)` | 从公式+文字列表构建 Document |

### text_pipeline

| 函数 | 说明 |
|------|------|
| `format_text_output(ctx)` | 从 Document 提取所有文字，存入 metadata |
| `build_document_from_text(lines)` | 从文字行列表构建 Document |

### mixed_pipeline

| 函数 | 说明 |
|------|------|
| `format_mixed_output(ctx)` | 格式化混合内容（公式用 `$$`，文字直接输出） |
| `build_document_from_mixed(regions)` | 从混合区域列表构建 Document |

### RegionType

```rust
pub enum RegionType { Formula, Text }
```

## 依赖关系

```
Pipeline
↑ 依赖 AST, Image
↓ 被 Engine 依赖
```
