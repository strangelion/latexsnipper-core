# Core 结构化公式补全

更新：2026-10-15。

当前提供有限 LaTeX 编辑结构、候选及应用接口，位于
`latexsnipper_conversion::formula_completion`。Rust 路线已有；C/Python/JSONL/WASM-JS
公共绑定与实际编辑器接入仍待完成，不能把 serde 数据类型当作已经发布的跨绑定协议。

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
`schemaVersion` 当前为 1；Rust 的 `revision` 为 `u64`，未来绑定须保留整数精度。

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

相关资料：[辅助路线图](formula-assistance-roadmap.md)、
[收尾清单](closing-issue-ledger.md)。
