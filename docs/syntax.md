# Syntax Crate

> 语法层 — Parser + Renderer for LaTeX/Typst/Markdown

## 核心原则

1. **Parser 把语法转为 Document AST**
2. **Renderer 把 Document AST 转为语法**
3. **转换不直接互转，都经过 AST**

```
LatexParser → Document → LatexRenderer
            → Document → TypstRenderer
            → Document → MarkdownRenderer
```

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `parser` | parser.rs | Parser trait |
| `renderer` | renderer.rs | Renderer trait |
| `latex` | latex.rs | LaTeX Parser + Renderer |
| `typst` | typst.rs | Typst Renderer + latex_to_typst |
| `markdown` | markdown.rs | Markdown Renderer |

## 关键 Trait

### Parser
```rust
pub trait Parser {
    fn parse(&self, input: &str) -> Result<Document>;
    fn name(&self) -> &str;
}
```

### Renderer
```rust
pub trait Renderer {
    fn render(&self, doc: &Document) -> Result<String>;
    fn name(&self) -> &str;
}
```

## LatexParser

解析 LaTeX 字符串为 Document：
- `$...$` → 内联公式
- `$$...$$` → 显示公式
- 其他 → 文本段落

## TypstRenderer

LaTeX → Typst 转换：
- `\frac{a}{b}` → `(a)/(b)`
- `\sqrt{x}` → `sqrt(x)`
- `\pi` → `pi`
- 200+ 符号映射

## 测试

9 项测试覆盖：LaTeX 解析（文本/内联/显示/混合）、渲染、Typst 转换、Markdown 渲染、往返测试。

## 依赖关系

```
Syntax
↑ 依赖 AST
↓ 被 Engine 间接依赖
```
