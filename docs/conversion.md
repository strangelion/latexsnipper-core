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
| `latex_parser` | latex_parser.rs | LaTeX → LaTeX AST |
| `latex_ast` | latex_ast.rs | LaTeX AST 定义（含 Display 实现） |
| `latex_to_typst` | latex_to_typst.rs | LaTeX AST → Typst |
| `latex_utils` | latex_utils.rs | 辅助函数 |
| `omml_parser` | omml_parser.rs | OMML → LaTeX |
| `mathml_parser` | mathml_parser.rs | MathML → LaTeX |
| `markdown_parser` | markdown_parser.rs | Markdown → AST |
| `html_parser` | html_parser.rs | HTML → AST |
| `typst_parser` | typst_parser.rs | Typst → LaTeX |
| `table_export` | table_export.rs | 表格导出 |

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

## DocumentConverter

统一转换入口，支持所有格式和页面选择。

```rust
impl DocumentConverter {
    pub fn new(format: OutputFormat) -> Self;

    /// 转换整个文档
    pub fn convert(&self, doc: &Document) -> Result<String>;

    /// 转换指定页面（0-based 索引）
    pub fn convert_pages(&self, doc: &Document, pages: &[usize]) -> Result<String>;

    /// 转换所有格式
    pub fn convert_all(doc: &Document) -> Result<Vec<(OutputFormat, String)>>;

    /// 从 LaTeX 字符串转换
    pub fn convert_latex_string(latex: &str, format: OutputFormat) -> Result<String>;

    /// 从 MathML 字符串转换
    pub fn convert_mathml_string(mathml: &str, format: OutputFormat) -> Result<String>;

    /// 从 OMML 字符串转换
    pub fn convert_omml_string(omml: &str, format: OutputFormat) -> Result<String>;

    /// 从 Typst 字符串转换
    pub fn convert_typst_string(typst: &str, format: OutputFormat) -> Result<String>;

    /// 从 Markdown 字符串转换
    pub fn convert_markdown_string(md: &str, format: OutputFormat) -> Result<String>;
}
```

### 使用示例

```rust
use latexsnipper_conversion::{DocumentConverter, OutputFormat};

// 转换整个文档
let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc)?;

// 只转换第 1、3 页（0-based）
let pages = DocumentConverter::new(OutputFormat::Typst)
    .convert_pages(&doc, &[0, 2])?;

// 转换所有格式
let all = DocumentConverter::convert_all(&doc)?;
```

## 表格支持

### 输入解析

支持从四种格式解析表格：

```rust
use latexsnipper_conversion::{
    parse_latex_table, parse_markdown_table,
    parse_html_table, parse_typst_table,
};

// LaTeX tabular 环境
let latex = r"\begin{tabular}{|c|c|} A & B \\ C & D \end{tabular}";
let table = parse_latex_table(latex).unwrap();

// Markdown 管道表格
let md = "| A | B |\n|---|---|\n| C | D |";
let table = parse_markdown_table(md).unwrap();

// HTML table 标签
let html = "<table><tr><td>A</td><td>B</td></tr></table>";
let table = parse_html_table(html).unwrap();

// Typst table 语法
let typst = "#table(columns: 2, [A], [B], [C], [D])";
let table = parse_typst_table(typst).unwrap();
```

### 输出格式

| 格式 | 合并单元格 | 样式支持 |
|------|-----------|---------|
| LaTeX | ✅ `\multicolumn` | ✅ alignment |
| HTML | ✅ colspan/rowspan | ✅ border/background/alignment |
| Typst | ✅ `cell()` 函数 | ✅ stroke |
| Markdown | ❌ 语法不支持 | ❌ 语法不支持 |

### TableCell 样式字段

```rust
pub struct TableCell {
    pub inlines: Vec<Inline>,
    pub colspan: u32,
    pub rowspan: u32,
    pub border_style: Option<BorderStyle>,   // Solid/Dashed/Dotted/Double/...
    pub border_width: Option<u32>,           // 像素
    pub border_color: Option<String>,        // hex 或颜色名
    pub background: Option<String>,          // 背景色
    pub alignment: Option<CellAlignment>,    // Left/Center/Right/Justify
    pub geometry: Option<Rect>,
    pub source: Option<SourceInfo>,
}
```

## LaTeX 支持情况

### 已支持的 LaTeX 结构

**数学公式（核心强项）**：

