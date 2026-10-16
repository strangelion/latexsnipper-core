# 显式原生偏好存储

`latexsnipper-preference-storage` 是 workspace 内的可信宿主适配 crate，当前提供 Windows
`WindowsPreferenceCheckpoint`。不被转换/WASM 包依赖，不自动创建用户历史、启用学习或接入应用 UI。
已有可选 [C ABI 存储生命周期](../../docs/formula-preferences-c.md#可选-windows-受保护存储)；
其他系统和浏览器 provider、Python/JSONL/JS 存储生命周期仍待实现。

## 数据与发布

完整 `ProtectedPreferenceRecord`（含私有反馈/storage keys、scope 和认证快照）用当前用户
DPAPI 加密后写为不可覆盖的新对象。Credential Manager 的精确专用命名空间只保存闭合的
owner/密文 SHA-256 选择记录，不保存原公式或私有密钥。文件写完、同步并回读后才选择新 head；
读写均持有同一 OS 文件锁，所有协作进程都必须使用此 provider。

每次读取复验私有目录/文件 DACL、单硬链接、密文摘要、DPAPI 解密、owner/scope 和内层 HMAC。
根目录 owner 必须是当前用户；文件 owner 也允许已在严格 DACL 中持有完整访问的
SYSTEM/Administrators，以兼容提升权限 token 的默认对象所有者，不增加任何访问主体。
Windows 的默认 owner 来自创建进程的 token，见 [微软说明](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object)。
丢失、损坏或不符的当前对象明确拒绝，不从目录挑选“最新”或旧副本，不自动初始化。
同基线 CAS 只允许一个赢家；新 head/keys/历史作为一个完整记录被选中，不分别推进。
错误和 Debug 不输出私有记录、密钥或路径。
可信宿主可用 `read_current_record()` 区分有界的原生诊断类别；协调器接口仍返回统一的
provider 错误，不向插件转发系统原始错误或私有内容。

DPAPI 未使用 machine-wide 解密标志，也不弹出系统提示。
`LogonSession` 与 `ThisUserThisComputer` 是不同命名空间；后者仍是当前用户凭据，
不是所有用户可用的机器级授权。此行为依据微软的
[DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) 和
[Credential Manager](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew) 接口。

## 接入顺序

1. 宿主决定当前稳定用户/项目/文档身份和固定应用 owner，不接受插件自报身份或存储权限。
2. 首次明确创建用 `create_private_root(parent, single_child, ...)`；已有目录用
   `open_verified_private_root(...)`。只接受可验证的本地固定卷；不修改父目录或修复已有 ACL。
   若创建后的复验失败，留下新目录供检查，不递归删除可能已被其他代码改变的路径。
3. 以 `check_no_directory_overlap(&actual_directory_capabilities)` 检查真实 guest 目录授权，
   并在授权/根变化后复查。父目录、私有根、子目录及大小写别名均拒绝；无法解析物理身份的
   远程/特殊目录授权也拒绝，不能只凭路径标签判断。此方法不拦截 IO 或自动授予/撤销权限，
   宿主必须在执行边界调用，不能将 root 再交给未受检的 guest。
4. 从 `backend.owner_sha256()` 取得此槽位的 owner，显式 `FormulaPreferenceStorageSession::open`
   或经用户明确确认的首次 `new`；绝不能将 `NotFound` 自动当作首次使用。
   新建密钥使用 `generate_host_keys()`，从系统安全随机源取得两个独立密钥。
5. 按 [事务接口](../../docs/formula-preference-transactions.md) 明确 prepare/commit/reconcile。
   每次复验当前存储/敏感许可；恢复仍默认关闭学习，provider 本身不是用户同意凭据。
6. 用户明确清空时，轮换 generation 和两个密钥，发布空 epoch 墓碑；关闭存储后也可完成。
   更新候选目录代数并丢弃旧排队反馈，不能单删 head 以“重置”。

```rust
use latexsnipper_conversion::formula_completion::preferences::persistence::transactions::{
    FormulaPreferenceStorageSession, FormulaPreferenceStorageIdentity,
};
use latexsnipper_preference_storage::windows::{
    WindowsPreferenceCheckpoint, WindowsPreferencePersistence,
};

// Arguments below come from the trusted host and its current consent state.
let backend = WindowsPreferenceCheckpoint::open_verified_private_root(
    host_private_root, "example.formula-completion.v1", current_identity.clone(),
    WindowsPreferencePersistence::ThisUserThisComputer,
    std::time::Duration::from_secs(5),
)?;
backend.check_no_directory_overlap(&actual_guest_directories)?;
let owner = backend.owner_sha256().to_owned();
let mut session = FormulaPreferenceStorageSession::open(
    backend, owner, current_identity, current_storage_permission,
)?;
// No automatic set_policy(), retry, history merge or document write follows.
```

新 crate 尚未发布到 crates.io；以上为 workspace Rust 宿主接口。C ABI 通过显式
`protected-preferences` 特性启用，不自动迁移普通 owner；Python/JSONL/JS 尚无系统存储生命周期，
也不代表 Office 已接入。

## 清理与故障边界

`cleanup_unselected(maximum)` 是独立、明确的有界清理：每次最多删除 1–128 个本槽位未选中对象，
最多检查 4096 个目录项；只匹配完整 namespace/hash 文件名，不遍历子目录或枚举凭据。
当前 head、空 epoch 墓碑、选中对象和锁文件都保留，当前状态不完整时停止清理。
调用出错前可能已移除部分旧对象，不是可回滚的删除事务；不承诺擦除备份或磁盘残留。
不得在仍有 provider/协作进程时另行删除锁文件或私有根。

锁等待和分块 IO 有截止时间检查，但同步 Win32 调用不保证可中断。
进入 head 写入后遇到失败、迟到结果或回读不确定，返回 `Indeterminate` 并由协调器暂停普通保存；
不自动重发。没有后台悬挂的发布线程，回读同样需取得 OS 锁，不能以暂时旧值放行未决写入。
瞬态故障恢复不清空历史、不恢复旧反馈令牌，也不永久禁用 provider。

威胁边界不包括拥有当前用户任意原生权限、管理员权限或回滚整个 Windows 凭据存储的攻击者。
DPAPI/文件同步不构成硬件防回滚或完整安全内存擦除；仅清理临时明文缓冲区，不能保证所有
Rust/OS/备份副本已擦除。共享/远程卷不纳入此每计算机 head 的实现，卷 GUID 限制见
[微软路径接口](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew)。

## 验证范围

Windows 原生测试覆盖两种持久性枚举的**同一登录会话**保存/恢复、默认关闭、密文/内层认证/身份、
ACL 变化与恢复、物理目录重叠、真实双进程 CAS、五处发布边界强制终止及限定清理。
进程终止发生在命名发布边界，不能推断在 Win32 RPC 内部强杀、断电、注销重登、漫游配置或
真实应用部署中均已验收；这些与 x86/其他文件系统和其他 OS provider 仍待补齐。

```powershell
cargo test --locked -p latexsnipper-preference-storage --all-targets
cargo clippy --locked -p latexsnipper-preference-storage --all-targets -- -D warnings
```

Windows CI 使用独立 fixture 父目录并检查其清空。测试只删除各自精确的随机命名凭据和临时目录；
不枚举、读取或移除用户/应用已有凭据。
