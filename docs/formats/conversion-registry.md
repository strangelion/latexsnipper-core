# 可执行转换注册路由 v1

更新：2026-10-11。可信 Rust 与既有受限 WASI host 的公式字符串纵向路线已接通，整体第三方插件/应用接入尚未完成。

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

## 显式可用性 fallback

原 `convert` 和 `DocumentConverter::convert_registered_formula` 不自动 fallback。
Rust 调用者可明确调用 `convert_with_fallback(request, policy)`，指定原后端外最多三个
按序备用 ID。策略默认空列表；版本、规范 ID、数量和去重先校验，不自动安装或启用后端。

只有注册器自身确认的未注册、未启用或路线/模式/平台不匹配可以继续选择；这些项标为
`unavailable`，不会执行 handler。一旦调用实际转换，成功或失败均结束，最多执行一次转换。
输入/输出校验、预算、未知格式、信任撤销、取消与超时均不能重试；handler 或验证器返回
类似“不可用”的错误码也不能授权继续。模式、来源/目标、原始输入及宿主上下文不改变。
这不是执行失败后恢复或自动寻找语义替代器。

报告用闭合 `converted`/`rejected` 结果区分候选和拒绝，列出 Core 版本、已验证的路线及实际考虑过的后端、版本、
配置和状态。`policySha256` 绑定有序策略；有效请求的 `requestSha256` 绑定 Core 版本、
平台、路线、原文摘要和调用方上下文；`decisionSha256` 再绑定实际选择和结果。
无效策略不产生可复用摘要，无效请求没有请求摘要；报告自身仍不是缓存许可或文档写入许可。
缓存必须使用决策摘要并重新检查实际 Core build、定义、字体、样式及宿主身份，不能只看
备用列表或最终后端名称。拒绝报告不能作为候选写入。

贡献者示例和 `snipper formula` 已有实际新格式 fallback；C/Python/WASM/Office 尚未接入此策略，不据此宣称
它们已支持动态后端或自动恢复。WASI 后端仍保留既有启用和每次执行信任门禁。

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
  LaTeX 来源可以包含一个合法的 `$…$`、`$$…$$`、`\(…\)` 或 `\[…\]` 包装，
  交共享形状检查后得到裸公式；多个公式、混合正文、损坏包装和文档命令仍拒绝。
  来源摘要保留原始输入，不用去包装后的文本冒充原文身份。
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

## 声明式 WASI profile 与 CLI

`WasiFormulaProfile::from_json` 接收最多 16 KiB 的闭合 v1 JSON；`prepare(store)` 只激活
既有已验证、已启用的包，返回新 registry，避免失败污染调用方已有注册项。没有组件路径、URL、
安装、权限或包启用字段。以下配置是贡献者示例，需要真实包声明对应 transport/AST 能力；
并非内置安装的 `publisher:power` 支持。

```json
{
  "schemaVersion": 1,
  "pluginId": "publisher.power",
  "backendId": "publisher:import",
  "transport": "application/vnd.publisher.power",
  "operation": { "kind": "importer", "output": "omml" },
  "format": {
    "id": "publisher:power",
    "mimeType": "application/vnd.publisher.power",
    "description": "One lowercase ASCII variable and exponent 0..9",
    "validation": { "kind": "regex", "pattern": "[a-z]:[0-9]" }
  }
}
```

`operation` 可改为 `{"kind":"exporter","input":"latex"}`，将真实单公式 AST 导出到该 custom 格式。
importer 目标只能是规范内置语义 ID；exporter 源只能是当前实现的 LaTeX/MathML/OMML/Typst AST 投影。
字符串 API 仍不接二进制 MTEF、图片或 OLE。配置及所有嵌套字段拒绝未知项，版本不匹配拒绝。

验证器为 Core 编译的完整字符串正则，不运行脚本、宏或 guest validator；模式最多 2048 字节、
编译及 DFA cache 各限 64 KiB、nest limit 32，输入最多 64 KiB。完整匹配拒绝尾随正文/换行，除非
配置明确声明接受它们。该检查只能保证声明的字符串子集，不证明数学语义，也不是完整 JSON/XML schema。
内置目标继续执行 Core 安全校验。配置规范化摘要绑定格式、方向、验证规则和元数据，并加入后端配置摘要；
验证器改变即使候选文本相同，路线身份也必须改变。包 identity、限制及每次执行信任复验仍保留。
Rust 调用方可用 `prepare_with_cancellation` 共享取消 token；激活前已取消则拒绝，
注册后取消继续由既有 handler/host 检查。CLI 本轮没有新增交互式取消或监督进程恢复。

CLI 默认没有 WASI profile loader；`--features wasi-formula` 是显式构建开关。
`formula capabilities` 使用同一能力投影，新 profile 后端默认显示禁用；转换必须显式启用并选择后端，
或将它列入明确的可用性 fallback。操作、上下文身份、输出及退出码见 [CLI](../cli.md#registered-single-formula-conversion)。
实际组件和 CLI importer/exporter 回归只验证有限 power fixture，不当作通用第三方格式或 Office 宿主验收。

## 尚未接通

注册器自身不是加载器或沙箱，可信同步 Rust handler 不具备任意线程强制超时、内存限额或取消。
响应预算是返回后观察预算，不是 handler 分配的峰值内存限制；格式验证器也属于可信代码。
不可信第三方必须走既有受限 WASI host；签名安装/启用/每次执行信任复验不能以一个 `check_ready = Ok` 示例代替。
WASI/WIT 单公式适配、声明式 profile、CLI 和 Rust 的预执行可用性 fallback 已有；其他公开绑定/Office 的扩展选择、
二进制格式/容器及其他 fallback 范围继续推进，不能借此重试执行失败。
LaTeX/Typst/Markdown/HTML 字符串候选不会在此执行；展示、TeX 编译或宿主插入仍必须经过对应安全管线。

## 本地验证

- 注册路由专项 17 项及可运行示例通过；覆盖默认路由、新格式验证、显式启用/撤销、可用性 fallback、执行失败不重试、
  原文不变、预算/版本/平台/注册冲突、身份摘要与输出安全反例。
- 转换 featureless 全套 483 项、native 全套 536 项（含文档测试）通过，各有一项显式忽略；既有往返契约通过。
- all-targets/all-features Clippy warnings-as-errors、`wasm32-unknown-unknown` featureless 编译、格式及公共契约冻结校验通过。
- WASI 单公式 importer/exporter 实际组件及禁用门禁回归通过；其他公开绑定/Office 的扩展选择和新样式真实 Word 验收仍未完成。

相关：[扩展计划](../conversion-extension-roadmap.md)、[插件边界](../plugin.md)、[格式计划](../formula-format-roadmap.md)。
