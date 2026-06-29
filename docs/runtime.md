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

## OnnxRuntimeBackend

已实现，使用 ort crate 2.0.0-rc.12：

- `Environment::current()` 初始化 ORT 环境
- `Session::builder().commit_from_file()` 加载模型
- `Value::from_array()` 创建输入张量
- `session.run()` 执行推理
- `Mutex<Session>` 保证线程安全

## 依赖关系

```
Runtime
↑ 不依赖 Pipeline
↓ 被 Inference 依赖
```