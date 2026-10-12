# 定界符尺寸与上下标边界

更新：2026-10-12。属于 FMT-02/FMT-03/FMT-04 的常用有限子集，不是完整 TeX 排版支持。

## 已实现

- `\left...\right` 按完整控制词和单个定界符 token 解析，支持嵌套、非对称配对以及点号表示的不可见侧。
  常用圆/方/花括号、尖括号、floor/ceil、单/双竖线、斜线及垂直箭头使用共享有限映射；命名定界符不会混入正文。
- 注释中的假结束标记、控制词前缀和转义控制符不截断配对；严格语法检查拒绝跨分组、环境或数学围栏的结束标记。
  缺失、未知及超深定界符在 best-effort 路径保留原文，在严格替换路径拒绝，不静默制造右括号。
- OMML、MathML 和 Typst 输出保留定界符内分式、嵌套括号、外层上下标及后续表达式。
  普通同时上下标 `x_i^2` 使用组合脚本结构，与显式分组 `{x_i}^2` 区分；既有大型算子 limits 和样式范围继续保留。
- canonical LaTeX 保留控制词分隔和 `\,` 等间距命令；常用 OMML/MathML 围栏重建保留默认侧与显式不可见侧。
  旧 MathML `mfenced` 输入仍支持单个/default separator，未知围栏、多个 separator、限定命名空间的围栏属性及裸混合文本明确拒绝。
  OMML 重复围栏属性明确拒绝，不把嵌套或相邻矩阵冒充一个外层矩阵。
