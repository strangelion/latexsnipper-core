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

`crates/wasm/js` 提供官方 `WasmWorkerClient`。它维持单并发有界队列、public request ID、独立的
internal wire ID、progress event 和 stale-response suppression，用户 ID 不会与 control/inference RPC
命名空间碰撞。`error`、`messageerror`、协议损坏、RPC timeout、AbortSignal 和 postMessage 失败会终止旧
worker、拒绝受影响的 active/RPC 请求、创建新 worker、重新加载已校验模型，然后继续队列；若 recovery
worker 再次失败，剩余队列会以结构化错误终止，避免无限重启。

取消 active inference 会 terminate worker 并丢弃 session，这是浏览器 hard cancellation。发送给 worker
的 cooperative-cancel 消息与 inference handler 串行，不能抢占当前同步计算，只能取消 queued 请求或在
stage boundary 被观察；直接调用主线程 API 同样只有 stage-boundary cooperative cancellation，并会发出
一次阻塞 UI 警告。

可选 IndexedDB cache 使用 schema/namespace version 2，记录 artifact key、SHA-256、profile、
source URL、byte length 和 last-used time。读取与写入均验证 SHA-256；支持 per-model clear、
全量 clear、LRU budget eviction、quota error 与 schema migration。每次操作都关闭连接，数据库
version change 也会主动关闭旧连接。能力元数据按当前浏览器是否存在 IndexedDB 动态报告，不能将 Node
或禁用存储的环境报告为 available。

incremental downloader 支持 `Content-Length` 与未知长度、maximum size、AbortSignal、progress、
mirror fallback 和 structured error。partial bytes 不会写 cache 或激活；完整下载通过 SHA-256
后才可进入 cache 与 `MemoryModelResolver`。默认 `best-effort` cache policy 下，quota/写入失败只产生
结构化 warning，不会让已验证下载失败或切换 mirror；只有显式选择 `required` 才把 cache 写入失败视为
下载失败。本地 `Uint8Array` 加载不依赖 downloader。

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
