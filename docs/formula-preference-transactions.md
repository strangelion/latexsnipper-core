# 可信宿主偏好事务

Rust `formula_completion::preferences::persistence::transactions` 协调检查点发布、
显式回读和清空。它可用于原生或 WASM 宿主，不访问文件、系统密钥库、浏览器存储或网络，
也不授予用户同意。独立的 [Windows 原生 provider](../crates/preference-storage/README.md) 已有，
Windows 可选 C/Python 已接共享显式工厂、有限 stage、提交/回读/清空和限定清理；
其他系统/浏览器 provider、JSONL/JS 存储生命周期和应用接入仍待实现。

## 宿主与 provider 的责任

`FormulaPreferenceCheckpointBackend` 是**可信宿主接口**，不能直接交给不可信插件实现。
provider 必须保护整个记录及其当前新鲜度，并在所有协作进程之间原子比较并交换：

- `load()` 返回当前受保护记录；不存在不等于允许自动重建。
- `compare_exchange(expected, candidate)` 比较完整内容 witness，原子选择整个候选记录。
  首次发布的 `expected=None` 只允许不存在的槽位，不能覆盖已有根。
- `Applied` 表示已原子选择；`Conflict` 表示已有其他版本，不得覆盖或静默合并。
- `NotCommitted` 仅用于能证明没有选中的情况；超时、迟到 native 写入、部分失败等
  不能证明未提交的情况必须返回 `Indeterminate`。

成功读取必须排在此前所有可能选中根的写入落定之后；若 native 写入仍在进行，
`load` 必须返回 `ProviderUnavailable`，不能把暂时读到旧值报告为可重试的 `Unchanged`。
写入超时不等于取消 IO，也不能提前释放与其他进程/读取之间的协调边界。

记录包含两个**私有密钥**、完整 scope、正 u64 revision/checkpoint 及认证快照。
`encode_for_protected_storage()` 返回敏感明文，只供 provider 加密/保护；
不能写进普通快照文件、日志、公共检查点 API 或插件配置。
`decode_from_protected_storage()` 只做闭合结构解码，不能使不可信文件自动变成可信状态。
记录最大 1 MiB，内嵌快照仍最多 256 KiB；u64 使用规范十进制字符串，不经过浮点数。
Debug 输出隐藏记录与 witness；没有堆内存安全擦除保证。

witness 是带域分离的完整规范化记录 SHA-256，不是认证、授权或密钥替代品。
仅加密/HMAC 普通文件，或仅使用进程内锁，不能满足 protected freshness 和多进程 CAS。
如果攻击者能回放整个受保护根，本协调器不能独自发现；provider 必须提供对应防回滚边界。

## 显式使用顺序

1. 由可信宿主取得稳定 user/project/document 身份与应用 owner 摘要；generation 属于
   当前受保护状态。安全随机生成两个独立非零密钥，不能从公式、路径或固定常量派生。
2. 首次明确初始化用 `FormulaPreferenceStorageSession::new`；它不读取或写入 provider。
   恢复用 `open`，需当前存储许可；它从 provider 取得 generation、密钥和检查点，
   然后认证快照。缺失/错误身份/篡改不自动回退到新建或旧副本。
3. 通过 `preferences_mut()` 根据当前同意设置学习策略；恢复始终默认关闭。
   查询、实际编辑后的反馈与撤销仍遵循 [偏好接口](formula-completion-preferences.md)。
4. `prepare(current_permission)` 捕获不可变候选并推进本地导出序号，**不写 provider**。
   stage 不可克隆/反序列化，绑定当前 Core 实例和受保护基线；不是存储许可凭据。
5. `commit(&mut stage, current_permission)` 重新检查当前存储/敏感许可、实例和基线，
   再调用 provider CAS。成功后 stage 消费；暂存之后新增的本地反馈不会被旧快照覆盖，
   可在下一次显式保存中包含。

stage 丢弃后检查点可以跳号，不复用已导出的序号。受保护 revision 只在成功采用时推进。
多个同基线 stage 不能依次覆盖：采用一个后其他 stage 失效。
通过可变引用替换或直接清空原始 Core 会使会话拒绝后续存储；持久清空应使用下面的专用流程。

## 不确定结果和冲突

协调器在进入 provider 写入前就设置恢复栅栏，因此 provider 在写后 panic/unwind
也不会让后续普通保存继续。不承诺 `panic=abort` 或进程终止时执行内存清理。
`Conflict`/`Indeterminate` 后不自动重试，不重放反馈；普通保存暂停，本地确定性补全仍可用。

宿主保留原 stage，明确调用 `reconcile(&mut stage, current_permission)`：

| 当前受保护状态 | 结果和下一步 |
| --- | --- |
| 与候选完全相同 | `Applied`，采用并消费 stage，不再发布 |
| 与原基线完全相同 | `Unchanged`，允许显式重试同一 stage；不会自动写入 |
| 不同的赢家、缺失原有根或无效状态 | 保留冲突/错误，不合并、不覆盖；明确重新打开或放弃本地未保存状态 |
| provider 不可用 | 保留恢复栅栏，待可用后明确回读 |

`NotCommitted` 允许显式重试同一份 stage；它不证明下一次仍有权限。
恢复/重试均复验当前许可。关闭存储后不为普通历史保存偷偷重新启用权限。
重启后内存 stage 不存在；应从当前 provider 显式 `open`，不按旧快照自选序号。

## 关闭存储后的明确清空

`prepare_clear(new_generation, fresh_keys)` 读取当前受保护元数据但不恢复历史，
要求 generation 高于本地和受保护状态，两个新密钥均不得复用或交换任一旧密钥。
它生成新 generation、checkpoint=1 的空记录；不删除防旧历史复活所需的墓碑。
即使之前保存结果不确定、存储/敏感历史许可已关闭，用户的明确清空仍可发布或回读这个空记录。
这不是一般的后台存储授权，只能发布已暂存的空重置。

清空也要经过 CAS；有新赢家时重新确认状态，不能强行覆盖。
若清空基线比本地新，`Unchanged`/`NotCommitted` 只允许重试这次清空，普通保存仍暂停。
清空成功后废弃旧 stage/capture/receipt 和密钥，保留**当前进程的有效学习策略**；
下次恢复仍默认关闭，不将同意写入记录。宿主须同步更新候选目录 generation 并重新查询。

revision/generation 到达 u64::MAX 时明确拒绝回绕；checkpoint 到顶可通过显式轮换空 epoch
重启序列。备份、旧加密对象、宿主缓存与排队任务的清理由具体 provider/宿主处理，
本接口不保证安全擦除，也不允许自动重建丢失的当前根。

## 已验证与未交付

事务测试使用原子内存 CAS 模拟 provider，覆盖双线程竞争、写前/写后不确定、写后 panic、
明确回读、敏感许可变化、关闭存储后清空、旧实例/令牌失效、上限与整数耗尽。
这证明协调逻辑，不证明磁盘加密、OS 密钥隔离、跨进程 CAS、真实断电或登录会话持久性。
Windows provider 另有真实文件/系统凭据、双进程 CAS、五处终止恢复和限定清理验证，见其独立文档。
Windows C ABI 的有限生命周期另见 [接口文档](formula-preferences-c.md#可选-windows-受保护存储)，
Python 保留同一 Core 拥有对象及不透明暂存令牌，见其 [存储接口](../crates/python/README.md#optional-windows-protected-preference-storage)。
其他系统/浏览器 provider、备份清理及 JSONL/JS/应用存储生命周期验收继续保留待办。

[检查点数据边界](formula-preference-checkpoints.md) · [辅助计划](formula-assistance-roadmap.md)
