//! Stable, process-lifetime C ABI for application recognition sessions.
//!
//! The ABI exposes numeric handles rather than Rust pointers. Each handle owns
//! one [`RecognitionSession`] and serializes calls through a per-session mutex.

use std::collections::HashMap;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use latexsnipper_api_types::{
    ApiEnvelopeV3, ApiErrorV3, RecognitionProfile, API_ENVELOPE_VERSION_V3,
};
use latexsnipper_conversion::{
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};
use latexsnipper_engine::application::{ApplicationError, RecognitionOptions, RuntimePreference};
use latexsnipper_engine::{
    DocumentParseMode, RecognitionIntegrationApi, RecognitionRequest, RecognitionResult,
    RecognitionSession,
};
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::common::{free_string, string_to_cstr};

/// Version of the opaque application-session C ABI.
pub const APPLICATION_SESSION_ABI_VERSION: u32 = 1;

const MAX_SESSIONS: usize = 64;
const MAX_REQUEST_JSON_BYTES: usize = 1024 * 1024;
const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;
const MAX_FORMULA_RESULT_DATA_BYTES: usize = 256 * 1024;
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1_000;

type SessionEntry = Arc<Mutex<RecognitionSession>>;

static SESSIONS: Lazy<Mutex<SessionRegistry>> =
    Lazy::new(|| Mutex::new(SessionRegistry::default()));

#[derive(Default)]
struct SessionRegistry {
    next_handle: u64,
    sessions: HashMap<u64, SessionEntry>,
}

impl SessionRegistry {
    fn insert(&mut self, session: RecognitionSession) -> Result<u64, AdapterError> {
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(AdapterError::new(
                "SESSION_LIMIT_REACHED",
                "The process session limit has been reached.",
                false,
            ));
        }

        // Zero is reserved as the invalid handle. Handles are not reused until
        // the counter wraps, making stale handles fail in normal process life.
        for _ in 0..=MAX_SESSIONS {
            self.next_handle = self.next_handle.wrapping_add(1);
            if self.next_handle == 0 {
                continue;
            }
            if !self.sessions.contains_key(&self.next_handle) {
                let handle = self.next_handle;
                self.sessions.insert(handle, Arc::new(Mutex::new(session)));
                return Ok(handle);
            }
        }

        Err(AdapterError::new(
            "SESSION_HANDLE_EXHAUSTED",
            "No application session handle is available.",
            false,
        ))
    }

    fn get(&self, handle: u64) -> Result<SessionEntry, AdapterError> {
        if handle == 0 {
            return Err(AdapterError::invalid_handle());
        }
        self.sessions
            .get(&handle)
            .cloned()
            .ok_or_else(AdapterError::invalid_handle)
    }

    fn remove(&mut self, handle: u64) -> Result<SessionEntry, AdapterError> {
        if handle == 0 {
            return Err(AdapterError::invalid_handle());
        }
        self.sessions
            .remove(&handle)
            .ok_or_else(AdapterError::invalid_handle)
    }
}

#[derive(Debug)]
struct AdapterError {
    code: String,
    message: String,
    recoverable: bool,
    details: Option<Value>,
}

impl AdapterError {
    fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
            details: None,
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    fn invalid_handle() -> Self {
        Self::new(
            "SESSION_NOT_FOUND",
            "The application session handle is unknown or already closed.",
            false,
        )
    }

    fn poisoned() -> Self {
        Self::new(
            "SESSION_POISONED",
            "The application session cannot continue after an internal panic.",
            false,
        )
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL", message, false)
    }

    fn into_api(self) -> ApiErrorV3 {
        ApiErrorV3 {
            code: self.code,
            message: self.message,
            recoverable: self.recoverable,
            details: self.details,
        }
    }
}

