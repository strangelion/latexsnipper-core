use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use latexsnipper_foundation::{Result, SnipperError};

pub const FFI_RESPONSE_VERSION_V3: u32 = 3;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfiContractVersions {
    pub ffi_response_version: u32,
    pub diagnostic_schema_version: u32,
    pub document_schema_version: &'static str,
    pub core_version: &'static str,
}

impl FfiContractVersions {
    pub const fn current() -> Self {
        Self {
            ffi_response_version: FFI_RESPONSE_VERSION_V3,
            diagnostic_schema_version: latexsnipper_api_types::DIAGNOSTIC_SCHEMA_VERSION_V3,
            document_schema_version: latexsnipper_ast::DOCUMENT_SCHEMA_VERSION,
            core_version: env!("CARGO_PKG_VERSION"),
        }
    }
}

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
    pub versions: FfiContractVersions,
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
            versions: FfiContractVersions::current(),
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
            versions: FfiContractVersions::current(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_json_is_self_describing_without_changing_legacy_result_fields() {
        let value: serde_json::Value =
            serde_json::from_str(&FfiResponse::success("x", 0.9, 1).to_json()).unwrap();
        assert_eq!(value["versions"]["ffiResponseVersion"], 3);
        assert_eq!(value["versions"]["documentSchemaVersion"], "1.0.0");
        assert_eq!(value["done"], true);
        assert_eq!(value["latex"], "x");
    }
}
