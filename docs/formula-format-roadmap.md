# 公式格式与 LaTeX 补全计划

更新：2026-10-09。仅记录能力边界与剩余工作。

## 分层

Core 负责语法、AST、转换和损失诊断，复用现有 capability registry 与 planner。
Office 负责文档/对象定位、OLE 封装、写入确认和恢复。
VSTO 不是公式格式；本应用 OLE 不等于 MathType 对象。

## 当前进度与下一步

| 编号 | 当前状态 | 剩余工作 |
| --- | --- | --- |
| FMT-01 | 基础完成：Rust/C/Python/WASM-JS 已有方向和模式能力投影 | 随新格式同步注册表与绑定；严格支持仍是有限 LaTeX→OMML 子集 |
| FMT-02 | LaTeX 解析与缺失命令补全进行中 | 补 array 列规格、注释/定界符、复杂嵌套命令、星号多行环境等缺口；明确不支持诊断 |
| FMT-03 | AST 与导出映射进行中 | 完善分式、多行、矩阵、上下标及样式映射；避免 substack 原始身份与空行丢失 |
| FMT-04 | 导入与往返进行中 | 完善真实 OMML/MathML 结构、XML 属性、字体和布局；SVG 文本位置与裁切单独处理 |
| FMT-05 | UnicodeMath/AsciiMath 各有独立有限 experimental parser | 扩大语法与损失范围，接入正式输入/输出注册表、各绑定与 Office UI |
| FMT-06 | raw MTEF v5 检查、精确去重/结构分组和有限引用/槽位诊断已有 | 补 MTCode/显式字体引用的有限 AST 映射及容器只读提取，再接入 Office 文档索引及读取接口；真实/旧版样例需明确来源 |
| FMT-07 | 随新增能力持续收尾 | 同步生成矩阵、公共契约冻结、说明文档和 Office pin；不放宽已有门禁 |
| FMT-08 | Word 身份门禁、Core 源码探测/PNG/SVG 提取、Word 单 PNG 候选及本应用 SVG 源绑定/行内更新已有 | 安装版待验收；补显示/编号及跨格式图片更新、第三方 SVG/XMP、受限 CFB/MTEF、通用读取和其他宿主；见 [读取计划](formats/foreign-formula-read-plan.md) |

## 执行顺序

1. 先补影响现有 Office 输出的 LaTeX、OMML 和布局缺口。
2. 推进 UnicodeMath 与 AsciiMath 的有限正式接入；两者保持独立语法，不混用拼写。
3. 推进 MTEF 只读语义；第三方 OLE 容器提取属于独立宿主工作。
4. 每批能力同步 API、注册表和 UI，明确支持、降级与拒绝范围。

## 必须保留的边界

- UnicodeMath/AsciiMath 当前不在正式 OutputFormat/API/UI 中，pilot 不代表已正式支持。
- MTEF 检查完成仅表示字节边界可读，不代表语义转换、MathType 排版或写入兼容。
- 无元数据的原生 Office 公式导出 LaTeX 必须标注为重建；不保证原源码完全恢复。
- TikZ/PGFPlots、化学式与普通公式分开说明；Core 的 OLE 路线不能代替宿主创建对象。
- 未知/损坏/超限输入明确诊断，保留原文或原字节，不静默删除内容。
- 写入第三方对象、SDK/资产再分发与 MTEF writer 需要独立规范和许可评审。

## 相关资料

- [生成能力矩阵](generated/formula-capabilities.md)
- [AsciiMath 边界](formats/asciimath-pilot.md)
- [UnicodeMath 边界](formats/unicodemath-pilot.md)
- [MTEF 只读边界](formats/mtef-readonly.md)
- [总收尾计划](closing-plan.md)
