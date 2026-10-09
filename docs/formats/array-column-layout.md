# Array 列规格与有限布局支持

更新：2026-10-09。属于 FMT-02/FMT-03/FMT-04，不代表完整 TeX 排版或宿主验收。

## 能力边界

`\begin{array}{列规格}` 的列规格现在独立于数学单元格保存。
转换 AST 使用 `LatexNode::Array`；共享布局 `EnvInfo.column_spec` 为可选字段，旧 JSON 缺省为 `None`。
原始规格（包括重复语法和规格内注释）在 LaTeX 投影中保留，不计入数学符号数。
新增 Rust 枚举分支/结构字段需要穷尽匹配和直接结构构造的调用方同步编译；不能以 JSON 兼容替代 Rust 源兼容检查。

| 目标 | 当前范围 | 拒绝或降级 |
| --- | --- | --- |
| 严格 LaTeX→OMML | `l/c/r`、有界 `*{n}{...}`、短行补空单元格，逐列 `mcJc` | 任意竖线、未知修饰、列数溢出拒绝；普通导出将数组作为原文文本保留 |
| MathML | 逐列对齐、内部单竖线、两侧单竖线、短行补空 | 双竖线及未知宽度/声明作为原文 `mtext` 保留 |
| Typst | grid 逐列对齐与单竖线；行内数学单元格，允许数组嵌入分式/其他数组 | 双竖线及未知修饰使用 `raw` 保留原文，不宣称等价排版 |
| OMML→LaTeX | `mcs/mc/mcPr/count/mcJc`，默认居中、重复列组、XML 数字引用 | 非法/重复属性、重复列属性、零计数和超限组拒绝；外层定界符不覆盖数组列规格 |
| MathML→LaTeX | `columnalign/columnlines` 列表、缺省/末值重复、单侧边线 | 不支持的列样式产生不可严格替换的诊断命令；调用方仍须保留原 XML |

规格最多 4096 字节、128 列、16 层重复嵌套；共享布局原文读取另外限制 4096 字符。
`p/m/b` 固定宽度、`@/!` 列间材料、`>/<` 声明、宏执行、双线间距尚未映射。
默认 catcode 的常用正文/参数/环境注释后续已补，见 [注释与源信息](latex-comment-source.md)；不执行动态 catcode 或 verbatim 排版。
共享布局保留规格不代表其所有图像渲染器已经实现列对齐/竖线。
XML 重建会规范化列规格，不能恢复原先的重复写法、注释或源码格式。

## 验证

- 转换 featureless 全套：440 项（含文档测试）通过；数组专项 8 项及规格解析 2 项包含往返、嵌套、空格位、拒绝和边界。
- 转换 native 全套：493 项（含文档测试）通过。
- AST 22 项及 1 项文档测试通过；共享布局解析 14 项通过，旧 JSON 兼容与规格不计入符号数得到覆盖。
- Conversion featureless 全目标与 AST/Inference 测试目标 Clippy `-D warnings` 通过；`formula-layout` 功能编译通过。
- 本地 Typst `0.15.1 (9dfd3a08)` 实际编译 7 个实例，包含左右居中、竖线、短行、嵌套、分式、显式文字/算子和原文降级。
  输出经过图像查看，确认行内单元格避免显示公式居中覆盖列对齐。
- 实际编译还暴露连续字母被解释为 Typst 标识符：裸 ASCII 字母串现在投影为独立数学字母，显式文字、算子名和颜色参数保持独立处理；相关断言同步为数学原子序列，不删除验证。
- 图像查看发现旧 `\vec` 导出误用了 Typst 列向量 `vec`；现在向量重音使用 `arrow`，重建支持相应单次完整调用，原先有限 `vec` 输入兼容不在本批删除。该修正只验证常用重音，不代表所有字体的箭头尺寸等价。

Typst 可复现命令（普通 CI 不要求外部编译器）：

```sh
cargo test --locked -p latexsnipper-conversion --no-default-features --test array_columns exported_arrays_compile_with_local_typst -- --ignored --nocapture
```

可用 `LATEXSNIPPER_TYPST_TEST_BIN` 指定本机编译器。
渲染产物在忽略的 `target/array-column-typst-*` 下，不向版本库提交机器路径或图片。
最后一次 7 例渲染 PNG SHA-256：`3fdfeaf17a22b79742a9fe855683bcac8b88f9694e67292462bbd266bcdd30d2`。
后续隐藏 x64 Word 16.0 生产适配器验证：6 例 × 行内/显示/编号共 18 项，插入、保存及只读重开后
列组展开、行/格数及源码身份保持一致。Office 同时修复编号表格导出的生成 `rsidTr` 导致的源读入误拒绝。
见 [Office 专项](https://github.com/strangelion/LaTeXSnipper-Office/blob/main/docs/office/array-column-layout-20261009.md)。
该证据不代表安装版加载、Word 字体观感、x86 或全部 DPI；不将结构或 Typst 证据写成完整 Office 排版验收。

## 规范依据

- [Microsoft OMML 列对齐定义](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.matrixcolumnjustification?view=openxml-3.0.1)
- [Microsoft 列属性与分组](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.matrixcolumnproperties?view=openxml-3.0.1)
- [Typst grid](https://typst.app/docs/reference/layout/grid/)
- [Typst math.mat](https://typst.app/docs/reference/math/mat/)
- [Typst 重音定义](https://typst.app/docs/reference/math/accent/)

选用 grid 是为了逐列对齐，不把只有整体对齐参数的 math.mat 当成完整 array 等价实现。
