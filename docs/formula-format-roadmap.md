# 公式格式能力与 LaTeX 补全后续计划

更新：2026-10-04。执行计划，不是新增格式已实现的声明。

## 分层原则

Core 统一负责格式识别、解析、AST、转换、损失诊断和严格校验；Office 层负责
宿主能力、选区/对象定位、OLE 封装、COM/Office.js 写入、回滚与保存重开。
VSTO 是加载项技术，不作为公式格式。应用自己的 OLE 不等于 MathType 对象。

复用 `crates/conversion/src/capability_registry.rs` 和 `planner.rs`，不建立另一套
与现有能力表互相漂移的注册表。区分语义格式、图像格式、宿主容器；明确输入/输出
方向、编译目标、运行时、可编辑性和损失。不要由前端按钮推断后端完整支持。

## 已有代码入口与待补方向

| 格式/对象 | 当前代码入口或边界 | 下一步 |
| --- | --- | --- |
| LaTeX | syntax、latex_parser、latex_ast、严格 OMML 源校验 | 建立缺失命令/环境回归台账，逐项补解析、AST、导出与拒绝诊断 |
| MathML | mathml、mathml_parser、convert_mathml_string | 从已有输入/输出能力生成双向矩阵；注明原源重建与语义损失 |
| OMML | omml、omml_parser、convert_omml_string | 继续补 Word OMath 实测缺失节点；无元数据导出 LaTeX 必须标注重建 |
| Typst | typst、typst_parser、convert_typst_string | 同一 corpus 对照上下标、矩阵、cases、样式和嵌套语义，不宣称任意源无损 |
| Markdown 数学 | markdown_parser、MarkdownInline/Block 输出 | 明确分隔符、正文/代码块保护与数学源提取 |
| UnicodeMath / AsciiMath | 现有语义 OutputFormat 不含这两个目标 | 先定义语法/AST映射、有限支持范围与 fixture，验收后加入注册表/API/UI |
| MathType MTEF | 现有 OutputFormat 不含 MTEF | 独立评估规范、接口、样例来源和授权边界；先只读解析/诊断，验证后考虑写入 |
| OLE / 图片 | planner 路线不代表 Core 可执行 Windows OLE | Core 提供源/payload/图像和损失信息；宿主层创建并回读真正的目标对象 |

## 排队顺序与验收条件

- [x] FMT-01：将输入方向与严格/最佳努力模式补进现有能力投影，输出可读支持矩阵；
  保持已有 API 兼容，并明确 WASM、C/Python 与原生运行时限制。
  - [x] Rust 能力投影包含输入、目标、模式、重建路径和拒绝原因，复用现有导出注册表。
  - [x] 新增 `DocumentConverter::convert_formula_string`；旧函数与既有序列化字段不变。
  - [x] [生成矩阵](generated/formula-capabilities.md)含原生/WASM 方向及尚未实现输入；
    表格漂移由转换 crate 测试阻止。5 种现有输入各测试 9 个语义输出，另测严格拒绝。
  - [x] Python 新增无模型的 `convert_formula` 和 `formula_conversion_capabilities`，
    默认 strict，仅已支持的 LaTeX→OMML 子集可用；其他模式需显式 best-effort。
    能力查询和执行复用相同注册表；旧 Session/识别入口不变，转换时释放 GIL。
    Windows Python 3.11 release wheel 已构建、安装并执行 smoke：144 条路线中
    46 条可用、98 条明确拒绝；错误标签、严格语法拒绝、超限与失败隔离通过。
    Python 6 项 Rust 单测、轻量 270 项/原生 310 项转换测试、Clippy、WASM 编译及
    冻结检查通过；其他平台由已有 wheel CI 验收。
  - [x] C 新增无模型 `latexsnipper_formula_convert` / `latexsnipper_formula_capabilities`；
    复用 v3 envelope、旧字符串释放方式及 strict 默认，未知 JSON 字段/枚举明确拒绝。
    Windows release 动态库 ctypes 实测 46 条转换、98 条拒绝，错误/空指针/UTF-8/
    长度边界及旧 Session 兼容通过；三平台 CI 已接入相同 smoke，尚未宣称远端全绿。
  - [x] WASM 已实现 `convert_formula_v3` / `formula_capabilities_v3` 和 JS 强类型
    同步模块助手，复用原 v3 envelope；release Node 与初始化 Web ESM 包在 Node 中
    各实测 144 条路线（46 条转换、98 条拒绝），默认 strict、错误/超限/DTD 拒绝通过。
    实际编译后的 TS 助手、bundler 构建、42 项 JS 单测、WASM 全目标编译及 Clippy 通过。
    浏览器单测已接入原 Chrome/Firefox CI，本地未做浏览器视觉/真实 WebView2 验收。
    本批不扩展识别 Worker RPC，转换的隔离执行和硬取消由调用方负责。
  - 新入口对源及重建 LaTeX 增加单次 64 KiB、64 层词法嵌套、512 个结构 token
    保守预算，并拒绝 XML DTD；超限明确失败，旧转换/批量接口不受此预算影响。
    这是资源保护而非完整语法校验，也不代表真实 Office 的准确率或视觉验收。
