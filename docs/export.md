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
SVG / PDF / Text
```

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `render_tree` | render_tree.rs | RenderTree 中间表示 |
| `generator` | generator.rs | Generator trait |
| `svg` | svg.rs | SVG Generator |
| `text` | text.rs | Plain Text Generator |
| `pdf` | pdf.rs | PDF Generator (printpdf) |

## 关键类型

### RenderNode

```rust
pub enum RenderNode {
    Text(String),
    Formula { latex: String, display_mode: bool },
    Paragraph(Vec<RenderNode>),
    Heading { level: u8, nodes: Vec<RenderNode> },
    Table { rows: Vec<Vec<Vec<RenderNode>>> },
    List { ordered: bool, items: Vec<Vec<RenderNode>> },
    Code { language: Option<String>, code: String },
    Quote(Vec<RenderNode>),
    HorizontalRule,
    Page(Vec<RenderNode>),
}
```

### RenderTree

```rust
pub struct RenderTree { pub nodes: Vec<RenderNode> }

impl RenderTree {
    /// Build from entire document.
    pub fn from_document(doc: &Document) -> Self;

    /// Build from specific pages (0-based indices).
    pub fn from_document_pages(doc: &Document, page_indices: &[usize]) -> Self;
}
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

## PDF Generator

使用 `printpdf 0.7` 生成 PDF 文件。

### 支持的文档元素

| 元素 | 渲染方式 |
|------|---------|
| 标题 (Heading) | HelveticaBold，字号按级别递减 |
| 段落 (Paragraph) | Helvetica 11pt |
| 公式 (Formula) | LaTeX 源码文本显示（$...$ / $$...$$） |
| 表格 (Table) | 等宽列布局，9pt 字体 |
| 列表 (List) | 项目符号/编号 |
| 代码 (Code) | Courier 等宽字体 |
| 引用 (Quote) | 竖线前缀 |
| 分隔线 (HR) | 横线字符 |

### 使用示例

```rust
use latexsnipper_export::{PdfGenerator, RenderTree};
use latexsnipper_export::generator::Generator;

let tree = RenderTree::from_document(&doc);
let generator = PdfGenerator;
let pdf_bytes = generator.generate(&tree)?;
std::fs::write("output.pdf", pdf_bytes)?;
```

## 页面选择

### AST 层方法

```rust
impl Document {
    /// 按 0-based 索引过滤页面
    pub fn filter_pages(&self, indices: &[usize]) -> Document;

    /// 按 1-based 页码过滤
    pub fn filter_page_numbers(&self, numbers: &[u32]) -> Document;

    /// 解析页面范围字符串 "1-3,5,8-10"
    pub fn parse_page_range(range: &str) -> Vec<u32>;
}
```

### Conversion 层方法

```rust
impl DocumentConverter {
    /// 转换指定页面
    pub fn convert_pages(&self, doc: &Document, pages: &[usize]) -> Result<String>;
}
```

## 测试

- PDF 生成器：有效 PDF 头、元数据、所有节点类型
- RenderTree：从 Document 构建、页面过滤
- 页面范围解析：`"1-3,5,8-10"` → `[1,2,3,5,8,9,10]`

## 依赖关系

```
Export
↑ 依赖 AST, Syntax, printpdf
↓ 被 Engine 间接依赖
```
