# ONNX Runtime TensorRT Execution Provider

TensorRT EP 是 ONNX Runtime 的执行加速层，不是独立 Runtime。模型 variant 仍声明
`runtime: "onnx-runtime"`，并通过有序 `providers` 列表明确加速与 fallback：

```json
{
  "runtime": "onnx-runtime",
  "options": {
    "providers": [
      {
        "name": "tensorrt",
        "options": {
          "device_id": 0,
          "fp16": true,
          "engine_cache": true,
          "engine_cache_path": "runtime-cache/tensorrt",
          "timing_cache": true,
          "timing_cache_path": "runtime-cache/tensorrt/timing"
        }
      },
      { "name": "cuda" },
      { "name": "cpu" }
    ]
  }
}
```

顺序就是优先级，CPU 如果存在必须位于链尾且 provider 不可重复。TensorRT 不可用时
只会尝试后续声明的 CUDA 和 CPU；列表没有 CPU 时，Core 会关闭 ORT 的隐式 CPU
fallback，无法完整放置到 GPU 的图会明确加载失败。每个原生 EP 的注册错误会进入 runtime
诊断，而不会被伪装成该 EP 已成功启用。

## TensorRT options

支持以下易读别名，并在创建 session 时转换为 ORT 的 `trt_*` provider option：

- `device_id`
- `fp16`, `int8`, `dla`, `dla_core`
- `max_workspace_size`, `min_subgraph_size`, `max_partition_iterations`
- `engine_cache`, `engine_cache_path`, `engine_cache_prefix`
- `timing_cache`, `timing_cache_path`, `force_timing_cache`
- `force_sequential_engine_build`, `context_memory_sharing`
- `detailed_build_log`, `build_heuristics`, `sparsity`
- `builder_optimization_level`, `auxiliary_streams`, `tactic_sources`
- `extra_plugin_lib_paths`
- `profile_min_shapes`, `profile_opt_shapes`, `profile_max_shapes`
- `cuda_graph`

原生 ORT `trt_*` key 也可直接使用。值必须是字符串、数字或布尔值，未知别名和嵌套
JSON 会被拒绝，避免拼写错误静默失效。动态 shape 的 profile 使用 ORT 字符串格式，
例如 `input:1x1x128x128`。

## Engine cache

TensorRT EP cache 目录属于本机运行时缓存，不能作为通用模型 artifact 分发。部署时应让
每台机器使用自己的可写目录；TensorRT、GPU、precision/profile 或模型发生变化时由 ORT/
TensorRT 重新构建。模型包只声明策略和相对缓存位置，不携带其他机器生成的 engine。

## Benchmark

仓库提供单输入 f32 ONNX 模型的可重复 benchmark：

```powershell
cargo run -p latexsnipper-runtime --example onnx_provider_benchmark -- `
  crates/wasm/tests/fixtures/tiny-text-rec.onnx `
  '[{"name":"tensorrt","options":{"fp16":true,"engine_cache":true,"engine_cache_path":"runtime-cache/tensorrt"}},{"name":"cuda"},{"name":"cpu"}]' `
  x 1x3x48x320 10 100
```

输出 JSON 包含 provider 链、warmup/iteration 数以及 mean、p50、p95 延迟。对比 CPU 时将
provider JSON 改为 `[{"name":"cpu"}]`，保持模型、输入、warmup 和 iteration 完全相同。
