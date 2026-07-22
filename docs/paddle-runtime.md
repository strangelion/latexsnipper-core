# Paddle Inference Runtime

Paddle Inference 是可选的 Native Runtime，默认构建不启用。它执行官方完整
Paddle inference program，包括 PP-FormulaNet 的 encoder、while、并行生成与 KV
cache；最终用户不需要 Python 或 PaddlePaddle Python wheel。

## 构建与启用

启用 Rust 集成：

```text
cargo build -p latexsnipper-engine --features paddle
```

`paddle` feature 默认关闭，因此普通 ONNX 构建不会链接或加载 Paddle。

官方预编译 SDK 提供 C++ API。LaTeXSnipper 使用
`crates/runtime-paddle/native` 中的薄 C ABI v1 bridge 隔离 C++ ABI。发布人员可将解压的
SDK 打包为运行时目录：

```text
python scripts/package_paddle_runtime.py \
  --sdk path/to/paddle-inference-sdk \
  --output dist/runtime/paddle
```

Windows 使用较新 Visual Studio generator 时，可通过 `--cmake` 指向 Visual Studio
自带的 CMake。输出目录包含 bridge、Paddle 动态库、依赖动态库、`version.txt` 和带
SHA-256 的 `runtime-manifest.json`。应用发布时可将该目录复制到：

```text
resources/runtime/paddle/
```

## 运行时发现

发现顺序固定为：

1. `RuntimeOptions.extra.libraryPath` / `paddleHome` 或 Factory 显式路径；
2. `LATEXSNIPPER_PADDLE_HOME`；
3. 可执行文件旁的 `resources/runtime/paddle/`；
4. 系统动态库搜索路径。

找不到 bridge 或依赖库时，`RuntimeProbe` 返回 unavailable 和完整尝试路径，不影响
ONNX Runtime。成功加载的 Paddle 动态库保留到进程退出，避免 Paddle/oneDNN 的全局
worker 在线程退出与卸载阶段发生竞态；Predictor 和 Session 仍按各自生命周期销毁。

## 模型 manifest

Fallback 必须由模型包显式声明：

```json
{
  "runtimeVariants": [
    {
      "id": "paddle-native",
      "runtime": "paddle-inference",
      "status": "stable",
      "priority": 100,
      "artifacts": {
        "model": "inference.json",
        "params": "inference.pdiparams",
        "tokenizer": "tokenizer.json"
      },
      "fallbacks": ["onnx-fullseq"]
    },
    {
      "id": "onnx-fullseq",
      "runtime": "onnx-runtime",
      "status": "experimental",
      "priority": 50,
      "artifacts": {
        "encoder": "encoder_model.onnx",
        "decoder": "decoder_fullseq_parallel.onnx",
        "tokenizer": "tokenizer.json"
      }
    }
  ]
}
```

Paddle 加载或 probe 失败时，resolver 只遍历这里声明的 fallback，不会选择任意 ONNX
文件。

## 数据与并发约束

第一版使用 CPU copy，支持 `f32`、`f16`、`i64`、`i32`、`u8` 和 `bool`。每个
`PaddleSession` 串行访问一个 Predictor。Paddle 3.0 Windows CPU 的 oneDNN scale
kernel 在 PP-FormulaNet 完整 while 图中会对非平凡序列抛异常，因此 bridge 默认关闭
oneDNN，保留普通 IR 优化和 portable CPU kernel；oneDNN 只能在后续按模型验证后显式
启用。

## PP-FormulaNet 模型准备与验证

模型准备发生在开发/打包阶段：

```text
python scripts/prepare_ppfn_paddle_inference.py
```

该脚本下载并校验官方 Paddle 3.0 完整 inference program。严格 parity 使用同一批预处理
张量比较官方 Python Paddle 和 Rust Native Runtime：

```text
python scripts/validate_ppfn_paddle_parity.py \
  --runtime-home dist/runtime/paddle \
  --count 20
```

Native smoke test是 opt-in，普通 CI 不要求本机安装 Paddle：

```text
LATEXSNIPPER_PADDLE_HOME=dist/runtime/paddle \
LATEXSNIPPER_PPFN_MODEL_HOME=models/formula-rec/pp-formulanet-s \
cargo test -p latexsnipper-runtime-paddle --test native_runtime
```
