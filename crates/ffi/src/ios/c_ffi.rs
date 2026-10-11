use once_cell::sync::Lazy;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

use latexsnipper_engine::{RecognitionProfile, RecognitionRequest, RecognitionSession};

use crate::common::FfiResponse;

/// Return the stable FFI JSON response contract version.
#[no_mangle]
pub extern "C" fn latexsnipper_ffi_response_version() -> u32 {
    crate::common::FFI_RESPONSE_VERSION_V3
}

// The old ABI has no handle argument, so it retains one serialized default
// session. New callers should use the independent generic C session handles.
static SESSION: Lazy<Mutex<Option<RecognitionSession>>> = Lazy::new(|| Mutex::new(None));
const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;
const MAX_MODELS_PATH_BYTES: usize = 1024 * 1024;

/// Initialize the engine.
/// Returns 1 on success, 0 on failure.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn latexsnipper_init(models_dir: *const c_char) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if models_dir.is_null() {
            return 0;
        }
        // SAFETY: The legacy ABI requires a readable NUL-terminated C string.
        let bytes = unsafe { CStr::from_ptr(models_dir) }.to_bytes();
        if bytes.is_empty() || bytes.len() > MAX_MODELS_PATH_BYTES {
            return 0;
        }
        let Ok(dir) = std::str::from_utf8(bytes) else {
            return 0;
        };
        let path = std::path::Path::new(dir);
        if dir.trim().is_empty() || !path.is_dir() {
            return 0;
        }
        // Serialize creation as well as recognition to bound temporary runtimes.
        let mut slot = SESSION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(session) = RecognitionSession::builder()
            .models_dir(path)
            .max_threads(4)
            .build()
        else {
            return 0;
        };
        if let Some(mut previous) = slot.replace(session) {
            previous.close();
        }
        SESSION.clear_poison();
        1
    }))
    .unwrap_or(0)
}

/// Recognize formula.
/// data: raw RGB pixel data
/// width, height: image dimensions
/// Returns a JSON string that must be freed with latexsnipper_free_string.
#[no_mangle]
pub extern "C" fn latexsnipper_recognize_formula(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> *mut c_char {
    recognize_response(data, len, width, height, RecognitionProfile::Formula)
}

/// Recognize an already-cropped single formula without running detection.
#[no_mangle]
pub extern "C" fn latexsnipper_recognize_cropped_formula(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> *mut c_char {
    recognize_response(data, len, width, height, RecognitionProfile::CroppedFormula)
}

/// Recognize text.
#[no_mangle]
pub extern "C" fn latexsnipper_recognize_text(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> *mut c_char {
    recognize_response(data, len, width, height, RecognitionProfile::Text)
}

/// Recognize mixed.
#[no_mangle]
pub extern "C" fn latexsnipper_recognize_mixed(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> *mut c_char {
    recognize_response(data, len, width, height, RecognitionProfile::Mixed)
}

/// Release engine.
#[no_mangle]
pub extern "C" fn latexsnipper_release() {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let mut slot = SESSION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut session) = slot.take() {
            session.close();
        }
        SESSION.clear_poison();
    }));
}

/// Free a string.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn latexsnipper_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

fn recognize_response(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
    profile: RecognitionProfile,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        let response = recognize_sync(data, len, width, height, profile);
        CString::new(response.to_json())
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or_else(|_| {
        CString::new(
            FfiResponse::error("The legacy session operation panicked; reinitialize before reuse")
                .to_json(),
        )
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
    })
}

fn rgb_length(len: usize, width: u32, height: u32) -> std::result::Result<usize, &'static str> {
    if len == 0 || len > MAX_INPUT_BYTES {
        return Err("Invalid image data length");
    }
    if width == 0 || height == 0 || width > 10000 || height > 10000 {
        return Err("Invalid image dimensions");
    }
    let required = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or("Image dimensions overflow")?;
    if required > MAX_INPUT_BYTES || len < required {
        return Err("Image data too short or too large for given dimensions");
    }
    Ok(required)
}

