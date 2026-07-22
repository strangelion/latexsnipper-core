# Custom Runtime Plugin ABI v1

`latexsnipper-runtime-plugin-api` 允许第三方硬件厂商增加新的模型 Runtime，而不实现或导出
Rust trait object。边界是冻结的 C ABI v1，公共头文件位于：

```text
crates/runtime-plugin-api/include/latexsnipper_runtime_plugin_v1.h
```

插件只导出一个入口：

```c
const LatexSnipperRuntimePluginV1 *
latexsnipper_runtime_plugin_entry_v1(void);
```

function table 包含 ABI/身份信息以及 `probe`、`create_session`、`destroy_session`、`run`、
`free_output`、`last_error`。host 在调用任何函数前检查 `struct_size`、`abi_version`、全部必需
函数以及 descriptor 与 DLL 内的 runtime id/plugin version 是否完全一致。

## 安装、信任与发现

Native runtime plugin 等同于执行本机代码。模型包不能携带或触发加载 DLL/SO/dylib。插件必须
安装到应用单独管理的 runtime-plugin 根目录，每个插件目录包含：

```text
runtime-plugins/
└── vendor-npu/
    ├── runtime-plugin.json
    └── vendor_npu_runtime.dll
```

descriptor：

```json
{
  "schemaVersion": 1,
  "runtimeId": "vendor.npu",
  "pluginVersion": "1.2.0",
  "library": "vendor_npu_runtime.dll",
  "sha256": "<64 lowercase or uppercase hex characters>"
}
```

descriptor 不是信任声明。应用必须用独立来源获得的 SHA-256 显式 enrollment，再显式 enable：

```rust
use latexsnipper_runtime_plugin_api::{
    RuntimePluginTrustStore,
};

let mut trust = RuntimePluginTrustStore::new();
trust.enroll(
    "vendor.npu",
    "/application/runtime-plugins/vendor-npu/vendor_npu_runtime.dll",
    expected_sha256,
)?;
trust.set_enabled("vendor.npu", true)?;

let persisted_json = trust.to_json_pretty()?;
```

trust registry JSON 采用 schema v1，保存 canonical absolute path、digest 与 enabled 状态；
`from_json` 会重新验证 schema、runtime id、digest 格式和绝对路径。实际 discovery 每次仍重新读取
library SHA-256，并同时要求：

- descriptor 有效且不经符号链接；
- library 是 package 内真实普通文件，使用当前平台动态库扩展名；
- canonical path 与 trust enrollment 完全一致；
- descriptor digest、trust digest、实际 digest 三者相同；
- trust entry 已显式 enabled；
- DLL function table 身份与 descriptor 相同。

只放置裸动态库不会被发现；未信任、disabled、被篡改或加载失败的插件进入结构化 discovery
issues，不会注册为可用 Runtime。

Engine 使用可选 feature，默认关闭：

```toml
latexsnipper-engine = { version = "3", features = ["native", "runtime-plugins"] }
```

```rust
let (registry, report) = latexsnipper_engine::runtime_registry_with_plugins(
    models_dir,
    &runtime_plugin_install_roots,
    &trust,
)?;
```

这个入口只扫描调用者传入的插件安装根；不会扫描 `models_dir`。

## 模型 manifest

模型只引用 runtime id 和模型 artifact：

```json
{
  "id": "vendor-ocr",
  "runtimeVariants": [
    {
      "id": "vendor-npu-native",
      "runtime": "custom:vendor.npu",
      "status": "stable",
      "priority": 100,
      "artifacts": { "model": "vendor-ocr.bin" },
      "fallbacks": ["onnx-cpu"]
    },
    {
      "id": "onnx-cpu",
      "runtime": "onnx-runtime",
      "status": "stable",
      "priority": 50,
      "artifacts": { "model": "vendor-ocr.onnx" }
    }
  ]
}
```

模型包不能指定 plugin library path，也不能改变 trust registry。插件未安装、未信任或不可用时，
resolver 只沿 manifest 明确声明的 fallback 继续。

## Tensor 与 ownership

ABI 不跨 DLL 传递 `Vec`、`String`、`std::vector` 或 `std::string`。输入由 host 拥有，只在同步
调用期间借用：

```c
typedef struct LatexSnipperTensorViewV1 {
    LatexSnipperBytesV1 name;
    int32_t dtype;
    const int64_t *shape;
    size_t rank;
    const void *data;
    size_t byte_len;
} LatexSnipperTensorViewV1;
```

插件输出以 `LatexSnipperOwnedTensorListV1` 转移给 host。只要 `owner/tensors/tensor_count` 任一
表示存在 allocation，host 就在 session 仍存活且锁定时调用一次 `free_output`，包括：

- 正常输出复制完成；
- 输出 metadata、shape、dtype 或 byte length 非法；
- `run` 已分配输出后返回错误。

`free_output` 之后 host 将本地 view 清零，不会重复释放。`create_session` 同理：只要返回非空
handle，host 在 metadata 无效或 create 返回错误时仍调用一次 `destroy_session`。

ABI v1 覆盖 `f32/f16/i64/i32/u8/bool`。session metadata 使用 UTF-8 JSON 的公共
`SessionMetadata` wire shape，host 会验证 runtime id、重复名称、method、dtype、输入/输出
shape 与 requested outputs。

## 崩溃边界

v1 是 in-process native ABI。插件不得让 Rust panic 或 C++ exception 穿过 C ABI；普通错误必须
设置 `last_error` 并返回非零状态。host 对所有正常返回、错误返回和畸形输出路径保证单次释放，
mock cdylib 的回归测试会统计并验证 `free_output`/session 生命周期。

硬件插件发生 access violation、abort 等硬崩溃时，当前进程仍会终止；host 不会尝试在崩溃栈上
二次释放 native allocation。真正的故障隔离留给后续 out-of-process runtime host，而不是在
ABI v1 中伪装成已隔离。
