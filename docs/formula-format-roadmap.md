# 公式格式与 LaTeX 补全计划

更新：2026-10-14。仅记录能力边界与剩余工作。

## 分层

Core 负责语法、AST、转换和损失诊断，复用现有 capability registry 与 planner。
Office 负责文档/对象定位、OLE 封装、写入确认和恢复。
VSTO 不是公式格式；本应用 OLE 不等于 MathType 对象。

## 当前进度与下一步

| 编号 | 当前状态 | 剩余工作 |
| --- | --- | --- |
| FMT-01 | 基础完成：Rust/C/Python/WASM-JS 已有内置方向和模式能力投影 | 随新格式同步注册表/绑定，第三方统一路由按 EXT-01 至 EXT-04 接通；严格支持仍是有限 LaTeX→OMML 子集 |
| FMT-02 | 进行中：常用 array、注释/源保护、常用定界符/middle、混合 middle 的 MathML 及有限固定尺寸目标/共享解析已有 | 补混合 middle 的 OMML、固定尺寸 OMML/SVG、复杂命令、星号多行环境及高级列修饰；不执行宏或动态 catcode |
| FMT-03 | 进行中：现代浏览器围栏、行内/前缀命名空间、常用列布局/脚本归属、Typst cases 条件列及有限固定尺寸目标已有 | 补字体伸缩、多行/高级样式与真实宿主；有限导出不代表共享图像布局完成 |
| FMT-04 | 进行中：常用数组/定界符、有限现代/固定尺寸 MathML 与 Typst、混合 middle 与 XML 字面字符可回读，既有数组 Word 三入口已验证 | 补更广 Typst 语法及新增围栏/尺寸的真实宿主；安装版、其他 XML/字体/布局和 SVG 裁切另行验收 |
| FMT-05 | UnicodeMath/AsciiMath 已有独立有限语法、正式单公式绑定及原生 WASI profile 路线；UnicodeMath 引号文本、有限函数/n-ary、上下限与被运算式归属、有限显式上下限位置与保护投影已有 | 继续任意名称、其余 n-ary 样式/增长/占位符标志和损失诊断，接 Office UI 与真实 Word buildup/字体验收；见 [线性转换](formats/linear-formula-conversion.md) |
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

- UnicodeMath/AsciiMath 正式支持仍是有限单公式路线；旧文档 OutputFormat 不变，Office UI 和完整语法未完成。
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
- [Typst cases 条件列](formats/typst-cases-layout.md)
- [AsciiMath 边界](formats/asciimath-pilot.md)
- [UnicodeMath 边界](formats/unicodemath-pilot.md)
- [MTEF 只读边界](formats/mtef-readonly.md)
- [总收尾计划](closing-plan.md)