impl From<ApplicationError> for AdapterError {
    fn from(error: ApplicationError) -> Self {
        let code = serde_json::to_value(error.code)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "INTERNAL".to_string());
        let details = error.detail.map(|detail| json!({ "detail": detail }));
        Self {
            code,
            message: error.message,
            recoverable: error.retryable,
            details,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionCreateRequest {
    models_dir: String,
    #[serde(default)]
    quality_baselines_dir: Option<String>,
    #[serde(default)]
    provider_smoke_fixture: Option<String>,
    #[serde(default)]
    runtime_preference: RuntimePreference,
    #[serde(default)]
    parse_mode: DocumentParseMode,
    #[serde(default = "default_max_threads")]
    max_threads: usize,
}

const fn default_max_threads() -> usize {
    4
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FormulaConvertRequest {
    content: String,
    input_format: FormulaInputFormat,
    output_format: String,
    #[serde(default = "default_formula_mode")]
    mode: FormulaConversionMode,
}

const fn default_formula_mode() -> FormulaConversionMode {
    FormulaConversionMode::Strict
}

impl FormulaConvertRequest {
    fn convert(self) -> Result<Value, AdapterError> {
        let label = self
            .output_format
            .trim()
            .to_ascii_lowercase()
            .replace('-', "_");
        let fragment = label == "latex_fragment";
        let output_label = if fragment { "latex_display" } else { &label };
        let output = OutputFormat::all()
            .iter()
            .copied()
            .find(|format| format.name() == output_label)
            .ok_or_else(|| {
                AdapterError::new("INVALID_ARGUMENT", "Unknown formula output format.", false)
            })?;
        let capability = CapabilityRegistry::formula_conversion(
            self.input_format,
            output,
            self.mode,
            CapabilityTarget::Native,
        );
        if !capability.available {
            return Err(AdapterError::new(
                "UNSUPPORTED_FORMAT",
                "The formula conversion route is not supported.",
                false,
            )
            .with_details(json!({ "detail": capability.unavailable_reason })));
        }
        let content = if fragment {
            DocumentConverter::convert_formula_fragment(&self.content, self.input_format, self.mode)
        } else {
            DocumentConverter::convert_formula_string(
                &self.content,
                self.input_format,
                output,
                self.mode,
            )
        }
        .map_err(|error| AdapterError::from(ApplicationError::from(error)))?;
        let mut data = json!({ "content": content, "capability": capability });
        if fragment {
            data["contentKind"] = json!("latex-fragment");
        }
        bound_formula_data(data)
    }
}

fn bound_formula_data(data: Value) -> Result<Value, AdapterError> {
    let serialized = serde_json::to_vec(&data)
        .map_err(|_| AdapterError::internal("The formula result could not be serialized."))?;
    if serialized.len() > MAX_FORMULA_RESULT_DATA_BYTES {
        return Err(AdapterError::new(
            "OUTPUT_TOO_LARGE",
            "Serialized formula result data exceeds the 256 KiB limit.",
            false,
        ));
    }
    Ok(data)
}

impl SessionCreateRequest {
    fn build(self) -> Result<RecognitionSession, AdapterError> {
        if self.models_dir.trim().is_empty() {
            return Err(AdapterError::new(
                "INVALID_ARGUMENT",
                "modelsDir must not be empty.",
                false,
            ));
        }
        if !(1..=256).contains(&self.max_threads) {
            return Err(AdapterError::new(
                "INVALID_ARGUMENT",
                "maxThreads must be between 1 and 256.",
                false,
            ));
        }

        let mut builder = RecognitionSession::builder()
            .models_dir(PathBuf::from(self.models_dir))
            .runtime_preference(self.runtime_preference)
            .parse_mode(self.parse_mode)
            .max_threads(self.max_threads);
        if let Some(path) = self.quality_baselines_dir {
            if path.trim().is_empty() {
                return Err(AdapterError::new(
                    "INVALID_ARGUMENT",
                    "qualityBaselinesDir must not be empty when provided.",
                    false,
                ));
            }
            builder = builder.quality_baselines_dir(PathBuf::from(path));
        }
        if let Some(path) = self.provider_smoke_fixture {
            if path.trim().is_empty() {
                return Err(AdapterError::new(
                    "INVALID_ARGUMENT",
                    "providerSmokeFixture must not be empty when provided.",
                    false,
                ));
            }
            builder = builder.provider_smoke_fixture(PathBuf::from(path));
        }
        builder.build().map_err(AdapterError::from)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionWarmupRequest {
    profile: RecognitionProfile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionRecognizeRequest {
    profile: RecognitionProfile,
    #[serde(default)]
    parse_mode: Option<DocumentParseMode>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    strict: bool,
}

impl SessionRecognizeRequest {
    fn options(&self) -> Result<RecognitionOptions, AdapterError> {
        if self.timeout_ms.is_some_and(|value| value > MAX_TIMEOUT_MS) {
            return Err(AdapterError::new(
                "INVALID_ARGUMENT",
                format!("timeoutMs must not exceed {MAX_TIMEOUT_MS}."),
                false,
            ));
        }
        Ok(RecognitionOptions {
            parse_mode: self.parse_mode,
            include_source_asset: false,
            timeout: self.timeout_ms.map(Duration::from_millis),
            strict: self.strict,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionCreated {
    abi_version: u32,
    api_envelope_version: u32,
    handle: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionClosed {
    handle: u64,
    closed: bool,
}

fn registry() -> Result<MutexGuard<'static, SessionRegistry>, AdapterError> {
    SESSIONS
        .lock()
        .map_err(|_| AdapterError::internal("The application session registry is unavailable."))
}

fn session_entry(handle: u64) -> Result<SessionEntry, AdapterError> {
    registry()?.get(handle)
}

fn with_session<T>(
    handle: u64,
    operation: impl FnOnce(&mut RecognitionSession) -> Result<T, AdapterError>,
) -> Result<T, AdapterError> {
    let entry = session_entry(handle)?;
    let mut session = entry.lock().map_err(|_| AdapterError::poisoned())?;
    operation(&mut session)
}

unsafe fn parse_request<T: DeserializeOwned>(
    request_json: *const u8,
    request_json_len: usize,
) -> Result<T, AdapterError> {
    if request_json.is_null() || request_json_len == 0 || request_json_len > MAX_REQUEST_JSON_BYTES
    {
        return Err(AdapterError::new(
            "INVALID_ARGUMENT",
            format!("requestJson must contain between 1 and {MAX_REQUEST_JSON_BYTES} bytes."),
            false,
        ));
    }
    let request_json = unsafe { std::slice::from_raw_parts(request_json, request_json_len) };
    serde_json::from_slice(request_json).map_err(|error| {
        AdapterError::new("INVALID_JSON", "The request JSON is invalid.", false)
            .with_details(json!({ "detail": error.to_string() }))
    })
}

fn response_json<T: Serialize>(result: Result<T, AdapterError>) -> *mut c_char {
    let envelope = match result {
        Ok(data) => ApiEnvelopeV3::success(data, Vec::new()),
        Err(error) => ApiEnvelopeV3::<T>::failure(error.into_api(), Vec::new()),
    };
    let json = serde_json::to_string(&envelope).unwrap_or_else(|error| {
        let fallback = ApiEnvelopeV3::<Value>::failure(
            ApiErrorV3 {
                code: "SERIALIZATION_FAILED".to_string(),
                message: "The response could not be serialized.".to_string(),
                recoverable: false,
                details: Some(json!({ "detail": error.to_string() })),
            },
            Vec::new(),
        );
        serde_json::to_string(&fallback).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"code\":\"SERIALIZATION_FAILED\",\"message\":\"The response could not be serialized.\",\"recoverable\":false}}"
                .to_string()
        })
    });
    string_to_cstr(&json).unwrap_or(std::ptr::null_mut())
}

fn ffi_response<T: Serialize>(operation: impl FnOnce() -> Result<T, AdapterError>) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(operation)).unwrap_or_else(|_| {
        Err(AdapterError::internal(
            "The application session operation panicked.",
        ))
    });
    response_json(result)
}

/// Return the stable opaque application-session ABI version.
#[no_mangle]
pub extern "C" fn latexsnipper_session_abi_version() -> u32 {
    APPLICATION_SESSION_ABI_VERSION
}

/// Query native formula direction/mode support without creating a session.
/// Release the returned v3 JSON envelope with [`latexsnipper_string_free`].
#[no_mangle]
pub extern "C" fn latexsnipper_formula_capabilities() -> *mut c_char {
    ffi_response(|| {
        Ok::<_, AdapterError>(CapabilityRegistry::formula_conversions(
            CapabilityTarget::Native,
        ))
    })
}

/// Convert a declared formula string without loading recognition models.
/// Mode defaults to strict; source/reconstruction budgets apply to this call.
/// Release the returned v3 JSON envelope with [`latexsnipper_string_free`].
///
/// # Safety
///
/// `request_json` must reference `request_json_len` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_convert(
    request_json: *const u8,
    request_json_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: FormulaConvertRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        request.convert()
    })
}

/// Create a long-lived application session.
///
/// `request_json` must contain at least `modelsDir`. The returned JSON string
/// must be released with [`latexsnipper_string_free`].
///
/// # Safety
///
/// `request_json` must reference `request_json_len` readable bytes for the
/// duration of this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_session_create(
    request_json: *const u8,
    request_json_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: SessionCreateRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        let session = request.build()?;
        let handle = registry()?.insert(session)?;
        Ok(SessionCreated {
            abi_version: APPLICATION_SESSION_ABI_VERSION,
            api_envelope_version: API_ENVELOPE_VERSION_V3,
            handle,
        })
    })
}

