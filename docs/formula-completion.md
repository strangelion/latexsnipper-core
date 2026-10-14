# Core 结构化公式补全

更新：2026-10-15。

当前提供有限 LaTeX 编辑结构、候选及应用接口，位于
`latexsnipper_conversion::formula_completion`。Rust、C 与 JSONL 路线已有；Python/WASM-JS
公共绑定与实际编辑器接入仍待完成。

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
普通定界符配对、更多命令签名及自定义定义接入仍需继续补齐。

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
目前没有自定义符号注册、历史检索、持久反馈、学习排序或数学续写；这些沿 C-10/SC-03 之后推进。

输入最多 64 KiB、编辑树最多 4096 节点、递归深度低于 64，候选最多 128 个。
字符/空白也计入节点预算，超限分析返回 `LIMIT_EXCEEDED`，不返回半棵树。
若某候选插入后超预算，该候选不列出，其余可用候选仍可返回。
构造候选需要分析拟议源码；真实编辑延迟与 IME/宿主交互仍须独立评估。

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
它是 C 参考调用者，不代表正式 PyO3 wheel 的补全接口已完成。

相关资料：[辅助路线图](formula-assistance-roadmap.md)、
[收尾清单](closing-issue-ledger.md)。
