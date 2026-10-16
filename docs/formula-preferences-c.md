# C ABI 补全偏好对象

更新：2026-10-16。正式 C 传输复用 Rust 的默认关闭、有限偏好与显式认证检查点，
不自动采集输入、写磁盘、恢复授权或修改宿主文档，不提供模型。Windows 可选特性另有显式受保护存储，见下节。

## 入口与所有权

公开声明在 `crates/ffi/include/latexsnipper_session.h`：

- `latexsnipper_formula_preferences_capabilities()`：查询独立 schema 1 和实际限制。
- `latexsnipper_formula_preferences_create(json, length)`：创建默认关闭的独立对象。
- `latexsnipper_formula_preferences_restore(json, length)`：认证检查点并创建默认关闭的新对象。
- `latexsnipper_formula_preferences_open_windows(json, length)`：显式创建/打开 Windows 受保护存储对象。
- `latexsnipper_formula_preferences_command(handle, json, length)`：执行显式命令。
- `latexsnipper_formula_preferences_close(handle)`：关闭并释放对象。

所有返回值均为 Core v3 JSON envelope，由调用方以 `latexsnipper_string_free` 释放一次。
创建请求为 `{schemaVersion:1, scope, keyHex}`。`scope` 沿用编辑接口的完整用户/项目/文档/代数身份，
`keyHex` 为宿主私有随机 32 字节 key 的 64 位小写十六进制编码，不得输出到日志或插件可读配置。
返回 `data.handle` 为规范十进制 `u64` **字符串**；以无损整数解析后传给 C 的 `uint64_t` 参数，不能经过浮点数。
旧库可能没有新符号，须先做能力探测。对象与其标识只在当前加载的库内有效；卸载或进程重启后全部丢弃。

## 闭合命令

| kind | 其他字段 | 返回 data |
| --- | --- | --- |
| `info` | 无 | 策略、作用域、事件和未消费标识数量；不含 key、候选名称或事件内部数据 |
| `set-policy` | `policy` | 当前 policy |
| `capture` | 无 | `{captureId}` |
| `discard-capture` | `captureId` | `{discarded:true}` |
| `rank-fragments` | `request` | `{completion, order, features}` |
| `record-acceptance` | `captureId, eventId, request` | `{receiptId:string|null}` |
| `undo` | `receiptId` | `{undone:true}` |
| `clear` | `generation, keyHex` | 清空后的 info |
| `export-checkpoint` | `storageKeyHex, checkpoint, permission` | `{snapshotJson, checkpoint}` |

`policy` 必须显式给出 `rankingEnabled/feedbackEnabled/allowSensitiveFeedback` 三个布尔值；
默认全 false。`request` 分别沿用片段编辑 schema 3 的查询或接受请求，所有既有源码/快照/目录检查保留。
标识、`eventId`、`generation` 及嵌套版本只接受规范十进制 `u64` 字符串，不接受 JSON 数字、前导零或指数。
所有字段闭合，包含 `info/capture` 在内的无参数命令也拒绝未知字段；错误不回显私人 JSON 诊断。

## 宿主顺序与失效

宿主取得用户同意后显式启用策略。派发接受/反馈任务**之前**取得 `captureId`，
在真实编辑成功后才报告 `record-acceptance`，使用捕获的接受前请求和严格递增的正 `eventId`。
Core 只验证并记录反馈，不能证明实际宿主事务成功。关闭状态返回 null receipt，普通补全仍可独立使用。
成功调用（包含 null receipt）消费 capture；失败不消费，调用方可明确选择修正后重试或丢弃，不能自动重放宿主编辑。
真实撤销后消费 `receiptId` 一次。反馈失败不得回滚已成功的文档事务或阻塞输入。

capture/receipt 在当前库内采用全局不回绕标识，不能在另一个对象中碰巧别名。
策略实际发生变化或 clear 会废弃全部未消费标识；设置相同策略不废弃。
关闭再开启策略不能恢复旧任务，不得为迟到任务重新取得 capture 来掩盖过期。
清空须提高真实 scope generation 并更换不同的非零 key，随后更新目录代数并重新查询快照。
清空只处理对象逻辑状态；宿主还须清理自己的排队反馈、持久记录和备份恢复路径。

原始 Core 结果不重排、不修改，只返回候选索引排列及对齐的有限统计；接受仍走原片段接口的当前状态复验。
敏感文档的检索放行不是记录偏好的同意，另受 `allowSensitiveFeedback` 限制。
句柄、反馈标识、哈希和布尔策略均不是权限凭据；不能将这些可信宿主入口直接暴露给不可信插件/MCP。

## 限制与生命周期

