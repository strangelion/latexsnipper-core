# CLI Crate

> 命令行工具 — snipper 命令

## 模块

| 模块 | 文件 | 说明 |
|------|------|------|
| `main` | main.rs | clap CLI 入口 + 子命令 |

## 子命令

| 命令 | 说明 | 状态 |
|------|------|------|
| `snipper recognize` | 识别图像内容并导出 | ✅ 可用 |
| `snipper import` | 统一导入并输出 JSON AST | ✅ 可用 |
| `snipper convert` | 统一 AST 跨格式转换 | ✅ 可用 |
| `snipper export` | SVG/PDF/PNG/纯文本视觉导出 | ⚠️ 实验性 |
| `snipper inspect` | 检查格式、页、块、资产与诊断 | ✅ 可用 |
| `snipper validate` | 解析并验证输入包 | ✅ 可用 |
| `snipper capabilities` | 查询真实注册表与 runtime provider | ✅ 可用 |
| `snipper parse` | 解析 LaTeX 为 AST | ✅ 可用 |
| `snipper render` | 渲染 AST 为 LaTeX | ✅ 可用 |
| `snipper version` | 显示版本信息 | ✅ 可用 |

## 统一导入与转换

```bash
snipper convert input.docx --to markdown -o output.md
snipper convert input.pptx --to pdf -o output.pdf
snipper convert input.xlsx --to html -o output.html
snipper convert input.pdf --to docx -o output.docx
snipper convert input.md --to docx -o output.docx
snipper convert input.json --to svg -o page.svg
snipper inspect input.docx --json
snipper validate output.docx
snipper capabilities --format json
```

PDF、PNG、DOCX、PPTX、XLSX 是 binary target，必须提供 `-o/--output`；CLI
直接写入 bytes，不经过 UTF-8 转换。转换诊断写入 stderr，机器可读的能力与 runtime
诊断通过 `capabilities --format json` 输出。

## recognize 命令

### 参数

| 参数 | 短选项 | 说明 | 默认值 |
|------|--------|------|--------|
| `--input` | `-i` | 输入图像路径 | 必填 |
| `--format` | `-f` | 输出格式 | `latex` |
| `--output` | `-o` | 输出文件路径 | stdout |

### 支持的格式

| 格式 | 关键字 | 扩展名 |
|------|--------|--------|
| LaTeX | `latex`, `tex` | `.tex` |
| Markdown | `markdown`, `md` | `.md` |
| Typst | `typst` | `.typ` |
| HTML | `html` | `.html` |
| MathML | `mathml` | `.xml` |
| OMML | `omml` | `.xml` |
| JSON | `json` | `.json` |

### 格式推断

当指定 `--output` 时，自动根据文件扩展名推断格式：

```
output.tex → latex
output.typ → typst
output.md  → markdown
output.html → html
output.json → json
```

### 使用示例

```bash
# 输出到 stdout（默认 latex）
snipper recognize -i image.png

# 导出为 LaTeX 文件
snipper recognize -i image.png -o output.tex

# 导出为 Typst 文件
snipper recognize -i image.png -o output.typ

# 指定格式 + 输出文件
snipper recognize -i image.png -f markdown -o output.md

# 导出 JSON
snipper recognize -i image.png -o result.json
```

## parse 命令

解析 LaTeX 字符串为 AST JSON。

```bash
snipper parse --latex "$$\frac{a+b}{c}$$"
```

输出 Document AST 的 JSON 表示。

## render 命令

将 LaTeX 字符串解析后重新渲染。

```bash
snipper render --latex "$$\frac{a+b}{c}$$"
```

## version 命令

```bash
snipper version
# snipper 1.0.0
# LaTeXSnipper Core — Real ONNX Runtime Mode
```

## 依赖关系

```
CLI
↑ 依赖 Engine, Mock, Syntax, Export, Pipeline
```
