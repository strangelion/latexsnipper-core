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
| `region_graph` | region_graph.rs | RegionGraph + RecognitionTarget + ArtifactRef 路由 |
| `text_recognition_service` | text_recognition_service.rs | 共享文本识别服务（单 session、配置驱动） |
| `opendoc_hybrid` | opendoc_hybrid.rs | DocumentParseMode + OpenDoc Hybrid 编排 |

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
    pub region_candidates: Vec<RegionCandidate>,
    pub resolved_regions: Vec<ResolvedRegion>,
    pub recognition_targets: Vec<RecognitionTarget>,
}
```

### RecognitionTarget

```rust
pub enum RecognitionTarget {
    TopLevelText { detection_index: usize },
    TopLevelFormula { detection_index: usize },
    TopLevelHandwriting { detection_index: usize },
    TableCell { table_index: usize, cell_index: usize },
}
```

`RecognitionTarget` 是 `RegionResolveNode` 的核心输出，告诉各识别器精确处理哪些区域，避免重复识别。

### PipelineGraph

```rust
pub struct PipelineGraph { name, entries: Vec<NodeEntry> }
// methods: new(), add_node(), add_node_with_deps(), run(), len()
```

`run()` 按拓扑排序执行节点，显式依赖保证正确顺序。

## YAML 清单

`PipelineManifest::from_yaml` 使用固定版本的 `serde-saphyr` 反序列化，保留
`name`、`description`、`nodes` 与 JSON-compatible `config` 的结构。
Unicode、块字符串和有界别名仍可使用；`<<` 按普通配置键保留，不自动展开合并，
与旧入口的实际行为一致。JSON 入口不受此次 YAML 迁移影响。

YAML 输入上限为 4 MiB，只接受一个文档；结构深度为 64，节点、解析事件、
保留的锚点事件和重放事件各有 100,000 的独立上限。
最多 1,024 个锚点、10,000 次别名引用；标量、注释和保留锚点内容分别受 4 MiB 预算约束。
正常别名仍可重用，大字符串的反复展开须受预算限制。
重复键、未知标签、外部 `!include` 与超出预算的输入明确返回错误。
未启用文件包含或环境插值特性，`${NAME}` 保持字面字符串。
错误不附带完整源码片段；调用方仍应谨慎处理其自行记录的配置和错误信息。

## 依赖关系

```
Pipeline
↑ 依赖 AST, Image, Runtime
↓ 被 Engine 依赖
```
