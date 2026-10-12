# Typst cases 条件列

更新：2026-10-12。属于 FMT-03/FMT-04 的有限布局与回读能力。

## 导出与回读

Cases AST 与兼容的 `Matrix { env: "cases" }` 使用同一导出路径。
每个分支以 `&` 开始，数学列之间使用 `&& quad`：值与条件保持左对齐、明确分列和一档相对间距，
不再通过空格拼接而得到类似 `x²x>0` 的内容。
依据：[Typst cases 对齐点](https://typst.app/docs/reference/math/cases/)、[数学多对齐点](https://typst.app/docs/reference/math/#alignment)。

有限回读认识这套完整形状，移除布局标记并恢复 LaTeX cases 的 `&` 列；普通明确对齐点也可恢复数学列。
空列、额外列、长短不一的分支、嵌套 cases/矩阵、脚本与后续表达式保留。
分隔只发生在该分支顶层：字符串内的逗号、分号和字面 `&`，以及嵌套调用内的分隔符不拆开。
不认识的交替对齐形状、参数或代码表达式明确拒绝，不猜测 TeX 列样式，也不执行 Typst 代码。
旧的无列标记输出仍不能唯一恢复条件边界；不从空格推测列。

## 范围与证据

本机 Typst 0.15.1 编译七组作者编写的样例及各自回读后的导出副本，源码逐对相同，
并检查实际渲染中的条件列、空列、额外列、嵌套与外层脚本。
忽略目录 `target/typst-cases-layout/generated.png` 的 SHA-256：
`8fc732cea6c4109ef35dbaf918878679652a3f6edb0dde8d71f2a0cb37fa48bf`。

这不代表 Typst 与 AMS cases 的全部字体、数学样式、行距或高级对齐完全等价，
也不替代安装版 Office、不同字体/版本或独立外部语料验收。
