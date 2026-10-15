# Core 结构化公式补全

更新：2026-10-15。

当前提供有限 LaTeX 编辑结构、候选及应用接口，位于
`latexsnipper_conversion::formula_completion`。Rust、C、Python、JSONL 与 WASM/JS
公共绑定均有 v1 及独立的定义驱动签名/作用域 v2 接口；
Rust/C/Python/JSONL/WASM-JS 另有独立的私有片段编辑 schema 3；
Rust 另有显式内存态偏好/重置与频率排序基础，偏好公共绑定、持久化及实际编辑器接入仍待完成。

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
有限显示定界符、常用内置签名及有限自定义可选参数已有；更广语法仍需补齐。自定义签名见下方独立作用域接口。

常用重音、上下箭头标注、数学字体、`boxed/ensuremath/phantom/hphantom/vphantom` 使用数学参数，
`stackrel` 有两个数学参数；`operatorname*` 保留星号，名称参数仍按数学内容分析。
`textnormal/textmd/textsc/textsl/textup/mbox/fbox` 与既有 text 家族使用文本上下文，不提供数学候选。
这些有限签名只描述编辑参数归属，不加载宏包，也不承诺对应命令已支持渲染或所有转换目标。

圆/方括号、转义花括号、`langle/rangle`、`lceil/rceil`、`lfloor/rfloor`、`lvert/rvert`、
`lVert/rVert` 及有限 Unicode 对应字形使用现有
`Group` 节点，`range` 包含定界符，`content` 仅含正文；不新增节点类型或改变编辑 schema 1/2。
圆/方括号可混合端点表示区间；裸 `|`、`<`、`>` 不被猜作配对。
`\left...\right` 可嵌套，允许有限异形端点、`.` 不可见端点及直接 `\middle` 标记。
未完成的 `\right` 头部仍归外层 Group，正文 `content` 不必包含该头部光标；宿主应使用 `ownerPath`。

`bigl/bigr/bigm`、`Bigl/Bigr/Bigm`、`biggl/biggr/biggm`、`Biggl/Biggr/Biggm` 同样有有限显示归属。
固定尺寸的左右端可刻意混用不同高度，分析和补全不重写已有尺寸；不与动态 `left/right` 交叉配对。
中性的 `big/Big/bigg/Bigg` 和独立固定尺寸 middle 标记仅消费一个已知定界符，不臆造配对组。
这类显示归属不是 TeX 的词法参数组。

显示定界符不把未加花括号的 TeX 单 token 参数变成整段参数；矩阵 `&`、行分隔、环境结束和
外层词法花括号/可选参数仍保留各自边界。注释、text、verb、array 规格和 cfrac 对齐内不建立数学围栏。
缺少/孤立定界符有结构诊断；不展开私有定界符宏，也不据这些诊断断言完整 TeX 的有效性。

可选方括号参数（sqrt 指数、cfrac 对齐和已声明的可选参数）以首个未被花括号保护的裸 `]` 结束，
注释与转义 token 不会误截断；裸方括号自身不提供词法嵌套保护。数学显示围栏、verb 或私有宏
不能越过这个参数边界。缺失 `]` 时保留未闭合槽位，不补造后续必需参数；外层裸 `}` 不被吞掉。

`closed` 及结构诊断只说明有限编辑结构是否闭合，不代表公式数学正确、转换支持或排版一致。
空占位符是编辑入口；宿主在渲染/转换/写入前仍使用对应的 Core 验证与执行路径。

## 候选与应用

Core 输出固定顺序的有界命令/结构模板、环境模板、空槽位建议、矩阵分隔符及局部闭合编辑。
候选包含稳定 ID、内置来源、当前槽位、替换范围、插入内容及结构遍历顺序的占位符。
已有花括号参数或 sqrt 可选指数时，仅补命令，不重复插入参数。
`operatorname-star` 提供星号模板；已有星号、参数及其间注释/空白均保留，不复制星号或参数。
命令中间的补全替换完整命令 token，保留参数和后继正文。
未完成的 begin 名称可生成完整环境；未完成的 end 名称仅闭合当前环境。
现有已闭合环境的重命名不由这一局部编辑接口自动处理。

