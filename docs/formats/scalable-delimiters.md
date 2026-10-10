# 可伸缩定界符与上下标边界

更新：2026-10-10。属于 FMT-02/FMT-03/FMT-04 的常用有限子集，不是完整 TeX 排版支持。

## 已实现

- `\left...\right` 按完整控制词和单个定界符 token 解析，支持嵌套、非对称配对以及点号表示的不可见侧。
  常用圆/方/花括号、尖括号、floor/ceil、单/双竖线、斜线及垂直箭头使用共享有限映射；命名定界符不会混入正文。
- 注释中的假结束标记、控制词前缀和转义控制符不截断配对；严格语法检查拒绝跨分组、环境或数学围栏的结束标记。
  缺失、未知及超深定界符在 best-effort 路径保留原文，在严格替换路径拒绝，不静默制造右括号。
- OMML、MathML 和 Typst 输出保留定界符内分式、嵌套括号、外层上下标及后续表达式。
  普通同时上下标 `x_i^2` 使用组合脚本结构，与显式分组 `{x_i}^2` 区分；既有大型算子 limits 和样式范围继续保留。
- canonical LaTeX 保留控制词分隔和 `\,` 等间距命令；常用 OMML/MathML 围栏重建保留默认侧与显式不可见侧。
  MathML `mfenced` 支持单个/default separator，未知围栏、多个 separator、限定命名空间的围栏属性及裸混合文本明确拒绝。
  OMML 重复围栏属性明确拒绝，不把嵌套或相邻矩阵冒充一个外层矩阵。
- Typst 尖括号和双竖线按当前官方符号名输出 `chevron.l/r`、`bar.v.double`，避免旧符号名导致编译失败。

## 验证与证据

- 7 个新增专项测试覆盖命名/嵌套/不可见/非对称定界符、三种行尾注释、严格拒绝、XML 往返、上下标归属和间距。
- 转换全套 featureless 454 项、native 507 项通过（含文档测试）；共享布局解析 16 项通过。
  相关 AST/Syntax 回归、Clippy warnings-as-errors、格式与公共契约冻结校验通过。
- 本地 Typst 0.15.1 编译 17 个混合实例，并目视检查嵌套分式、单侧竖线、上下标以及命名符号。
  忽略目录 `target/array-column-typst-1791594139700774000/`，PNG SHA-256：
  `6e6aad52185f14612a606edb659f8d5ff4ebbe5d482684906d0832a855f01e69`。
  符号名参考 [Typst 官方符号表](https://typst.app/docs/reference/symbols/sym/)。
- 本批真实 Word 保存/重开、安装版和跨平台字体验证尚未完成；不能用 XML 或 Typst 检查代替宿主验收。

## 仍需推进

`\middle` 的语法 token 检查不代表严格转换已支持；固定大小定界符、任意宏、动态 catcode、完整文本/未分组参数语义、
高级 array、多表达式 OMML separator 与所有 MathML 围栏布局仍需独立补齐。
共享布局接口目前保留有限命令表示，不声称已有全部定界符语义树。
上述证据不证明所有旧版 Typst、TeX 字体/间距等价或 10000 条真实文档保真。

相关：[格式计划](../formula-format-roadmap.md)、[注释与源信息](latex-comment-source.md)、[数组布局](array-column-layout.md)。
