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
| `snipper parse` | 解析 LaTeX 为 AST | ✅ 可用 |
| `snipper render` | 渲染 AST 为 LaTeX | ✅ 可用 |
| `snipper version` | 显示版本信息 | ✅ 可用 |

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
