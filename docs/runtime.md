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
| `model_handle` | model_handle.rs | ModelHandle（替代 Path，支持 bytes 加载） |
| `model_resolver` | model_resolver.rs | ModelResolver trait + FsModelResolver + MemoryModelResolver |
| `model_package` | model_package.rs | ModelPackage/ModelExecutor traits + ModelTask + ModelDescriptor |
| `model_registry` | model_registry.rs | ModelRegistry + ModelManifest (TOML) |
| `model_validation` | model_validation.rs | SHA-256 checksum + validation |
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
    model_bytes: Option<Vec<u8>>,
}
// methods: new(), with_path(), with_bytes(), id(), category(), variant(), model_path(), model_bytes()
```

`ModelHandle` 封装模型的 category/variant 信息，支持文件路径和内存字节两种加载方式。

## ModelResolver

统一模型解析接口，消除管道节点的硬编码路径：

```rust
pub trait ModelResolver: Send + Sync {
    fn resolve(&self, id: &ModelId) -> Result<ModelHandle>;
    fn is_available(&self, id: &ModelId) -> bool;
}
```

| 实现 | 说明 |
|------|------|
| `FsModelResolver` | 原生端：从文件系统解析 |
| `MemoryModelResolver` | WASM 端：从内存存储解析 |

## ModelPackage / ModelExecutor

模型包架构的核心 trait，允许自定义模型集成而无需修改 pipeline 代码：

```rust
pub trait ModelPackage: Send + Sync {
    fn descriptor(&self) -> &ModelDescriptor;
    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>>;
}

pub trait ModelExecutor: Send {
    fn run(&mut self, input: ModelInput, ctx: &mut InferenceContext) -> Result<ModelOutput>;
}
```

### 内建实现

| 适配器 | 任务 | 输入 | 输出 |
|--------|------|------|------|
| `YoloV8DetectorPackage` | FormulaDetection | RGB 图像字节 | `Detections` (bbox + confidence) |
| `TrOcrFormulaPackage` | FormulaRecognition | RGB 图像字节 | `Formula` (LaTeX + confidence) |
| `CrnnTextRecognizerPackage` | TextRecognition | RGB 图像字节 | `Text` (文本 + confidence) |

### 惰性会话加载

Executor 在首次 `run()` 调用时自动加载模型会话，无需预先加载：

```rust
let package = YoloV8DetectorPackage::from_config(&config, model_id)
    .with_model_path("models/formula-det/model.onnx".into());
let mut executor = package.create_executor(runtime)?;
// 首次 run() 时自动加载模型
let output = executor.run(input, &mut ctx)?;
```

### Pipeline 集成

Pipeline 节点优先使用 ModelPackage，回退到直接函数调用：

```rust
// 注册 ModelPackage
ctx.register_model_package(ModelTask::FormulaDetection, Arc::new(package));

// DetectorNode 会自动使用 ModelPackage（如果已注册）
// 否则回退到直接调用 detect_formulas()
```

## ModelRegistry

模型注册表，支持 TOML manifest：

```rust
let registry = ModelRegistry::from_dir("models")?;
let models = registry.find_by_task(ModelTask::TextRecognition);
```

## ModelValidation

SHA-256 完整性检查：

```rust
let report = validate_model(&path, expected_checksum)?;
assert!(report.valid);
```

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
