use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latexsnipper_foundation::{Result, SnipperError};

/// Convert a C string to a Rust String.
///
/// # Safety
///
/// - `ptr` must be a valid pointer to a null-terminated C string.
/// - `ptr` must remain valid for the duration of this function.
pub unsafe fn cstr_to_string(ptr: *const c_char) -> Result<String> {
    if ptr.is_null() {
        return Err(SnipperError::Other("Null pointer".into()));
    }
    let cstr = CStr::from_ptr(ptr);
    cstr.to_str()
        .map(|s| s.to_string())
        .map_err(|e| SnipperError::Other(e.to_string()))
}

/// Convert a Rust String to a C string (caller must free with free_string).
pub fn string_to_cstr(s: &str) -> Result<*mut c_char> {
    CString::new(s)
        .map(|cs| cs.into_raw())
        .map_err(|e| SnipperError::Other(e.to_string()))
}

/// Free a C string allocated by string_to_cstr.
///
/// # Safety
///
/// - `ptr` must be a valid pointer to a C string allocated by `string_to_cstr`.
/// - `ptr` must not be used after this function is called.
pub unsafe fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

/// JSON response structure for FFI.
#[derive(serde::Serialize)]
pub struct FfiResponse {
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<u64>,
}

impl FfiResponse {
    pub fn success(latex: &str, confidence: f32, time_ms: u64) -> Self {
        Self {
            done: true,
            latex: Some(latex.to_string()),
            text: Some(latex.to_string()),
            confidence: Some(confidence),
            error: None,
            time_ms: Some(time_ms),
        }
    }

    pub fn error(msg: &str) -> Self {
        Self {
            done: true,
            latex: None,
            text: None,
            confidence: None,
            error: Some(msg.to_string()),
            time_ms: None,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}
