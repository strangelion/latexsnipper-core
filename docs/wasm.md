# WASM adapter

状态：语义转换稳定；浏览器推理与模型内存 API 为 experimental。

纯 WASM 构建使用 Tract，不链接 ONNX Runtime、PDF/PNG/Office native
exporter、文件系统下载器或 native Tokio runtime。`capabilities_v2()` 是当前
浏览器构建和已加载模型 readiness 的事实来源。

## v2 API

- `api_info_v2()` / `capabilities_v2()`：返回 API、schema、capability 版本。
- `load_model_v2()`：校验 SHA-256；ONNX artifact 在上线前由 Tract 解析。
- `begin_model_update_v2()` / `commit_model_update_v2()` /
  `rollback_model_update_v2()`：批量事务更新。
- `unload_model_v2()` / `clear_models_v2()`：释放 artifact。
- `recognize_v2()`：真正的 async Promise API，直接 await engine。
- `recognize_v2_with_progress()`：在 validation、model readiness、inference、
  completion 边界报告进度。
- `cancel_recognition_v2()`：stage-boundary cooperative cancellation。
- `convert_v2()`：返回 text/binary-safe artifact；若未来启用 binary exporter，
  bytes 以 `Uint8Array` 返回。

所有 v2 调用返回稳定 envelope：`ok`、`apiVersion`、`capabilityVersion`、
`coreVersion`、`schemaVersion`、`diagnostics`，以及 `data` 或结构化 `error`。

## 能力边界

Formula、text、mixed 和 formula-layout 只有在所需 config、model、tokenizer/
keys artifact 完整时才会报告 ready。Table 与 handwriting 浏览器 pipeline 尚未实现，
因此始终 unavailable。PDF、PNG、DOCX、PPTX、XLSX native exporter 被明确隔离，
不会在 WASM 中虚假报告支持。

IndexedDB cache 和 incremental download 尚未实现。主线程执行大模型会阻塞页面，
生产集成应在 Web Worker 中加载 package 并调用 async API。

## 验证

```bash
cargo check -p latexsnipper-engine --no-default-features --features wasm --target wasm32-unknown-unknown
cargo clippy -p latexsnipper-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings
wasm-pack build crates/wasm --target web --release --out-dir ../../target/wasm-web
wasm-pack test crates/wasm --headless --chrome
```
