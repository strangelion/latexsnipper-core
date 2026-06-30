# 测试规范

> 统一测试文件，不分散在各 crate 中。

## 测试文件位置

```
crates/tests/src/
├── all_tests.rs          # 模块级单元测试
├── real_model_tests.rs   # 真实模型端到端测试（需要 test-models/）
├── contract_tests.rs     # API 契约测试
└── dual_track_tests.rs   # Mock vs ONNX 对照测试
```

```
crates/tests/benches/
├── core_bench.rs         # 核心模块基准测试
└── recognition_bench.rs  # 三种识别模式基准测试（harness=false）
```

## 当前测试覆盖（151 项）

| 测试组 | 测试数 | 覆盖内容 |
|---|---|---|
| Foundation | 5 | Error/Result/Config/EventBus |
| AST | 14 | Document/Block/Formula/Rect/Serialization/Visitor |
| Tensor | 2 | Float32/Int64/Serialization |
| Image | 40 | New/Resize/Letterbox/Normalize/Crop/Pad/Channel/View |
| Model | 5 | Config parse/Manifest validate/Manager paths/Quantization |
| Syntax | 4 | LaTeX parse/render/roundtrip/Typst |
| Export | 2 | RenderTree/SVG/Text generator |
| Inference | 5 | Formula detector/Text detector/Text recognizer/LaTeX repair/Text segmentation |
| Runtime | 4 | StubRuntime/ModelHandle/AccelerationMode/Session caching |
| FFI | 3 | Response serialization/iOS C FFI/Android JNI |
| Plugin | 2 | Plugin trait/Registry |
| Engine | 8 | Recognize/Mock→AST→Export/Parse→Export/JobQueue/Streaming |
| Real Model | 8 | Text det/rec/E2E, Formula det/rec/E2E, Doc-ori, Multi-model |
| Dual-track | 3 | Runtime init/Consistency/Mode distinction |
| Contract | 3 | API surface contracts |

## 运行测试

```bash
# 全部测试
cargo test --workspace

# 统一测试
cargo test -p latexsnipper-tests

# 真实模型测试（需要 test-models/）
cargo test -p latexsnipper-tests --test real_model -- --nocapture

# Dual-track 对照测试
cargo test -p latexsnipper-tests --test dual_track -- --nocapture

# 基准测试
cargo bench --bench recognition_bench -- --nocapture
```

## 测试约定

1. 所有测试集中在 `crates/tests/src/`，不在各 crate 中
2. `all_tests.rs`：模块级单元测试
3. `real_model_tests.rs`：真实模型端到端测试
4. `dual_track_tests.rs`：Mock vs ONNX 对照测试
5. `contract_tests.rs`：API 契约测试
6. 测试文件不打包进成品（.gitignore 已配置）
7. 新增功能必须同步添加测试

## 文件排除

`.gitignore` 配置：
- `target/` — 构建产物
- `test-models/` — ONNX 模型文件
- `fixtures/*.typ` — Typst 源文件