- [ ] FMT-02：记录 Office/Core 失败样例，按缺失命令、环境、AST 节点、导出节点、
  样式损失、性能超限分类；每条都先有可复现的预期结构再实现，不只补渲染别名。
  - [x] 首批合成最小样例登记到现有 failure-corpus 流程：`dfrac`、`tfrac`、
    `cfrac`、`substack`；哈希、预期结构和严格拒绝/源码保留测试齐备。
    记录仍为 minimized，未宣称修复，也未提升到 approved/promoted。
  - [ ] 补齐环境、样式丢失、跨格式 AST/导出及性能超限台账，逐项修复并验收。
  - 2026-10-04：`dfrac` / `tfrac` 已保留两项 AST 操作数，MathML 导出和回读保留
    display/text 样式，Typst 使用 `display(frac(...))` / `inline(frac(...))` 并回读。
    依据：https://typst.app/docs/reference/math/sizes/ 。嵌套、相邻表达式、上标有回归测试。
    OMML 最佳努力保留分数结构，强制尺寸样式未经 Word 实测，严格模式仍拒绝。
    原 minimized fixture 是哈希固定的入库失败证据，保持原文，不代表当前实现状态；
    当前结构回归见 `crates/conversion/tests/styled_fractions.rs`。
    连分数与堆叠结构继续分批验收，不将本批记作 FMT-02/03 全部完成。
    本批范围是花括号操作数；轻量 257 项、原生 297 项测试通过，Clippy、WASM
    编译和冻结检查通过。未运行 Typst 排版或 Word 尺寸视觉验收。
  - 2026-10-04：接续未提交的 `substack` 实现，以 Command 的各项操作数保留各行；
    LaTeX AST 和 standalone Typst `script(vec(delim: none, ...))` 支持结构回读。
    MathML 导出居中多行表，OMML 最佳努力导出 `eqArr` 并保持求和下限归属。
    7 项专项覆盖嵌套栈、矩阵内部换行、空行、中文/空白、单行和严格拒绝；
    新增 Typst 无定界符向量回读只接受完整调用，尾部或不完整表达式保留原文。
    轻量 264 项、原生 304 项转换测试通过，Clippy、WASM 编译和格式检查通过。
    MathML/OMML 回读重建、任意 Typst 文档/空行往返及真实 Word 行距尺寸未验收，
    严格 OMML 继续拒绝；原 minimized 入库证据不修改、不提升为 promoted。
  - 2026-10-04：`cfrac` 首批保留两项操作数、嵌套连分数与可选对齐源参数，
    第三项 Command 参数专门存放该可选参数（不是第三个数学操作数）。
    MathML 映射 display 样式，合法 `[l]` / `[r]` 映射为 `mfrac` 的
    `numalign`；依据 [MathML 3 分数属性](https://www.w3.org/TR/MathML3/chapter3.html#presm.mfrac)。
    Typst 映射为 display 分数、OMML 最佳努力保留分子/分母结构，但两者不保留
    可选对齐；未知对齐仅保留源，MathML 最佳努力使用默认对齐，严格 OMML 均拒绝。
    5 项专项覆盖嵌套、可选参数的 AST 回读、外围上下标、中文/空白、无效参数及
    XML 属性注入防护，另检查导出的 XML 良构；测试不代表 MathML schema/宿主视觉验收。
    本批限定花括号操作数，不承诺任意省略括号语法；Typst/MathML 回读会重建为
    display 分数而非保证 `cfrac` 原命令身份，可选对齐回读尚未支持。
    轻量 269 项、原生 309 项转换测试、Clippy、WASM 编译和冻结检查通过。
    连分数具体尺寸、间距及浏览器对 `numalign` 的支持未实测；FMT-02–04 仍为部分完成。
- [ ] FMT-03：优先覆盖分数/根式、上下标、定界符、矩阵、align/cases、嵌套积分、
  概率统计和化学输入；保留源区间，未知命令不能静默掉字或冒充可编辑目标。
  - 2026-10-04：新增 MathML 表格行列丢失台账及哈希固定合成样例。
    修复 `mtable` 回读跳过多列整行、将单列多行合并为一行的问题；按 XML 行边界
    重建矩阵，不再从 LaTeX 文本中的 `&` 猜测单元格。7 项专项覆盖行列、空行/空单元格、
    嵌套矩阵、命名空间、生成矩阵和 standalone `substack` 的行数回读。
    `substack` 的 MathML 回读目前重建为 `matrix`，保留各行，不保留原命令身份/字号；
    带括号矩阵、标签行的排版和真实 Word 视觉仍待验收，不能据此声称任意 MathML 无损。
    原 minimized 失败证据不改写、不提升为 promoted；当前验收见 `mathml_tables.rs`。
    轻量 278 项/原生 318 项转换回归、Clippy、WASM 编译和冻结检查通过。
    新发现 XML 实体独立事件和文本空格丢失列为下一批，不混入矩阵修复验收。
  - 2026-10-04：OMML `m:m` 回读现在重建完整 `matrix` 环境，不再输出裸行列分隔符；
    单个完整矩阵的定界符回读保持 `pmatrix` / `bmatrix` / `Bmatrix` / `vmatrix` /
    `Vmatrix`，覆盖 1×1 和单列。7 项专项还检查嵌套 OMML、空单元格/空行、再导出
    的行列数以及相邻矩阵/尾部不被错误展开；保留原有兼容行读取分支。
    LaTeX 嵌套矩阵源在现有 AST 解析中仍被错误拆行，已复现并排入下一批；
    本批嵌套测试是直接 OMML 输入，不宣称该 LaTeX 导出链路已修好。
    结构保留不代表定界符尺寸/空行的 Word 排版验收，严格支持范围不扩大。
    轻量 293 项/原生 333 项转换回归、Clippy、WASM 编译及冻结检查通过。
  - 2026-10-04：接续修复 LaTeX 嵌套矩阵 AST。环境结束按嵌套环境栈定位，避免
    内层结束截断外层；行列分割复用同一顶层扫描器，不拆分花括号/内层环境中的分隔符。
    空单元格和内部空行保留，最终行终止符不额外产生空行；环境命令间空白/中文有覆盖。
    7 项专项验证 nested LaTeX→OMML→LaTeX、尾部表达式、分组/堆叠条件、空行和
    不配对环境的严格拒绝。其他格式输出及 MathML 嵌套环境整体提取仍需后续回归，
    注释、array 列规格和定界符视觉不纳入本批的完整支持声明。
    轻量 300 项/原生 340 项转换回归、Clippy、WASM 编译及冻结检查通过。
- [ ] FMT-04：对每条补全运行 LaTeX 与 MathML/OMML/Typst 的结构/语义往返、
  严格拒绝及 best-effort 诊断；化学与 TikZ/PGFPlots 必须独立标识和测试。
  - 2026-10-04：修复 MathML→LaTeX、OMML→LaTeX/布局回读忽略独立 XML 引用事件
    的丢字问题；支持 5 个预定义实体、十进制/十六进制合法 XML 1.0 字符引用和字面 CDATA。
    文本节点的空格保留，结构缩进不作为数学内容；未知实体和非法字符引用明确失败，
    不扩展外部实体/DTD。8 项专项复现后通过，原生 326 项回归、Clippy、WASM 编译通过。
    任意 LaTeX 文本特殊字符转义、XML 属性保真和 Office 文件导入读取器仍需单独验收；
    这不是对任意 MathML/OMML、真实 Office 或排版完全无损的声明。
  - 2026-10-04：MathML 矩阵输出改为读取已解析 AST，保留前后表达式、相邻/嵌套
    矩阵、上下标及矩阵分子；五类定界符位于同一 mrow，cases 不再添加右花括号。
    Matrix/Cases 的 AST 源码序列化输出真实行列，不再用省略号替代操作数。
    新增 7 项专项；array 列规格、矩阵位于根式/其他命令内部的组合与视觉保真另列待办。
    同时修复前批 OMML 矩阵环境改动引发的 cases 回归：完整单侧定界矩阵回读为 cases，
    新增单格、单行、多行和嵌套专项，不放宽原断言。远端失败 run 37169775399 的
    12 项 conversion_roundtrip 本地全部通过；本批没有修改 CI 或锁文件。
    Windows 全工作区 `cargo test --locked --workspace`、轻量 308 项转换回归、Clippy、
    WASM 编译及 28 文件/19 源码树冻结检查通过；仅刷新 conversion 源码树哈希。
  - 2026-10-04：DOCX 正文/超链接/简单域、PPTX 文本、XLSX 行内/共享字符串和
    单元格公式接入同一 XML 文本/引用/CDATA 解码；保留文本节点空格，未知或非法引用
    与 XML 标签错配明确失败，不返回看似完整的部分结果。DOCX 自闭合粗体/斜体/下划线
    属性及显式关闭值一起补回归。8 项内存 ZIP 专项、原生 356 项转换回归和原生/轻量
    Clippy 通过；表格独立导入、SVG 文本、属性值保真和真实宿主视觉继续待办。
    上一批 e0520f5 的远端 CI、WASM、CodeQL 已确认全绿。
  - 2026-10-04：DOCX 表格预处理改用 XML 事件的真实字节边界，不把 tblPr 算作嵌套
    表格，也不按任意字节切片扫描中文。新增 3 项表格属性/相邻表格/中文/非法引用专项；
    单元格文本保留实体、CDATA 和空格，无法解码的表格明确失败。行列/合并解析仍为
    既有有限子集，不宣称复杂嵌套表格无损。
    SVG 文本新增 4 项专项：独立引用/CDATA/空格、嵌套与空 tspan、非法文本诊断以及
    带样式/坐标的 tspan 降级警告。普通文字不再提前截断；样式坐标仍会扁平化，需保留
    原 SVG 资产获取视觉保真。本批不处理 Office 插图边界/缩放验收。
    原生 363 项、轻量 312 项转换回归，原生/轻量 Clippy、12 项根往返、WASM 编译
    与冻结检查通过；仅刷新 conversion 源码树，无新依赖或公共 API/ABI/schema 变更。
- [ ] FMT-05：UnicodeMath/AsciiMath 有界 pilot；用公开、可复现样例验证优先级、
  矩阵、空白和分组规则，不用简单字符串替换冒充 parser。
- [ ] FMT-06：MathType/MTEF 单独设计审查；未通过前继续显示 unsupported，
  不修改第三方 OLE、不删除作者原件。
- [ ] FMT-07：接入固定语料和错误语料，记录耗时、语义保留、损失及拒绝率，
  再由 Office 对可插入的组合做真实宿主矩阵；更新 freeze 时核对哈希而非放宽门禁。

## Office 进度关联

2026-10-03 Office 格式弹窗已做桌面与 Office.js 流程验证；原生 Word 的行内、
行间本应用 OMML/OLE 双向转换及保存重开专项通过（单个积分语法、两种显示方式，
不是综合准确率）。编号/交叉引用迁移、跨故事、真实桌面管道和实际 Office.js
宿主仍未验收。详见 Office 仓库 `docs/office/batch-update-plan.md` 与
`docs/office/real-host-acceptance.md`；Core 不因此宣称第三方格式兼容已完成。

## 本轮契约审查

FMT-01 首批是 Rust 增量 API；没有修改旧的 `TargetFormatCapability` 字段、
`OutputFormat` 枚举、旧转换接口、C ABI 或 v3/WASM envelope。
严格模式目前仅调用现有 LaTeX→OMML 源校验，不代表任意源无损；其他严格路线
拒绝执行。UnicodeMath、AsciiMath、MTEF 仅登记 unsupported，没有新增 parser。
按上述差异复核后刷新 `v3-contract-freeze.json`，不删减合约文件或公共源码树门禁。
生成矩阵及轻量/原生回归不替代全面语义准确率、Office 真机或浏览器 WASM 运行测试。
`substack` 与 `cfrac` 本批复用既有 Command 表达行/操作数，没有新增公共枚举或序列化字段，
没有改变 C ABI 或 WASM envelope；复核新增解析/导出和私有辅助函数后，
只刷新 conversion 公共源码树哈希，保持全部冻结门禁。
Python 本批新增模块函数和类型声明；新模式入口增加保守输入预算及 XML DTD 拒绝，
旧转换/识别 API、C ABI、WASM envelope 和依赖锁文件未变。复核后仅刷新 conversion
公共源码树哈希；未删减 28 个文件和 19 个源码树的冻结门禁。
C 本批仅增加两个无模型函数、头文件声明和请求 DTO，旧 Session ABI v1、符号签名、
v3 envelope 及字符串所有权不变。输入/模式枚举增加 Deserialize，保持原 kebab-case
拼写；Cargo.lock 仅增加已有 conversion crate 的 FFI 依赖边，不升级第三方包。
复核后刷新 capability_registry 文件及 conversion/ffi 源码树哈希，保持全部门禁。
WASM/JS 本批只增加模型无关的两个 v3 函数和 TS 模块助手，不修改既有 v3 envelope
字段、Worker 协议、识别和旧文档转换接口；没有新依赖。复核后只刷新 conversion/wasm
源码树哈希；能力矩阵漂移、全部冻结和既有 Worker 单测门禁保持。
