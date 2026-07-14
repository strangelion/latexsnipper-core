# Plugin system

当前稳定范围包括：built-in Rust plugin 生命周期、版本化 isolated-process IPC host、外部 package
离线校验、typed hooks、确定性排序、事务 patch、能力注册以及强制资源预算。独立的
`latexsnipper-plugin-wasi` crate 已实现 WIT v1 Component host；legacy plugin registry/CLI 尚未接入。
Native dynamic-library ABI 仍未实现。

## 执行类别

`PluginExecutionClass` 明确区分三种边界：

- `TrustedInProcess`：可信 Rust 代码。超时是 cooperative soft timeout，任意 Rust 线程无法被安全强杀。
- `IsolatedProcess`：版本化 JSON IPC 子进程。host 可以在 deadline、内存或输出预算触发时终止并回收进程。
- `WasiComponent`：manifest v3 + WIT v1 的 default-deny Component host；当前通过 Rust API 使用。

可信插件通过 `PluginExecutionContext` 获取 cancellation token、deadline、effective permissions 和
diagnostic sink。长任务必须定期调用 `checkpoint()`。达到 soft timeout 后，host 返回
`PLUGIN_SOFT_TIMEOUT`，并设置：

```json
{
  "softTimeout": true,
  "executionMayStillBeRunning": true
}
```

plugin 会被 quarantine，立即重入被拒绝；每个 plugin 的 outstanding worker 数受
`maxConcurrentExecutions` 限制。后台执行结束后，host 必须显式调用 `reset_quarantine()` 才能重试。
这可以阻止无界线程累积，但不会伪装成 hard termination。

isolated-process host 使用 ABI/IPC version 1、空环境、安全随机命名的独立临时工作目录、同一次执行的
request/response 文件、response 文件观察预算和 deadline。Unix 在 `exec` 前创建独立 session/process
group 并设置 address-space limit；超时会向整个 process group 发送 `SIGKILL`，再回收直接子进程。该路径
覆盖没有主动逃离 session 的普通后代进程；自行调用 `setsid` 等方式逃离的进程仍需要真正的 OS sandbox
约束。Windows 使用 Job Object 的 process-memory 与 kill-on-close 限制，但当前在 `spawn` 后才完成
assignment，仍存在极短的启动竞态，因此只适合经过审核的本地 plugin。

Unix 工作目录权限显式设为 `0700`，request 文件使用 `0600` 和 `create_new`。Windows 依赖当前用户临时
目录继承的 ACL，尚未创建显式的仅当前用户 ACL。`outputLimitBytes` 只限制 host 观察到的 response 文件，
不是整个工作目录的磁盘配额，也不是进程级总 I/O 配额。

## Permissions

外部 plugin 默认拒绝 ambient access。manifest 可分别声明：

- `filesystemReadPaths` / `filesystemWritePaths`
- `networkHosts`
- `environmentVariables`
- `modelAccess`
- `temporaryDirectory`
- `memoryLimitBytes` / `outputLimitBytes` / `timeoutMillis`
- importer、exporter、runtime 和 capability registration grants

文件路径会在检查前 canonicalize；`..`、absolute bypass 和 symlink escape 都会被拒绝。环境变量只允许
按名称读取，网络和模型访问只允许精确 grant。未授权的 registration hook 或 capability declaration
会返回 `PLUGIN_PERMISSION_DENIED`。`plugin doctor` 只报告 effective grant 数量和预算，不输出可能敏感
的完整路径、host 或环境值。

isolated-process 的 permissions 只约束 host broker 提供的文件、网络、模型和注册操作。外部 native
进程仍可直接调用操作系统 API（例如 `std::fs` 或 `TcpStream`），因此这些 grant 不是 OS sandbox，不能
阻止任意 filesystem/network 访问。`plugin doctor` 会明确报告
`nativeProcessOsSandboxed: false` 和 `enforcementScope: brokered-host-operations`。需要处理不可信第三方
代码时，应使用 WASI Component host，而不是把 process plugin 权限误当成 OS sandbox。远程 WASI 安装已通过
signed registry、Ed25519 threshold、signature/provenance、rollback/freeze 和受限 archive 校验，但安装后保持
disabled；public execution 集成仍需使用 hardened WASI host 完成。

WASI Component world 不链接 ambient CLI、stdio、environment、filesystem、socket 或 process WASI
接口，只暴露 manifest grant 对应的 typed broker。每次调用使用新的 Store/instance，并受 fuel、独立
epoch deadline/cancellation、memory/table/resource/input/output/diagnostic/model/temp 和 concurrency
限制。完整边界与诊断见 [WASI Component host v1](v3/wasi-component-host.md)，远程信任与存储边界见
[signed plugin registry threat model](v3/plugin-registry-threat-model.md)。

## Manifest、顺序与 failure policy

`PluginManifest` 声明稳定 ID、semver、plugin/core API、external ABI、typed hooks、priority、dependencies、
before/after、permissions、平台、架构、license、entrypoint、SHA-256/signature metadata 和 configuration
schema。

Registry 在 `init()` 前拒绝重复 ID、不兼容 manifest 和 dependency cycle。执行顺序先满足 dependency/
before/after，再按 priority、registration order 与 ID 确定。cleanup 成功后才移除 plugin。
`PluginFailurePolicy` 支持 `Stop`、`Continue`、`DisablePlugin` 和 `Rollback`；panic 会转换为 typed
diagnostic，不会击穿 host。

`DocumentPatch` 对 page、block、asset 和 diagnostic 执行有界 mutation。apply 是原子的，任一操作失败
会恢复原 document。Legacy `handle` API 继续兼容。启用 plugin 的 typed `FormatCapability` 合并到共享
`CapabilityMatrix`，disable/unregister 时移除。

## Package 与远程安装

package 必须提供有界 relative entrypoint、匹配 SHA-256 和 ABI version。校验拒绝 symlink、路径穿越、
超过 256 个文件、总量超过 128 MiB 或 entrypoint 超过 64 MiB。安装使用 staging、二次校验和原子 rename，
初始状态始终 disabled，安装过程不执行代码。registry 更新由跨进程文件锁串行化，并在同目录临时文件
flush/fsync 后执行原子替换；Unix 还会 fsync 父目录，Windows 使用 write-through replace。该保证针对
registry 文件本身，不把 package 目录与 registry 声明为跨资源事务。

```bash
snipper plugin verify ./plugin-package
snipper plugin install ./plugin-package
snipper plugin list
snipper plugin info example.plugin
snipper plugin enable example.plugin
snipper plugin disable example.plugin
snipper plugin doctor
snipper plugin uninstall example.plugin
```

远程 URL 安装继续保持 unsupported。启用前仍需完成 HTTPS-only registry/allowlist、checksum、signature、
redirect/content-type policy、用户确认、provenance 和 update-channel trust model。
