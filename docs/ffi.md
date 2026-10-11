# FFI Crate

> 外部函数接口 — Android JNI, iOS C FFI

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `common` | common.rs | FFIResponse, CStr 转换工具 |
| `application_c` | application_c.rs | 独立 opaque session handle 与有界公式转换 |
| `android` | android/session_bridge.rs | `android-jni` 下的真正 JNI；用 `NativeSessionBridge`，见 [JNI 文档](../crates/ffi/java/README.md) |
| `android` 旧接口 | android/jni_bridge.rs | 历史 Java 命名 C shim，签名不是 JVM JNI，不要据名称加载为 native method |
| `ios` 兼容接口 | ios/c_ffi.rs | 无句柄 RGB C ABI；内部复用 `RecognitionSession`，不是已验收的 Apple SDK |

## 旧 Android C shim（不是真正 JNI）

以下名称仅记录兼容符号。新 JVM 调用者必须使用上面的 `NativeSessionBridge`，
不能把本表的 C 指针和整数签名当成 `JNIEnv` / `jclass` / Java 数组。

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
| `latexsnipper_recognize_formula` | data, len, width, height | *mut c_char | 公式识别 |
| `latexsnipper_recognize_cropped_formula` | data, len, width, height | *mut c_char | 已裁剪单公式识别（跳过检测） |
| `latexsnipper_recognize_text` | data, len, width, height | *mut c_char | 文字识别 |
| `latexsnipper_recognize_mixed` | data, len, width, height | *mut c_char | 混合识别 |
| `latexsnipper_release` | — | void | 释放 |
| `latexsnipper_free_string` | ptr | void | 释放字符串 |

完整 C 声明见 [兼容头文件](../crates/ffi/include/latexsnipper_legacy.h)。签名与旧返回字段保留，
内部不再创建 StubRuntime 或每次新建 Tokio runtime。由于旧函数没有句柄参数，
只维护一个串行默认会话；多调用者应使用 [Generic C session ABI](../crates/ffi/include/latexsnipper_session.h)。

初始化要求现有 UTF-8 模型目录；参数或构建失败不替换已有会话。
初始化成功不等于模型、provider 或质量就绪；普通识别失败不销毁会话，
内部 panic 会隔离毒化会话，重新初始化或释放后才可复用。
RGB 必须紧密排列，尺寸为 `1..10000`，有效字节不超过 100 MiB；
允许旧式尾部 padding，只复制 `width*height*3` 前缀。
所有返回字符串需释放一次，调用者负责指针有效性；旧 NUL 字符串 ABI 无法凭空获得显式输入长度。
同步识别应放到后台线程，释放会等待当前识别完成，不是硬取消接口。

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

```

当前 crate 声明 `lib` / `cdylib`，不因此承诺可直接获得已签名的 iOS `.a` / XCFramework。
Apple SDK、静态链接/框架包装、设备上的运行时与真实模型仍需在 Apple 工具链中单独验收。
三桌面平台共享库检查也不能代替 iOS 设备验证。

## 依赖关系

```
FFI
↑ 依赖 Engine
```
