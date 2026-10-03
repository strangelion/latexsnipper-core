# LaTeXSnipper Core 与 Office 收尾计划

> 状态：执行台账（不是已完成能力声明）  
> 适用范围：`latexsnipper-core`、LaTeXSnipper Office 适配器、Windows Office 验收环境  
> 更新规则：每完成一项必须补充证据、运行环境、commit 和已知限制；没有证据的项目保持未完成。

## 1. 目标与边界

本计划的目标是为 Core、Office 转换链路和真实 Office 应用建立一套可重复的收尾验收，最终让用户能够根据实测数据判断是否选择本应用。

Core 负责解析、统一 AST、转换、渲染、包结构验证和可重复 benchmark。Office 应用负责 WebView2/Tauri UI、Office 自动化、OLE、托盘和真实应用交互。不能用 Core 的包结构测试替代 Word、Excel、PowerPoint 的真实打开、编辑、保存和回读。

以下能力必须分开报告，不合并成一个容易误导的“总支持率”：

- 普通数学公式、绘图（TikZ/PGFPlots）、化学公式、OCR 识别；
- 结构有效、语义保留、视觉一致、可编辑、round-trip；
- Word、Excel、PowerPoint 的不同插入方式；
- 可自动化的 Core/包测试与需要本地 Office 的 GUI/OLE 测试。

## 2. 当前基线

### 已有基础

- `crates/benchmark` 已有 Core smoke、转换耗时和增量编辑 benchmark。
- 增量测试已经覆盖 10、100、1,000、10,000 条公式规模。
- `crates/evaluation` 已提供语料和质量评估基础。
- `crates/fidelity` 已提供 DOCX、PPTX、XLSX、PDF 的包结构、语义、资产、诊断和可选视觉证据。
- `fidelity/corpora/index.json` 已有确定性 fixture 和 SHA-256 校验。
- 现有转换 round-trip 测试已经覆盖矩阵、分段、嵌套结构、颜色和样式等部分场景。
- v3 freeze 清单已修复，当前主线和最近 CI/WASM/CodeQL 结果为绿色。
- Core C 会话接口和 Python 示例已经存在，可作为批量测试入口。

### 当前状态与剩余边界

| 领域 | 当前状态 | 剩余证据或边界 |
| --- | --- | --- |
| 10,000 条综合语料 | 固定 manifest、seed、类别配额、摘要哈希和公开 Core 报告已完成 | 外部真实分布准确率声明仍需许可明确的独立语料 |
| 定界符和 Markdown | display、inline、bracketed math 与 500 个混合 Markdown 文档已进入冻结语料和报告 | 真实 Office 文档排版仍由应用 harness 验证 |
| 错误容忍 | 故意错误 LaTeX 已按预期结果分类并进入 PR 门禁 | 新解析器回归继续进入失败语料库 |
| TikZ/PGFPlots | 需与普通公式分开衡量 | 生成、预览、SVG 安全化、Office 插入和尺寸证据 |
| 化学公式 | 语法、解析器和导出支持范围需明确 | 支持/降级/拒绝矩阵及示例 |
| Office 包 | Core 已有六维、可计数 package round-trip 证据，不等于真实 Office 视觉一致 | DOCX/PPTX/XLSX 在真实应用重新打开、保存和回读报告 |
| Office GUI | Word/Excel/PowerPoint 的实际插入方式未形成完整矩阵 | 应用版本、截图/PDF、回读结果、失败原因 |
| OLE | x64 DLL 打包、安装、注册和 Word/Excel/PowerPoint 回读已在 2026-09-30 真机验证 | x86 Office、不同 DPI/RDP 和剩余宿主操作矩阵 |
| 交叉引用 | Core 已覆盖书签、SEQ、REF 和安全的 dirty 更新请求；固定 Word 环境已验证前置编号变化后的 SEQ/REF 重算及保存重开 | UI 点击、跳转、公式目录及 OOXML 差异摘要 |
| OCR 模型 | readiness 不等于准确率，长时间 `running` 需纳入测试 | 模型身份、完成率、CER/Token 距离、超时和资源数据 |
| Office 应用 UI | 公式库、自定义符号、预览、源码着色、主题、颜色选择器、布局和托盘行为属于应用层 | 真机 WebView2/Tauri 回归录像或截图、控制台/日志 |
| Zig 结合 | Windows x86_64 隔离 C ABI pilot 已完成，现有候选未达到 1.20x 门槛，当前决定不接入主构建 | 只有跨平台 profile 找到新热点后才重开；现有报告见 reports/zig-image-kernel-pilot-2026-09-27.md |

