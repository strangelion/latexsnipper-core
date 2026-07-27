# WASM adapter

状态：语义转换稳定；浏览器推理与模型内存 API 为 experimental。Text、table 和 handwriting 已有可执行浏览器流水线，但 OCR accuracy 尚未进入生产门禁。

纯 WASM 构建使用 Tract，不链接 ONNX Runtime、PDF/PNG/Office native exporter、文件系统下载器或 native Tokio runtime。`capabilities_v2()` 是当前浏览器构建和已加载模型 readiness 的事实来源。

## v2 API

- `api_info_v2()` / `capabilities_v2()`：返回 API、schema、capability 版本。
- `load_model_v2()`：校验 SHA-256；ONNX artifact 在上线前由 Tract 解析。
- `begin_model_update_v2()` / `commit_model_update_v2()` / `rollback_model_update_v2()`：批量事务更新。
- `unload_model_v2()` / `clear_models_v2()`：释放 artifact。
- `recognize_v2()`：真正的 async Promise API，直接 await engine。
- `recognize_v2_with_progress()`：在 validation、model readiness、inference、completion 边界报告进度。
- `cancel_recognition_v2()`：stage-boundary cooperative cancellation。
- `convert_v2()`：返回 text/binary-safe artifact；若未来启用 binary exporter，bytes 以 `Uint8Array` 返回。

所有 v2 调用返回稳定 envelope：`ok`、`apiVersion`、`capabilityVersion`、`coreVersion`、`schemaVersion`、`diagnostics`，以及 `data` 或结构化 `error`。

## 能力边界

Formula、croppedFormula、text、mixed 和 formula-layout 只有在所需
config、model、tokenizer/keys artifact 完整时才会报告 ready。
croppedFormula 只要求 formula-rec artifact，不要求 formula-det。

Table 流水线要求 text-rec profile：

- 没有显式 table-struct artifact 时使用内置 `table-struct/projection`，执行 region -> crop/normalize -> structure -> geometry -> cell text -> `TableBlock`。
- 若加载 table-struct profile，则 config、model、preprocessing 和 I/O metadata 必须全部通过验证。
- table detector 是可选项；没有 detector 时，显式 table 请求把完整输入图像作为 region。
- 浏览器默认使用 projection structure 与 production-compatible text recognizer。它能保留 geometry、merged cell、empty cell、multilingual text、confidence 和 diagnostics，但复杂视觉表格结构仍是 best effort。

Handwriting 流水线要求 formula-rec encoder/decoder、tokenizer、preprocessing、decoding 和 I/O metadata：

- handwriting detector 是可选项；没有 detector 时，显式 handwriting 请求把完整输入图像作为 region。
- 实际路径为 region -> preprocess -> recognizer -> tokenizer/keys -> postprocess -> AST。
- 浏览器端单次解码最多 32 tokens，native 上限为 256 tokens。

当前 production model 边界：

- MIT-licensed TrOCR encoder/decoder/tokenizer 已通过真实 Tract session 和一次真实 decoder step；PR CI 从固定的 `models-v2.0.0/latexsnipper-formula-rec.zip` 下载并校验 SHA-256 `fdcc414b9073c73325614faafc6473ddd7e91a80305cdc55f7267195de152a21`。这证明 runtime compatibility，不等于 handwriting accuracy 门禁。
- Table Transformer combined model 能由 Tract 编译，但在 256/384 输入下超过 120 秒浏览器 hard-timeout，因此不作为默认 production browser profile。
- 当前 SLANet/PPStructure mobile ONNX 导出包含 Tract 尚不支持的 `Loop`，不会被虚假报告为 ready。

PDF、PNG、DOCX、PPTX、XLSX native exporter 被明确隔离，不会在 WASM 中虚假报告支持。

## Worker、取消与恢复

`crates/wasm/js` 提供官方 `WasmWorkerClient`。它维护单并发有界队列、public request ID、独立 internal wire ID、progress event 和 stale-response suppression，用户 ID 不会与 control/inference RPC 命名空间碰撞。

`error`、`messageerror`、协议损坏、RPC timeout、task timeout、AbortSignal 和 `postMessage` 失败会终止旧 worker、拒绝受影响的 active/RPC 请求、创建新 worker、重新加载已校验模型，然后继续队列。若 recovery worker 再次失败，剩余队列会以结构化错误终止，避免无限重启。

取消 active inference 会 terminate worker 并丢弃 session，这是浏览器 hard cancellation。发送给 worker 的 cooperative-cancel 消息与 inference handler 串行，不能抢占当前同步计算，只能取消 queued 请求或在 stage boundary 被观察；直接调用主线程 API 同样只有 stage-boundary cooperative cancellation，并会发出一次阻塞 UI 警告。

## 缓存与下载

可选 IndexedDB cache 使用 schema/namespace version 2，记录 artifact key、SHA-256、profile、source URL、byte length 和 last-used time。读取与写入均验证 SHA-256；支持 per-model clear、全量 clear、LRU budget eviction、quota error 和 schema migration。每次操作都会关闭连接，数据库 version change 也会主动关闭旧连接。

能力元数据按当前浏览器是否存在 IndexedDB 动态报告，不能把 Node 或禁用存储的环境报告为 available。默认 cache budget 为 512 MiB。

Incremental downloader 支持 `Content-Length` 与未知长度、maximum size、AbortSignal、progress、mirror fallback 和 structured error。Partial bytes 不会写 cache 或激活；完整下载通过 SHA-256 后才可进入 cache 与 `MemoryModelResolver`。默认 `best-effort` cache policy 下，quota/写入失败只产生结构化 warning；只有显式选择 `required` 才把 cache 写入失败视为下载失败。本地 `Uint8Array` 加载不依赖 downloader。

## 浏览器资源预算

Rust balanced 默认预算：单模型 128 MiB、总模型 256 MiB、图像最大 8192 x 8192、最多 40,000,000 pixels、4096 table elements、16 MiB serialized result。Low-memory profile 使用 64 MiB、128 MiB、4096 x 4096、16,000,000 pixels、2048 table elements、8 MiB result。

Worker client 默认预算：queue 32、RPC timeout 30 秒、task timeout 120 秒、单模型 128 MiB、总模型 256 MiB、图像最大 8192 x 8192 / 40,000,000 pixels、result 16 MiB。所有自定义预算必须是正安全整数；图像、模型、下载和结果会在 clone、allocation 或 `postMessage` 前检查。Caller 还可为每次下载设置独立 `maxBytes`。

## 模型验证等级

- Deterministic synthetic fixtures：真实执行 ONNX bytes -> resolver -> Tract -> tensor -> OCR pipeline -> Document AST -> serialization，用于确定性单元与浏览器回归；不是生产权重。
- Production-derived compatibility smoke：scheduled workflow 从官方固定 revision 下载 `PaddlePaddle/PP-LCNet_x1_0_doc_ori_onnx`（Apache-2.0，SHA-256 `af9a0a4f317ff0709ce752067807f819cb15d883f8ecad89f28df1c6ee2d9c92`），以真实 PNG 在 Tract/WASM 执行 `[1,3,224,224] -> [1,4]`。它记录 model/session/first/warm/memory 指标，只证明 production-model compatibility，不证明 OCR accuracy。
- Production handwriting compatibility：固定 TrOCR encoder、decoder、tokenizer artifact，建立真实 session 并执行 encoder 与一次 decoder step；用于防止 exporter/runtime 回归，不代替 corpus accuracy 评估。

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
