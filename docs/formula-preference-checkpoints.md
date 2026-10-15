# 补全偏好检查点

Rust `formula_completion::preferences::persistence` 提供显式、有界的认证检查点。
Core 不创建文件、不选择数据库或密钥提供者，也不自动恢复或记录用户历史。
这是持久化的数据和恢复边界；Rust/C ABI/Python/JSONL/WASM-JS 的显式存取绑定已有，
宿主受保护存储及断电恢复仍待实现。

## 授权与接口

`FormulaPreferenceStoragePermission::default()` 全关闭。每次导出和恢复都需可信宿主提供
`storage_enabled`；敏感反馈还另需 `allow_sensitive_storage`。
检索授权、反馈授权、磁盘存储授权相互独立，不能从一个推导出另一个。
两个字段是宿主策略输入，不是插件可自行申请的权限凭据。

- `store.export_checkpoint(&storage_key, checkpoint, permission) -> Result<String, FormulaPreferenceStorageError>`
- `FormulaPreferenceStore::restore_checkpoint(current_scope, feedback_key, &storage_key, expected_checkpoint, json, permission) -> Result<FormulaPreferenceStore, FormulaPreferenceStorageError>`

feedback key 仍是原实例的私有 32 字节随机 key；storage key 必须是另一个独立、非零的 32 字节随机 key。
Core 不能验证随机熵。宿主应由系统密钥存储隔离管理，不能把 key 写入快照、日志、仓库或未授权插件配置。
恢复前须由当前用户/项目/文档的可信状态取得完整 scope、反馈 key、storage key 与期望检查点。
**不得从待导入快照中复制 scope/checkpoint 作为“当前可信值”**。

检查点是严格递增的正 u64；Rust 参数使用 u64，JSON 中均为 canonical decimal 字符串。
同一实例不允许重复/倒退导出序号；恢复后保留已使用的检查点和事件序号，不回绕。
到达 u64::MAX 后需明确清空、推进 scope generation、更换反馈 key，再创建新检查点序列。
成功导出会推进实例的最后检查点，但不证明宿主已经保存：
磁盘写入失败时可以显式重试保存**同一份返回字节**，不要重新导出旧序号。
也可以导出更高序号，但不能把未提交的序号误写成当前可恢复版本。

## 快照内容与校验

JSON envelope 只有 body 和 authentication。body 的 schemaVersion 为 1，包含完整 scope、
checkpoint、反馈 key 的绑定摘要及最多 1024 条事件。
事件只含 eventId、key 派生候选摘要、undone 和 sensitive 标记。
不包含原公式、候选 ID/名称、私有标签、编辑版本、时间戳、密钥、捕获对象、撤销回执或授权开关。
敏感标记只用于存储授权复验，禁用学习后仍保留该标记，不能通过关闭学习绕过敏感存储许可。

使用独立 storage key 的 HMAC-SHA256，分别对反馈 key 绑定和带域分离的规范化 body JSON 认证。
认证验证采用 HMAC 库的验证接口；不使用普通字符串相等比较 MAC。
JSON 拒绝未知/重复字段、非规范整数、超额事件、非递增或零事件 ID、无效摘要及版本。
大小上限为 256 KiB UTF-8 JSON；错误不附带私有 JSON 字段或值。
空白和字段顺序可以变化，认证的是闭合类型重新编码后的语义内容，不是原始文件字节。

HMAC **不是加密、匿名化或安全擦除**。scope、计数和敏感标记仍可被读取，候选摘要只是伪名化。
宿主应按用户数据保护快照及其备份，必要时在系统受保护存储上另加密。
持有 storage key 的可信代码能够重新签名；它不应提供给未授权插件。

## 恢复、清空与故障

恢复成功总是新建独立实例，并将 ranking/feedback/sensitive learning 策略保持默认关闭。
快照不会恢复用户同意；宿主应根据当前有效同意显式设置策略。
旧 capture/receipt 绑定旧实例，不能在恢复实例上重放；已有事件的序号也不能再次提交。
只恢复历史排序摘要及已撤销标记，不恢复跨重启的撤销令牌或编辑权限。
当前有效目录仍须重新查询/校验，历史分数不能注入候选或绕过 Core 验收。

`expected_checkpoint` 必须与快照完全相同；旧版本、新于已提交版本及其他作用域均拒绝。
这提供了**依赖宿主可信新鲜度状态**的防回滚校验，不是 Core 单独保证磁盘不被回滚。
仅有 HMAC 而没有受保护的新鲜度/重置记录，攻击者仍可回放旧的有效文件。
宿主需要单写者或 CAS、受保护的检查点/代数记录，以及原子提交/恢复协议；
不能在写入失败后盲目把“最新序号”推进到并不存在的文件。

清空操作先推进可信 scope generation 并更换反馈 key，再清理宿主持久文件、备份和排队反馈。
之后仅允许按新 scope 恢复。单纯删除文件而丢失代数/检查点高水位，不构成完整的防旧状态复活。
禁用存储不等于删除；文件清理与备份保留由宿主明确处理，不由 Core 偷偷执行。
读取、认证或存储故障时应保留可诊断状态、返回确定性补全，不自动启用策略、覆盖文档、
回滚成功编辑或循环重放反馈。

Rust 测试覆盖有界恢复、默认关闭、跨实例旧反馈拒绝、独立 Python HMAC 参考、
宿主文件写读与清理，以及篡改/作用域/检查点/敏感授权拒绝。
这不替代受保护存储、密钥提供者、跨绑定和真实断电/多进程宿主验收。

## 公共绑定

C ABI 提供 `export-checkpoint` 命令及独立 `latexsnipper_formula_preferences_restore` 入口，
恢复只新建默认关闭的句柄；原始请求直接闭合解码，规范 u64 字符串不经过浮点数，
见 [C ABI 接口](formula-preferences-c.md#显式检查点)。
Python 提供 `export_checkpoint` 与静态 `restore_checkpoint`，检查点使用严格 Python u64 int，
返回独立拥有对象并释放 GIL；见 [Python 接口](../crates/python/README.md#explicit-preference-checkpoints)。
两者均要求独立存储许可和 key，不代管磁盘、系统密钥或用户同意。
JSONL 提供 `export-checkpoint` 及 `formula.preferences.restore`；恢复在新 Worker 命名空间创建
默认关闭的 owner，不恢复旧路由标识。请求行仍限制 1 MiB，检查点字符串限制 256 KiB，
见 [JSONL 接口](../crates/worker/README.md#explicit-authenticated-checkpoints)。
WASM-JS 提供原始恢复工厂、直接包装对象和显式 Worker 恢复 RPC；Worker 重启后不重放检查点操作，
恢复后仍默认关闭学习且拒绝旧令牌，见 [WASM/JS 接口](formula-preferences-wasm.md#显式检查点)。
所有绑定只认证/返回数据；不能把公共检查点接口的完成视为宿主可信存储和真实故障恢复验收完成。
