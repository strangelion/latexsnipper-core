# Runtime Crate

> 推理运行时抽象 — Session, Tensor, Acceleration

## 核心原则

1. **Runtime 只管 Session/Tensor/Device，不管 Pipeline**
2. **Core 只认识 RuntimeBackend trait，不认识 OrtSession**
3. **Runtime 可替换：ONNX → TensorRT → NCNN → OpenVINO**

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `backend` | backend.rs | RuntimeBackend trait |
| `session` | session.rs | InferenceSession trait |
| `acceleration` | acceleration.rs | AccelerationMode (Cpu/Gpu/Auto) |
| `model_handle` | model_handle.rs | ModelHandle（替代 Path） |
| `providers/stub` | stub/mod.rs | StubRuntime（测试用） |
| `providers/onnx` | onnx/backend.rs | OnnxRuntimeBackend（ORT 实现） |
| `providers/onnx/platform` | onnx/platform.rs | Platform 检测 + Acceleration 检测 |

## 关键 Trait

### RuntimeBackend

```rust
pub trait RuntimeBackend: Send + Sync {
    fn create_session(&self, handle: &ModelHandle, acceleration: AccelerationMode) -> Result<Box<dyn InferenceSession>>;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
}
```

### InferenceSession

```rust
pub trait InferenceSession: Send + Sync {
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>>;
    fn input_names(&self) -> Vec<String>;
    fn output_names(&self) -> Vec<String>;
    fn release(&mut self);
}
```

## ModelHandle

```rust
pub struct ModelHandle {
    id: String,
    category: String,
    variant: String,
    model_path: Option<PathBuf>,
}
// methods: new(), with_path(), id(), category(), variant(), model_path()
```

`ModelHandle` 替代裸 `Path`，封装模型的 category/variant 信息，支持显式路径和自动发现。

## StubRuntime

测试用 Runtime，返回空结果：

```rust
pub struct StubRuntime;
impl RuntimeBackend for StubRuntime { /* ... */ }
```

- `create_session()` 返回 `StubSession`
- `StubSession::run()` 返回与输入同 shape 的零值 Tensor
- 始终可用（`is_available() = true`）

## OnnxRuntimeBackend

使用 ort crate 2.0.0-rc.12 的完整实现：

```rust
pub struct OnnxRuntimeBackend {
    env: Arc<Environment>,
    models_dir: PathBuf,
    platform: Platform,
    acceleration: Acceleration,
    sessions: Mutex<HashMap<String, Mutex<Session>>>,
}
```

| 方法 | 说明 |
|------|------|
| `OnnxRuntimeBackend::new(models_dir)` | 自动检测平台和 GPU |
| `OnnxRuntimeBackend::with_acceleration(models_dir, accel)` | 指定加速模式 |
| `platform()` | 返回检测到的平台 |
| `acceleration()` | 返回当前加速模式 |

### 模型路径解析

1. 如果 `ModelHandle` 有显式 `model_path`，直接使用
2. 否则按 `models_dir/category/variant/` 查找
3. 尝试 `model.onnx` → `model_int8.onnx` → `{category}.onnx`
4. Fallback：目录下任意 `.onnx` 文件

## Platform 检测

```rust
pub enum Platform {
    WindowsX64, WindowsArm64,
    LinuxX64, LinuxAarch64,
    MacOsArm64, Unknown,
}
```

编译时自动检测，用于生成 ORT 下载 URL。

## Acceleration 检测

```rust
pub enum Acceleration {
    CpuOnly, Cuda12, Cuda13, Directml, Tensorrt,
}
```

运行时检测：
1. TensorRT（检查 `TENSORRT_PATH` 或默认路径）
2. CUDA（检查 `CUDA_PATH` 或 `/usr/local/cuda`）
3. DirectML（Windows 平台默认）
4. CPU Only

## 依赖关系

```
Runtime
↑ 不依赖 Pipeline
↓ 被 Inference, Engine 依赖
```
