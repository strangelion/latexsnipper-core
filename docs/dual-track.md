# Dual-Track Testing（Mock vs ONNX 对照测试）

> 自动验证 Mock Runtime 和 ONNX Runtime 的行为一致性。

## 目标

```text
MockRuntime ───► Document
ONNXRuntime ──► Document
        ↓ diff
   一致性报告
```

## 测试内容

| 测试 | 说明 |
|---|---|
| dual_runtime_initialization | 两种 Runtime 都能初始化 |
| mock_consistency | Mock 多次运行结果一致 |
| mode_distinction | 不同模式产生不同 Document 结构 |

## 运行

```bash
cargo test -p latexsnipper-tests --test dual_track -- --nocapture
```

## 扩展方向

未来可添加：
- 相同输入图片，对比 Mock 和 ONNX 的输出 JSON
- 性能基准测试（Mock vs ORT 推理速度）
- 模型替换回归测试（换模型后 diff 结果）
