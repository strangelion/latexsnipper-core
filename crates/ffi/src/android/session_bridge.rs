//! Genuine JNI entry points for the shared opaque-handle application C ABI.
//! Historical NativeBridge symbols remain raw C compatibility exports.

use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};

use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;

use crate::application_c as core;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 100 * 1024 * 1024;

enum BridgeError {
    Invalid(&'static str),
    Jni(jni::errors::Error),
    Internal(&'static str),
}

impl From<jni::errors::Error> for BridgeError {
    fn from(error: jni::errors::Error) -> Self {
        Self::Jni(error)
    }
}

struct OwnedResponse(*mut c_char);

impl Drop for OwnedResponse {
    fn drop(&mut self) {
        // SAFETY: This guard exclusively owns one string from the Core C ABI.
        unsafe { core::latexsnipper_string_free(self.0) };
    }
}

fn request_bytes(env: &mut JNIEnv<'_>, request: &JString<'_>) -> Result<Vec<u8>, BridgeError> {
    if request.is_null() {
        return Err(BridgeError::Invalid("request must not be null"));
    }
    // Bound UTF-16 units before Modified UTF-8 decoding can allocate a copy.
    let units = env.call_method(request, "length", "()I", &[])?.i()?;
    if units <= 0 || units as usize > MAX_REQUEST_BYTES {
        return Err(BridgeError::Invalid(
            "request exceeds the 1 MiB transport limit",
        ));
    }
    let text: String = env.get_string(request)?.into();
    if text.len() > MAX_REQUEST_BYTES {
        return Err(BridgeError::Invalid(
            "request exceeds the 1 MiB UTF-8 limit",
        ));
    }
    Ok(text.into_bytes())
}

fn image_bytes(env: &JNIEnv<'_>, image: &JByteArray<'_>) -> Result<Vec<u8>, BridgeError> {
    if image.is_null() {
        return Err(BridgeError::Invalid("image must not be null"));
    }
    let length = env.get_array_length(image)?;
    if length <= 0 || length as usize > MAX_IMAGE_BYTES {
        return Err(BridgeError::Invalid(
            "encoded image must contain 1 to 100 MiB of bytes",
        ));
    }
    Ok(env.convert_byte_array(image)?)
}

fn response<'local>(
    env: &mut JNIEnv<'local>,
    operation: impl FnOnce(&mut JNIEnv<'local>) -> Result<*mut c_char, BridgeError>,
) -> jstring {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let owned = OwnedResponse(operation(env)?);
        if owned.0.is_null() {
            return Err(BridgeError::Internal("Core response allocation failed"));
        }
        // SAFETY: The application C ABI returns an owned, terminated UTF-8 string.
        let text = unsafe { CStr::from_ptr(owned.0) }
            .to_str()
            .map_err(|_| BridgeError::Internal("Core returned invalid UTF-8"))?;
        Ok(env.new_string(text)?.into_raw())
    }))
    .unwrap_or(Err(BridgeError::Internal("The JNI operation panicked")));
    match result {
        Ok(string) => string,
        Err(error) => {
            // Preserve exceptions already raised by the JVM instead of masking them.
            if !env.exception_check().unwrap_or(true) {
                let (class, message) = match error {
                    BridgeError::Invalid(message) => {
                        ("java/lang/IllegalArgumentException", message.to_owned())
                    }
                    BridgeError::Internal(message) => {
                        ("java/lang/IllegalStateException", message.to_owned())
                    }
                    BridgeError::Jni(error) => {
                        ("java/lang/IllegalStateException", error.to_string())
                    }
                };
                let _ = env.throw_new(class, message);
            }
            std::ptr::null_mut()
        }
    }
}

/// JNI arguments and local references are supplied and owned by the JVM.
#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_abiVersion(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jint {
    core::latexsnipper_session_abi_version() as jint
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_create(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    request: JString<'_>,
) -> jstring {
    response(&mut env, |env| {
        let bytes = request_bytes(env, &request)?;
        // SAFETY: Core reads the owned bytes only during this synchronous call.
        Ok(unsafe { core::latexsnipper_session_create(bytes.as_ptr(), bytes.len()) })
    })
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_health(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    // Java long transports the opaque u64 handle's bit pattern unchanged.
    response(&mut env, |_| {
        Ok(core::latexsnipper_session_health(handle as u64))
    })
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_warmup(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    request: JString<'_>,
) -> jstring {
    response(&mut env, |env| {
        let bytes = request_bytes(env, &request)?;
        // SAFETY: Request storage remains live for the complete Core call.
        Ok(
            unsafe {
                core::latexsnipper_session_warmup(handle as u64, bytes.as_ptr(), bytes.len())
            },
        )
    })
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_recognizeBytes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    request: JString<'_>,
    image: JByteArray<'_>,
) -> jstring {
    response(&mut env, |env| {
        let request = request_bytes(env, &request)?;
        let image = image_bytes(env, &image)?;
        // Move the JNI copy into the shared session path; never pin the Java array
        // throughout inference or copy this buffer through the raw C ABI again.
        Ok(core::recognize_owned_bytes(handle as u64, &request, image))
    })
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_reloadModels(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    response(&mut env, |_| {
        Ok(core::latexsnipper_session_reload_models(handle as u64))
    })
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_close(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    response(&mut env, |_| {
        Ok(core::latexsnipper_session_close(handle as u64))
    })
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_formulaCapabilities(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    response(&mut env, |_| Ok(core::latexsnipper_formula_capabilities()))
}

#[no_mangle]
pub extern "system" fn Java_com_latexsnipper_core_NativeSessionBridge_convertFormula(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    request: JString<'_>,
) -> jstring {
    response(&mut env, |env| {
        let bytes = request_bytes(env, &request)?;
        // SAFETY: Request storage remains live for the complete Core call.
        Ok(unsafe { core::latexsnipper_formula_convert(bytes.as_ptr(), bytes.len()) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jni_session_signatures_use_jvm_environment_classes_and_arrays() {
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>) -> jint =
            Java_com_latexsnipper_core_NativeSessionBridge_abiVersion;
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>, JString<'_>) -> jstring =
            Java_com_latexsnipper_core_NativeSessionBridge_create;
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>, jlong) -> jstring =
            Java_com_latexsnipper_core_NativeSessionBridge_health;
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>, jlong, JString<'_>) -> jstring =
            Java_com_latexsnipper_core_NativeSessionBridge_warmup;
        let _: extern "system" fn(
            JNIEnv<'_>,
            JClass<'_>,
            jlong,
            JString<'_>,
            JByteArray<'_>,
        ) -> jstring = Java_com_latexsnipper_core_NativeSessionBridge_recognizeBytes;
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>, jlong) -> jstring =
            Java_com_latexsnipper_core_NativeSessionBridge_reloadModels;
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>, jlong) -> jstring =
            Java_com_latexsnipper_core_NativeSessionBridge_close;
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>) -> jstring =
            Java_com_latexsnipper_core_NativeSessionBridge_formulaCapabilities;
        let _: extern "system" fn(JNIEnv<'_>, JClass<'_>, JString<'_>) -> jstring =
            Java_com_latexsnipper_core_NativeSessionBridge_convertFormula;
    }
}