## 3. 10,000 条公式语料规格

数字表示公式记录数；另建 500 个复合 Office 文档场景，复用这些公式，不把复合文档重复计入公式总数。

| 主类别 | 数量 |
| --- | ---: |
| 基础结构、分数、根号、上下标 | 1,400 |
| 矩阵、行列式、数组 | 1,100 |
| `align`、`gather`、多行环境 | 1,000 |
| `cases`、分段函数、条件结构 | 800 |
| 复杂嵌套积分、求和、极限 | 1,300 |
| 概率、统计、分布、期望、贝叶斯公式 | 1,100 |
| 化学公式和反应式 | 700 |
| 自定义符号、字体、颜色、样式 | 600 |
| TikZ、PGFPlots 绘图 | 1,000 |
| 故意错误或不完整 LaTeX | 500 |
| Office 复合文档和交叉引用场景 | 500 |
| **合计** | **10,000** |

下列是正交标签，每条记录至少带有一个标签：

- 包装：`$$...$$`、`\\(...\\)`、`\\[...\\]`、无包装；
- 上下文：纯公式、中文正文混排、标题、代码块、列表、表格、混合 Markdown；
- 复杂度：L1 基础、L2 组合、L3 多层嵌套、L4 多行/绘图、L5 极限和压力样例；
- 结果预期：有效、可恢复错误、必须拒绝；
- 输出目标：LaTeX、MathML、OMML、Typst、SVG、PNG、DOCX、PPTX、XLSX；
- 插入方式：原生公式、SVG、PNG、OLE、剪贴板、批量、行内、独立显示。

语料必须保存：`id`、原文、规范化结果、预期 AST 摘要、预期诊断、类别标签、生成器版本、seed、许可证和哈希。外部数据必须记录来源和许可证，不能把未经授权的论文或截图直接纳入仓库。

## 4. 指标与验收口径

### Core 转换

- 解析成功率；
- AST 结构有效率；
- 语义等价率；
- MathML、OMML、Typst、SVG 输出有效率；
- LaTeX ↔ MathML/OMML/Typst round-trip 成功率；
- 错误诊断的类别、位置和可恢复性；
- 无 panic、无死循环、无无期限等待。

### 性能

- 单条和批量 P50/P95/P99 延迟；
- 100、1,000、10,000 条吞吐量；
- 首次初始化、首次转换和热转换；
- 峰值内存、输出大小、超时率；
- 增量编辑的 touched/reparsed/converted 节点数和缓存命中率。

### OCR

- 完全匹配率、字符错误率 CER、Token 编辑距离；
- 检测 IoU（公式区域）；
- 模型加载、首次推理、热推理和任务总耗时；
- `running` 超时、取消、重试和最终状态；
- CPU/CUDA/DirectML/TensorRT/Paddle 等 provider 的真实可用性。

### Office 六维证据

沿用现有 fidelity contract，不能用一个布尔值替代：

1. `structuralValidity`
2. `semanticPreservation`
3. `layoutPreservation`
4. `visualFidelity`
5. `editability`
6. `roundTripFidelity`

建议首轮基线目标如下，跑完 pilot 后再冻结为 release 门槛：

- 有效输入无 panic/死循环：100%；
- 有效公式解析成功率：至少 99.5%；
- 输出结构有效率：至少 99.9%；
- 核心普通公式语义等价率：至少 98%；
- 无静默丢失的 round-trip：至少 99%；
- 支持的 OOXML 包重新打开：100%；
- 已声明支持的 Office GUI 场景：100%；
- 性能相对基线回退超过 10% 时阻止 release。

TikZ、PGFPlots、化学公式、OLE 和识别模型必须展示独立分数，不能用普通公式分数掩盖限制。

## 5. Office 综合验证矩阵

### Word

