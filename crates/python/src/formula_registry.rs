//! Formal Python binding of the same native registered-conversion instance.
use std::ffi::{c_char, CStr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use latexsnipper_api_types::{ApiEnvelopeV3, ApiErrorV3};
use latexsnipper_ffi::application_c::latexsnipper_string_free;
use latexsnipper_ffi::formula_registry_c::*;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use serde::Serialize;
use serde_json::{json, value::RawValue, Value};

use crate::{json_to_python, python_error, BindingError};

type Result<T> = std::result::Result<T, ApiErrorV3>;
type JsonOperation = unsafe extern "C" fn(u64, *const u8, usize) -> *mut c_char;
type HandleOperation = extern "C" fn(u64) -> *mut c_char;
const MAX_REQUEST: usize = 1024 * 1024;
const MAX_RESPONSE: usize = 2 * 1024 * 1024 + 64 * 1024;

struct Response(*mut c_char);
impl Drop for Response {
    fn drop(&mut self) {
        // SAFETY: Only native-owned buffers from this linked ABI are received.
        unsafe { latexsnipper_string_free(self.0) };
    }
}

fn error(code: &str, message: &str) -> ApiErrorV3 {
    ApiErrorV3 {
        code: code.into(),
        message: message.into(),
        recoverable: false,
        details: None,
    }
}

fn decode(pointer: *mut c_char) -> Result<Value> {
    if pointer.is_null() {
        return Err(error(
            "INTERNAL",
            "Native formula response allocation failed.",
        ));
    }
    let response = Response(pointer);
    // SAFETY: The trusted linked C implementation returns one live NUL-terminated
    // allocation; the owner remains alive until all data is deserialized.
    let bytes = unsafe { CStr::from_ptr(response.0) }.to_bytes();
    if bytes.len() > MAX_RESPONSE {
        return Err(error(
            "OUTPUT_TOO_LARGE",
            "Native envelope exceeds the transfer budget.",
        ));
    }
    let envelope: ApiEnvelopeV3<Value> = serde_json::from_slice(bytes)
        .map_err(|_| error("INTERNAL", "Native formula response is not valid JSON."))?;
    if !envelope.has_valid_contract() {
        return Err(error(
            "INTERNAL",
            "Unsupported native formula response contract.",
        ));
    }
    if envelope.ok {
        envelope
            .data
            .ok_or_else(|| error("INTERNAL", "Native formula data is missing."))
    } else {
        Err(envelope
            .error
            .unwrap_or_else(|| error("INTERNAL", "Native formula error is missing.")))
    }
}

fn to_python_error(py: Python<'_>, value: ApiErrorV3) -> PyErr {
    let exception = python_error(
        py,
        BindingError {
            code: value.code,
            message: value.message,
            retryable: value.recoverable,
            detail: value.details.as_ref().map(Value::to_string),
        },
    );
    if let Some(details) = value.details {
        if let Ok(details) = json_to_python(py, details.to_string()) {
            let _ = exception.value(py).setattr("details", details);
        }
    }
    exception
}

fn to_python(py: Python<'_>, result: Result<Value>) -> PyResult<Py<PyAny>> {
    json_to_python(
        py,
        result
            .map_err(|value| to_python_error(py, value))?
            .to_string(),
    )
}

struct State {
    handle: AtomicU64,
    metadata: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Declaration {
    schema_version: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin_store: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<Box<RawValue>>,
}

impl State {
    fn create(plugin_store: Option<PathBuf>, profile_json: Option<String>) -> Result<Self> {
        let profile = match profile_json {
            Some(source) => {
                if source.len() > 16 * 1024 {
                    return Err(error("LIMIT_EXCEEDED", "Profile exceeds 16 KiB."));
                }
                // Preserve raw nested fields: parsing to Value would silently
                // erase duplicate keys before the closed native profile checks.
                Some(
                    RawValue::from_string(source)
                        .map_err(|_| error("INVALID_JSON", "Profile is not valid JSON."))?,
                )
            }
            None => None,
        };
        let bytes = serde_json::to_vec(&Declaration {
            schema_version: 1,
            plugin_store,
            profile,
        })
        .map_err(|_| {
            error(
                "INVALID_ARGUMENT",
                "Declaration cannot be serialized as UTF-8.",
            )
        })?;
        if bytes.len() > 32 * 1024 {
            return Err(error("LIMIT_EXCEEDED", "Creation JSON exceeds 32 KiB."));
        }
        // SAFETY: The owned byte vector lives through the synchronous ABI call.
        let mut metadata =
            decode(unsafe { latexsnipper_formula_registry_create(bytes.as_ptr(), bytes.len()) })?;
        let handle = metadata
            .get("handle")
            .and_then(Value::as_u64)
            .filter(|value| *value != 0)
            .ok_or_else(|| error("INTERNAL", "Native formula handle is missing."))?;
        metadata
            .as_object_mut()
            .ok_or_else(|| error("INTERNAL", "Native formula metadata is invalid."))?
            .remove("handle");
        Ok(Self {
            handle: AtomicU64::new(handle),
            metadata,
        })
    }

    fn handle(&self) -> Result<u64> {
        let handle = self.handle.load(Ordering::Acquire);
        if handle == 0 {
            Err(error(
                "FORMULA_REGISTRY_NOT_FOUND",
                "Formula registry is closed.",
            ))
        } else {
            Ok(handle)
        }
    }

    fn call(&self, operation: HandleOperation) -> Result<Value> {
        decode(operation(self.handle()?))
    }

    fn json(&self, bytes: &[u8], operation: JsonOperation) -> Result<Value> {
        if bytes.len() > MAX_REQUEST {
            return Err(error("LIMIT_EXCEEDED", "Request JSON exceeds 1 MiB."));
        }
        // SAFETY: A valid owned/borrowed buffer is retained until return, without
        // passing foreign pointers or callbacks into the registered backend.
        decode(unsafe { operation(self.handle()?, bytes.as_ptr(), bytes.len()) })
    }

    fn close(&self) -> Result<()> {
        let handle = self.handle.swap(0, Ordering::AcqRel);
        if handle != 0 {
            decode(latexsnipper_formula_registry_close(handle))?;
        }
        Ok(())
    }
}

impl Drop for State {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// One independent registry; valid finite host consent never re-prompts per call.
#[pyclass(frozen, module = "latexsnipper_core._native")]
pub struct FormulaRegistry {
    inner: State,
}

#[pymethods]
impl FormulaRegistry {
    #[new]
    #[pyo3(signature = (*, plugin_store=None, profile_json=None))]
    fn new(
        py: Python<'_>,
        plugin_store: Option<PathBuf>,
        profile_json: Option<String>,
    ) -> PyResult<Self> {
        py.detach(move || State::create(plugin_store, profile_json))
            .map(|inner| Self { inner })
            .map_err(|value| to_python_error(py, value))
    }

    #[getter]
    fn closed(&self) -> bool {
        self.inner.handle.load(Ordering::Acquire) == 0
    }

    fn info(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json_to_python(py, self.inner.metadata.to_string())
    }

    fn capabilities(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(
            py,
            py.detach(|| self.inner.call(latexsnipper_formula_registry_capabilities)),
        )
    }

    fn set_enabled(&self, py: Python<'_>, backend: &str, enabled: bool) -> PyResult<()> {
        if backend.len() > 96 {
            return Err(to_python_error(
                py,
                error("INVALID_ARGUMENT", "Backend ID exceeds 96 bytes."),
            ));
        }
        let bytes = json!({"backend":backend,"enabled":enabled})
            .to_string()
            .into_bytes();
        py.detach(move || {
            self.inner
                .json(&bytes, latexsnipper_formula_registry_set_enabled)
        })
        .map(|_| ())
        .map_err(|value| to_python_error(py, value))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (content, *, input_format, output_format, context_sha256, mode="strict", backend=None, fallback=None))]
    fn convert(
        &self,
        py: Python<'_>,
        content: &str,
        input_format: &str,
        output_format: &str,
        context_sha256: &str,
        mode: &str,
        backend: Option<&str>,
        fallback: Option<Vec<String>>,
    ) -> PyResult<Py<PyAny>> {
        let fallback = fallback.unwrap_or_default();
        if content.len() > 64 * 1024
            || input_format.len() > 96
            || output_format.len() > 96
            || context_sha256.len() > 64
            || mode.len() > 32
            || backend.is_some_and(|value| value.len() > 96)
            || fallback.len() > 3
            || fallback.iter().any(|value| value.len() > 96)
        {
            return Err(to_python_error(
                py,
                error(
                    "LIMIT_EXCEEDED",
                    "Formula request arguments exceed transfer budgets.",
                ),
            ));
        }
        let bytes = json!({"schemaVersion":1,"input":input_format,"output":output_format,
            "mode":mode,"contextSha256":context_sha256,"backend":backend,"content":content,
            "policy":{"schemaVersion":1,"backends":fallback}})
        .to_string()
        .into_bytes();
        to_python(
            py,
            py.detach(move || {
                self.inner
                    .json(&bytes, latexsnipper_formula_registry_convert)
            }),
        )
    }

    /// Closed approval JSON; only trusted host code may grant after human consent.
    fn approval(&self, py: Python<'_>, command_json: &str) -> PyResult<()> {
        if command_json.len() > 16 * 1024 {
            return Err(to_python_error(
                py,
                error("LIMIT_EXCEEDED", "Approval command exceeds 16 KiB."),
            ));
        }
        let bytes = command_json.as_bytes().to_vec();
        py.detach(move || {
            self.inner
                .json(&bytes, latexsnipper_formula_registry_approval)
        })
        .map(|_| ())
        .map_err(|value| to_python_error(py, value))
    }

    fn approvals(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(
            py,
            py.detach(|| self.inner.call(latexsnipper_formula_registry_approvals)),
        )
    }

    fn cancel(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.inner.call(latexsnipper_formula_registry_cancel))
            .map(|_| ())
            .map_err(|value| to_python_error(py, value))
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.inner.close())
            .map_err(|value| to_python_error(py, value))
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyResult<PyRef<'_, Self>> {
        slf.inner
            .handle()
            .map_err(|value| to_python_error(slf.py(), value))?;
        Ok(slf)
    }

    fn __exit__(
        &self,
        py: Python<'_>,
        _exception_type: &Bound<'_, PyAny>,
        _exception_value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }
}

