# Plugin system

当前稳定范围是 built-in Rust plugin lifecycle 与外部 package 的离线校验/状态管理。
Native ABI 和 WASI Component execution host 尚未实现，因此 capability 与 doctor 不会把它们
报告为可执行。

每个 plugin 通过 `PluginManifest` 声明稳定 ID、semver、plugin/core API 约束、typed hooks、
priority、dependencies、before/after、permissions、平台、架构、license、entrypoint、
SHA-256/signature metadata 和 configuration schema。

Registry 在 `init()` 前拒绝重复 ID、不兼容 manifest 和 dependency cycle。执行顺序先满足
dependencies/before/after，再按 priority、registration order 与 ID 确定。Cleanup 成功后
才会移除 plugin。`PluginFailurePolicy` 支持 Stop、Continue、DisablePlugin 和 Rollback；
in-process init/handle/cleanup panic 会转换为 typed diagnostic。

`timeoutMillis` 已接入实际 worker deadline：超时会让 host 返回 `PLUGIN_TIMEOUT`，而不是等待
plugin 完成后才比较耗时。Rust 线程不能安全强制终止，因此超时 plugin 的 worker 可能在后台
结束；DisablePlugin 策略可阻止后续调用。第三方不可信代码仍必须由未来的 sandbox host 执行。

`DocumentPatch` 提供页、块、asset 与 diagnostic 的有界 mutation。Patch plugin 从只读
`DocumentView` 生成操作列表；apply 过程是原子的，任一越界、重复 asset 或仍被引用的 asset
删除都会恢复原文档。Legacy `handle` API 保持兼容。Plugin 声明的 typed
`FormatCapability` 会合并进同一个 `CapabilityMatrix`，并随 enable/disable/unregister 自动
增加或移除。

外部 package 管理命令只接受本地目录或 manifest 文件。Native/WASI package 必须提供受限
相对 entrypoint 和匹配 SHA-256；package 拒绝 symlink、路径穿越、超过 256 个文件、总计超过
128 MiB 或 entrypoint 超过 64 MiB。安装采用 staging + re-verification，初始状态始终为
disabled，并且安装过程不会加载或执行 entrypoint。

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

`enable` 只记录供兼容 execution host 使用的期望状态；在本版本中不会加载 Native ABI 或 WASI
代码。`plugin doctor` 会验证已安装 entrypoint 的 checksum，并明确报告两个外部 host 均不可用。
