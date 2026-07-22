# Native TensorRT Runtime

Native TensorRT 用于需要直接构建/加载 TensorRT engine 的 NVIDIA 部署。普通 ONNX 模型
应优先使用 [ONNX Runtime TensorRT EP](onnx-tensorrt-ep.md)；只有预构建策略、动态 profile、
插件或极致性能需要脱离 ORT 时，manifest 才声明 `runtime: "tensorrt"`。

## 构建与发现

`runtime-tensorrt` 不链接 TensorRT。应用默认不启用 `tensorrt` feature；启用后也只注册
factory，真正的 bridge 在运行时按顺序发现：

1. factory/variant 的 `libraryPath`
2. `LATEXSNIPPER_TENSORRT_HOME`
3. 应用 `resources/runtime/tensorrt/`
4. 系统动态库搜索路径

当前 bridge 固定使用版本化 C ABI v1，并针对 TensorRT 10.x C++ ABI 构建。普通 workspace
构建和没有 NVIDIA 驱动的机器不需要 CUDA/TensorRT SDK。

TensorRT-RTX 使用独立的 `RuntimeKind::TensorRtRtx`、bridge 和 `.rtxplan`，详见
[TensorRT-RTX Runtime](tensorrt-rtx-runtime.md)。两种 bridge 会校验 runtime id，不能混装。

```powershell
python scripts/package_tensorrt_runtime.py `
  --runtime tensorrt `
  --tensorrt-root C:\SDK\TensorRT-10.16 `
  --cuda-root "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.9" `
  --output dist\runtime\tensorrt
```

打包脚本验证 SDK major version、构建 bridge、复制 TensorRT/ONNX parser/plugin/CUDA runtime
依赖，并生成包含 SHA-256 的 `runtime-manifest.json`。最终用户运行时不需要 Python/CMake。

## Manifest

静态 ONNX 模型可首次运行本机构建 engine：

```json
{
  "id": "native-tensorrt",
  "runtime": "tensorrt",
  "artifacts": { "source": "model.onnx" },
  "options": {
    "device": "gpu",
    "precision": "fp16",
    "workspaceBytes": 4294967296,
    "cache": true
  }
}
```

动态输入必须声明 profile，不允许猜 shape：

```json
{
  "options": {
    "profiles": {
      "input": {
        "min": [1, 1, 128, 128],
        "opt": [1, 1, 384, 384],
        "max": [4, 1, 1024, 1024]
      }
    }
  }
}
```

缺失动态 input、rank 不一致、修改静态维度或不满足 `min <= opt <= max` 都会明确失败。
动态 shape tensor 需要值 profile，不能从维度 profile 推导，v1 bridge 会明确拒绝该模型。

也可声明 `artifacts.engine`/`.plan` 加载预构建 engine，但 engine 是可执行的 CUDA artifact，
只应加载可信来源。默认 engine 绑定构建时 TensorRT 版本、平台和 GPU；不要在模型包中把它
当作通用跨机器 artifact。

## Cache

从 ONNX 构建时，本地 cache key 包含：

- ONNX SHA-256
- TensorRT runtime 完整版本
- GPU 名称、PCI identity、compute capability 与显存指纹
- precision、workspace 和完整 optimization profiles

cache 文件原子发布；空文件或反序列化失败会失效并重新构建。默认位置是操作系统用户缓存
目录，可用 `cacheDir` 覆盖。cache 只在本机使用，不进入模型包。

## Tensor 与执行

bridge 使用 tensor name、`setTensorAddress` 和 `enqueueV3`。支持 `f32/f16/i64/i32/u8/bool`
CPU copy；一个 execution context 由 Rust mutex 串行调用。可预计算的动态输出按 context shape
分配，数据依赖动态输出通过 `IOutputAllocator` 分配，不做 zero-copy。

## 硬件 parity

在装有 NVIDIA GPU、TensorRT 10 runtime 和打包 bridge 的机器上运行：

```powershell
$env:LATEXSNIPPER_TENSORRT_TEST_RUNTIME = "dist\runtime\tensorrt"
$env:LATEXSNIPPER_TENSORRT_TEST_MODEL = "crates\wasm\tests\fixtures\tiny-text-rec.onnx"
cargo test -p latexsnipper-runtime-tensorrt --test native_runtime -- --nocapture
```

测试会比较 ONNX Runtime CPU 与 Native TensorRT 的输出 shape/name，并要求 f32
`max_abs_error <= 1e-4`。未配置环境变量时普通 CI 不加载任何 NVIDIA 动态库。
