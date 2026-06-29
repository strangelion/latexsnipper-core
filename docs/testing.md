# 测试规范

> 统一测试文件，不分散在各 crate 中。

## 测试文件位置

```
crates/tests/src/
├── all_tests.rs          # 统一测试（39 项）
└── dual_track_tests.rs   # Mock vs ONNX 对照测试（3 项）
```

## 当前测试覆盖

| 测试组 | 测试数 | 覆盖内容 |
|---|---|---|
| Foundation | 5 | Error/Result/Config/EventBus |
| AST | 3 | Document/Block/Formula/Rect/Serialization |
| Tensor | 3 | Float32/Int64/Serialization |
| Image | 5 | New/Resize/Letterbox/Normalize/Crop |
| Model | 3 | Config parse/Manifest validate/Manager paths |
| Syntax | 3 | LaTeX parse/render/roundtrip |
| Export | 3 | RenderTree/SVG/Text generator |
| Mock | 3 | FakePipeline/FakeDocument |
| Engine | 3 | Mock recognize/Mock→AST→Export/Parse→Export |
| Runtime | 3 | StubRuntime/ModelHandle/AccelerationMode |
| FFI | 2 | Response serialization |
| Dual-track | 3 | Runtime init/Consistency/Mode distinction |
| **总计** | **42** | |

## 运行测试

```bash
# 统一测试（推荐）
cargo test -p latexsnipper-tests

# Dual-track 对照测试
cargo test -p latexsnipper-tests --test dual_track

# 显示输出
cargo test -p latexsnipper-tests -- --nocapture
```

## 测试约定

1. 所有测试集中在 `crates/tests/src/`，不在各 crate 中
2. `all_tests.rs`：模块级单元测试
3. `dual_track_tests.rs`：Mock vs ONNX 对照测试
4. 测试文件不打包进成品（.gitignore 已配置）
5. 新增功能必须同步添加测试

## 文件排除

`.gitignore` 配置：
- `target/` — 构建产物
- `test-models/` — ONNX 模型文件
- `fixtures/*.png` — 测试图片
- `fixtures/*.typ` — Typst 源文件
