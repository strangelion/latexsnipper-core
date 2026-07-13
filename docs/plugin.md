# Plugin system

状态：built-in Rust plugin lifecycle 稳定；Native ABI 与 WASI Component host 未实现。

每个 plugin 通过 `PluginManifest` 声明稳定 ID、semver、plugin/core API 约束、typed
hooks、priority、dependencies、before/after、capabilities、permission budgets、平台、
架构、license、entrypoint、checksum/signature 和 configuration schema。

Registry 在 `init()` 前拒绝重复 ID 和不兼容 manifest。执行顺序使用确定性拓扑排序：
依赖/before/after 优先，然后按较高 priority、较早 registration order 和 ID 排序；
环在注册阶段拒绝。Cleanup 成功后才删除 plugin。

`PluginFailurePolicy` 明确定义 Stop、Continue、DisablePlugin 和 Rollback。Trusted
in-process plugin 的 init/handle/cleanup panic 使用 `catch_unwind` 转换为 typed
diagnostics；这防止宿主意外退出，但不是安全 sandbox。Capability 会随 plugin
enable/disable/unregister 动态变化。

Typed hooks 包含 import、recognition、conversion、export 前后阶段，以及 validate、
register importer/exporter/runtime/model adapter。Legacy action string 仍可映射到 typed
hook，以保持兼容。

Native ABI 不能暴露 Rust trait object；未来实现必须使用稳定 C ABI 或经过审计的稳定
ABI。第三方 plugin 的首选目标是 WASI Component，并且只有在 timeout、memory、
filesystem/network/env/model permission 与 deterministic host call 均有可执行测试后
才能标记支持。当前版本不会下载或静默执行外部 plugin。