`langle/lceil/lfloor/lvert/lVert` 和固定尺寸 left 家族提供成对模板和正文占位符；已有后继源码时只补开命令，不复制结尾。
`structure:close-delimiter` 在未闭合围栏末尾插入已知结束标记，未完的右命令前缀可用
`structure:close-delimiter-prefix` 替换；不跨未完成的必需参数或内层花括号闭合外层。
这些是显式候选，不自动改源码；旧 `structure:close-group` 和环境候选 ID 保留。
固定尺寸的缺失右端默认沿用开端高度；已经写入兼容的右端命令时只补字形，不覆盖用户选择的高度。

接受候选时再次校验当前源码摘要、编辑版本和光标，过期结果返回 `STALE_CONTEXT`。
Core 重新生成被选 ID 的编辑和占位符，返回载荷里的替换范围/插入文本不能扩大这次编辑。
返回的占位符为新源码的绝对范围，供宿主选择、Tab 跳转和一次撤销。
v1 没有自定义定义注册；v2 显式传入有界签名，不保存注册表。
历史检索、持久反馈、模型排序和数学续写仍未提供；独立 Rust 偏好基础见 [偏好边界](formula-completion-preferences.md)。

输入最多 64 KiB、编辑树最多 4096 节点、递归深度低于 64，候选最多 128 个。
字符/空白也计入节点预算，超限分析返回 `LIMIT_EXCEEDED`，不返回半棵树。
若某候选插入后超预算，该候选不列出，其余可用候选仍可返回。
构造候选需要分析拟议源码；真实编辑延迟与 IME/宿主交互仍须独立评估。

## 自定义命令签名与作用域

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
- 签名最多四个参数，必需参数标记 `math` 或 `text`；允许一个首位 `optional-math` 或 `optional-text`。
  可选参数后只能跟必需参数，不能重复或位于必需参数之后；也允许仅一个可选参数。空列表表示零参数自定义符号。Core 不保存或展开默认值。
  参数具有结构所有权，text 内不生成数学候选；无可选参数、宏体、展开或任意片段执行。
- 最多 64 个输入定义（包括异作用域项）。只保留同用户的用户级项和当前项目项；
  其他项在检查私有名称/签名前被过滤，不进入候选、诊断或有效定义摘要。
  有效项的重复 ID/命令、用户级与项目级冲突及已支持内置命令覆盖明确拒绝，不猜测优先级。

查询的 `snapshot` 绑定当前源码 SHA-256、编辑版本、光标及有效定义/作用域/重置代数摘要。
摘要也绑定签名实际内容，即使忘记递增定义版本也会使旧候选失效；输入定义顺序不影响摘要。
内置命令目录扩展后，旧目录若重复声明这些名称会被明确拒绝；宿主应移除重复声明，不将内置名称静默覆盖为私有签名。

可选参数是 schema 2 的增量输入能力；宿主发送前检查对应能力的 `argumentKinds` 和
`maxLeadingOptionalArguments`，旧构建缺少这两个字段时不发送可选种类。既有必需签名和候选 ID 不变。
省略可选参数的 `definition:<id>` 模板仅填必需参数；`definition:<id>:optional` 显式插入 `[]` 和
必需参数，返回各自占位符。已有 `[` 或 `{` 参数列表时仅补命令，不重复插入列表；定义摘要也绑定可选种类。
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

此签名接口不读取符号资产、保存偏好或上传公式；该声明不授予渲染/转换/插件执行权限。
独立 Rust 偏好模块已有显式开关与内存态清空；偏好公共绑定、持久化及私有存储检索宿主仍是后续工作；显式传入片段的边界见下节。

## 显式私有片段

`formula_completion::fragments` 的 `complete_formula_fragments` / `apply_formula_candidate_fragments`
使用独立编辑 schema 3；JSONL 协议仍为 1，动作是 `formula.complete.fragments` 与
`formula.completion.apply.fragments`。C、正式 Python 与 WASM-JS 的独立接口见下文；片段接受不能通过 v1/v2 降级。

查询仍带 source/inputFormat/cursor/revision，catalog 为：

```json
{
  "commands": {
    "scope": {"userSha256": "<64 lowercase hex>", "projectSha256": null, "documentSha256": null, "generation": "0"},
    "definitions": []
  },
  "policy": {"retrievalEnabled": true, "sensitiveDocument": false, "allowInSensitiveDocument": false},
  "fragments": [{"id": "saved-integral", "revision": "0", "userSha256": "<same user hash>",
    "projectSha256": null, "documentSha256": null, "trigger": "myintegral", "label": "Saved integral",
    "source": "\\int_{}^{} {}\\,dx"}]
}
```

