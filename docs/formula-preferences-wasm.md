# WASM / JS 内存态补全偏好

完整和 `--no-default-features --features conversion-only` 包均提供相同的默认关闭偏好对象。
这是已有正确候选的频率/最近使用排序，不是公式生成模型、自动历史采集或持久学习。
可信宿主负责授权、随机 key、真正的接受/撤销事务及私有目录；不要把对象或令牌交给未授权插件。

## 原始 WASM 与直接 JS

- `formula_preference_capabilities_v3()` 返回 v3 envelope，列出 schema 1、容量及编码。
- `create_formula_preferences_v1(configuration_json)` 返回拥有状态的 `FormulaPreferencesWasm`。
  配置为闭合 JSON `{schemaVersion:1,scope,keyHex}`；`scope` 与片段目录作用域完全相同。
  `keyHex` 是宿主安全生成的 32 字节非零 key，编码为 64 位小写十六进制；不要记录到日志。
  原始工厂失败时抛出失败 envelope，不返回可用对象。
- 对象的 `command(command_json)` 返回 v3 envelope；`close()` 幂等关闭并释放状态，
  最终仍须 `free()` 释放 wasm-bindgen 对象。关闭后的原始命令返回 `PREFERENCE_CLOSED`。
- 创建对象还要求当前 JS 环境提供 `crypto.getRandomValues`，为内部反馈身份生成独立随机命名空间。
  没有安全随机源时明确拒绝创建，不退回时间戳或 `Math.random`；普通转换/补全不受影响。

TypeScript 包提供 `FormulaPreferences`，管理 `close/free`，同时导出配置、策略、命令和结果类型：

```ts
import { FormulaPreferences } from "@latexsnipper/wasm-runtime";

function createPreferences(api, scope) {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  const keyHex = Array.from(bytes, b => b.toString(16).padStart(2, "0")).join("");
  return new FormulaPreferences(api, { schemaVersion: 1, scope, keyHex });
}
```

先初始化对应 WASM 包，再传入其 exports。包装对象是同步 API，不能硬中断执行；
编辑器实时调用建议使用下面的 Worker 路线。`close()` 后禁止调用，重复关闭不重复释放。
包装器保护准备参数时的递归调用和 getter/toJSON 导致的关闭，避免已释放的 WASM 对象被再次调用。

## 命令

所有命令及嵌套结构拒绝未知/重复字段，原始 JSON 不先转换为会丢失重复键的 `Value`。
`eventId`、`generation`、所有目录版本和快照版本都是 canonical decimal u64 **字符串**，不能传 JS number。

| kind | 额外字段 | data |
|---|---|---|
| `info` | 无 | scope、policy、eventCount、captureCount、receiptCount 及内存态标记，不含 key |
| `set-policy` | `policy` 三个必填布尔值：rankingEnabled / feedbackEnabled / allowSensitiveFeedback | `{policy}` |
| `capture` | 无 | `{captureId}` |
| `discard-capture` | `captureId` | `{discarded:true}` |
| `rank-fragments` | `request` 为显式片段查询 | `{completion,order,features}` |
| `record-acceptance` | `captureId,eventId,request`，request 为接受前的完整片段 apply 请求 | `{receiptId:string\|null}` |
| `undo` | `receiptId` | `{undone:true}` |
| `clear` | 严格递增的 `generation`、不同非零 `keyHex` | 更新后的 info |

`order` 仅为原始候选索引排列，`features` 与原始候选对齐，包含 accepted、undone、lastAcceptedEvent。
不修改候选源码、增加候选或写入文档。禁用排序/敏感学习未授权时保持原始顺序和零特征。

正确的反馈顺序：派发接受任务**之前** capture → Core apply 复验与宿主实际编辑 →
仅在真实接受后 record-acceptance；未接受时 discard-capture。
真正撤销成功后再 undo。不得因反馈失败回滚成功编辑，也不得循环重试或临时重建 capture 掩盖过期。
禁用反馈时成功返回 null 回执并消费 capture，但不增加事件。
操作校验失败不消费 capture，宿主可明确丢弃；成功接受或撤销只可消费一次。
策略实际变化、清空、关闭及换对象会废弃相关旧捕获/回执；重复设置相同策略不会废弃。
清空后还须更新目录 generation、重新查询快照；清空本身不隐式关闭用户已同意的策略。

同一 WASM 模块最多 64 个未关闭对象，每对象 128 个待处理 capture、1024 条事件（含已撤销事件）。
关闭立即释放对象配额，free 也会清理尚未关闭的对象。计数及令牌不回绕，不静默逐出旧回执。
配置和元数据命令最多 4 KiB，查询/接受命令最多 1 MiB UTF-8 JSON，结果 data 最多 1 MiB；
超过限额返回明确失败，不改变普通补全/转换的原有限额。

## 浏览器 Worker

`WasmWorkerClient.createFormulaPreferences(configuration, options?)` 返回 `WorkerFormulaPreferences`。
后者提供 `info/setPolicy/captureContext/rankFragments/command/close`，均为异步调用，
接受现有 timeoutMillis 和 AbortSignal；其他命令用类型化 `command` 发起。
由加载中的同源、应用控制的 WASM 工厂创建，不接受命令指定代码 URL。
没有对应导出的旧包返回 `WORKER_PREFERENCES_UNAVAILABLE`，不降级到伪成功。

Worker protocol 1 的附加消息为 `formula-preferences-create {configuration}` 和
`formula-preferences-command {ownerId,command}`，关闭为后者的 `{kind:"close"}`。
Worker 中的拥有对象与内部捕获/回执令牌都不可跨实例复用；包装对象额外绑定 client generation。
所有令牌只是身份，不是同意、认证或插件执行权限。

- 共用既有有界执行队列，排队时深复制输入，不受后续目录/策略修改影响。
- 预先或排队取消不终止当前工作；活动任务取消/超时会硬终止 Worker。
- 重启拒绝排队的偏好任务，旧对象返回 `WORKER_PREFERENCES_STALE`；
  **不重建偏好对象、不重放策略、捕获、接受或撤销**。新建对象默认关闭且为空。
- 若返回的偏好响应超过宿主结果限额，丢弃整个 Worker 的实例状态，避免遗留未知 capture/receipt；
  错误标明 workerRestarted，不自动重试。创建句柄响应使用独立 4 KiB 上限，避免小查询限额吞掉有效句柄。
- 成功 close 后释放对象；排队拒绝或 Core 拒绝时允许宿主显式重试 close。
  同时发起的 close 共用同一 Promise，关闭中拒绝新命令。
- client.terminate() 丢弃整个 Worker；原始 Worker 再初始化时显式 close/free 已有对象。

对象、捕获和回执均无持久化导出/导入，不能恢复到新 Worker 或 WASM 模块。
跨进程/页面重启恢复、磁盘存储、层级偏好合并、可替换学习模型和排序效果基准仍是后续工作。

## 复验入口

`npm test` 覆盖协议及队列生命周期；`preference-package-smoke.mjs <target-package-directory>`
对真实 Node 包验证边界，已接入 WASM CI 的完整/精简包检查。
`formula-worker-server.mjs` 的显式文件白名单提供
`preference-worker-browser-smoke.html?profile=conversion` / `profile=full`，
验证真实浏览器中的关闭、重置、隔离、敏感策略、硬取消及接受超时不重放。
本地回归不替代远端 CI、宿主授权或真实编辑器接入验收。