/// Return a session health report as versioned JSON.
#[no_mangle]
pub extern "C" fn latexsnipper_session_health(handle: u64) -> *mut c_char {
    ffi_response(|| with_session(handle, |session| session.health_check().map_err(Into::into)))
}

/// Warm the models required by one recognition profile.
///
/// # Safety
///
/// `request_json` must reference `request_json_len` readable bytes for the
/// duration of this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_session_warmup(
    handle: u64,
    request_json: *const u8,
    request_json_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: SessionWarmupRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        with_session(handle, |session| {
            session.warmup(request.profile).map_err(Into::into)
        })
    })
}

/// Recognize encoded image bytes with a persistent session.
///
/// # Safety
///
/// `request_json` and `data` must reference their declared readable byte ranges
/// for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_session_recognize_bytes(
    handle: u64,
    request_json: *const u8,
    request_json_len: usize,
    data: *const u8,
    data_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: SessionRecognizeRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        if data.is_null() || data_len == 0 || data_len > MAX_INPUT_BYTES {
            return Err(AdapterError::new(
                "INVALID_ARGUMENT",
                format!("data must contain between 1 and {MAX_INPUT_BYTES} bytes."),
                false,
            ));
        }
        let options = request.options()?;
        let bytes = unsafe { std::slice::from_raw_parts(data, data_len) }.to_vec();
        recognize_owned(handle, request, options, bytes)
    })
}

