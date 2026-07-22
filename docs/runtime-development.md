# Runtime Development

> 如何为 LaTeXSnipper 实现一个新的 Runtime backend

## 概述

Runtime 是 LaTeXSnipper 的推理执行层。每个 Runtime 负责：
- 加载模型文件（ONNX、Paddle、Core ML 等格式）
- 执行推理（forward pass）
- 管理设备（CPU / GPU / NPU）
- 提供 session metadata（输入/输出 tensor shape 和 dtype）

## 需要实现的 Traits

### 1. RuntimeFactory

```rust
pub trait RuntimeFactory: Send + Sync {
    /// 检查 Runtime SDK 在当前设备上是否可用
    fn probe(&self) -> RuntimeProbe;

    /// 创建推理 session
    fn create_session(
        &self,
        kind: RuntimeKind,
        artifacts: &RuntimeArtifacts,
        options: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<Box<dyn RuntimeSession>>;
}
```

`probe()` 返回 SDK 是否已安装、可用设备列表、以及 runtime 级别的 capabilities。

### 2. RuntimeSession

```rust
pub trait RuntimeSession: Send + Sync {
    fn run(&self, request: RunRequest) -> Result<RunResponse>;
    fn metadata(&self) -> &SessionMetadata;
}
```

### 3. RuntimeProbe

```rust
pub struct RuntimeProbe {
    pub available: bool,
    pub version: Option<String>,
    pub devices: Vec<RuntimeDevice>,
    pub capabilities: Vec<String>,
    pub issues: Vec<String>,
}
```

## 实现步骤

### Step 1: 创建 crate

```bash
mkdir crates/runtime-{name}
cp crates/runtime-paddle/Cargo.toml crates/runtime-{name}/Cargo.toml
# 编辑 Cargo.toml 修改 name / dependencies
```

### Step 2: 实现 RuntimeFactory

```rust
use latexsnipper_runtime::{RuntimeFactory, RuntimeKind, RuntimeSession, RunRequest, RunResponse,
    SessionMetadata, RuntimeProbe, RuntimeDevice, RuntimeCapabilities};

pub struct MyRuntimeFactory {
    // Optional: SDK library handle, preloaded symbols, etc.
}

impl RuntimeFactory for MyRuntimeFactory {
    fn probe(&self) -> RuntimeProbe {
        // Check if the SDK dynamic library can be loaded
        // Detect available devices (CPU, GPU, etc.)
        // List capabilities (specific features this runtime supports)
        RuntimeProbe {
            available: check_sdk_present(),
            version: Some(get_sdk_version()),
            devices: detect_devices(),
            capabilities: vec!["my-capability".to_string()],
            issues: vec![],
        }
    }

    fn create_session(
        &self,
        kind: RuntimeKind,
        artifacts: &RuntimeArtifacts,
        options: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<Box<dyn RuntimeSession>> {
        // Load model from artifacts
        // Configure device based on options
        // Return a session
    }
}
```

### Step 3: 实现 RuntimeSession

```rust
struct MySession {
    metadata: SessionMetadata,
    // Internal: loaded model handle, device context, etc.
}

impl RuntimeSession for MySession {
    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        // Execute inference with the model
        // Build a RunResponse with output tensors
    }

    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }
}
```

### Step 4: 注册到 workspace

在根 `Cargo.toml` 中：

```toml
[workspace]
members = [
    # ...
    "crates/runtime-{name}",
    "crates/runtime-{name}/fixtures/mock-{name}-plugin",
]

[workspace.dependencies]
# ...
latexsnipper-runtime-{name} = { path = "crates/runtime-{name}", version = "3.0.1" }
```

### Step 5: 注册到 Engine

在 `crates/engine/Cargo.toml` 中：

```toml
[dependencies]
latexsnipper-runtime-{name} = { workspace = true, optional = true }

[features]
{name} = ["dep:latexsnipper-runtime-{name}"]
```

在 `crates/engine/src/runtime_registry.rs` 中：

```rust
#[cfg(feature = "{name}")]
let registry = {
    let mut registry = registry;
    registry.register(latexsnipper_runtime_{name}::MyRuntimeFactory::new())?;
    registry
};
```

### Step 6: 添加 RuntimeKind 变体

在 `crates/runtime/src/kind.rs` 中：

```rust
pub enum RuntimeKind {
    // ... existing variants ...
    // Third-party runtime with a string identifier
    Custom(String),
}
```

### Step 7: 编写模型 manifest

为模型添加 `runtimeVariants` 入口：

```json
{
  "runtimeVariants": [
    {
      "id": "{name}-native",
      "runtime": "{name}-runtime",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "model": "model.{ext}",
        "config": "config.json"
      },
      "capabilities": ["my-capability"],
      "fallbacks": []
    }
  ]
}
```

## Runtime contract 测试

每个 Runtime crate 应包含 contract 测试，验证：
- Session 创建和销毁
- Error 类型正确传播
- Metadata 格式有效
- 对无效 artifacts 返回合理错误

```rust
#[test]
fn factory_probe_reports_availability_correctly() {
    let factory = MyRuntimeFactory::new();
    let probe = factory.probe();
    assert!(probe.available || !probe.issues.is_empty());
}

#[test]
fn create_session_with_missing_model_returns_error() {
    let factory = MyRuntimeFactory::new();
    let artifacts = RuntimeArtifacts::new();
    let result = factory.create_session(
        RuntimeKind::Custom("test".into()),
        &artifacts,
        None,
    );
    assert!(result.is_err());
}
```

## P0 检查清单

- [ ] `probe()` 在 SDK 缺失时不 panic
- [ ] `create_session()` 对错误的模型文件返回 Error（不 undefined behavior）
- [ ] 线程安全（`Send + Sync`）
- [ ] Session 销毁正确释放 GPU 内存
- [ ] `target_arch = "wasm32"` 时不编译（via cfg gate）
- [ ] 文档说明 SDK 安装步骤

## 相关文档

- [Runtime Architecture](runtime-architecture.md) — 整体架构
- [Model Package Format](model-package-format.md) — `runtimeVariants` schema
- [Custom Runtime Plugin](custom-runtime-plugin.md) — 第三方插件 ABI（WASM/dlopen）