| 类别 | 支持的命令/环境 |
|------|----------------|
| 分数 | `\frac`, `\dfrac`, `\tfrac` |
| 根号 | `\sqrt`, `\sqrt[n]` |
| 上下标 | `x^{n}`, `x_{i}` |
| 求和/积分/连乘 | `\sum`, `\int`, `\prod`, `\lim` |
| 矩阵 | `matrix`, `pmatrix`, `bmatrix`, `vmatrix`, `smallmatrix` |
| 对齐 | `cases`, `aligned`, `align`, `gather` |
| 重音 | `\hat`, `\bar`, `\vec`, `\dot`, `\ddot`, `\tilde` |
| 希腊字母 | 全部小写和大写 |
| 运算符 | `\sin`, `\cos`, `\log`, `\operatorname` |
| 二项式 | `\binom` |
| 定界符 | `\left`, `\right` |
| 装饰 | `\overbrace`, `\underbrace`, `\overset`, `\underset` |

**文档块**：

| Block 类型 | LaTeX 输出 |
|-----------|-----------|
| Heading | `\section` ~ `\subparagraph`（5 级） |
| Paragraph | 纯文本 + 内联公式 |
| Formula | `$...$` / `$$...$$` |
| Table | `\begin{tabular}` + 对齐 + `\multicolumn` |
| Figure | `\includegraphics` + `\caption` |
| List | `\begin{itemize}` / `\begin{enumerate}` |
| Quote | `\begin{quote}` |
| Code | `lstlisting` / `verbatim` |

### 已知限制

| 限制 | 说明 |
|------|------|
| 输入解析 | 仅提取数学公式，不解析 `\section` 等文档命令 |
| 表格内公式 | 输出时只渲染文本内容，不保留公式格式 |
| 交叉引用 | 无 `\ref`/`\cite`/`\label` 支持 |
| 脚注 | 无 `\footnote` 支持 |
| 浮动体 | 无 `\begin{figure}`/`\begin{table}` 环境解析 |

## OMML 注意事项

Microsoft Word 对 OMML 处理有严格要求：

### nary 运算（求和/积分/连乘）

LaTeX AST 会先把同层的 n-ary 头（运算符及上下限）归一化，再把后续 operand
绑定到该节点。积分、求和、连乘、并集等的被积式/求和项必须位于
`<m:nary>/<m:e>`，不能作为 `</m:nary>` 之后的同级 run 输出。

```xml
<m:nary>
  <m:naryPr>
    <m:chr m:val="∫"/>
    <m:limLoc m:val="subSup"/>
    <m:grow m:val="1"/>
  </m:naryPr>
  <m:sub>0</m:sub>
  <m:sup>∞</m:sup>
  <m:e>x dx</m:e>
</m:nary>
```

同层 operand 在 relation/equality 处结束；matrix、cases 与 alignment cell
本身形成结构边界。连续积分从右向左绑定，因此外层积分的 `<m:e>` 包含内层
`<m:nary>`。无 operand 的独立 n-ary 使用显式空表达式，不使用普通空格伪造
operand。

`<m:e/>` 是否有效取决于父元素。serializer 在生成具体 construct 时决定空
表达式形式；`fix_omml()` 不再对全 XML 做 `<m:e/>` 字符串替换。例如
`m:func` 的空 expression 会保持自闭合。

canonical 结构与 tree ownership 案例位于
`contracts/fixtures/omml-nary-canonical-v1.json`。

### xrightarrow / xleftarrow / overset / underset

这些命令通过 LaTeX AST 的 `XArrow`/`Overset`/`Underset` 节点处理，在 OMML 中转换为：

| LaTeX | OMML |
|-------|------|
| `\xrightarrow{text}` | `<m:sSup><m:e>→</m:e><m:sup>text</m:sup></m:sSup>` |
| `\xrightarrow[below]{above}` | `<m:sSubSup><m:e>→</m:e><m:sub>below</m:sub><m:sup>above</m:sup></m:sSubSup>` |
| `\overset{*}{x}` | `<m:sSup><m:e>x</m:e><m:sup>*</m:sup></m:sSup>` |
| `\underset{n}{x}` | `<m:sSub><m:e>x</m:e><m:sub>n</m:sub></m:sSub>` |

### 跨格式回环测试

现有的回环测试覆盖以下路径（约 200 个测试）：
- LaTeX → OMML → LaTeX（验证分数/上下标/nary 结构保留）
- LaTeX → MathML → LaTeX（验证分数/上下标结构保留）
- LaTeX → 6 种输出格式（Typst/MathML/OMML/Markdown/HTML/LaTeX），验证数学结构完整性

