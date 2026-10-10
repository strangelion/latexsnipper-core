# 应用适配与兼容边界

本文描述已交付接口及使用约束，不保留开发阶段任务编号、执行过程或测试流水。
未完成的 Core 适配工作统一保留在 [收尾清单 C-08](closing-issue-ledger.md)。

## 接口选择

| 调用环境 | 现有接口 | 生命周期与边界 |
| --- | --- | --- |
| Rust 原生应用 | `RecognitionSession`、`DocumentConverter` | 会话复用引擎、运行时及模型；模型无关转换不要求创建识别会话 |
| C 原生应用 | Generic C session ABI | 进程内 opaque handle 与独立资源；遵守返回缓冲区释放约定 |
| Python | `Session` 与转换绑定 | Session 独立持有资源；识别 Core 调用释放 GIL；不依赖全局 singleton |
| 原生独立进程 | `latexsnipper-worker` JSONL v1 | 长驻、串行、有界会话；识别使用路径，公式转换使用声明格式的字符串 |
| 浏览器/支持 WASM 的 WebView | WASM v3 与官方 JS Worker | 公式转换可使用 conversion-only 构建；不依赖原生子进程 |
| CLI | 单次命令内会话 | 多次启动不自动共享引擎，不等于 daemon |
| Android / iOS 专用桥接 | 旧平台桥接 | 尚未接入 `RecognitionSession`；Android 的历史 `Java_*` 导出是原始 C 签名，不是完整 JNI；StubRuntime 不是实际识别后端 |

## 公式转换

以共享能力矩阵的 `available`、`mode`、目标平台和限制为准。
格式枚举、接口名称或存在一个试点 parser 均不代表该路线已开放。

- Rust/C/Python/WASM/JS 和原生 JSONL 提供显式裸公式入口或投影；
  `latex-fragment` 通过 `latex_display` 单公式形状检查获得裸 LaTeX。
- 旧 `latex` 目标仍保留完整文档输出，不能直接塞进数学编辑器或 `$...$`。
- 单公式 `markdown_inline` 使用行内包装；完整 Markdown 文档保留自己的
  文档/公式语义，不能只看目标名称决定去除所有块包装。
- 默认 strict；有限重建路线如 Typst 需要明确 best-effort。
  不支持时返回失败，不自动降级、选择扩展或重复调用。
- 裸公式检查不保证任意宏、MathLive/TeX 支持、视觉保真或宿主可编辑性。
  转换成功也不等于创建了 Windows OLE 对象。

## Worker 与资源约束

JSONL 的 `formula.capabilities` 和 `formula.convert` 复用原生 Core 门禁，
不创建、加载或清空识别会话；普通转换错误后可处理下一条请求。
输入最多 64 KiB UTF-8，序列化结果 `data` 最多 256 KiB；
协议外壳、回显 ID 和错误信息不包含在结果数据预算内。
Generic C 采用同样的 256 KiB 序列化 `data` 预算；Python 返回纯字符串，
其 256 KiB 预算按该字符串的 UTF-8 JSON 序列化大小计算，而非进程内字符数。
原生转换同步执行，不接受伪装成硬取消的 timeout 参数。
进程监督者负责路径授权、OS 隔离、强制终止、重启和过期响应丢弃。

官方 JS `WasmWorkerClient.convertFormula` 提供有限队列、输入与结果预算、
AbortSignal、活动执行超时和 Worker 重建。默认 64 KiB 输入、256 KiB
序列化 envelope、30 秒活动执行；排队时间不属于活动执行超时。
与 JSONL 不同，其结果预算包含 Core envelope，调用者不能混淆两者。

`conversion-only` 必须配合 `--no-default-features` 构建；Cargo features
相加，全功能构建并非精简包。精简包不包含识别运行时/模型能力；
收到识别请求时明确拒绝，不退回主线程。
JS runtime 当前是 private 包，正式分发必须固定匹配的胶水、WASM、协议和版本。
旧 C/Python 库即使已有转换方法，也可能拒绝新增裸公式标签；必须明确处理
不支持状态，不使用隐式文档剥离或转换降级来模拟新接口。
自定义宿主需要单独验证资源 URL、CSP、打包、卸载及移动端 WebView；
普通浏览器通过不能作为所有宿主的兼容保证。

## 持久化与恢复

会话持久化指进程内资源复用，不是序列化原生推理指针。
进程或 Worker 终止后必须显式恢复所需会话/模型；只允许复用已校验的
模型文件、合格 provider 记录和身份匹配的缓存。
关闭或失效的句柄不能继续工作，独立会话的失败不能污染其他会话。
宿主必须在写入文档前重新确认文档、范围和输入身份；取消、超时、崩溃恢复
不能隐式重放修改操作。真实模型准确性和真实宿主写回是独立验收范围。

## 接口资料

- [原生应用 API](application-integration.md)
- [JSONL 动作及错误](../crates/worker/README.md)
- [C session ABI](../crates/ffi/include/latexsnipper_session.h)
- [Python 绑定](../crates/python)
- [JS runtime](../crates/wasm/js/README.md)
- [精简 WASM 边界与测量](formats/conversion-only-wasm.md)
- [浏览器 Worker 验证范围](formats/browser-formula-worker.md)
- [总清单](closing-issue-ledger.md)