fn recognize_owned(
    handle: u64,
    request: SessionRecognizeRequest,
    options: RecognitionOptions,
    bytes: Vec<u8>,
) -> Result<RecognitionResult, AdapterError> {
    let recognition = RecognitionRequest::from_bytes(bytes, None)
        .with_profile(request.profile)
        .with_options(options);
    with_session(handle, |session| {
        session.recognize(recognition).map_err(Into::into)
    })
}

/// Share session dispatch without copying an already-owned JNI image a second time.
#[cfg(feature = "android-jni")]
pub(crate) fn recognize_owned_bytes(handle: u64, request: &[u8], bytes: Vec<u8>) -> *mut c_char {
    ffi_response(|| {
        // SAFETY: The slice supplies its exact readable length during this call.
        let request: SessionRecognizeRequest =
            unsafe { parse_request(request.as_ptr(), request.len()) }?;
        if bytes.is_empty() || bytes.len() > MAX_INPUT_BYTES {
            return Err(AdapterError::new(
                "INVALID_ARGUMENT",
                "Encoded image length is invalid.",
                false,
            ));
        }
        let options = request.options()?;
        recognize_owned(handle, request, options, bytes)
    })
}

/// Reload the model registry for a persistent session.
#[no_mangle]
pub extern "C" fn latexsnipper_session_reload_models(handle: u64) -> *mut c_char {
    ffi_response(|| {
        with_session(handle, |session| {
            RecognitionIntegrationApi::reload_models(session).map_err(Into::into)
        })
    })
}

