# ExecuTorch Runtime

ExecuTorch 是默认关闭的独立 Native Runtime，服务 PyTorch Edge 模型与 `.pte` 程序。
P2 的参考目标是 Windows x64 + XNNPACK；它不替代 ONNX Runtime，也不用于
PP-FormulaNet。

## 架构与启用

```text
ModelAdapter → RuntimeSession → versioned C ABI bridge → ExecuTorch Module → XNNPACK
```

启用 Rust 集成：

```text
cargo build -p latexsnipper-engine --features executorch
```

`executorch` feature 默认关闭。Rust crate 使用运行时动态发现，不链接 ExecuTorch C++
ABI。薄 bridge 位于 `crates/runtime-executorch/native`；Module API 变化只影响 bridge，
不会进入 Core API。

## SDK 与运行时打包

ExecuTorch SDK 必须在官方源码/固定 tag 上启用：

```text
EXECUTORCH_BUILD_XNNPACK=ON
EXECUTORCH_BUILD_EXTENSION_MODULE=ON
EXECUTORCH_BUILD_EXTENSION_TENSOR=ON
EXECUTORCH_BUILD_PORTABLE_OPS=ON
```

Windows x64 使用 Visual Studio x64 developer environment。完成 `cmake --install` 后：

```text
python scripts/package_executorch_runtime.py \
  --sdk path/to/executorch-sdk \
  --output dist/runtime/executorch \
  --generator Ninja
```

输出包含静态链接 ExecuTorch/XNNPACK 的 bridge、版本和 SHA-256 manifest。发布时复制到：

```text
resources/runtime/executorch/
```

最终用户不需要 Python、PyTorch、CMake 或 C++ SDK。

## 运行时发现与并发

发现顺序固定为：

1. `RuntimeOptions.extra.libraryPath` / `executorchHome` 或 Factory 显式路径；
2. `LATEXSNIPPER_EXECUTORCH_HOME`；
3. 可执行文件旁 `resources/runtime/executorch/`；
4. 系统动态库搜索路径。

找不到 runtime 时 `RuntimeProbe` 返回 unavailable，不影响默认 ONNX 构建。一个
ExecuTorch `Module` 不是线程安全的，因此每个 `ExecuTorchSession` 通过 mutex 串行执行；
第一版仅做 CPU copy，公共 tensor 支持 `f32/f16/i64/i32/u8/bool`。

## Manifest

`.pte` 是按目标平台和 delegate lowering 的程序，不是跨硬件通用模型：

```json
{
  "runtimeVariants": [
    {
      "id": "recognizer-xnnpack-win-x64",
      "runtime": "executorch",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "program": "recognizer-xnnpack-win-x64.pte"
      },
      "options": {
        "method": "forward"
      },
      "platforms": ["windows-x86_64"]
    }
  ]
}
```

Android XNNPACK/Vulkan/QNN 和 iOS XNNPACK/Core ML 应分别导出并声明自己的 variant。
resolver 只选择 manifest 明确声明的 variant/fallback。

## 导出与 parity

模型转换位于 `tools/model-export/executorch/`，只在开发/打包阶段执行：

```text
torch.export → Edge IR → XNNPACK lowering → .pte
```

参考 parity 同时验证 `forward` 与 `encode` named method：

```text
python tools/model-export/executorch/validate_parity.py \
  --runtime-home dist/runtime/executorch
```

opt-in native test 不加载 Python：

```text
LATEXSNIPPER_EXECUTORCH_HOME=dist/runtime/executorch \
LATEXSNIPPER_EXECUTORCH_PROGRAM=build/tiny-recognizer-xnnpack.pte \
cargo test -p latexsnipper-runtime-executorch --test native_runtime
```
