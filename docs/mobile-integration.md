# Mobile 集成 Rust Core

> Java 后端 → Rust Core 的迁移路径

## 架构

```
Android App (Java/Kotlin)
  ↓ JNI
Rust Core (FFI crate)
  ↓
Engine + StubRuntime
  ↓
Document AST
```

## FFI 函数

| 函数 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `nativeInit` | models_dir | i32 | 初始化引擎 |
| `nativeRecognizeFormula` | data, len | *mut c_char (JSON) | 公式识别 |
| `nativeRecognizeText` | data, len | *mut c_char (JSON) | 文字识别 |
| `nativeRecognizeMixed` | data, len | *mut c_char (JSON) | 混合识别 |
| `nativeRelease` | — | void | 释放资源 |
| `nativeFreeString` | ptr | void | 释放字符串 |

## 迁移步骤

1. 在 LaTeXSnipper_mobile 中添加 Rust Core 作为依赖
2. 修改 NativeOcrBridge 调用 Rust FFI 函数
3. 保持 Java 端接口不变，JS 层无需修改
4. 用 StubRuntime 验证全流程
5. 替换 StubRuntime → OnnxRuntimeBackend

## JSON 响应格式

```json
{
  "done": true,
  "latex": "...",
  "text": "...",
  "confidence": 0.95,
  "error": null,
  "time_ms": 1234
}
```