- Typst 尖括号和双竖线按当前官方符号名输出 `chevron.l/r`、`bar.v.double`，避免旧符号名导致编译失败。
- `\middle` 消耗单个定界符 token，不吞入后续操作数。在同一个 `\left...\right` 内支持一个或多个相同 glyph 的中间定界符，
  包括不可见点号、空分段、嵌套定界符、矩阵和外层同时上下标。
  OMML 使用一个 `sepChr` 与多个 `e`；MathML 使用独立 `mrow` 分段和显式 `mo`，有限常用集合内的同层混合 glyph 也可保留。
  回读 OMML 多个基底时不再将它们误作矩阵行；裸 OMML 多个顶层结构不会丢掉后续表达式，截断 XML 明确拒绝。
  Typst 使用周围 `lr` 的 `mid` 伸缩，不把 `middle` 输出成普通文本。
  映射依据：[Open XML delimiter](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.delimiter?view=openxml-3.0.1)、
  [Typst lr/mid](https://typst.app/docs/reference/math/lr/)。
  未增加公共 AST variant、WIT 或序列化字段；OMML 严格替换仍拒绝同层混合 glyph，所有严格源检查仍拒绝跨分组/环境/数学围栏或未知 token。
  混合 glyph 的 OMML best-effort 导出保留 canonical 原文，不伪装成已支持的普通运算符；MathML 可结构化导出和回读。
- 现代 MathML 围栏输出不再使用 `mfenced`：左右与中间 `mo` 处于同一 `mrow`，带明确的伸缩、对称、form、separator 和间距属性。
  矩阵、cases 和 abs/norm/floor/ceil 的围栏复用同一结构；不可见侧使用零 glyph 的显式 operator，保持源配对信息。
  回读在行内核对 opener/middle/closer，不跨分式/脚本基底配对；未知 glyph/属性、混合裸文本、截断 XML 与尾部非 XML 明确拒绝。
  普通无明确属性的 dictionary operator 不被猜测为可伸缩源配对；旧 `mfenced` 保持兼容读取。
  行内输出使用标准 `mstyle displaystyle="false"`，前缀 XML 变体提供子节点的默认 MathML 命名空间。
  依据：[MathML 3 围栏展开形式](https://www.w3.org/TR/MathML3/chapter3.html)、
  [Microsoft 对 Web mfenced 的说明](https://devblogs.microsoft.com/math-in-office/mathml-element-deprecated-on-web/)。
- 固定尺寸的 `big/Big/bigg/Bigg` 及每档的 `l/m/r` 形式，共 16 个命令按单 token 解析。
  四档分别投影到 `1.2em/1.8em/2.4em/3em`，类别保持 normal/opening/relation/closing，不把一对固定符号合成自动增长围栏。
  MathML 使用 `mo` 的相等 `minsize/maxsize`、明确 form/fence/spacing；有限回读只接受已支持的尺寸和类别形状，
  不支持的尺寸范围、样式、qualified 属性及未知 glyph 明确拒绝。数学子节点连接保留命名控制词与后续字母的边界。
  Typst 使用显式代码长度 `lr(size: #1.2em, ...)` 等与 `class`；共享布局保留现有 command/argument 结构和 canonical 源码。
  依据：[MathML Core operator 尺寸约束](https://www.w3.org/TR/mathml-core/)、[Typst class](https://typst.app/docs/reference/math/class/)。
  OMML 尚未实现相对字号固定尺寸映射：严格替换拒绝，best-effort 保留完整 command/token 文本；未替换为自动增长括号。
  这些分档是有限目标投影，不承诺所有 TeX 字体、math style 或目标字体 glyph 变体的高度/间距完全相等。
- Typst 固定尺寸回读接受上述四档、四类别和有限符号集合的 `class(..., lr(size: #...em, ...))`。
  新导出的不可见符号也通过 `lr(size: ..., #box())` 携带尺寸；旧的无尺寸 `class(..., #box())` 无法恢复原档位，明确保留/拒绝而不猜测。
  混合表达式按完整调用、字符串和脚本操作数划分，分式后续内容、命名控制词边界、组合脚本与显式分组不会被前缀调用吞入。
  有限 `lr/mid` 回读保留嵌套、非对称/不可见侧和混合 middle；中线不跨分式或脚本操作数。
  错误/截断、未知调用/尺寸/类别/选项、代码表达式和超限输入在 fallible 转换、导入及共享布局入口报告错误。
  旧的 String 解析入口保留原文，不代表输出一定是有效 LaTeX；不能将原文保留当作成功转换。
  不执行 Typst 宏或代码；公式输入限 64 KiB、有界嵌套。常用矩阵/cases 回读保留行/操作数，但 cases 已丢失的条件列信息不被猜测恢复。

## 验证与证据

- 7 个新增专项测试覆盖命名/嵌套/不可见/非对称定界符、三种行尾注释、严格拒绝、XML 往返、上下标归属和间距。
- 转换全套 featureless 454 项、native 507 项通过（含文档测试）；共享布局解析 16 项通过。
  相关 AST/Syntax 回归、Clippy warnings-as-errors、格式与公共契约冻结校验通过。
- 本地 Typst 0.15.1 编译 17 个混合实例，并目视检查嵌套分式、单侧竖线、上下标以及命名符号。
  忽略目录 `target/array-column-typst-1791594139700774000/`，PNG SHA-256：
  `6e6aad52185f14612a606edb659f8d5ff4ebbe5d482684906d0832a855f01e69`。
  符号名参考 [Typst 官方符号表](https://typst.app/docs/reference/symbols/sym/)。
- Office pin `beff6a8` 的 11 个数组混合样例通过隐藏 x64 Word 16.0 行内/显示/编号三入口，共 33 项插入、保存/只读重开验证。
  源码、数学文字、列布局、定界符 glyph 和脚本/操作数相关祖先结构保持；详细输入及证据哈希见
  [Word 报告](https://github.com/strangelion/LaTeXSnipper-Office/blob/main/docs/office/array-column-layout-20261009.md)。
  安装版、截图字体观感和跨平台字体验证尚未完成；直接生产适配器不等于安装版管道。
- 2026-10-12：定界符专项现在为 11 项，覆盖 `middle` token、分段、空分段、嵌套/矩阵/上下标、XML 回读、默认 separator、
  裸 OMML 尾部和明确拒绝。featureless 与 native 转换全套通过；上述 Word 报告不包含新增 `middle`，不能代替其真实宿主验收。
  本机 Typst 0.15.1 编译并目视检查 29 个导出实例（17 个旧例 + 12 个 `middle` 例）。忽略目录
  `target/array-column-typst-1791786423589480100/`，PNG SHA-256：
  `381ae539e8234bea51caf56f6a939be78e0951cd9912db6250fcf3930f3a3895`。
  两种 feature 配置的 all-targets Clippy warnings-as-errors、`wasm32-unknown-unknown` 检查、格式和冻结清单校验通过。
- 固定尺寸专项：16 个命令 × 19 个 token 的 MathML 分档/类别和回读，另有操作数/上下标归属、嵌套、注释、未知形状拒绝、
  OMML 源保护与共享布局测试；featureless/native 转换全套、AST/Syntax 和共享解析回归通过。
  本机 Typst 0.15.1 编译并目视检查 10 个实际导出实例（包含不可见定界符的脚本基底），忽略目录 `target/fixed-delimiter-runtime-1791788904898904500/`，
  PNG SHA-256：`ffd5c20a413524ceba78ba92170a52ca7a2517a2990eb915714826e54be53e9d`。
  实际 Chromium 154 检查四档括号逻辑高度依次为 40、47.33、61.33、89.33 CSS px；小档括号 40 px，邻接嵌套分式 75.875 px。
  不可见基底的组合脚本宽度 10.70 px、无横向溢出，console/network 无错误。浏览器截图 SHA-256：
  `b8aa6a5a6d180bec9742705d903141fe89359566513c35286ffdebc23de31272`。
  该轮截图暴露 legacy `mfenced` 围栏在 Chromium 中缺失，后续已按下项修复；反斜线的字体伸缩差异仍未完成。
- 现代围栏专项 7 项覆盖嵌套/非对称/空侧/脚本、混合 middle、旧输入、跨作用域与截断拒绝、矩阵/宏、行内及前缀命名空间。
  两种 feature 配置转换全套及 all-targets Clippy 通过。实际 Chromium 154 检查 15 个显示样例及 4 个行内 XML 变体：
  嵌套围栏/中线共享父行，矩阵两侧高度均为 61.33 px、矩阵内容 55.71 px；不可见侧宽度为零，混合竖线均可见。
  四个行内变体通过 DOMParser 命名空间检查并渲染为相同 69.25 × 40 CSS px；无横向溢出和 console/network 错误。
  截图 SHA-256：`3ada1a3deca8cf20c0c20b8563e4b5df57216c5fbc12a56915dfeb652d2e5b6a`。
  同轮本机 Typst 0.15.1 编译 15 个导出实例，忽略目录 `target/fixed-delimiter-runtime-1791792243219178800/`，
  PNG SHA-256：`4e3d3445a66a9062bb7eb7bd8f1d00dffc01643eb8d3dc7f81095c0995843a16`。
  Typst 的 cases 条件间距尚不与 TeX 等价；上述浏览器/编译证据不替代新增真实 Word、全部字体或完整 MathML schema 验收。
- Typst 回读专项覆盖 16 个命令 × 19 个 token，以及混合分式、嵌套、脚本归属、调用尾部、字符串分隔符及未知形状拒绝。
  本机 Typst 0.15.1 编译并检查更新后的 15 个导出实例；不可见符号的新尺寸包装与此前 PNG 的 SHA-256 相同：
  `4e3d3445a66a9062bb7eb7bd8f1d00dffc01643eb8d3dc7f81095c0995843a16`。

## 仍需推进

同层混合 glyph 的 OMML 映射、固定尺寸的 OMML/SVG 相对字号与字体保真、任意宏、动态 catcode、完整文本/未分组参数语义、
高级 array、有限集合外的 separator/字体伸缩、更广 Typst 回读和 cases 间距及完整 MathML 围栏布局仍需独立补齐。
共享布局接口目前保留有限命令表示，不声称已有全部定界符语义树。
上述证据不证明所有旧版 Typst、TeX 字体/间距等价或 10000 条真实文档保真。

相关：[格式计划](../formula-format-roadmap.md)、[注释与源信息](latex-comment-source.md)、[数组布局](array-column-layout.md)。
