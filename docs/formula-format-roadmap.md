# 公式格式与 LaTeX 补全计划

更新：2026-10-10。仅记录能力边界与剩余工作。

## 分层

Core 负责语法、AST、转换和损失诊断，复用现有 capability registry 与 planner。
Office 负责文档/对象定位、OLE 封装、写入确认和恢复。
VSTO 不是公式格式；本应用 OLE 不等于 MathType 对象。

## 当前进度与下一步

| 编号 | 当前状态 | 剩余工作 |
| --- | --- | --- |
| FMT-01 | 基础完成：Rust/C/Python/WASM-JS 已有内置方向和模式能力投影 | 随新格式同步注册表/绑定，第三方统一路由按 EXT-01 至 EXT-04 接通；严格支持仍是有限 LaTeX→OMML 子集 |
| FMT-02 | 进行中：常用 array、注释/源保护及常用嵌套/命名/不可见定界符已有 | 补 middle/固定大小定界符、复杂命令、星号多行环境及高级列修饰；不执行宏或动态 catcode |
| FMT-03 | 进行中：array 常用列布局与定界符/组合上下标归属已有 | 继续多行、样式与真实宿主验证；高级 array 和共享图像布局不由有限导出代表 |
| FMT-04 | 进行中：常用数组列属性与 XML 字面字符可重建，Word 三入口保存/重开源码、文字和结构已验证 | 补安装版与其他 XML/字体/布局；SVG 文本位置与裁切单独处理 |
| FMT-05 | UnicodeMath/AsciiMath 各有独立有限 experimental parser | 扩大语法与损失范围，接入正式输入/输出注册表、各绑定与 Office UI |
| FMT-06 | raw 检查、精确去重/结构分组、有限诊断、独立 experimental AST/LaTeX 读取及有界语义批次复用已有 | 补显式字体/更多模板和有界容器提取，再接 Office；真实/旧版样例与正式注册仍需独立验收 |
| FMT-07 | 随新增能力持续收尾 | 同步生成矩阵、公共契约冻结、说明文档和 Office pin；不放宽已有门禁 |
| FMT-08 | Word 身份门禁、Core 源码探测/PNG/SVG 提取、Word 单 PNG 候选及本应用 SVG 源绑定/行内更新已有 | 安装版待验收；补显示/编号及跨格式图片更新、第三方 SVG/XMP、受限 CFB/MTEF、通用读取和其他宿主；见 [读取计划](formats/foreign-formula-read-plan.md) |

## 执行顺序

1. 先补影响现有 Office 输出的 LaTeX、OMML 和布局缺口。
   当前定界符回归与 CI 收尾后，按 [转换扩展计划](conversion-extension-roadmap.md) 补统一路由、后端选择和最小贡献者示例。
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
- [Array 列规格与布局边界](formats/array-column-layout.md)
- [注释、定界符与源信息](formats/latex-comment-source.md)
- [可伸缩定界符与上下标](formats/scalable-delimiters.md)
- [AsciiMath 边界](formats/asciimath-pilot.md)
- [UnicodeMath 边界](formats/unicodemath-pilot.md)
- [MTEF 只读边界](formats/mtef-readonly.md)
- [总收尾计划](closing-plan.md)