- 原生 OMML；
- SVG、PNG；
- OLE；
- 剪贴板粘贴；
- 批量插入；
- 行内/独立显示；
- 字体、字号、颜色、对齐和尺寸；
- 公式编号、书签、`SEQ`、`REF` 和域更新；
- 保存、关闭、重开、回读、修改后再次转换。

### Excel

- 单元格附近公式图形；
- SVG、PNG、OLE；
- 批量单元格/图形插入；
- 行列缩放和锚定；
- 保存、重开、回读。

### PowerPoint

- 文本框或原生公式（按实际 API 能力标记）；
- SVG、PNG、OLE；
- 分组、旋转、缩放、对齐；
- 多页批量插入；
- 保存、重开、回读。

不适用的原生能力标记为 `N/A`，不计入失败率；声明支持的能力必须有真实应用证据。

## 6. 交叉引用专项

至少建立 500 个复合文档场景，覆盖：

- 公式、图、表编号；
- 书签和超链接；
- `SEQ` / `REF`；
- 章节标题引用；
- 删除前置对象后的编号更新；
- Word 更新域后的引用指向；
- 图文混排和跨页布局；
- 转换后对象仍可编辑。

每个场景保留保存前、保存后、重新打开后、更新域后的四组证据。

## 7. CI、夜间任务与发布门槛

### Pull Request smoke

- 每类 20～50 条，总量约 300～500 条；
- 覆盖全部定界符、主要输出格式、错误输入和至少一个 Office 包；
- 不在共享 runner 上设置脆弱的绝对耗时门槛；
- 目标完成时间 5～10 分钟。

### Nightly full

- 完整 10,000 条语料；
- Core 在 Linux/macOS/Windows 的转换和性能；
- Windows Office 包和应用专项；
- 上传 JSON、CSV、截图、PDF、OOXML diff、环境 manifest；
- 固定 commit、seed、字体、Office 版本和模型版本。

### Release candidate

- Core、WASM、CodeQL、依赖审计和 freeze 全绿；
- 10,000 条报告完整生成；
- DOCX/PPTX/XLSX 包重新打开通过；
- 已声明支持的 Word/Excel/PowerPoint 插入方式通过；
- OLE 未通过时必须阻止宣称“支持 OLE”，或明确标为实验能力；
- README、生成文档和已知限制同步；
- 发布 tag 前清理临时模型、临时 Office 文件和失败重试产物。

## 8. README 与用户可见报告

新增生成文档：`docs/generated/formula-office-benchmark.md`。

README 中文和英文都应展示：

- 10,000 条公式和 500 个复合文档的规模；
- 类别、定界符、Office 产品和插入方式；
- 语义、视觉、编辑、round-trip 分数；
- P50/P95/P99、吞吐量和内存；
- OCR 模型身份与准确率；
- OLE、TikZ、PGFPlots、化学公式的独立状态；
- 测试 commit、时间、操作系统、Office/字体版本；
- 复现命令、详细 JSON 报告和已知限制。

“已验证”“实验性”“未测量”“不支持”必须使用不同状态，不能把缺少证据显示为通过。

## 9. 分阶段执行清单

### 阶段 A：整理与冻结范围

- [x] 在 [closing-issue-ledger.md](closing-issue-ledger.md) 建立 Core / Office / GUI 三栏问题台账；
- [x] 为每一项指定维护责任、仓库、证据类型和 P0-P3 阻塞级别；
- [x] 冻结 10,000 条语料 schema、seed 和类别配额；
- [x] 明确普通公式、绘图、化学、OCR 的独立指标。

### 阶段 B：Core 语料和 runner

- [x] 实现确定性语料生成器和 manifest；
- [x] 生成小规模 pilot，冻结 schema、覆盖标签和摘要；
- [x] 接入 `crates/evaluation` pilot runner，输出解析、转换、回转和耗时证据；
- [x] 接入 `crates/benchmark` full runner、分位数、吞吐和进程生命周期峰值常驻内存统计；
- [x] 接入 scheduled 10,000 条 full runner，并输出各阶段 P50/P95/P99；
- [x] 生成 pilot / PR JSON 报告和预期结果分类证据；
- [x] 冻结错误 LaTeX 的结构诊断，并在 PR 语料门禁预期结果分类；
- [x] 覆盖定界符、Markdown、错误输入和复杂嵌套。
- [x] 将 full 合约语料从单模板编号变体扩展为多语法族的确定性组合生成；
- [x] 生成并校验 500 个摘要哈希冻结的 mixed-Markdown 复合文档，覆盖每条 full 公式一次；
- [ ] 纳入许可明确的外部真实分布语料后，再发布面向真实数据的准确率结论；

