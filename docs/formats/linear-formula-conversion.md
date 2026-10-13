# UnicodeMath / AsciiMath 单公式转换

更新：2026-10-13。两者已有有限正式输入/输出路线，不再仅能通过独立 pilot 调用。
这不是完整 AsciiMath、UnicodeMath/Word buildup、MathType 或视觉保真支持。
语法清单分别见 [AsciiMath](asciimath-pilot.md) 与 [UnicodeMath](unicodemath-pilot.md)，两者不混用关键字。

## 入口

Rust 使用 `DocumentConverter::convert_formula_to_target` 与新增 `FormulaOutputTarget`；
`Semantic(OutputFormat)` 保留原语义目标，新增 `AsciiMath`、`UnicodeMath` 仅适用于单公式。
旧文档 `OutputFormat` 的九种格式及 `convert_all` 保持不变，不将混合文档改成线性公式。
旧 `convert_formula_string` 现在也能将这两个输入转为原来的语义输出。

C 的 `latexsnipper_formula_convert`、Python 的 `convert_formula`、WASM 的
`convert_formula_v3`/JS `convertFormula`、官方 Web Worker 与原生 JSONL `formula.convert`
使用同一能力与执行路径。JSONL v1 消息和旧 recognition 输出选项不变。
正式注册转换器、C/Python 注册会话、JS 注册会话及 `snipper formula convert` 的
`core:builtin` 同样接通；不需要模型或新依赖。输入/输出规范 ID 为 `ascii-math`、
`unicode-math`；直接公式绑定同时接受对应下划线输出拼写，注册请求仍要求规范 ID。

例如 Python：

```python
convert_formula("(a+b)/c", input_format="ascii-math",
                output_format="unicode-math", mode="best-effort")
# (a + b)/(c)
```

两者仅提供 `best-effort` 路线。默认 `strict` 不变，仍只支持已验证 LaTeX→OMML 子集，
调用方必须显式指定模式并读取能力；不能把 “best-effort 成功” 当完整语义或排版一致的证明。
能力投影包括有限语法、规范化、源码重建及样式/字体/间距不保留的限制。
注册结果保留原输入与上下文摘要和损失说明，候选并非宿主写入许可。

## 支持与拒绝

线性来源使用各自独立 parser→AST。线性目标使用有界 writer，并将规范输出重新解析核对
可表达结构；不能表达的 AST、未知 glyph/命令、损坏结构、超预算拒绝，不返回部分结果。
AsciiMath 的 `a/b/c` 拒绝，而 UnicodeMath 的该串左结合；`abc/d` 的分子绑定也不同。
原拼写、冗余分组、空格宽度、字体及完整 rich-text 不由这种规范化保留。
UnicodeMath 另支持双引号普通文本及对应 LaTeX `text` 命令的有限投影：
文本保持为文本，先保护 TeX 特殊字符再投影 XML，不执行其中的命令。
引号内容计入词法工作预算；控制字符、跨行和嵌套数学 AST 不接收。
AsciiMath 不因此获得引号文本语法，跨到该目标仍须明确拒绝不可表达节点。
UnicodeMath 另有有限命名函数及 U+2061 应用路线，保留参数括号和函数名脚本；
规范写回显式括起参数，XML 桥接保留嵌套组，不把后继乘数吞进括号参数。
有限数学参数可回读；OMML 普通文字 run 不能证明原先为引号文本。
真实宿主排版、完整函数/n-ary、任意名称及字体还不是完成项。

其他来源先获得单公式 LaTeX 投影，再验证其有限语法并写入线性目标。
MathML/OMML/Typst 仍是重建，原解析器的 best-effort 限制不因新目标而消失。
Markdown→线性目标只接受一个 `$…$`、`$$…$$`、`\(…\)` 或 `\[…\]` 公式，
拒绝正文、代码块、多公式和损坏包装；不会只挑第一条或静默删除正文。
其他已有文档转换路线仍按原行为处理混合文档。

公式输入最多 64 KiB；新线性 writer/parser 继续执行原来的 token、深度、节点和输出预算，
转为语义格式时还受原重建 LaTeX 的 64 层/512 结构 token 限制。
接近 parser 上限的输入可能在规范输出或后续重建中超限，不承诺所有 parser 成功项都可导出。
Core 不执行 TeX、宏、JavaScript 或 Office 操作来完成这些字符串转换。

## 验收边界

固定合成语料验证规范输出及原语义目标，专项回归验证两种语法的差异、拒绝范围、
原文身份、旧文档 API 和模式/预算；它们不是独立真实分布准确率。
公开能力矩阵由注册表生成，包级 smoke 遍历每条路线。
Office UI、Word 原生 buildup 对比、移动设备、真实字体/视觉及
更广语法仍须分别接入和验收。MTEF 字节/宿主对象不由本接口顺带开放。

原生 WASI profile 也接通有限线性 importer 目标/exporter 来源，仍须先有已验证且启用的包，
再明确启用并选择 profile 后端；这不开放浏览器 WASI 或新的 filesystem/network 权限。
贡献者的 importer 可使用新增 `WasiFormulaBackend::importer_to_target`，旧接口保留。
见 [WASI/profile 边界](conversion-registry.md#受限-wasi-公式适配)。
