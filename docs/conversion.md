# Conversion Crate

> 格式转换 — AST → 多格式中间表示

## 核心原则

1. **所有转换都必须经过 AST，禁止格式直接互转**
2. **Conversion 负责 AST → 格式化字符串，Syntax 负责字符串 ↔ AST**
3. **新增格式只需新增 Converter，不修改已有代码**

## 转换路径

```
                    ┌─→ LatexConverter        ─→ $...$
                    ├─→ LatexDisplayConverter  ─→ \[...\]
                    ├─→ LatexEquationConverter ─→ \begin{equation}...\end{equation}
                    │
                    ├─→ MarkdownInlineConverter ─→ $...$
                    ├─→ MarkdownBlockConverter  ─→ $$...$$
                    │
                    ├─→ MathmlConverter        ─→ <math xmlns="...">
                    ├─→ MathmlMmlConverter     ─→ <mml:math>
                    ├─→ MathmlMConverter       ─→ <m:math>
                    ├─→ MathmlAttrConverter    ─→ <math mathmode="...">
                    │
Document AST ───────├─→ OmmlConverter         ─→ <m:oMath>
                    │
                    ├─→ TypstConverter         ─→ Typst syntax
                    │
                    └─→ HtmlConverter          ─→ HTML + MathJax
```

## 模块

| 模块 | 文件 | 说明 |
|------|------|------|
| `converter` | converter.rs | Converter trait 定义 |
| `latex` | latex.rs | LaTeX (inline/display/equation) |
| `omml` | omml.rs | OMML XML |
| `mathml` | mathml.rs | MathML (standard/mml/m/attr) |
| `typst` | typst.rs | Typst |
| `markdown` | markdown.rs | Markdown (inline/block) |
| `html` | html.rs | HTML + MathJax |

## Converter Trait

```rust
pub trait Converter {
    fn convert(&self, doc: &Document) -> Result<String>;
    fn name(&self) -> &str;
    fn extension(&self) -> &str;
    fn mime_type(&self) -> &str;
}
```

## 支持的格式

| Converter | name | 扩展名 | 说明 |
|-----------|------|--------|------|
| `LatexConverter` | `latex` | `.tex` | LaTeX inline `$...$` |
| `LatexDisplayConverter` | `latex_display` | `.tex` | LaTeX display `\[...\]` |
| `LatexEquationConverter` | `latex_equation` | `.tex` | LaTeX equation |
| `MarkdownInlineConverter` | `markdown_inline` | `.md` | Markdown inline `$...$` |
| `MarkdownBlockConverter` | `markdown_block` | `.md` | Markdown block `$$...$$` |
| `MathmlConverter` | `mathml` | `.xml` | MathML (standard namespace) |
| `MathmlMmlConverter` | `mathml_mml` | `.mml` | MathML (mml: prefix) |
| `MathmlMConverter` | `mathml_m` | `.xml` | MathML (m: prefix) |
| `MathmlAttrConverter` | `mathml_attr` | `.xml` | MathML (attribute form) |
| `OmmlConverter` | `omml` | `.xml` | Office Math ML |
| `TypstConverter` | `typst` | `.typ` | Typst |
| `HtmlConverter` | `html` | `.html` | HTML + MathJax |

## 与 Syntax Crate 的区别

| | Syntax | Conversion |
|---|--------|-----------|
| 方向 | 字符串 ↔ AST（双向） | AST → 字符串（单向） |
| 输入 | LaTeX/Typst 源码 | Document AST |
| 输出 | Document AST | 12 种格式 |
| 用途 | 解析输入 / 渲染输出 | 格式化导出 |

## 依赖关系

```
Conversion
↑ 依赖 AST
↓ 被 Engine 间接依赖
```
