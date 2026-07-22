# Native Core ML Runtime

`latexsnipper-runtime-coreml` 运行原生 `.mlmodel`、`.mlpackage` 与 `.mlmodelc`。它是独立
Runtime，不等同于 ONNX Runtime 的 CoreML Execution Provider，也不等同于 ExecuTorch
的 Core ML backend。ONNX 来源模型仍优先使用 ORT CoreML EP；这个 crate 面向模型包中
原生携带的 Apple 模型。

## 构建与平台边界

Engine 的 `coreml` feature 默认关闭：

```toml
latexsnipper-engine = { version = "3", features = ["native", "coreml"] }
```

Apple target 会编译 `crates/runtime-coreml/native/coreml_bridge.mm`，并链接系统 CoreML 与
Foundation frameworks。Windows/Linux 不编译 bridge、不链接 Apple framework，但仍注册
`CoreMlFactory`；其 probe 明确返回“当前平台不可用”，manifest resolver 只能使用模型包
声明的 fallback。

## Manifest

源码模型包：

```json
{
  "id": "apple-recognizer",
  "runtimeVariants": [
    {
      "id": "coreml-native",
      "runtime": "coreml",
      "status": "stable",
      "priority": 100,
      "artifacts": { "package": "Recognizer.mlpackage" },
      "options": { "computeUnits": "all" }
    },
    {
      "id": "onnx-fallback",
      "runtime": "onnx-runtime",
      "status": "stable",
      "priority": 50,
      "artifacts": { "model": "Recognizer.onnx" }
    }
  ]
}
```

`artifacts` 接受：

- `model`: `.mlmodel` 源码模型或 `.mlmodelc` 编译模型；
- `package`: `.mlpackage` 源码包；
- `compiled`: `.mlmodelc` 编译模型。

扩展名与文件系统类型严格匹配：`.mlmodel` 必须是普通文件，`.mlpackage` 和 `.mlmodelc`
必须是目录；符号链接会被拒绝。

## 编译缓存

`.mlmodel` / `.mlpackage` 通过系统 `MLModel.compileModel(at:)` 编译，再从 `.mlmodelc`
加载。默认缓存键包含：

- 完整模型/包内容（目录项按名称排序后递归 SHA-256）；
- cache schema；
- Apple OS/Core ML 运行环境字符串；
- CPU 架构。

发布缓存通过同目录临时模型和 rename 完成。`cache: false` 时，临时 `.mlmodelc` 由 session
持有，session 销毁时只清理经过路径边界校验的私有临时目录。

```json
{
  "runtime": "coreml",
  "options": {
    "computeUnits": "cpu-neural-engine",
    "cache": true,
    "cacheDir": "/application/cache/coreml"
  }
}
```

`computeUnits` 支持 `all`、`cpu-only`、`cpu-gpu`、`cpu-neural-engine`；公共
`DeviceKind::{Auto,Cpu,Gpu,Npu}` 会分别映射到对应策略。Native Core ML 没有 ORT 风格的
provider fallback 链，因此互相冲突的 provider、device 与 `computeUnits` 会直接报错。

## Tensor 与并发契约

第一版只接受 `MLMultiArray`，对应公共 Tensor 的 `f32`、`f16` 与 `i32`。image、
dictionary、sequence，以及 `i64/u8/bool` 输入会明确报错。bridge 按 shape/strides 做
row-major CPU copy，因此不假设 Core ML 输出连续，也不跨 ABI 共享 Apple/Rust 内存所有权。

每个 native session 拥有一个 `MLModel` 和专用 serial dispatch queue。Rust 侧另有 mutex，
保证一个模型实例不会被多个 worker 并发调用。需要并行吞吐时应创建多个 session；未来可在
这一约束上增加 session pool。

## Parity 验证

macOS CI 会编译、链接并运行 crate 测试。真实模型 parity 是显式的模型资产测试：

```bash
LATEXSNIPPER_COREML_PARITY_MODEL=/models/Recognizer.mlpackage \
LATEXSNIPPER_COREML_PARITY_CASE=/models/recognizer-case.json \
cargo test -p latexsnipper-runtime-coreml --test native_runtime -- --nocapture
```

case 由官方 Python/Core ML 参考路径产生：

```json
{
  "inputs": {
    "x": { "dtype": "f32", "shape": [1, 2], "values": [1.0, 2.0] }
  },
  "expected": {
    "y": { "dtype": "f32", "shape": [1, 2], "values": [2.0, 4.0] }
  },
  "atol": 0.00001
}
```

测试比较输出名、shape、dtype，并对 f32 检查最大绝对误差；f16 bit pattern 与 i32 精确比较。