示意哈希必须换成真实的宿主身份摘要。commands 复用 v2 的定义目录；片段最多 64 条、每条源码最多 8 KiB，
ID 为 1–64 字节 ASCII 字母/数字/`-_.`，trigger 为 1–64 个 ASCII 字母（不含反斜线），
label 为不含控制字符的单行非空文字，最多 128 字节。源码保留原字节，不能含非空白控制字符，须通过裸公式形状及有限编辑结构检查。
文档/包载入声明、独立显示环境和 math-mode 外壳会拒绝（如 `align` 应由宿主显式准备成 `aligned`，不在此自动重写）；
末尾注释必须已有换行，不能吞掉插入点后的正文。注释及合法空白原样保留。
这不证明数学正确、完整 TeX 合法或可安全执行；片段中任何命令均不展开，渲染、转换与文档写入仍走各自门禁。

只在控制字前缀匹配 trigger 时给出片段候选；注释、文本、环境名称及 array/cfrac 头部不检索。
片段按 ID 排序，置于定义/内置候选之前，共享 128 项总上限。`fragment:<id>` 的 edit.source 为
`core:fragment`，fragment 提供 ID/精确版本，definition 为 null；定义和内置项的 fragment 为 null。
空的类型化槽位产生占位符，接受只替换当前完整命令 token，不删除周围正文，也不会自动写入宿主。

禁用检索，或敏感文档没有显式允许时，片段在私有名称、源码和标签校验/摘要之前全部过滤；
非当前用户、非当前项目、非当前文档片段亦先过滤。null 项目/文档表示当前用户授权的更广范围，
不表示可读取其他用户内容。有效 ID 冲突明确拒绝，不猜测覆盖顺序；错误不包含私有片段值。

快照的 catalogSha256 绑定有效片段的实际内容/版本、commands 摘要及所有策略字段；源码、光标和编辑版本也复验。
目录顺序和外来片段不影响摘要，关闭检索、改变敏感文档策略、移除片段或更改内容后，旧接受均失败。
接受请求必须使用宿主实际的当前目录和策略，不得原样回送旧请求作为授权证明；清空/重置仍须递增 scope.generation，防止旧状态恢复后重用快照。

此片段接口不读取数据库、目录或私人历史，不保存公式、偏好或学习反馈，不联网。宿主先取得检索授权再提供目录；
这些布尔策略和哈希只供确定性隔离与复验，不能替代用户同意或授予插件权限。
本片段接口不自动记录偏好；Rust 的独立内存态反馈/排序基础见 [偏好边界](formula-completion-preferences.md)，偏好公共绑定、学习和实际编辑器接入仍独立待办。
存储/检索失败或有效目录不合法时，宿主仍可独立查询 v1/v2 的确定性候选；接受必须使用产生该候选的原协议和当前快照，
不得将失败的片段接受降级为其他协议或自动写入。裸源码形状检查也可单独使用 `formula_fragment::validate_latex_fragment_source`，不会替调用方剥壳或改写输入。

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

v2 作用域另增 `latexsnipper_formula_scoped_completion_capabilities()`、
`latexsnipper_formula_complete_scoped(...)` 和 `latexsnipper_formula_completion_apply_scoped(...)`。
请求/data 复用上文 scoped JSONL 形状，旧 v1 能力响应不追加或改写字段。
有效声明错误为 `INVALID_CATALOG`，闭合 JSON 格式错误仍为 `INVALID_JSON`；
长度/所有权/序列化预算及无会话生命周期与 v1 相同。ctypes 示例支持这三个新符号；
已用实际 Windows DLL 验证查询、接受、迟到拒绝、私有定义过滤和反复释放，其他平台依 CI 验收。

v3 私有片段另有 `latexsnipper_formula_fragment_completion_capabilities()`、
`latexsnipper_formula_complete_fragments(...)` 与 `latexsnipper_formula_completion_apply_fragments(...)`，
复用上文片段 JSONL 的 catalog、策略、快照和候选形状。能力响应同时给出请求/结果、定义和片段预算；
原 v1/v2 能力数据不变。调用同步、无句柄，既不打开模型/私人存储，也不执行返回的 LaTeX。
三个符号均须探测；所有成功或失败响应仍使用同一个 `latexsnipper_string_free` 恰好释放一次。

ctypes 参考调用者已支持片段；这不是正式 PyO3 绑定。实际 Windows DLL 和 MSVC C11/C++17
消费程序已验证公开头文件、查询/接受及旧接口兼容；Linux CI 已接同一消费程序，未用本地结果代替其他平台验收。
运行 `python crates/ffi/examples/c_header_smoke.py --library-dir target/debug`（Windows 须在 x64 VS 开发环境中），
会在独立临时目录编译并调用实际库，退出后清理自己创建的编译产物，不删除构建库或项目文件。

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

