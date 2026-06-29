# Export Crate

> 导出能力 — RenderTree + Generator

## 核心原则

1. **Document → RenderTree → Generator → Output**
2. **RenderTree 避免重复遍历 AST**
3. **Generator 可插拔，新增格式只需实现 trait**

## 架构

```
Document
  ↓ RenderTree::from_document()
RenderTree
  ↓ Generator::generate()
SVG / PNG / PDF / Text
```

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `render_tree` | render_tree.rs | RenderTree 中间表示 |
| `generator` | generator.rs | Generator trait |
| `svg` | svg.rs | SVG Generator |
| `text` | text.rs | Plain Text Generator |

## 关键类型

### RenderTree
```rust
pub struct RenderTree { pub nodes: Vec<RenderNode> }
pub enum RenderNode { Text, Formula, Paragraph, Page }
// from_document(doc) → RenderTree
```

### Generator trait
```rust
pub trait Generator {
    fn generate(&self, tree: &RenderTree) -> Result<String>;
    fn extension(&self) -> &str;
    fn mime_type(&self) -> &str;
    fn name(&self) -> &str;
}
```

## 测试

5 项测试覆盖：RenderTree 构建、SVG 生成、Text 生成、XML 转义、结构验证。

## 依赖关系

```
Export
↑ 依赖 AST, Syntax
↓ 被 Engine 间接依赖
```
