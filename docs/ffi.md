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
| `nativeResponseVersion` | — | u32 | 返回 FFI JSON contract version（当前为 3） |
| `nativeRecognizeFormula` | data: byte[], len: int | String (JSON) | 公式识别 |
| `nativeRecognizeCroppedFormula` | data: byte[], len: int | String (JSON) | 已裁剪单公式识别（跳过检测） |
| `nativeRecognizeText` | data: byte[], len: int | String (JSON) | 文字识别 |
| `nativeRecognizeMixed` | data: byte[], len: int | String (JSON) | 混合识别 |
| `nativeRelease` | — | void | 释放资源 |
| `nativeFreeString` | ptr: long | void | 释放字符串 |

## C FFI 函数（iOS）

| 函数 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `latexsnipper_init` | models_dir: *const c_char | i32 | 初始化 |
| `latexsnipper_ffi_response_version` | — | u32 | 返回 FFI JSON contract version（当前为 3） |
| `latexsnipper_recognize_formula` | data, len | *mut c_char | 公式识别 |
| `latexsnipper_recognize_cropped_formula` | data, len | *mut c_char | 已裁剪单公式识别（跳过检测） |
| `latexsnipper_recognize_text` | data, len | *mut c_char | 文字识别 |
| `latexsnipper_recognize_mixed` | data, len | *mut c_char | 混合识别 |
| `latexsnipper_release` | — | void | 释放 |
| `latexsnipper_free_string` | ptr | void | 释放字符串 |

## FfiResponse JSON 格式

```json
{
  "versions": {
    "ffiResponseVersion": 3,
    "diagnosticSchemaVersion": 1,
    "documentSchemaVersion": "1.0.0",
    "coreVersion": "3.0.0"
  },
  "done": true,
  "latex": "...",
  "text": "...",
  "confidence": 0.95,
  "error": null,
  "time_ms": 1234
}
```

`versions` 是 additive 字段；原有结果字段与 C/JNI pointer/length ABI 保持不变。

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
