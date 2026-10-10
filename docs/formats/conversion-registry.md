# 可执行转换注册路由 v1

更新：2026-10-10。可信 Rust 与既有受限 WASI host 的公式字符串纵向路线已接通，整体第三方插件/应用接入尚未完成。

## 使用与贡献

入口为 `latexsnipper_conversion::conversion_registry::SemanticConversionRegistry`，可通过
`DocumentConverter::convert_registered_formula` 调用。原 `OutputFormat` 和各旧入口不变，不用修改内部枚举来增加示例格式。

1. 用 `register_trusted_format` 注册 `publisher:name` 格式、MIME、说明及可调用验证器。内置 ID/别名和 `core:` 命名空间不能覆盖。
2. 实现 `RegisteredFormulaBackend`，用 `register_trusted_backend` 声明版本、配置/可执行身份摘要、许可、有限路线/平台、损失边界和预算。
3. 注册后默认禁用，显式启用；请求必须显式选择扩展后端。没有选择时只走 `core:builtin`，不自动挑选“最优”插件。
4. `check_ready` 在启用、能力发现及每次调用时检查依赖/撤销状态。出错保留调用方原文，不隐式降级、不重试。

可运行示例：

```text
cargo run --locked -p latexsnipper-conversion --no-default-features --example conversion_extensions
```

示例一将有限 `demo:power-json` 的变量/指数映射到 AST，再输出 LaTeX/OMML；不同输入产生不同内容和来源摘要。
示例二显式选择 `demo:square-command`，将完整 `\demoSquare{单个 ASCII 小写字母}` 映射到幂 AST 后交内置转换器，
拒绝尾部表达式、复合参数和任意宏。它演示局部缺失命令的扩展，不是修复一个已证实的通用 Core 缺陷，也不采用全局字符串替换。
贡献者应提交支持/拒绝样例、许可及损失说明；通用解析/排版错误仍优先修内置实现并增加回归。

## 候选与门禁

- 请求/描述/结果版本为 1；ID 和描述有界，每类注册最多 128 项，元数据最多 16 KiB。
- 输入最多 64 KiB、输出最多 256 KiB，后端可以声明更低预算；空输出、无效损失诊断和超预算结果拒绝。
- 每个新格式必须有可调用验证器，候选返回后再次按目标验证。OMML/MathML 检查根/命名空间、有限元素、Word 属性归属、
  XML 结构预算、字符、实体，以及外链/事件/DTD/处理指令；这不是完整 XML schema 或数学正确性证明。
- `Strict` 仍只用于已验证的 LaTeX→OMML 源语法子集，扩展不能借此放开未知宏。best-effort 候选不是无损转换声明。
- 结果保留后端 ID/版本/配置、原始输入和输出摘要、损失/限制与路线摘要。
  调用方必须提供包含实际 Core build、定义、字体、样式和宿主依赖的上下文 SHA-256；不能只用包版本判断缓存有效。
- 不修改 Office 文档、不返回写入许可。宿主仍需定位、确认、修订、回读/回滚及旧预览失效门禁。

## 本轮发现的内置输出问题

新目标校验发现有色/字号 OMML 的 Word 属性缺少命名空间且嵌进 `m:rPr`；修复为 `m:r` 内的同级 `w:rPr`，
单独导出的包络声明 Word 命名空间，内层明确颜色/字号优先，其他 run 仍接收外层样式。
结构参考 [Microsoft 数学 Run 子元素说明](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.run?view=openxml-3.0.1)。
MathML 显示公式由非标准 `displaymath` 改为 `mstyle displaystyle="true"`，参考
[W3C mstyle](https://www.w3.org/TR/MathML3/chapter3.html#presm.mstyle)。源码保护与分式/脚本结构保持。
已验证 XML 结构，不据此宣称真实 Word 字号/颜色观感或全版式保真。

## 受限 WASI 公式适配

`latexsnipper_plugin_wasi::WasiFormulaBackend` 接收已经校验、显式启用的
`ActivatedRemoteWasiPlugin`，不会从请求中的路径或 URL 加载代码。用 `importer` 或
`exporter` 建立明确的一条 best-effort 路线，再注册和显式启用；WIT v1 不变。

- importer 必须声明 transport→AST 的可用公式能力、钩子与 importer grant。
  返回 `application/vnd.latexsnipper.document+json`、当前 schema 的真实 Document JSON；
  只接收一页一个独立公式，拒绝额外块、assets、notes、outline、诊断、布局/编号及未知公式字段。
  公式源交 Core 已实现的 LaTeX/MathML/OMML/Typst 转换器，不能将任意字符串改名为 OMML。
- exporter 必须声明 AST→transport 及 exporter grant。Core 先生成经过形状检查的裸 LaTeX，
  再序列化单公式 Document；不会将完整 `latex` 文档塞入 Formula.source。输出须匹配 transport、
  为有效 UTF-8，错误诊断拒绝；其他诊断进入损失记录，注册器继续执行目标格式校验。
- 保留现有每次执行信任复验、fuel/epoch、内存/表/资源、并发、输入/输出及诊断预算。
  配置身份包含组件摘要、manifest、路线和实际宿主限额；禁用/撤销、取消或超时不会隐式 fallback。
- 裸公式、AST 投影和 best-effort 是有限重建，不保留完整文档样式/元数据，也不保证视觉或可编辑保真。
  这不是浏览器 Web Worker、动态格式绑定或 Office 写入授权。

可运行 conformance：`cargo test --locked -p latexsnipper-plugin-wasi`。
`fixtures/success-component` 的有限 power importer/exporter 用不同变量/指数验证实际输出变化，
并检查错误 MIME、非法 JSON、多块、非法 UTF-8、超预算、取消和无限循环中止。
这是本地受控包及 target/provenance 的宿主回归，不代替真实远程 registry 签名服务验收。
fixture 独立锁文件已固定；重建的 core WASM SHA-256 为
`cd51c67716a727b85a75dbfe03a7bed93db1a4c407bc9ceec6ba967f05bc1eef`，已扫描无本机路径标记。

## 尚未接通

注册器自身不是加载器或沙箱，可信同步 Rust handler 不具备任意线程强制超时、内存限额或取消。
响应预算是返回后观察预算，不是 handler 分配的峰值内存限制；格式验证器也属于可信代码。
不可信第三方必须走既有受限 WASI host；签名安装/启用/每次执行信任复验不能以一个 `check_ready = Ok` 示例代替。
WASI/WIT 单公式适配已有；公开绑定/CLI/Office 的扩展选择、二进制格式/容器和明确 fallback 策略继续推进。
LaTeX/Typst/Markdown/HTML 字符串候选不会在此执行；展示、TeX 编译或宿主插入仍必须经过对应安全管线。

## 本地验证

- 注册路由专项 10 项及两个可运行示例通过；覆盖默认路由、新格式验证、显式启用/撤销、失败不重试、
  原文不变、预算/版本/平台/注册冲突、身份摘要与输出安全反例。
- 转换 featureless 全套 464 项、native 全套 517 项（含文档测试）通过；既有往返契约通过。
- all-targets/all-features Clippy warnings-as-errors、`wasm32-unknown-unknown` featureless 编译、格式及公共契约冻结校验通过。
- WASI 宿主 unit/integration 40 项通过，含三项注册公式实际组件回归；公开绑定/CLI/Office 的扩展选择和新样式真实 Word 验收仍未完成。

相关：[扩展计划](../conversion-extension-roadmap.md)、[插件边界](../plugin.md)、[格式计划](../formula-format-roadmap.md)。