正式 wheel 另提供 `complete_formula_scoped`、`apply_formula_completion_scoped` 与
`formula_scoped_completion_capabilities`；查询/接受都传当前 `catalog` 字典，
结果形状沿用 scoped JSONL。编辑版本仍为 Python 整数，定义版本和重置代数仍为字典中的规范十进制字符串。
目录 JSON 单独限制 64 KiB，快照 4 KiB，Core 计算/结果序列化释放 GIL；无持久注册表。
schema/编码错误为 `INVALID_ARGUMENT`，声明语义错误为 `INVALID_CATALOG`，其余保持 Core 错误。
Windows wheel 的实际导入、隔离并发和会话兼容已有验证；不替代其他平台或真实编辑器验收。

正式 wheel 另有 `complete_formula_fragments`、`apply_formula_completion_fragments` 与
`formula_fragment_completion_capabilities`，使用编辑 schema 3 和上文片段 catalog/策略/快照形状；
调用方式沿用 v2，catalog 和 snapshot 均为字典，编辑 revision 仍须为非 bool 的 Python u64 整数。
片段/定义版本与 generation 仍为字典中的规范十进制字符串；不存在向 v1/v2 的接受降级。

片段目录使用独立的 1 MiB UTF-8 JSON 预算，不把非 ASCII 字符膨胀成转义后的尺寸；旧 v2 的 64 KiB 目录限制不变。快照 4 KiB、返回 data 1 MiB，
片段 64 条/每条 8 KiB，其余源/树/候选预算不变。查询/接受的 Rust 解码、Core 计算及结果序列化释放 GIL；
Python 字典的 JSON 编码与结果构建仍持有 GIL。预算指序列化 JSON，不是 Python 对象或编码阶段峰值内存保证。
Windows wheel 已实际验证导出、敏感策略/作用域/重置复验、并发隔离、超限后恢复及旧会话兼容；其他平台和宿主仍独立验收。
这不是私有存储、学习器或权限界面：宿主仍先取得授权，再提供真实当前目录和策略。

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

完整/精简包另有 `complete_formula_scoped_v3`、`apply_formula_completion_scoped_v3` 和
`formula_scoped_completion_capabilities_v3`。JS 提供对应类型和同步助手；
`WasmWorkerClient.completeFormulaScoped/applyFormulaCompletionScoped` 复用现有队列及生命周期。
请求/data 形状沿用 scoped JSONL，主线程和 Worker 检查闭合传输结构；
Worker 包装另限目录 JSON 为 64 KiB，原始 WASM 请求仍为 1 MiB。
排队时深拷贝目录/快照，活动取消/超时重建不重放编辑，不以旧 v1 或主线程计算降级。
完整/精简 Node 包及真实精简浏览器 Worker 的作用域接受、过期拒绝、硬取消/超时和恢复已有验证。

完整/精简包另提供 `complete_formula_fragments_v3`、`apply_formula_completion_fragments_v3`
及 `formula_fragment_completion_capabilities_v3`，编辑 schema 为 3。
JS 提供相应类型和同步助手；交互宿主使用
`WasmWorkerClient.completeFormulaFragments/applyFormulaCompletionFragments`。
请求沿用上文显式片段目录；快照使用 `catalogSha256`，候选含独立可空的 definition/fragment 引用。
主线程与 Worker 验证闭合形状和规范版本，Core 过滤非活动私有内容后验证有效语义。
片段完整请求按实际 UTF-8 JSON 限 1 MiB，旧 scoped 目录的 64 KiB 限制不变；不宣称峰值内存受同一预算约束。
目录、策略和快照排队时深拷贝；接受时复验当前策略/内容/重置代数，失败不降级、不自动重放。
完整/精简 Node 实际包和真实精简浏览器 Worker 已验证隐私过滤、敏感文档显式放行、Unicode 编辑、
过期拒绝、结果上限、硬取消/超时及接受不重放和恢复；不代替实际编辑宿主/移动平台验收。
宿主仍先授权检索，再传真实当前目录；本接口不读取历史、不持久化、不学习，策略字段不是权限凭据。

相关资料：[辅助路线图](formula-assistance-roadmap.md)、
[收尾清单](closing-issue-ledger.md)。