### 阶段 C：Office 包和转换

- [x] 扩展 `fidelity/corpora/index.json`，加入包级能力期望和 round-trip token；
- [x] 增加 OMML、SVG、PNG 和多对象批量内容的可计数包级回转证据，并将 OLE 激活、剪贴板与真实批量插入明确保留给阶段 D；
- [x] 增加交叉引用 fixture，并为安全的 SEQ / REF 域写入 w:dirty 更新请求；真实 Word 域重算仍保留给阶段 D；
- [x] 生成 Office 六维包级能力报告；
- [x] 明确包级不支持能力、未测量能力和降级诊断。

当前增量已经验证 OMML、SVG、PNG、书签、`SEQ` 和 `REF` 在 Core
DOCX round-trip 后仍有确定性包 token；XLSX OLE 编辑能力明确为
`unsupported`。真实 Word 域更新和 OLE 激活现已由独立宿主证据覆盖；
剪贴板和批量插入仍属于阶段 D，不能由包测试代替。

### 阶段 D：真实 Office 和 OLE

- [ ] Office 加载项功能排版和调整让用户可更舒适快捷使用。
- [x] 在固定 Windows/Office 环境安装并校验 OLE DLL；
- [x] 在真实 Word 中验证 78/78 原生 OMML 保存重开、章节 `STYLEREF`/重置 `SEQ` 以及 `REF`/`PAGEREF` 显示值；
- [x] 在真实 Word 中删除前置编号公式，验证陈旧 `SEQ`/`REF` 经文档级更新同步变化并在保存重开后保持；
- [ ] Word/Excel/PowerPoint 逐项执行插入、保存、重开、回读；
- [ ] 保存截图、PDF、OOXML diff、安装日志和版本信息；
- [ ] 记录 GUI、OLE、字体和尺寸问题；
- [ ] 只把有真实证据的能力标为“支持”。

2026-09-30 增量：固定 Windows 11 / Office 16.0.18526.20672 环境已完成
x64 MSI 安装、DLL 哈希、COM 注册与真实 OLE 激活；Word 的 24 个 OLE、
6 个可编辑图片对象和 78 个原生 OMML 对象，以及 Excel/PowerPoint 各 2 个
OLE 与 2 个图片对象，已通过保存、关闭、重开和源状态回读。Word 还验证了
章节 `STYLEREF`/重置 `SEQ` 的 `1.1` 显示值，以及 `REF` 的 `(1)` 和
`PAGEREF` 的页码。详细记录位于 Office 仓库
`docs/office/real-host-acceptance.md`。2026-10-01 的 O-05 专项进一步删除前置
编号公式，使目标 `SEQ`/`REF` 在刷新前保持陈旧值 `2`，文档级更新后同步为
`1`，保存重开后仍为 `1`；宿主实现提交为
`4cf6d74d45396fbd5b7fc096294b138d7502b516`，证据记录提交为
`9eea8707d5c992f58c20b9b813e613d9f6703b98`。阶段 D 仍未整体完成，因为
OOXML 差异摘要、公式目录、UI 交叉引用流程、批量/剪贴板和更多宿主几何操作
尚缺证据。

2026-10-02 O-06 已启动：真实 Word 扫描 250 条同一积分公式，覆盖四种分隔符；
249 条转换成功，1 条故意无效的 OMML 保留原文，邻接正文、公式数量在保存重开后
仍正确，剪贴板序列号不变。100 条一批实测最高 114.5 秒，因此 Office 调整为
25 条一批。此证据不代表多样公式准确率；管道超时状态核对、实际剪贴板粘贴、
页眉/文本框和 Excel/PowerPoint 批量矩阵仍待完成。详见 Office 真实宿主记录。

