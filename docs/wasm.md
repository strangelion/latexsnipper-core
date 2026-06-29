# WASM Crate

> WebAssembly 绑定 — 浏览器端运行 Core

## 核心原则

1. **WASM 只是 Engine 的另一个 Adapter**
2. **共享同一套 Core 代码，不重复实现**
3. **所有转换逻辑在 WASM 内完成，无需网络请求**

## 架构

```
Browser
  ├── DOM / Canvas / File Picker
  └── WASM Boundary
       ├── parse_latex() → Document JSON
       ├── render_latex/typst/markdown()
       ├── convert_document(json, format)
       └── formula_to_latex()
```

## 导出函数

| 函数 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `init()` | — | — | 初始化模块 |
| `parse_latex(latex)` | LaTeX 字符串 | Document JSON | 解析 LaTeX 为 AST |
| `render_latex(doc_json)` | Document JSON | LaTeX 字符串 | 渲染为 LaTeX |
| `render_typst(doc_json)` | Document JSON | Typst 字符串 | 渲染为 Typst |
| `render_markdown(doc_json)` | Document JSON | Markdown 字符串 | 渲染为 Markdown |
| `convert_document(doc_json, format)` | Document JSON + 格式名 | 目标格式字符串 | 格式转换 |
| `formula_to_latex(formula_json)` | Formula JSON | LaTeX 字符串 | 公式转 LaTeX |
| `available_formats()` | — | JSON 数组字符串 | 获取支持的格式列表 |
| `health_check()` | — | "ok" | 健康检查 |

## 支持的转换格式

`convert_document()` 的 format 参数：

| format | 说明 |
|--------|------|
| `latex` | LaTeX inline `$...$` |
| `latex_display` | LaTeX display `\[...\]` |
| `latex_equation` | LaTeX equation |
| `markdown_inline` | Markdown inline `$...$` |
| `markdown_block` | Markdown block `$$...$$` |
| `mathml` | MathML |
| `omml` | Office Math ML |
| `typst` | Typst |
| `html` | HTML + MathJax |

## 依赖

```
wasm-bindgen, js-sys, serde-wasm-bindgen
latexsnipper-{foundation, ast, syntax, conversion}
```

## 与 FFI Crate 的区别

| | FFI | WASM |
|---|-----|------|
| 目标平台 | Android / iOS | Browser |
| 接口类型 | JNI / C ABI | wasm-bindgen |
| Runtime | ONNX Runtime (native) | onnxruntime-web（未来） |
| 图像输入 | byte[] | ImageData / Blob |

## 构建

```bash
# 需要 wasm32-unknown-unknown target
rustup target add wasm32-unknown-unknown

# 构建
cargo build --target wasm32-unknown-unknown --release

# 或使用 wasm-pack
wasm-pack build --target web
```

## 依赖关系

```
WASM
↑ 依赖 Syntax, Conversion, AST
↓ 被 Web 前端调用
```
