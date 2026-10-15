# Core 结构化公式补全

更新：2026-10-15。

当前提供有限 LaTeX 编辑结构、候选及应用接口，位于
`latexsnipper_conversion::formula_completion`。Rust、C、Python、JSONL 与 WASM/JS
公共绑定已有。定义驱动签名/作用域接口先提供 Rust 与 JSONL；
其他绑定的作用域扩展、更广语法及实际编辑器接入仍待完成。

## 使用

```rust
use latexsnipper_conversion::formula_completion::{
    analyze_formula_context, complete_formula, apply_formula_completion,
    FormulaCompletionError, FormulaSlot,
};

fn main() -> Result<(), FormulaCompletionError> {
    let source = r"\fr";
    let revision = 42; // The host owns the editing revision.
    let result = complete_formula(source, source.len(), revision)?;
    let selected = apply_formula_completion(
        source, source.len(), revision, &result, "builtin:frac",
    )?;
    assert_eq!(selected.source, r"\frac{}{}");
    assert_eq!(selected.cursor, 6);
    assert_eq!(selected.tab_stops.len(), 2);

    let partial = r"\frac{a}{\sqrt[";
    let context = analyze_formula_context(partial, partial.len(), revision)?;
    // The cursor belongs to the radical index inside the fraction denominator.
    assert_eq!(context.slot, FormulaSlot::RadicalIndex);
    Ok(())
}
```

这三种操作只返回数据，不读写文件、保存偏好、联网或修改宿主文档。
宿主显式接受候选后，将返回的源码作为一次可撤销编辑，并更新自身编辑版本。

## 结构与范围

源码和光标位置使用 UTF-8 字节；范围为 `[start, end)`，空槽位使用零长度范围。
JavaScript 编辑器若使用 UTF-16，须在绑定层转换位置，不能直接传递字符串字符数。
上下文带原输入 SHA-256、调用方版本、光标、父节点路径和编辑结构树；原输入不做归一化。
`schemaVersion` 当前为 1；Rust 的 `revision` 为 `u64`，JSON 一律使用规范十进制字符串，
包括请求、快照及结果上下文；拒绝数值、前导零、符号、空白、指数与溢出。
这是首次补全传输协议；未发布的 Rust serde 上下文数值版本改为字符串，不影响 Rust 函数签名。

树保留命令、参数、组、上/下标、注释、环境、单元格和分隔符的源码范围。
上下标记录其基底节点；分式上下文区分分子/分母，根式区分指数/被开方项。
支持有限 `frac/dfrac/tfrac/cfrac`、`binom/dbinom/tbinom`、`sqrt` 及常见字体/标注参数。
环境正文和结束名称均有所有者；矩阵/cases/aligned/array 的行列编号从 0 开始，
嵌套环境有自己的编号。转义 `\&`、参数内的 `&` 和内层行分隔不会改变外层列号。
array 列规格、cfrac 对齐设置及未闭合参数有独立上下文，不被猜作正文。

未知命令保留为命令/组源码，不展开宏，也不推测其参数声明。
注释、普通 text 参数及有限单行 `verb/verb*` 内不提供数学候选。
当前不是完整 TeX、化学式、TikZ、Typst 或 UnicodeMath 的编辑语法。
普通定界符配对、更多内置命令签名仍需继续补齐；自定义签名见下方独立作用域接口。

`closed` 及结构诊断只说明有限编辑结构是否闭合，不代表公式数学正确、转换支持或排版一致。
空占位符是编辑入口；宿主在渲染/转换/写入前仍使用对应的 Core 验证与执行路径。

## 候选与应用

Core 输出固定顺序的有界命令/结构模板、环境模板、空槽位建议、矩阵分隔符及局部闭合编辑。
候选包含稳定 ID、内置来源、当前槽位、替换范围、插入内容及结构遍历顺序的占位符。
已有花括号参数或 sqrt 可选指数时，仅补命令，不重复插入参数。
命令中间的补全替换完整命令 token，保留参数和后继正文。
未完成的 begin 名称可生成完整环境；未完成的 end 名称仅闭合当前环境。
现有已闭合环境的重命名不由这一局部编辑接口自动处理。

接受候选时再次校验当前源码摘要、编辑版本和光标，过期结果返回 `STALE_CONTEXT`。
Core 重新生成被选 ID 的编辑和占位符，返回载荷里的替换范围/插入文本不能扩大这次编辑。
返回的占位符为新源码的绝对范围，供宿主选择、Tab 跳转和一次撤销。
v1 没有自定义定义注册；v2 显式传入有界签名，不保存注册表。
历史检索、持久反馈、学习排序和数学续写仍未提供。

