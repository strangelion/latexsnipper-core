# ExecuTorch Model Export

模型转换只发生在开发/模型打包阶段：

```text
PyTorch nn.Module → torch.export → Edge IR → XNNPACK lowering → .pte
```

`export_smoke_model.py` 导出固定权重的微型 recognizer，同时包含 `forward` 和
`encode` 两个 named method。它用于验证 Windows x64 + XNNPACK Runtime，不是
PP-FormulaNet 的替代实现。

```text
python tools/model-export/executorch/export_smoke_model.py \
  --output build/tiny-recognizer-xnnpack.pte \
  --expected build/tiny-recognizer-reference.json
```

中文 Windows 如果 Python 未默认启用 UTF-8，先设置 `PYTHONUTF8=1`。导出脚本会自动
发现版本匹配 wheel 中的 `flatc`，也可用 `FLATC_EXECUTABLE` 显式覆盖。

端到端 parity（当前 Python 环境需安装版本匹配的 PyTorch/ExecuTorch；最终用户不需要）：

```text
python tools/model-export/executorch/validate_parity.py \
  --runtime-home dist/runtime/executorch
```

不同 delegate、OS、架构应导出不同 `.pte`，不能把一个 XNNPACK Windows 程序当作
Vulkan、QNN 或 Core ML 的通用产物。