最多 64 个对象，每对象最多 128 个未消费 capture 和 1024 条事件（含已撤销事件），不静默逐出未处理标识。
create/元数据命令限制 4 KiB JSON；编辑命令和结果 data 各为 1 MiB，源码/树/候选限制不变。
这些是序列化预算，不是峰值内存承诺；无源码、名称或 key 的安全内存擦除保证。
过期反馈为 `STALE_FEEDBACK`，作用域不符为 `SCOPE_MISMATCH`，闭合结构/版本编码错误为 `INVALID_JSON`，
预算及身份/编码错误保持明确错误码，输出超限后下一次查询仍可用。

全局表和单实例争用均立即返回可恢复的 `PREFERENCE_BUSY`，不排队、不等待或自动重放。
busy close 不删除对象，可在协调并发后明确再关闭。成功 close 使先前已取得的内部引用也失效，
旧句柄不复用；重复 close/未知句柄为 `PREFERENCE_NOT_FOUND`。close 不撤销已返回的历史编辑数据。
mutex 中毒不自动复位或恢复偏好，返回 `INTERNAL`，由可信宿主决定恢复方式。

## 显式检查点

能力探测的 `authenticatedCheckpoints:true` 表示支持认证数据导出/恢复；
普通 owner 的 `persistence:false` 表示不做磁盘 IO；`automaticStorage:false` 始终表示不自动保存。
能力探测的 `persistence` 表示此构建是否支持下面的可选存储，不将所有 owner 自动升级为持久对象。
`export-checkpoint` 的 `permission` 必须显式提供 `storageEnabled/allowSensitiveStorage` 两个布尔值，
它独立于反馈/检索同意，缺字段、非布尔或额外字段均拒绝。
`storageKeyHex` 是另一把独立、非零的宿主私有随机 key，不能复用反馈 key。
`checkpoint` 是严格递增的正 u64 十进制字符串；成功导出会推进本实例序号。
存储失败时只能显式重试保存同一返回数据，不能重新导出旧序号或自动重放。

恢复请求为 `{schemaVersion:1, scope, keyHex, storageKeyHex, expectedCheckpoint, snapshotJson, permission}`。
完整请求最多 1 MiB；`snapshotJson` 最多 256 KiB UTF-8，其他字段仍严格闭合。
`scope/expectedCheckpoint` 必须来自当前可信宿主的新鲜度记录，不得取自待导入文件。
恢复会检查 HMAC、反馈 key 绑定、完整作用域、精确检查点及敏感存储许可，
只新建 owner，不修改旧 owner；策略全关闭，capture/receipt 数量为零，旧反馈令牌不能重放。
64 个 owner 的上限也适用于恢复，失败不占用新句柄。

存储关闭为 `PREFERENCE_STORAGE_DISABLED`；无效检查点内容为 `INVALID_SNAPSHOT`，
认证失败为 `AUTHENTICATION_FAILED`，序号不符为 `STALE_CHECKPOINT`，
敏感许可不足为 `SENSITIVE_STORAGE_DENIED`；作用域/预算错误沿用既有错误码，均不回显私有内容。
HMAC 不是加密。宿主仍需保护密钥、单写者/CAS、新鲜度及清空代数、原子提交与备份恢复路径；
这些入口是可信宿主接口，不是插件可自批的权限机制。详见 [检查点边界](formula-preference-checkpoints.md)。

## 可选 Windows 受保护存储

以 `cargo build -p latexsnipper-ffi --features protected-preferences` 明确启用。
`open_windows` 在所有构建中导出；未启用/非 Windows 时返回 `PREFERENCE_STORAGE_UNAVAILABLE`，
不解引用输入。启用后的闭合请求最多 16 KiB：

```json
{
  "schemaVersion": 1,
  "owner": "example.formula-completion.v1",
  "identity": {"userSha256": "<64 lowercase hex>", "projectSha256": null, "documentSha256": null},
  "persistence": "logon-session",
  "operationTimeoutMs": 5000,
  "permission": {"storageEnabled": true, "allowSensitiveStorage": false},
  "action": {"kind": "create-new", "parentPath": "<trusted local parent>", "directoryName": "preferences", "generation": "0"}
}
```

`persistence` 另可选 `this-user-this-computer`，仍是当前用户而非机器所有用户。
`action` 另有 `open-existing {rootPath}` 与经用户确认的 `initialize-existing {rootPath,generation}`。
generation 为规范 u64 字符串；打开已有状态从受保护 head 取得代数/密钥，不从快照猜测。
缺失/损坏/身份不符不自动初始化；初始化拒绝已有 head，后续 CAS(None) 仍检查竞争。
创建只生成全新私有子目录，不修复父/已有 ACL；创建后失败可能留下该新目录，不擅自递归删除。
工厂先检查当前存储许可，再进行目录 IO；返回默认关闭的普通句柄，内部生成独立随机密钥且从不返回。