输入最多 64 KiB、编辑树最多 4096 节点、递归深度低于 64，候选最多 128 个。
字符/空白也计入节点预算，超限分析返回 `LIMIT_EXCEEDED`，不返回半棵树。
若某候选插入后超预算，该候选不列出，其余可用候选仍可返回。
构造候选需要分析拟议源码；真实编辑延迟与 IME/宿主交互仍须独立评估。

## 自定义命令签名与作用域（Rust / JSONL）

`formula_completion::scoped` 是新增的 v2 编辑协议，不替换 v1。
`complete_formula_scoped(source, cursor, revision, &catalog)` 返回
`ScopedFormulaCompletion { snapshot, context, candidates }`；候选为
`{ edit, definition }`，自定义项附定义 ID/精确版本，内置项 `definition` 为 null。
`apply_formula_candidate_scoped(..., &catalog, &snapshot, candidate_id)` 接受时重建规范编辑，
不信任客户端的 `insert/replace/tabStops`。对应的两个共享请求类型也有 `execute()`。

宿主每次传入 `FormulaDefinitionCatalog { scope, definitions }`：

- scope 的 `userSha256`、可选 `projectSha256/documentSha256` 是 64 个小写十六进制字符的不透明身份，
  不是用户名、路径、权限凭据或匿名化保证。宿主负责身份产生和定义检索授权。
- `generation` 为宿主维护的重置代数；清空/重置时推进。Core 无持久状态，不能自行发现未上报的重置。
- 每个定义包含 `id/revision/userSha256/projectSha256/command/arguments`。
  ID 为 1–64 字节 ASCII 字母/数字/`-_.`，命令为 1–64 字节 ASCII 字母，不带反斜杠。
  `mysymbol2` 不是单个 TeX 控制词，明确拒绝，不静默改名。
- 签名最多四个必需参数，逐个标记 `math` 或 `text`；空参数列表表示零参数自定义符号。
  参数具有结构所有权，text 内不生成数学候选；无可选参数、宏体、展开或任意片段执行。
- 最多 64 个输入定义（包括异作用域项）。只保留同用户的用户级项和当前项目项；
  其他项在检查私有名称/签名前被过滤，不进入候选、诊断或有效定义摘要。
  有效项的重复 ID/命令、用户级与项目级冲突及已支持内置命令覆盖明确拒绝，不猜测优先级。

查询的 `snapshot` 绑定当前源码 SHA-256、编辑版本、光标及有效定义/作用域/重置代数摘要。
摘要也绑定签名实际内容，即使忘记递增定义版本也会使旧候选失效；输入定义顺序不影响摘要。
切换文档或项目、定义变更、清空/重置后，旧候选返回 `STALE_CONTEXT`。
context 和 snapshot 的 `schemaVersion` 都为 2，不能经旧 v1 接受接口降级绕过复验。
这些是迟到结果保护，不是防止恶意宿主伪造当前状态的认证机制。

JSONL 新增 `formula.complete.scoped` / `formula.completion.apply.scoped`，传输协议仍为 1。
查询 params 使用 `source/inputFormat/cursor/revision/catalog`；接受再加返回的
`snapshot` 与 `candidateId`。所有编辑版本、定义版本和重置代数都使用规范十进制 u64 字符串。
查询 data 直接为 `snapshot/context/candidates`，不要按 v1 的 `completion` 嵌套形状读取。
`formula.capabilities.data.scopedCompletion` 提供版本、动作、签名预算和无持久化/宏展开声明。
原 v1 动作及其能力字段保持不变；源/树/候选及 JSONL 请求/结果预算仍适用。
无效声明返回 `INVALID_CATALOG`，未知 JSON 字段仍为 `INVALID_PARAMS`，超预算明确失败。

Core 不读取符号资产、保存偏好或上传公式；该声明不授予渲染/转换/插件执行权限。
敏感文档的学习开关、偏好清空、私有片段检索和其他绑定接入仍是后续工作，不能视为已完成。

## 独立进程调用

`formula.complete` 与 `formula.completion.apply` 是 JSONL v1 的新增动作，
无需模型、识别会话或文件访问。查询使用共享 `FormulaCompletionRequest`，返回
`FormulaCompletionReply { snapshot, completion }`；接受候选使用共享
`FormulaCompletionApplyRequest`。Rust 也可直接调用这两个请求的 `execute()`。

```json
{"version":1,"id":"suggest","action":"formula.complete","params":{"source":"\\fr","inputFormat":"latex","cursor":3,"revision":"9007199254740993"}}
```

接受时发送当前 `source/inputFormat/cursor/revision`，回显查询返回的简明 `snapshot`，
并指定 `candidateId`，例如 `builtin:frac`。不发送整棵树或编辑文本；未知请求/快照字段拒绝。
当前 `inputFormat` 仅接受精确 `latex`，转换能力矩阵中的其他输入不代表可补全。
`apply_formula_candidate` 也可直接接受 `context.snapshot()`；返回新源码、光标和绝对占位符，
不替宿主写入文档或递增版本。

