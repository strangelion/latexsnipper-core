# WASM/JS 注册转换接口

更新：2026-10-12。复用 Core 的 `SemanticConversionRegistry`、候选校验和可用性 fallback，
新增导出不替代既有单公式接口。此处只支持应用自己信任的同步 JavaScript 适配器，
不是浏览器 WASI 沙箱，也不提供插件安装、权限授予或 Office 写入。

## 直接调用

`create_formula_registry_v1()` 创建实例级注册器；SDK 的 `FormulaRegistry` 管理关闭及释放。
`registerTrustedFormat` 接收闭合 v1 描述和同步验证器；验证器只有返回 `true` 才通过。
`registerTrustedBackend` 接收闭合 v1 后端描述、同步 readiness 和转换器；转换器接收
`RegisteredConversionRequest` 的 JSON 字符串，返回 `{"content":"...","losses":[]}` 的 JSON 字符串。

描述中的平台应为 `wasm32-unknown-unknown`。每个扩展默认禁用，须调用 `setEnabled`，
并在转换请求的 `backend` 中明确选择；`null` 只选择内置后端。目标候选仍交 Core 校验。
`convert(request, policy)` 返回 v3 包络；先检查 `ok`，再检查 `data.outcome.kind`，
只有 `converted` 的候选可继续进入宿主预览管线。`rejected` 报告不等于顶层传输异常。

请求、上下文摘要、有限路由及 fallback 规则沿用 [注册路由 v1](conversion-registry.md)。
转换执行一旦开始，即使抛异常、输出非法或 readiness 撤销也不自动尝试另一转换器。
调用方必须计算实际 Core build、定义、样式、字体与宿主依赖的上下文摘要；不能照抄 fixture 摘要。

直接同步调用不能强制中止无限循环，也没有 callback 峰值内存限制。JavaScript 适配器及验证器
可能访问自身宿主环境，Core 的返回后大小检查不构成权限沙箱。Promise/异步返回拒绝，
不等待、不管理 callback 自己启动的后台任务或 Promise rejection；应用须保证它们真正同步。

原始 wasm-bindgen 调用者须 `close()` 后 `free()`；SDK 的 `close()` 同时处理二者，
支持重复关闭及 callback 中关闭，候选会被丢弃。禁止在 callback 内直接 `free()` 正在执行的
原始实例。每个 JS agent 最多 64 个未释放实例；关闭但未释放的原始对象仍占实例名额。

## 官方 Worker 调用

`WasmWorkerClient.createFormulaRegistry(configurationKey?, options?)` 创建
`WorkerFormulaRegistry`，提供 `capabilities`、`setEnabled`、`convert`、`close`。
协议版本仍为 1；新增 `formula-registry-create`、`formula-registry-command` 为闭合消息。
旧包缺少工厂时明确返回 `WORKER_REGISTRY_UNAVAILABLE`，不退回主线程执行。

Core 自带工厂仅接受未指定的配置，创建内置注册器。扩展由应用构建时选择的可信同源 bridge
覆盖工厂并按有限 `configurationKey` 注册；key 不是模块 URL、文件路径或脚本。
每个转换请求不能指定代码、callback、权限或插件安装。信任 bridge 属于应用的独立决定，
不是 Core/WASI 用户审批许可，也不能用它加载未审查的第三方代码。

每个任务使用现有有界队列；默认活动执行限时 30 秒（不含排队），可通过调用 options
提供 `timeoutMillis` 与 `AbortSignal`。排队取消只移除该任务；活动取消、超时或 Worker 故障
终止 Worker，旧注册器及排队中的注册操作全部以 `WORKER_REGISTRY_STALE` 失效，不自动重放
注册、启用或转换。恢复后必须明确创建新实例；旧 wrapper 的 `closed` 只表示主动关闭，
不能作为 Worker generation 仍有效的证明。使用独立转换 client 可避免影响共享识别任务。
关闭未能入队或执行失败时允许再次请求关闭；并发关闭共享同一个终态，不会提前返回已释放的假象。

输入最多 64 KiB UTF-8，候选内容最多 256 KiB，描述最多 16 KiB；后端可以降低限额。
原始 candidate JSON 最多 512 KiB，序列化报告数据最多 2 MiB，Worker 包络另留 64 KiB。
这些是传输/返回预算，不是浏览器进程或第三方 callback 的峰值内存承诺。

## 复现与边界

Node 实际生成包验证：在 `crates/wasm/js` 执行
`node scripts/registry-package-smoke.mjs wasm-conversion-nodejs`。
浏览器：生成 `target/wasm-conversion-web` 后构建 SDK，在同一目录执行
`node scripts/registry-browser-server.mjs wasm-conversion-web`，打开打印的 loopback 地址。
服务只提供 allowlist 内的 SDK、WASM 与明确命名 fixture，不暴露整个仓库。

有限 power fixture 验证真实 Core 输出随输入变化、默认禁用、终态失败、关闭及释放。
真实浏览器 Worker 验证无限 callback 的强制取消、超时、旧会话失效和显式恢复；
无限循环只在测试 Worker 内执行。2026-10-12 本机 Chromium 回归通过，控制台及网络检查无错误。
CI 执行 Node 包级验证；这不代表 CI 已运行此浏览器 fixture，也不代表 WebKit、移动设备或
Office 宿主验收。浏览器不可信 WASI/component loader、审批 UI、持久策略及二进制/文档扩展仍未接通。
