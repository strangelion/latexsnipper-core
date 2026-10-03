# 公式格式能力与 LaTeX 补全后续计划

更新：2026-10-03。执行计划，不是新增格式已实现的声明。

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

- [ ] FMT-01：将输入方向与严格/最佳努力模式补进现有能力投影，输出可读支持矩阵；
  保持已有 API 兼容，并明确 WASM、C/Python 与原生运行时限制。
- [ ] FMT-02：记录 Office/Core 失败样例，按缺失命令、环境、AST 节点、导出节点、
  样式损失、性能超限分类；每条都先有可复现的预期结构再实现，不只补渲染别名。
- [ ] FMT-03：优先覆盖分数/根式、上下标、定界符、矩阵、align/cases、嵌套积分、
  概率统计和化学输入；保留源区间，未知命令不能静默掉字或冒充可编辑目标。
- [ ] FMT-04：对每条补全运行 LaTeX 与 MathML/OMML/Typst 的结构/语义往返、
  严格拒绝及 best-effort 诊断；化学与 TikZ/PGFPlots 必须独立标识和测试。
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