## LaTeX → Typst 转换

通过 `latex_parser::parse_latex()` 解析后经 `latex_to_typst::latex_ast_to_typst()` 转换。

### 支持的特殊结构

| 结构 | LaTeX | Typst |
|------|-------|-------|
| 分数 | `\frac{a}{b}` | `frac(a, b)` |
| 开方 | `\sqrt[3]{x}` | `root(3, x)` |
| 上下标 | `x_i^2` | `x_(i)^(2)` |
| 嵌套上下标 | `x^{y^{z}}` | `x^(y^(z))` |
| 嵌套分式 | `\frac{\frac{a}{b}}{c}` | `frac(frac(a, b), c)` |
| 积分限 | `\int_{0}^{\infty}` | `integral_(0)^(infinity)` |
| 求和限 | `\sum_{i=0}^{n}` | `sum_(i=0)^(n)` |
| 极限+箭头 | `\lim_{x \to 0}` | `limit_(x arrow.r 0)` |
| 括号 | `\left(\frac{a}{b}\right)` | `lr((frac(a, b)))` |
| hat/vec/bar | `\hat{x}` | `hat(x)` |

### 上下文敏感的间距

`Sequence` 节点处理时会自动抑制以下情况前的空格：
- 上标/下标（`^`/`_`）
- 闭括号、逗号、句点等标点符号

## LaTeX 语法扩展

### 文本格式命令

| 命令 | 说明 | OMML | LaTeX | HTML | Typst |
|------|------|------|-------|------|-------|
| `\underline{text}` | 下划线 | `<w:u>` | `\underline{}` | `<u>` | `#underline[]` |
| `\textbf{text}` | 粗体 | `<w:b/>` | `\textbf{}` | `<strong>` | `*text*` |
| `\textit{text}` | 斜体 | 自动 italic | `\textit{}` | `<em>` | `_text_` |

### 环境支持

| 环境 | 说明 | 输出策略 |
|------|------|----------|
| `\begin{description}` | 定义列表 | OMML: 粗体 label + content |
| `\begin{theorem}` 等 | 定理环境 | OMML: 粗体标题 + content |
| `\begin{proof}` | 证明环境 | OMML: 粗体 "Proof." + content + □ |
| `\begin{minipage}{w}` | 小页 | 直接输出 content |
| `\begin{figure}` / `\begin{table}` | 浮动体 | 直接输出 content |

### 占位符命令（OMML 输出）

| 命令 | OMML 输出 | 说明 |
|------|-----------|------|
| `\footnote{text}` | `[^footnote]` | 需要 Word API 插入实际脚注 |
| `\label{key}` | (空) | 标签不渲染 |
| `\ref{key}` | `(?key)` 或 `(key)` | 交叉引用占位符 |
| `\eqref{key}` | `(?key)` | 公式引用占位符 |
| `\cite{key}` | `[key]` | 参考文献占位符 |
| `\tableofcontents` | 目录 | 目录占位符 |

## 多格式输入解析器

### Markdown 解析器

```rust
use latexsnipper_conversion::parse_markdown_to_document;

let md = "# Title\n\n**bold** text with $x^2$ math.\n\n- item1\n- item2";
let doc = parse_markdown_to_document(md);
```

支持：标题、段落、粗体/斜体、行内/块级代码、有序/无序列表、引用块、水平线、数学公式。

### HTML 解析器

```rust
use latexsnipper_conversion::parse_html_to_document;

let html = "<h1>Title</h1><p>Hello <strong>world</strong>!</p>";
let doc = parse_html_to_document(html);
```

支持：`<h1>`-`<h6>`、`<p>`、`<strong>`/`<b>`、`<em>`/`<i>`、`<u>`、`<code>`、`<pre>`、`<ul>`/`<ol>`、`<blockquote>`、`<hr>`、`<math>`。

## 已知局限

1. **MathML→LaTeX 生成完整文档模板** — 包含 `\documentclass` 等，非纯公式字符串（MathML 解析器设计选择）
2. **OMML 的 `<msubsup>` 往返** — LaTeX→OMML→LaTeX 可能展开为 `<m:sSub>` + `<m:sSup>` 两层，语义正确但形式不同
3. **`parse_latex` 是纯文本解析器** — 不支持宏定义、条件分支等 TeX 引擎特性

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