/// Close a session, clear runtime caches, and invalidate its handle.
#[no_mangle]
pub extern "C" fn latexsnipper_session_close(handle: u64) -> *mut c_char {
    ffi_response(|| {
        let entry = registry()?.remove(handle)?;
        match entry.lock() {
            Ok(mut session) => session.close(),
            Err(poisoned) => {
                poisoned.into_inner().close();
                return Err(AdapterError::poisoned());
            }
        }
        Ok(SessionClosed {
            handle,
            closed: true,
        })
    })
}

/// Release a JSON string allocated by any application-session ABI function.
///
/// # Safety
///
/// `ptr` must be null or a pointer returned by this library that has not
/// already been freed.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_string_free(ptr: *mut c_char) {
    unsafe { free_string(ptr) };
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use serde_json::Value;

    use super::*;

    unsafe fn take_json(ptr: *mut c_char) -> Value {
        assert!(!ptr.is_null());
        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
        unsafe { latexsnipper_string_free(ptr) };
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn public_abi_signatures_keep_explicit_input_lengths() {
        let _: unsafe extern "C" fn(*const u8, usize) -> *mut c_char = latexsnipper_formula_convert;
        let _: extern "C" fn() -> *mut c_char = latexsnipper_formula_capabilities;
        let _: unsafe extern "C" fn(*const u8, usize) -> *mut c_char = latexsnipper_session_create;
        let _: unsafe extern "C" fn(u64, *const u8, usize) -> *mut c_char =
            latexsnipper_session_warmup;
        let _: unsafe extern "C" fn(u64, *const u8, usize, *const u8, usize) -> *mut c_char =
            latexsnipper_session_recognize_bytes;
    }

    #[test]
    fn formula_routes_and_errors_preserve_v3_envelopes_and_ownership() {
        let capabilities = unsafe { take_json(latexsnipper_formula_capabilities()) };
        assert_eq!(capabilities["ok"], true);
        let routes = capabilities["data"].as_array().unwrap();
        assert_eq!(routes.len(), 144);
        for route in routes {
            let content = match route["input"].as_str().unwrap() {
                "mathml" => "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>",
                "omml" => "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>",
                "typst" => "frac(a, b)",
                "markdown" => r"A fraction $\frac{a}{b}$",
                _ => r"\frac{a}{b}",
            };
            let request = json!({
                "content": content, "inputFormat": route["input"],
                "outputFormat": route["output"], "mode": route["mode"],
            })
            .to_string();
            let result = unsafe {
                take_json(latexsnipper_formula_convert(
                    request.as_ptr(),
                    request.len(),
                ))
            };
            assert_eq!(result["ok"], route["available"]);
            if route["available"] == true {
                assert_eq!(result["data"]["capability"], *route);
            } else {
                assert_eq!(result["error"]["code"], "UNSUPPORTED_FORMAT");
                assert_eq!(
                    result["error"]["details"]["detail"],
                    route["unavailableReason"]
                );
            }
        }
        let null = unsafe { take_json(latexsnipper_formula_convert(std::ptr::null(), 0)) };
        assert_eq!(null["error"]["code"], "INVALID_ARGUMENT");
        let oversized = unsafe {
            take_json(latexsnipper_formula_convert(
                b"x".as_ptr(),
                MAX_REQUEST_JSON_BYTES + 1,
            ))
        };
        assert_eq!(oversized["error"]["code"], "INVALID_ARGUMENT");
        for (request, code) in [
            (
                json!({"content":"x", "inputFormat":"latex", "outputFormat":"omml"}),
                "OK",
            ),
            (
                json!({"content":r"\unknownmacro+x", "inputFormat":"latex", "outputFormat":"omml"}),
                "CONVERSION_FAILED",
            ),
            (
                json!({"content":"x", "inputFormat":"latex", "outputFormat":"omml", "mode":"lossless"}),
                "INVALID_JSON",
            ),
            (
                json!({"content":"x", "inputFormat":"ole", "outputFormat":"omml"}),
                "INVALID_JSON",
            ),
            (
                json!({"content":"x", "inputFormat":"latex", "outputFormat":"pdf"}),
                "INVALID_ARGUMENT",
            ),
            (
                json!({"content":"x", "inputFormat":"latex", "outputFormat":"omml", "typo":true}),
                "INVALID_JSON",
            ),
            (
                json!({"content":"x".repeat(64 * 1024 + 1), "inputFormat":"latex", "outputFormat":"omml"}),
                "INPUT_TOO_LARGE",
            ),
        ] {
            let request = request.to_string();
            let result = unsafe {
                take_json(latexsnipper_formula_convert(
                    request.as_ptr(),
                    request.len(),
                ))
            };
            if code == "OK" {
                assert_eq!(result["ok"], true);
            } else {
                assert_eq!(result["error"]["code"], code);
            }
        }
        for request in [&b"\xff"[..], &b"{}garbage"[..]] {
            let result = unsafe {
                take_json(latexsnipper_formula_convert(
                    request.as_ptr(),
                    request.len(),
                ))
            };
            assert_eq!(result["error"]["code"], "INVALID_JSON");
        }
    }

    #[test]
    fn formula_fragment_c_api_keeps_document_and_inline_outputs_distinct() {
        for (input, source) in [
            ("latex", r"\frac{a}{b}"),
            ("typst", "frac(a,b)"),
            ("mathml", "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"),
            ("omml", "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>"),
        ] {
            let request = json!({
                "content": source, "inputFormat": input,
                "outputFormat": "latex-fragment", "mode": "best-effort"
            }).to_string();
            let result = unsafe {
                take_json(latexsnipper_formula_convert(request.as_ptr(), request.len()))
            };
            assert_eq!(result["ok"], true);
            assert_eq!(result["data"]["content"], r"\frac{a}{b}");
            assert_eq!(result["data"]["contentKind"], "latex-fragment");
            assert_eq!(result["data"]["capability"]["output"], "latex_display");
            assert_eq!(result["data"]["capability"]["target"], "native");
        }
        for (output, expected) in [
            ("latex", r"\documentclass"),
            ("markdown_inline", r"$\frac{a}{b}$"),
        ] {
            let request = json!({
                "content": "frac(a,b)", "inputFormat": "typst",
                "outputFormat": output, "mode": "best-effort"
            })
            .to_string();
            let result = unsafe {
                take_json(latexsnipper_formula_convert(
                    request.as_ptr(),
                    request.len(),
                ))
            };
            assert_eq!(result["ok"], true);
            assert!(result["data"]["content"]
                .as_str()
                .unwrap()
                .contains(expected));
            assert!(result["data"].get("contentKind").is_none());
        }
    }

    #[test]
    fn formula_c_api_rejects_fragment_splicing_and_oversized_result_data() {
        for (source, output, mode, code) in [
            (
                "frac(a,b)".into(),
                "latex-fragment",
                "strict",
                "UNSUPPORTED_FORMAT",
            ),
            (
                r"\documentclass{article}x".into(),
                "latex-fragment",
                "best-effort",
                "CONVERSION_FAILED",
            ),
            (
                format!("{}x", "x+".repeat(6000)),
                "omml",
                "strict",
                "OUTPUT_TOO_LARGE",
            ),
        ] {
            let request = json!({
                "content": source, "inputFormat": if code == "UNSUPPORTED_FORMAT" { "typst" } else { "latex" },
                "outputFormat": output, "mode": mode
            }).to_string();
            let result = unsafe {
                take_json(latexsnipper_formula_convert(
                    request.as_ptr(),
                    request.len(),
                ))
            };
            assert_eq!(result["error"]["code"], code);
        }
        let at_limit = json!({ "content": "x".repeat(MAX_FORMULA_RESULT_DATA_BYTES - 14) });
        assert!(bound_formula_data(at_limit).is_ok());
        assert_eq!(
            bound_formula_data(
                json!({ "content": "\"".repeat(MAX_FORMULA_RESULT_DATA_BYTES / 2) })
            )
            .unwrap_err()
            .code,
            "OUTPUT_TOO_LARGE"
        );
        let request = br#"{"content":"x","inputFormat":"latex","outputFormat":"omml"}"#;
        let healthy = unsafe {
            take_json(latexsnipper_formula_convert(
                request.as_ptr(),
                request.len(),
            ))
        };
        assert_eq!(healthy["ok"], true);
    }

    #[test]
    fn opaque_session_reuses_warmup_and_rejects_stale_handles() {
        let missing_request =
            unsafe { take_json(latexsnipper_session_create(std::ptr::null(), 0)) };
        assert_eq!(missing_request["ok"], false);
        assert_eq!(missing_request["error"]["code"], "INVALID_ARGUMENT");

        let models = tempfile::tempdir().unwrap();
        let create = json!({
            "modelsDir": models.path(),
            "runtimePreference": "cpu",
            "maxThreads": 1
        })
        .to_string();
        let created =
            unsafe { take_json(latexsnipper_session_create(create.as_ptr(), create.len())) };
        assert_eq!(created["ok"], true);
        assert_eq!(created["data"]["abiVersion"], 1);
        let handle = created["data"]["handle"].as_u64().unwrap();
        assert_ne!(handle, 0);

        let health = unsafe { take_json(latexsnipper_session_health(handle)) };
        assert_eq!(health["ok"], true);

        let warmup_request = br#"{"profile":"formula"}"#;
        let first = unsafe {
            take_json(latexsnipper_session_warmup(
                handle,
                warmup_request.as_ptr(),
                warmup_request.len(),
            ))
        };
        assert_eq!(first["ok"], true);
        assert_eq!(first["data"]["alreadyWarm"], false);
        let second = unsafe {
            take_json(latexsnipper_session_warmup(
                handle,
                warmup_request.as_ptr(),
                warmup_request.len(),
            ))
        };
        assert_eq!(second["ok"], true);
        assert_eq!(second["data"]["alreadyWarm"], true);

        let invalid_request = br#"{"profile":"formula"}"#;
        let invalid_input = unsafe {
            take_json(latexsnipper_session_recognize_bytes(
                handle,
                invalid_request.as_ptr(),
                invalid_request.len(),
                std::ptr::null(),
                0,
            ))
        };
        assert_eq!(invalid_input["ok"], false);
        assert_eq!(invalid_input["error"]["code"], "INVALID_ARGUMENT");

        let closed = unsafe { take_json(latexsnipper_session_close(handle)) };
        assert_eq!(closed["ok"], true);
        assert_eq!(closed["data"]["closed"], true);

        let stale = unsafe { take_json(latexsnipper_session_health(handle)) };
        assert_eq!(stale["ok"], false);
        assert_eq!(stale["error"]["code"], "SESSION_NOT_FOUND");
        let duplicate_close = unsafe { take_json(latexsnipper_session_close(handle)) };
        assert_eq!(duplicate_close["ok"], false);
        assert_eq!(duplicate_close["error"]["code"], "SESSION_NOT_FOUND");
    }
}
