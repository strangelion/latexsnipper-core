# TensorRT-RTX Runtime

TensorRT-RTX 是面向 Windows/Linux RTX PC 的独立 Runtime，不是传统 TensorRT 的模式，也不是
ONNX Runtime Execution Provider。manifest 使用 `runtime: "tensorrt-rtx"`，Engine 仅在
`tensorrt-rtx` feature 开启且独立 RTX bridge probe 成功时选择该 variant。

TensorRT-RTX 采用两阶段编译：AOT 生成 JIT-able `.rtxplan`，创建 execution context 时再针对
当前 RTX GPU 执行 JIT。它与传统 `.engine`/`.plan` 不兼容。LaTeXSnipper 的第一版 bridge 使用
`ComputeCapability::kCURRENT` 做 on-device AOT，因此本地 engine cache 仍按 GPU 指纹隔离；更新
GPU 或 TensorRT-RTX 版本会产生新 cache key。

## 构建与发现

SDK 1.5+ 需要单独从 NVIDIA Developer Program 下载。bridge 使用相同的 LaTeXSnipper C ABI，
但导出 `runtime_id = "tensorrt-rtx"`，Rust loader 会拒绝误装的传统 TensorRT bridge。

```powershell
python scripts/package_tensorrt_runtime.py `
  --runtime tensorrt-rtx `
  --tensorrt-root C:\SDK\TensorRT-RTX-1.5.0.114 `
  --cuda-root "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3" `
  --output dist\runtime\tensorrt-rtx
```

运行时按以下顺序发现：

1. factory/variant 的 `libraryPath`
2. `LATEXSNIPPER_TENSORRT_RTX_HOME`
3. 应用 `resources/runtime/tensorrt-rtx/`
4. 系统动态库搜索路径

普通构建不链接 NVIDIA SDK；未安装 RTX bridge 时 probe 返回 unavailable，不影响 ONNX 或传统
TensorRT。

## Manifest

从强类型 ONNX 在本机生成并缓存 RTX engine：

```json
{
  "id": "rtx-pc",
  "runtime": "tensorrt-rtx",
  "artifacts": { "source": "model.onnx" },
  "options": {
    "device": "gpu",
    "cache": true,
    "cacheDir": "runtime-cache/tensorrt-rtx",
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

也可声明 `artifacts.engine: "model.rtxplan"`。模型包必须明确选择 RTX variant，不能把传统
TensorRT engine 当 fallback 互换加载。

TensorRT-RTX 1.5 使用强类型网络；FP16/INT8/FP8 等精度必须编码在 ONNX tensor/Q-DQ 图中。
`precision: "fp16"` 和 `precision: "int8"` 是传统 TensorRT 10 的弱类型 builder 选项，RTX
factory 会明确拒绝，避免配置被静默忽略。

## Cache 与验证

ONNX AOT cache key 包含 runtime id、模型 SHA-256、TensorRT-RTX 完整版本、GPU/compute
capability 指纹、workspace 和全部 dynamic-shape profiles。context 创建负责 JIT；当前 bridge
不把 JIT kernel runtime cache 持久化到磁盘，因此首次启动可能有 JIT 延迟。

有 RTX GPU、SDK/runtime 1.5+ 和已打包 bridge 时运行：

```powershell
$env:LATEXSNIPPER_TENSORRT_RTX_TEST_RUNTIME = "dist\runtime\tensorrt-rtx"
$env:LATEXSNIPPER_TENSORRT_RTX_TEST_MODEL = "crates\wasm\tests\fixtures\tiny-text-rec.onnx"
cargo test -p latexsnipper-runtime-tensorrt --test native_runtime -- --nocapture
cargo test -p latexsnipper-engine --features tensorrt-rtx --test runtime_registry -- --nocapture
```

测试经真实 manifest/resolver/registry/session 执行，并与 ONNX Runtime CPU 比较输出，要求 f32
`max_abs_error <= 1e-4`。未配置环境变量时 CI 只验证 probe、配置、缓存域和 feature 隔离。

参考 NVIDIA 官方文档：

- [TensorRT-RTX architecture](https://docs.nvidia.com/deeplearning/tensorrt-rtx/latest/architecture/architecture-overview.html)
- [Porting from TensorRT](https://docs.nvidia.com/deeplearning/tensorrt-rtx/latest/inference-library/porting.html)
- [TensorRT-RTX 1.5 release notes](https://docs.nvidia.com/deeplearning/tensorrt-rtx/latest/getting-started/release-notes-1/1.5.html)
