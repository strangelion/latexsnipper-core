# Custom Runtime Plugin

> 第三方硬件厂商接入指南

## 概述

LaTeXSnipper 通过 `latexsnipper-runtime-plugin-api` 提供冻结的 C ABI，允许第三方
硬件厂商添加新的 Runtime（如 NPU、DSP、FPGA 加速器）而无需修改 Core 代码。

## 现有文档

关于 ABI 的完整规范，参见：
- [Custom Runtime Plugin ABI v1](runtime-plugin-api.md) — C ABI 规范、头文件、安装与信任模型

## 与内置 Runtime 的区别

| | 内置 Runtime | Custom Plugin |
|---|---|---|
| 实现语言 | Rust | C / C++ / 任何有 C FFI 的语言 |
| 编译 | workspace crate | 独立编译为 .dll/.so/.dylib |
| 发现 | `#[cfg(feature)]` | 运行时 `dlopen` |
| 信任 | 编译时 | SHA-256 enrollment + 显式 enable |
| 分发 | 内置在二进制中 | 独立安装到 `runtime-plugins/` 目录 |

## 快速开始

### 1. 下载 ABI 头文件

```bash
cp crates/runtime-plugin-api/include/latexsnipper_runtime_plugin_v1.h my-plugin/
```

### 2. 实现入口函数

```c
#include "latexsnipper_runtime_plugin_v1.h"

const LatexSnipperRuntimePluginV1 *
latexsnipper_runtime_plugin_entry_v1(void) {
    static LatexSnipperRuntimePluginV1 plugin = {
        .struct_size = sizeof(LatexSnipperRuntimePluginV1),
        .abi_version = 1,
        .probe = my_probe,
        .create_session = my_create_session,
        .destroy_session = my_destroy_session,
        .run = my_run,
        .free_output = my_free_output,
        .last_error = my_last_error,
    };
    return &plugin;
}
```

### 3. 编写 plugin descriptor

每个插件目录包含一个 `runtime-plugin.json` descriptor：

```json
{
  "schemaVersion": 1,
  "runtimeId": "vendor.npu",
  "pluginVersion": "1.2.0",
  "library": "vendor_npu_runtime.dll",
  "sha256": "<64 lowercase hex characters>"
}
```

注意：descriptor 本身不是信任声明。它只是声明了 runtime id、版本、库文件名和期望的 SHA-256。

### 4. 安装到插件目录

```text
runtime-plugins/
└── vendor-npu/
    ├── runtime-plugin.json
    └── vendor_npu_runtime.dll
```

### 5. Trust enrollment（应用必须显式 enroll + enable）

```rust
use latexsnipper_runtime_plugin_api::RuntimePluginTrustStore;

let mut trust = RuntimePluginTrustStore::new();

// Enroll a plugin with the canonical path and its expected SHA-256.
trust.enroll(
    "vendor.npu",
    "/app/runtime-plugins/vendor-npu/vendor_npu_runtime.dll",
    expected_sha256,
)?;

// Explicitly enable the enrolled plugin.
trust.set_enabled("vendor.npu", true)?;
```

信任机制是 **SHA-256 digest enrollment**（不是 Ed25519 签名）。实际加载时，host 会：
- 重新计算 library 文件的 SHA-256
- 验证 descriptor digest、trust enrollment digest、实际 digest 三者完全一致
- 验证 DLL function table 身份与 descriptor 一致
- 验证 trust entry 已显式 enabled

### 6. 为模型添加 RuntimeVariant

```json
{
  "runtimeVariants": [
    {
      "id": "vendor-npu",
      "runtime": "vendor.npu",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "model": "model.nb",
        "config": "config.json"
      },
      "platforms": ["linux-aarch64"],
      "capabilities": ["int8-quantized"],
      "fallbacks": ["onnx"]
    }
  ]
}
```

## 安全约束

- 插件不能携带在模型包中（必须独立安装）
- 插件目录由应用管理，不接受用户任意路径
- 信任基于 SHA-256 enrollment + 显式 enable，不是 Ed25519 签名
- 未 enroll、未 enable、被篡改或加载失败的插件进入结构化 discovery issues，不注册为可用 Runtime
- 只放置裸动态库不会被 discovery

## 相关文档

- [Custom Runtime Plugin ABI v1](runtime-plugin-api.md) — 完整 ABI 规范与信任模型
- [Runtime Development](runtime-development.md) — 编写内置 Rust Runtime
- [Runtime Architecture](runtime-architecture.md) — 整体架构
