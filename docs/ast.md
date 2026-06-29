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
    ├── width, height
    └── Block[]
        ├── Paragraph → Inline[]
        │   ├── Text → TextRun
        │   ├── Formula → FormulaSource
        │   └── Image
        ├── Formula → FormulaBlock
        ├── Table → TableCell[][]
        └── Figure → FigureBlock
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

### Formula
```rust
pub struct Formula {
    pub source: FormulaSource,  // Latex / Omml / Typst / MathML
    pub display_mode: bool,
    pub confidence: f32,
}
```

### Rect
```rust
pub struct Rect {
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
}
// 支持 iou(), contains(), overlaps() 等方法
```

### Operation（预留）
```rust
pub enum Operation {
    InsertBlock { page, index, block },
    RemoveBlock { page, index },
    ReplaceFormula { page, index, formula },
    ReplaceText { page, block_index, inline_index, text },
}
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

## 依赖关系

```
AST
↑
Image, Runtime, Inference, Pipeline, Conversion, Export 都依赖 AST
```
