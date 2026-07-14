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

`crates/wasm/js` 提供官方 `WasmWorkerClient`。它维持单并发有界队列、request ID、
progress event 和 stale-response suppression。取消 active inference 会 terminate worker，
丢弃 session，并在下一请求创建 worker 后重新加载已校验模型；这是浏览器 hard cancellation。
直接调用主线程 API 仍只能在 stage boundary cooperative cancel，并会发出一次阻塞 UI 警告。

可选 IndexedDB cache 使用 schema/namespace version 2，记录 artifact key、SHA-256、profile、
source URL、byte length 和 last-used time。读取与写入均验证 SHA-256；支持 per-model clear、
全量 clear、LRU budget eviction、quota error 与 schema migration。能力元数据按当前浏览器
是否存在 IndexedDB 动态报告，不能将 Node 或禁用存储的环境报告为 available。

incremental downloader 支持 `Content-Length` 与未知长度、maximum size、AbortSignal、progress、
mirror fallback 和 structured error。partial bytes 不会写 cache 或激活；完整下载通过 SHA-256
后才可进入 cache 与 `MemoryModelResolver`。本地 `Uint8Array` 加载不依赖 downloader。

## 模型验证等级

- deterministic synthetic fixtures：真实执行 ONNX bytes -> resolver -> Tract -> tensor -> OCR
  pipeline -> Document AST -> serialization，用于确定性单元与浏览器回归；不是生产权重。
- production-derived compatibility smoke：scheduled workflow 从官方固定 revision 下载
  `PaddlePaddle/PP-LCNet_x1_0_doc_ori_onnx`（Apache-2.0，SHA-256
  `af9a0a4f317ff0709ce752067807f819cb15d883f8ecad89f28df1c6ee2d9c92`），以真实 PNG 在
  Tract/WASM 执行 `[1,3,224,224] -> [1,4]`。它记录 model/session/first/warm/memory 指标，
  只证明 production-model compatibility，不证明 OCR accuracy。

## 验证

```bash
cargo check -p latexsnipper-engine --no-default-features --features wasm --target wasm32-unknown-unknown
cargo clippy -p latexsnipper-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings
wasm-pack build crates/wasm --target web --release --out-dir ../../target/wasm-web
wasm-pack test crates/wasm --headless --chrome
wasm-pack test crates/wasm --headless --firefox
cd crates/wasm/js
npm ci && npm audit && npm run typecheck && npm test && npm run build:example
```