2026-10-03 增量：真实 Word 的七处正文、独立分节页眉/页脚、首页页眉和文本框
均转换并保存重开，公式 ID、源及 OMML 清单仍可读；无效定位及旧哈希保留原文。
同时修复浮动绘图锚点导致 Word 位置与纯文本偏移不同的问题，补重复、Unicode、
长公式定位测试。250 条回归仍为 249/1/0，剪贴板不变，耗时 328.093 秒；
O-06 不因此整体关闭，display 段落样式、真实管道/粘贴及其他宿主矩阵仍未验收。
Office 旧提交 bb268a6 的三平台完整打包验证 37098619175 全绿，Windows 无人值守
证书信任及安装生命周期已通过；新源码 CI 需独立确认。

新增排队任务（用户 2026-10-03 明确提出，尚未实现）：

- [ ] 无 `$`/`$$` 的选区可明确按 LaTeX 预览转换；全文裸公式候选单独启用并人工确认，
  不以猜测方式替换普通文字；COM 与 Office.js 入口保持一致。
- [ ] 使用已有公式 ID/revision/hash 做预处理索引，文档修改后重新核对，不长期缓存裸坐标。
- [ ] 分阶段测量扫描、Core/缓存、COM/scratch、清单写入及域更新；按批提交清单、
  跳过未变化公式、集中更新交叉引用，并对冷/热运行进行对照。
- [ ] 可选离线 DOCX 快路径仅操作用户指定已保存副本，需备份、原子提交、Word 重开校验，
  不能覆盖打开中的未保存文档；原生 Word 公式无源元数据时不能声称无损还原原始 LaTeX。

详细实施和验收顺序位于 Office 仓库 `docs/office/batch-update-plan.md`。

### 阶段 E：Office 应用 UI 回归

- [ ] MathLive/公式预览和自定义符号混合公式；
- [ ] TikZ/PGFPlots/Graphviz/Mermaid 安全预览；
- [ ] 公式库、自定义符号、源码语法着色；
- [ ] 主题、颜色选择器、浅蓝默认风格、竖屏布局；
- [ ] 符号素材栏和画布工具栏尺寸；
- [ ] 托盘/任务栏缩略图左右键；
- [ ] 真实 WebView2 CSP/WASM 和 Tauri release 启动。

### 阶段 F：发布与清理

- [ ] PR smoke、nightly full、release candidate 全部通过；
- [x] 更新中英文 README，并生成 Core 10,000 条语料的用户可读报告；
- [ ] 将真实 Office harness、OCR 实测和外部许可语料结果合入最终生成报告；
- [ ] 更新 release checklist、已知限制和迁移文档；
- [ ] 重新确认 freeze manifest、版本和 lockfile；
- [ ] 清理临时模型、Office 测试文档、截图缓存和失败重试目录；
- [ ] 只在证据完整且 CI 全绿后创建 release tag。

## 10. Zig 与渲染后端的后续评估

Zig 不作为本轮收尾的硬依赖。Windows x86_64 隔离 pilot 已完成，
HWC 到 CHW 归一化与透视变换候选均未达到 1.20x 收益门槛，因此当前
明确保留 Rust 实现，不把 Zig 加入 Core 主构建。完整方法、数值和限制见
[zig-image-kernel-pilot-2026-09-27.md](reports/zig-image-kernel-pilot-2026-09-27.md)。
未来只有在跨平台 profile 发现新的原生热点后才重开试验，并继续满足：

- 通过现有 C ABI 调用，不改变 Rust 公共契约；
- 对比目标热点的吞吐、P95/P99、峰值内存和二进制体积；
- 覆盖 Windows、Linux、macOS 和 Android ARM64，并评估 WASM 兼容；
- 只有中位延迟至少改善 20%、内存无实质回退且通过 ABI/安全审查后，才进入 Core 主线。

MathLive/KaTeX 的替换也应先保留兼容 fallback，等 10,000 条渲染和 Office 预览数据通过后再移除依赖，避免把 UI 渲染重构和 Office 收尾混成一个不可回滚的大改动。

## 11. 执行规则

- 没有报告、截图、日志或可复现命令的项目不得标记为完成；
- `unsupported`、`not-measured` 和 `failed` 必须分开；
- 所有性能结论必须带平台、版本、seed 和重复次数；
- 每次修改先跑最小相关测试，再跑对应 CI 层级；
- 任何新发现的问题先追加到本计划，再决定是否阻止 release；
- release tag 前必须由本计划、release checklist 和生成能力表三方交叉核对。