这是**可信原生宿主入口**，不得作为插件/MCP 自助存储 RPC。宿主必须在执行边界确保私有根及其
别名/祖先/子目录不与任何 guest 目录授权重叠。JSON 身份/布尔值/路径不是实际隔离或用户同意的证明；
需要真实 capability 重叠检查的宿主应使用 [Rust provider](../crates/preference-storage/README.md) 的
`check_no_directory_overlap`，不能仅做字符串比较或信任 guest 自报目录。

受保护 owner 仍使用相同的 capture/rank/record/undo/policy；原始 `clear/export-checkpoint` 拒绝为
`PREFERENCE_STORAGE_MANAGED`，防止绕过事务或暴露内部密钥。新增显式元数据命令仍限制 4 KiB：

| kind | 字段 | 返回 data（均另含当前 `info`） |
| --- | --- | --- |
| `storage-prepare` | `permission` | `stageId, revision, checkpoint, eventCount, clear:false` |
| `storage-prepare-clear` | `generation` | 同上，`clear:true` |
| `storage-commit` | `stageId, permission` | `stageId, status:"applied"` |
| `storage-reconcile` | `stageId, permission` | `stageId, status:"applied"/"unchanged"` |
| `storage-discard-stage` | `stageId` | `stageId, discarded:true` |
| `storage-cleanup` | `maximum`（1–128） | `inspected, removed, more` |

stageId/revision/checkpoint 均为规范 u64 字符串；每 owner 最多 4 个 stage，达到配额先拒绝而不推进导出序号。
stage 是内存路由标识，不是凭据；不接受其他 owner 的 stage。普通 prepare 不做发布；commit/reconcile
每次复验当前存储与敏感许可。失败与 unchanged 保留 stage；applied 使所有旧基线 stage 失效，
暂存后新增的本地反馈不被旧快照覆盖。成功 prepare-clear 才替换旧 stage；失败保留原 stage。
明确清空轮换代数/双密钥并发布空 epoch，只有这种空重置可在存储许可已关闭时提交/回读。
清空成功废弃旧 capture/receipt，保留当前进程策略；下次恢复策略仍关闭，宿主须更新目录代数并重新查询。

冲突/不确定结果不自动重试，暂停普通写入；明确 reconcile 或重新打开，不能以 discard 消除恢复栅栏。
关闭只丢弃内存与 stage，不保存未提交反馈、不删除 head/墓碑。cleanup 是独立明确动作，可能在出错前
部分完成；保留当前对象/head/锁/墓碑，不承诺备份/磁盘残留擦除，也不枚举用户凭据。

原生调用同步，不能保证抢占取消；timeout 限制 1–30000 ms，只在 provider 检查点复验。
宿主应放到后台线程并管理队列，不能让编辑输入线程等待。IO 不持有全局 owner 表锁；工厂预留配额，
成功后仅用短时注册锁发布句柄。普通命令/关闭争用仍立即返回 `PREFERENCE_BUSY`，从不自动重放。
错误不回显路径、密钥或私有 JSON；除 BUSY 外不标记 recoverable，宿主不能据此自动重发存储。

明确错误包括 `PRIVATE_DIRECTORY_REQUIRED`、`PREFERENCE_STORAGE_NOT_FOUND`、`PREFERENCE_ALREADY_INITIALIZED`、
`PREFERENCE_PROVIDER_UNAVAILABLE`、`PREFERENCE_INTEGRITY_MISMATCH`、`INVALID_PROTECTED_RECORD`、
`PREFERENCE_DEADLINE_EXCEEDED`、`PREFERENCE_CLEANUP_LIMIT`、`PREFERENCE_STAGE_NOT_FOUND`、
`PREFERENCE_STALE_TRANSACTION`、`PREFERENCE_IDENTITY_MISMATCH`、`PREFERENCE_CONFLICT`、
`PREFERENCE_NOT_COMMITTED`、`PREFERENCE_INDETERMINATE` 与 `PREFERENCE_RECOVERY_REQUIRED`；
存储/敏感许可、规范输入及预算错误沿用前述代码。错误后可显式查询 info 的 pendingStages/recoveryRequired。
原生 ACL、DPAPI、当前用户凭据和真实故障边界以 provider 文档为准，不声称硬件防回滚或完全安全内存擦除。

真实 Windows DLL 和 MSVC C11/C++17 头文件/链接检查由 smoke 消费器验证；其他平台和发布包仍以 CI/实际产物验证为准。
JSONL/WASM-JS 也已有独立内存态偏好和显式检查点绑定。
其他语言的受保护存储、其他 OS provider、断电/注销重登、实际部署隔离、跨文档合并及学习效果验收仍开放。

相关资料：[Core 偏好边界](formula-completion-preferences.md)、[补全接口](formula-completion.md)。
