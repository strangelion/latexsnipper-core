# Core 补全偏好边界

更新：2026-10-16。当前是 `formula_completion::preferences` 的 Rust 进程内基础、C ABI、正式 Python 对象、JSONL 及 WASM-JS 绑定，
不是已接入应用的学习器。现有各绑定的补全接口均不自动记录反馈。

## 能力与接入顺序

`FormulaPreferenceStore` 由可信宿主持有，无全局注册表、文件/网络 IO 或自动历史采集。
创建时绑定完整的用户/项目/文档/重置代数和宿主提供的 32 字节私有随机 key。
跨文档共享、用户级/项目级合并与迁移尚未提供，不能把不同作用域的状态混用。

`FormulaPreferencePolicy::default()` 全关闭。宿主在明确同意后才启用
`ranking_enabled`、`feedback_enabled`；两者独立。
敏感文档默认既不记录反馈，也不使用已有分数，另需 `allow_sensitive_feedback`。
片段目录的 `allowInSensitiveDocument` 仅允许检索，不构成偏好记录授权。
上述开关与作用域都由可信宿主提供，不是插件可自行获得的权限凭据。

```rust
use latexsnipper_conversion::formula_completion::{
    fragments::FormulaFragmentCompletionRequest,
    preferences::{FormulaPreferenceError, FormulaPreferencePolicy, FormulaPreferenceStore},
};

fn create_after_host_consent(
    current: &FormulaFragmentCompletionRequest,
    host_random_key: [u8; 32],
) -> Result<FormulaPreferenceStore, FormulaPreferenceError> {
    let mut preferences = FormulaPreferenceStore::new(
        current.catalog.commands.scope.clone(), host_random_key,
    )?;
    preferences.set_policy(FormulaPreferencePolicy {
        ranking_enabled: true,
        feedback_enabled: true,
        allow_sensitive_feedback: false,
    })?;
    Ok(preferences)
}
```

`rank_fragments(&current_request)` 先重新生成并校验 Core 候选，返回原始
`completion`、索引排列 `order` 和与原始候选对齐的 `features`。
只按净接受次数、最后仍有效的接受事件顺序、原始候选顺序做确定性排序；
不增加候选、不修改插入内容、不应用编辑、不授予执行权限。
这只是频率/最近使用基线，不是模型排序或收益测量。

宿主派发接受/反馈任务前先调用 `capture_context()` 捕获当前实例与策略代数。
实际提交用户明确接受的编辑后，再用该上下文和捕获的接受前请求调用
`record_acceptance_at(&capture, &validated_apply_request, positive_event_id)`。
不得在迟到任务执行时重新生成 capture 来掩盖过期：策略关闭再打开、清空和换实例均会拒绝旧 capture。
Core 复验源码、快照、候选、目录与作用域，才记录反馈；这不证明宿主确实写入，
宿主仍必须验证真实事务结果。事件 ID 是严格递增的正 `u64`，不使用墙上时间。
返回 `None` 表示记录未启用，不影响常规补全。
容量、过期或其他反馈错误也不得让宿主回滚已成功的编辑、阻塞输入或循环重试。

成功记录会返回不可序列化的 `FormulaPreferenceReceipt`。真实撤销后调用
`record_undo(&receipt)`，只对同一实例、当前策略代数和未消费的事件生效一次。
支持乱序撤销并重算仍有效的最近接受事件。回执不是编辑权限，也不是跨进程凭据。
策略发生变化即废弃旧回执，关掉再打开也不能恢复；重复设置相同策略不废弃。

## 资源、隐私与重置

- 每个实例最多 1024 条事件，含已撤销事件；满时明确拒绝新记录，不静默逐出旧回执。
- 只存 key 派生的有限候选摘要、事件序号与撤销标记，以及不含明文的作用域身份。
  另存用于持久化授权复验的敏感标记；不存原始公式、候选名称、标签、编辑器版本、时间戳或文档内容；返回的当前候选仍是正常的 Core 数据。
- 摘要绑定有效目录与候选内容/版本。目录、策略或签名变化不会把旧分数继承给复用的 ID；
  保守地在整个有效目录摘要变化后重新积累分数。单纯改变源码位置或编辑版本仍能匹配相同候选。
- key 必须由宿主使用安全随机源生成、隔离保管，不得放在示例常量、日志或插件可读配置中。
  构造器只拒绝全零，不能验证熵。局部摘要是伪名化，不承诺匿名、加密、认证或密钥内存安全擦除。
- 禁用不等于删除：关闭两开关后保留历史但返回零特征/原始排序，不再记录新反馈；
  重新启用可继续使用当前目录下的既有分数。
- `clear(new_generation, new_key)` 在禁用状态下也可调用；必须提高完整作用域的 generation
  并更换非零 key，原子清空事件并废弃旧回执。接受调用必须同步更新目录代数并重新查询快照。
  generation 到达 `u64::MAX` 后不能回绕，应创建新的作用域/实例。
- 清空只针对本实例逻辑状态，不承诺安全擦除堆内存、磁盘备份或宿主缓存；
  宿主须同步清理自己的持久记录和排队反馈，防止重新导入旧数据。

正式 Python 另有 `FormulaPreferences`、不可直接构造的 `FormulaPreferenceCapture/Receipt`，
使用同一 Core 对象与策略，支持显式关闭/上下文管理、GIL 释放、有界数据和实例争用快速拒绝；
不使用全局句柄注册表。用法见 [Python 绑定](../crates/python/README.md#owned-preference-feedback)。
该偏好对象不替代普通补全函数，也不授权真实文档写入。

C ABI 另有显式创建/命令/关闭及有限句柄和反馈标识，复用同一 Core 策略并保留明确的所有权、争用和失效边界，
见 [C ABI 偏好对象](formula-preferences-c.md)。

JSONL 另提供显式偏好创建/命令/关闭，使用原始 JSON 验证和随机 worker 实例命名空间；
退出、EOF、异常及关闭均释放状态，重启后旧 owner/反馈不能别名到新对象，见 [JSONL 绑定](../crates/worker/README.md#explicit-volatile-preference-owners)。

WASM-JS 另有完整/精简包共用的拥有对象及 Worker 路线；实例隔离、默认关闭、撤销/清空边界与 Core 一致，
重启或丢失有界响应时不重放反馈，见 [WASM / JS 接口](formula-preferences-wasm.md)。

Rust/C/Python/JSONL/WASM-JS 另有默认关闭的显式认证检查点，复验当前作用域、key 绑定、检查点和敏感存储许可，
恢复后不启用策略或复活旧反馈令牌，见 [检查点边界](formula-preference-checkpoints.md)。
Rust 另有 [可信宿主事务协调器](formula-preference-transactions.md)，支持 CAS、不确定写入后明确回读、
关闭存储后的空重置；独立 Windows 当前用户受保护 provider 与显式可选 C/Python 存储生命周期已有，
其他系统/浏览器 provider、JSONL/JS 存储生命周期、应用接入及真实断电/登录会话恢复仍待完成；
没有跨进程持久句柄、用户/项目层级合并或云端模型。
后续先补宿主授权检索与可信存储，再比较排序后端与实际编辑体验。
在完成固定基准和真实反馈授权前，不宣称 Top-1、接受率或节省输入的提升。

相关资料：[补全接口](formula-completion.md)、[辅助计划](formula-assistance-roadmap.md)。
