# AST Crate

> Document AST — 整个系统的唯一数据源

## 核心原则

1. **OCR 永远输出 Document，而不是 LaTeX**
2. **所有格式转换都必须经过 AST**
3. **Document 是事实来源（Single Source of Truth）**

## 数据模型

```
Document
├── Metadata
└── Page[]
    ├── width, height, page_number
    └── Block[]
        ├── Paragraph → Inline[]
        │   ├── Text → TextRun (text, bold, italic)
        │   ├── Formula → FormulaSource
        │   └── Image → ImageInline
        ├── Formula → FormulaBlock
        ├── Table → TableCell[][] (colspan, rowspan)
        └── Figure → FigureBlock (image_data, caption)
```

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `document` | document.rs | Document, Page |
| `block` | block.rs | Block enum, ParagraphBlock, FormulaBlock, TableBlock, FigureBlock |
| `inline` | inline.rs | Inline enum, TextRun, ImageInline |
| `formula` | formula.rs | Formula, FormulaSource (Latex/Omml/Typst/MathML) |
| `geometry` | geometry.rs | Rect, Point, Size |
| `metadata` | metadata.rs | Metadata, OcrMetadata |
| `operation` | operation.rs | Operation enum（支持 undo/redo） |
| `visitor` | visitor.rs | DocumentVisitor trait, TextCollector |

## 关键类型

### Document

```rust
pub struct Document {
    pub metadata: Metadata,
    pub pages: Vec<Page>,
}
// methods: new(), block_count(), all_blocks()
```

### Page

```rust
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub blocks: Vec<Block>,
    pub page_number: Option<u32>,
}
```

### Block

```rust
pub enum Block {
    Paragraph(ParagraphBlock),
    Formula(FormulaBlock),
    Table(TableBlock),
    Figure(FigureBlock),
}
```

### Formula

```rust
pub struct Formula {
    pub source: FormulaSource,  // Latex / Omml / Typst / MathML
    pub display_mode: bool,
    pub confidence: f32,
}
```

### FormulaSource

```rust
pub enum FormulaSource {
    Latex(String),
    Omml(String),
    Typst(String),
    MathML(String),
}
```

### Rect

```rust
pub struct Rect {
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
}
// methods: new(), right(), bottom(), center_x(), center_y()
//          iou(), contains(), overlaps()
```

### TextRun

```rust
pub struct TextRun {
    pub text: String,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}
```

### TableCell

```rust
pub struct TableCell {
    pub inlines: Vec<Inline>,
    pub colspan: u32,
    pub rowspan: u32,
}
```

### FigureBlock

```rust
pub struct FigureBlock {
    pub image_data: Option<String>, // base64 or path
    pub caption: Option<String>,
    pub geometry: Option<Rect>,
}
```

### Metadata

```rust
pub struct Metadata {
    pub language: Option<String>,
    pub created_at: Option<String>,
    pub ocr_model: Option<String>,
    pub ocr_version: Option<String>,
    pub ocr_time_ms: Option<u64>,
}
```

### OcrMetadata

```rust
pub struct OcrMetadata {
    pub confidence: f32,
    pub geometry: Option<Rect>,
    pub rotation: Option<f32>,
    pub model: Option<String>,
    pub time_ms: Option<u64>,
}
```

### Operation（预留）

```rust
pub enum Operation {
    InsertBlock { page, index, block },
    RemoveBlock { page, index },
    ReplaceFormula { page, index, formula },
    ReplaceText { page, block_index, inline_index, text },
}
// methods: inverse()
```

## Visitor 模式

所有 Renderer 和 Parser 都通过 `DocumentVisitor` trait 遍历 Document：
```rust
pub trait DocumentVisitor<T> {
    fn visit_document(&mut self, doc: &Document) -> T;
    fn visit_page(&mut self, page: &Page) -> T;
    fn visit_block(&mut self, block: &Block) -> T;
    fn visit_inline(&mut self, inline: &Inline) -> T;
}
```

内置 `TextCollector`：收集 Document 中所有文本内容。

## 依赖关系

```
AST
↑
Image, Runtime, Inference, Pipeline, Conversion, Export 都依赖 AST
```