#[cfg(all(test, feature = "wasi-formula"))]
#[path = "../../plugin-wasi/tests/support/formula_profile_store.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::*;
    fn request(source: &str) -> Vec<u8> {
        json!({"schemaVersion":1,"input":"latex","output":"omml","mode":"strict",
            "backend":null,"content":source,"contextSha256":"a".repeat(64)})
        .to_string()
        .into_bytes()
    }

    #[test]
    fn linked_native_reports_owned_buffers_and_independent_close() {
        let first = State::create(None, None).unwrap();
        let second = State::create(None, None).unwrap();
        assert_ne!(first.handle().unwrap(), second.handle().unwrap());
        assert!(first.metadata.get("handle").is_none());
        let report = first
            .json(
                &request(r"\frac{1}{2}"),
                latexsnipper_formula_registry_convert,
            )
            .unwrap();
        assert_eq!(report["outcome"]["kind"], "converted");
        first.close().unwrap();
        first.close().unwrap();
        assert!(first
            .call(latexsnipper_formula_registry_capabilities)
            .is_err());
        assert_eq!(
            second
                .json(&request("x^2"), latexsnipper_formula_registry_convert)
                .unwrap()["outcome"]["kind"],
            "converted"
        );
        second.call(latexsnipper_formula_registry_cancel).unwrap();
        assert!(second
            .json(&request("x^2"), latexsnipper_formula_registry_convert)
            .is_err());
        second.close().unwrap();
    }

    #[cfg(feature = "wasi-formula")]
    #[test]
    fn raw_profile_keeps_duplicate_fields_and_real_wasi_route_is_explicit() {
        assert!(State::create(
            Some("absent-store".into()),
            Some(r#"{"schemaVersion":1,"schemaVersion":2}"#.into())
        )
        .is_err());
        let (temporary, store) = fixture::store();
        let source = fixture::profile(false).to_string();
        let state = State::create(Some(temporary.path().join("store")), Some(source)).unwrap();
        let value =
            json!({"schemaVersion":1,"input":"fixture:power","output":"omml","mode":"best-effort",
            "backend":"fixture:profile","content":"x:2","contextSha256":"a".repeat(64)})
            .to_string()
            .into_bytes();
        assert_eq!(
            state
                .json(&value, latexsnipper_formula_registry_convert)
                .unwrap()["outcome"]["error"]["code"],
            "BACKEND_DISABLED"
        );
        state
            .json(
                br#"{"backend":"fixture:profile","enabled":true}"#,
                latexsnipper_formula_registry_set_enabled,
            )
            .unwrap();
        assert_eq!(
            state
                .json(&value, latexsnipper_formula_registry_convert)
                .unwrap()["outcome"]["kind"],
            "converted"
        );
        store.set_enabled("fixture.component", false).unwrap();
        assert_eq!(
            state
                .json(&value, latexsnipper_formula_registry_convert)
                .unwrap()["outcome"]["kind"],
            "rejected"
        );
        state.close().unwrap();
        drop(state);
        if let Some(python) = std::env::var_os("SNIPPER_PYTHON_REGISTRY_SMOKE") {
            // Test-only finite fuel window for observing live cancellation. The
            // one-second package timeout and production host maxima still apply.
            let (long_temporary, long_store) = fixture::store_with_fuel(100_000_000);
            let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
            let mut command = std::process::Command::new(python);
            if let Some(wheel) = std::env::var_os("SNIPPER_PYTHON_REGISTRY_WHEEL") {
                command
                    .arg(tests.join("wheel_smoke.py"))
                    .arg(wheel)
                    .arg("--expect-wasi");
            } else {
                command.arg(tests.join("registry_smoke.py"));
            }
            let output = command
                .arg("--fixture-store")
                .arg(long_temporary.path().join("store"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "installed Python profile smoke failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            drop(long_store);
            long_temporary.close().unwrap();
        }
        drop(store);
        temporary.close().unwrap();
    }
}