快照用于检测迟到结果，不是签名或授权凭据。宿主仍须读取真正的当前编辑状态，
显式接受并实现一次撤销；不能用旧状态冒充当前状态，也不能把补全结构闭合当排版验证。
输入/树预算同上，JSONL 补全序列化 `data` 最多 1 MiB，转换仍为 256 KiB，
请求行仍为 1 MiB。超限/无效光标/过期/未知候选明确失败，同一进程可以继续处理后续请求。
同步执行的硬取消、队列、节流和子进程生命周期由宿主负责，不在每次按键重启进程。

详见 [JSONL 动作与错误边界](../crates/worker/README.md#model-free-formula-completion)。

## C 公共绑定

`latexsnipper_session.h` 新增 `latexsnipper_formula_completion_capabilities()`、
`latexsnipper_formula_complete(request_json, request_json_len)` 和
`latexsnipper_formula_completion_apply(request_json, request_json_len)`。
查询/接受的请求与 JSONL `params` 一致，返回原 v3 envelope，`data` 形状也一致。
未知字段、无效 UTF-8 或版本编码使用既有 C `INVALID_JSON`；空指针、零长度或
请求超过 1 MiB 为 `INVALID_ARGUMENT`，其他补全错误代码同 JSONL。

每个返回指针恰好用 `latexsnipper_string_free` 释放一次，指针输入仍须由宿主
保证可读且在调用期间有效。函数同步且无句柄，不持有识别会话锁，
补全结果 `data` 单独限制为 1 MiB，不改变旧转换的 256 KiB 预算。
旧 `latexsnipper_formula_capabilities()` 数组和 session ABI v1 不变；先探测新增符号，
不能假设旧 DLL 已支持。`crates/ffi/examples/python_session.py` 提供零依赖 ctypes 示例；
它是 C 参考调用者，不能和下面的正式 PyO3 接口混用。

## Python 公共绑定

正式 wheel 新增 `complete_formula`、`apply_formula_completion` 与
`formula_completion_capabilities`；无需 Session/FormulaRegistry，复用相同 Core 请求及校验。
调用参数 `revision` 为 `0..2**64-1` 的 Python 整数，拒绝 bool/浮点/字符串；
返回的 snapshot/context 版本仍为规范十进制字符串，能力投影分别说明两种编码。
`cursor` 及范围仍为 UTF-8 字节；接受时直接传回 `result["snapshot"]` 字典与实际当前状态。

Core 计算与结果序列化释放 GIL；结果 JSON 预算为 1 MiB，快照 JSON 为 4 KiB。
未知参数由 Python 绑定拒绝，格式/版本/快照错误为 `INVALID_ARGUMENT`，
过期/无效光标/超限/未知候选保持 Core 错误，不重试或写宿主文档。
Windows 构建 wheel 的实际导入、并发调用及会话兼容已验证；其他平台包仍依各自 CI 验收，
不据本地结果宣称发布或全部平台完成。用法见 [Python 文档](../crates/python/README.md#structured-formula-completion)。

## WASM/JS 公共绑定

完整包与 conversion-only 包均提供 `complete_formula_v3(requestJson)`、
`apply_formula_completion_v3(requestJson)` 及 `formula_completion_capabilities_v3()`。
JSON 请求与 JSONL `params` 相同，版本只接受规范十进制字符串；返回 v3 envelope。
编辑树中的空所有者/槽位保持 JSON `null`，不改成 JS `undefined`。

官方 JS 导出完整的树/候选/快照类型、直接调用包装和 UTF-16/UTF-8 边界转换函数；
拒绝拆开多字节字符或代理对，不以替换字符掩盖无效源码。
`WasmWorkerClient.completeFormula/applyFormulaCompletion` 接有界串行队列、请求身份、
快照复制、AbortSignal、活动执行超时、Worker 重建和旧代响应丢弃。
排队取消只移除当前请求，活动取消/超时硬终止并重建，不重放活动编辑。

Rust 的请求和 `data` 均为 1 MiB 上限；JS 补全 envelope 单独为 1 MiB + 64 KiB，
旧转换预算不变。Core 失败保持 envelope，传输/验证/Worker 错误则拒绝 Promise；
缺少新导出的旧包明确拒绝，不回退主线程。
已用真实生成的完整/精简 Node 包及精简包的浏览器 Worker 验证，
不代替实际编辑器、移动 WebView 或所有平台包验收；详见 [JS 使用边界](../crates/wasm/js/README.md#structured-formula-completion)。

相关资料：[辅助路线图](formula-assistance-roadmap.md)、
[收尾清单](closing-issue-ledger.md)。