fn recognize_sync(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
    profile: RecognitionProfile,
) -> FfiResponse {
    if data.is_null() {
        return FfiResponse::error("Null image data");
    }
    let expected_len = match rgb_length(len, width, height) {
        Ok(required) => required,
        Err(error) => return FfiResponse::error(error),
    };
    let mut slot = match SESSION.lock() {
        Ok(slot) => slot,
        Err(_) => {
            return FfiResponse::error("The legacy session is poisoned; reinitialize before reuse")
        }
    };
    let Some(session) = slot.as_mut() else {
        return FfiResponse::error("Engine not initialized");
    };
    // SAFETY: Metadata is bounded above; the caller must supply readable RGB bytes.
    // Legacy padding is accepted but only the active tightly packed prefix is copied.
    let data = unsafe { std::slice::from_raw_parts(data, expected_len) };

    let image = latexsnipper_image::SnipperImage::new(
        width,
        height,
        latexsnipper_image::color::PixelFormat::Rgb,
        data.to_vec(),
    );

    let result =
        match session.recognize(RecognitionRequest::from_image(image).with_profile(profile)) {
            Ok(result) => result,
            Err(error) => return FfiResponse::error(&error.to_string()),
        };
    let elapsed = result.metadata.elapsed.as_millis().min(u64::MAX as u128) as u64;
    let doc = result.document;

    let text: String = doc
        .pages
        .iter()
        .flat_map(|p| &p.blocks)
        .filter_map(|b| match b {
            latexsnipper_ast::Block::Formula(f) => Some(f.formula.as_latex().to_string()),
            latexsnipper_ast::Block::Paragraph(p) => {
                let t: String = p
                    .inlines
                    .iter()
                    .filter_map(|i| {
                        if let latexsnipper_ast::Inline::Text(t) = i {
                            Some(t.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(t)
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    FfiResponse::success(
        &text,
        doc.pages
            .first()
            .and_then(|p| p.blocks.first())
            .map_or(0.0, |b| match b {
                latexsnipper_ast::Block::Formula(f) => f.formula.confidence,
                _ => 0.0,
            }),
        elapsed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn response(pointer: *mut c_char) -> serde_json::Value {
        assert!(!pointer.is_null());
        let source = unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .unwrap()
            .to_owned();
        latexsnipper_free_string(pointer);
        serde_json::from_str(&source).unwrap()
    }

    #[test]
    fn legacy_signatures_and_rgb_bounds_remain_explicit() {
        let _: extern "C" fn(*const c_char) -> i32 = latexsnipper_init;
        for function in [
            latexsnipper_recognize_formula,
            latexsnipper_recognize_cropped_formula,
            latexsnipper_recognize_text,
            latexsnipper_recognize_mixed,
        ] {
            let _: extern "C" fn(*const u8, usize, u32, u32) -> *mut c_char = function;
        }
        assert_eq!(rgb_length(3, 1, 1), Ok(3));
        assert_eq!(rgb_length(4, 1, 1), Ok(3));
        for (length, width, height) in [
            (0, 1, 1),
            (2, 1, 1),
            (MAX_INPUT_BYTES + 1, 1, 1),
            (MAX_INPUT_BYTES, 10000, 10000),
            (3, 0, 1),
            (3, 1, u32::MAX),
            (3, u32::MAX, u32::MAX),
        ] {
            assert!(rgb_length(length, width, height).is_err());
        }
    }

    #[test]
    fn legacy_session_reuses_warmup_survives_failures_and_recovers_after_poison() {
        struct Cleanup;
        impl Drop for Cleanup {
            fn drop(&mut self) {
                latexsnipper_release();
            }
        }
        let _cleanup = Cleanup;
        latexsnipper_release();
        let models = tempfile::tempdir().unwrap();
        let path = CString::new(models.path().to_str().unwrap()).unwrap();
        assert_eq!(latexsnipper_init(path.as_ptr()), 1);
        {
            let mut slot = SESSION.lock().unwrap();
            let session = slot.as_mut().unwrap();
            assert!(
                !session
                    .warmup(RecognitionProfile::CroppedFormula)
                    .unwrap()
                    .already_warm
            );
        }
        let data = [255u8; 3];
        for _ in 0..2 {
            let value = unsafe {
                response(latexsnipper_recognize_cropped_formula(
                    data.as_ptr(),
                    data.len(),
                    1,
                    1,
                ))
            };
            assert_eq!(value["done"], true);
            assert_eq!(value["versions"]["ffiResponseVersion"], 3);
            assert!(value["error"].is_string());
            assert!(value.get("latex").is_none());
        }
        for invalid in [std::ptr::null(), c"".as_ptr(), c" ".as_ptr()] {
            assert_eq!(latexsnipper_init(invalid), 0);
        }
        {
            let mut slot = SESSION.lock().unwrap();
            assert!(
                slot.as_mut()
                    .unwrap()
                    .warmup(RecognitionProfile::CroppedFormula)
                    .unwrap()
                    .already_warm
            );
        }
        let oversized = unsafe {
            response(latexsnipper_recognize_formula(
                data.as_ptr(),
                MAX_INPUT_BYTES + 1,
                1,
                1,
            ))
        };
        assert!(oversized["error"].is_string());
        assert!(catch_unwind(|| {
            let _slot = SESSION.lock().unwrap();
            panic!("intentional compatibility-session poison");
        })
        .is_err());
        let poisoned = unsafe {
            response(latexsnipper_recognize_cropped_formula(
                data.as_ptr(),
                data.len(),
                1,
                1,
            ))
        };
        assert!(poisoned["error"].as_str().unwrap().contains("poisoned"));
        assert_eq!(latexsnipper_init(path.as_ptr()), 1);
        assert!(!SESSION.is_poisoned());
        assert!(
            !SESSION
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .warmup(RecognitionProfile::CroppedFormula)
                .unwrap()
                .already_warm
        );
        let runtime = tokio::runtime::Runtime::new().unwrap();
        {
            let _entered = runtime.enter();
            let nested = unsafe {
                response(latexsnipper_recognize_cropped_formula(
                    data.as_ptr(),
                    data.len(),
                    1,
                    1,
                ))
            };
            assert!(nested["error"].as_str().unwrap().contains("Tokio"));
            latexsnipper_release();
        }
        latexsnipper_release();
        let closed = unsafe {
            response(latexsnipper_recognize_formula(
                data.as_ptr(),
                data.len(),
                1,
                1,
            ))
        };
        assert_eq!(closed["error"], "Engine not initialized");
    }
}
