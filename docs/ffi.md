# FFI Crate

> 外部函数接口 — Android JNI, iOS C FFI

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `common` | common.rs | FFIResponse, CStr 转换工具 |
| `android` | android/jni_bridge.rs | JNI 函数导出 |
| `ios` | ios/c_ffi.rs | C ABI 函数导出 |

## JNI 函数（Android）

| 函数 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `nativeInit` | models_dir: String | i32 | 初始化引擎 |
| `nativeRecognizeFormula` | data: byte[], len: int | String (JSON) | 公式识别 |
| `nativeRecognizeText` | data: byte[], len: int | String (JSON) | 文字识别 |
| `nativeRecognizeMixed` | data: byte[], len: int | String (JSON) | 混合识别 |
| `nativeRelease` | — | void | 释放资源 |
| `nativeFreeString` | ptr: long | void | 释放字符串 |

## C FFI 函数（iOS）

| 函数 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `latexsnipper_init` | models_dir: *const c_char | i32 | 初始化 |
| `latexsnipper_recognize_formula` | data, len | *mut c_char | 公式识别 |
| `latexsnipper_recognize_text` | data, len | *mut c_char | 文字识别 |
| `latexsnipper_recognize_mixed` | data, len | *mut c_char | 混合识别 |
| `latexsnipper_release` | — | void | 释放 |
| `latexsnipper_free_string` | ptr | void | 释放字符串 |

## FfiResponse JSON 格式

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

## 构建

```bash
# Android .so
cargo ndk -t arm64-v8a -o ./android/app/src/main/jniLibs build --release

# iOS .a
cargo build --release --target aarch64-apple-ios
```

## 依赖关系

```
FFI
↑ 依赖 Engine
```
