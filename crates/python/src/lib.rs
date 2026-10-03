//! Persistent PyO3 adapter for the application-facing recognition session.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use latexsnipper_api_types::RecognitionProfile;
use latexsnipper_ast::InputFormat;
use latexsnipper_conversion::{
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};
use latexsnipper_engine::application::{ApplicationError, RecognitionOptions, RuntimePreference};
use latexsnipper_engine::application::{
    CancellationToken as CoreCancellationToken, RecognitionControl,
};
use latexsnipper_engine::{
    DocumentParseMode, RecognitionIntegrationApi, RecognitionRequest, RecognitionResult,
    RecognitionSession,
};
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use serde::Serialize;
use serde_json::{Map, Value};

const MAX_THREADS: usize = 256;
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;

pyo3::create_exception!(
    latexsnipper_core,
    LaTeXSnipperError,
    PyException,
    "A stable LaTeXSnipper Core application error."
);

#[derive(Debug)]
struct BindingError {
    code: String,
    message: String,
    detail: Option<String>,
    retryable: bool,
}

impl BindingError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_ARGUMENT".to_string(),
            message: message.into(),
            detail: None,
            retryable: false,
        }
    }

    fn closed() -> Self {
        Self {
            code: "INVALID_INPUT".to_string(),
            message: "The recognition session is closed.".to_string(),
            detail: None,
            retryable: false,
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "INTERNAL".to_string(),
            message: message.into(),
            detail: None,
            retryable: false,
        }
    }
}

impl From<ApplicationError> for BindingError {
    fn from(error: ApplicationError) -> Self {
        let code = serde_json::to_value(error.code)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "INTERNAL".to_string());
        Self {
            code,
            message: error.message,
            detail: error.detail,
            retryable: error.retryable,
        }
    }
}

fn python_error(py: Python<'_>, error: BindingError) -> PyErr {
    let exception = LaTeXSnipperError::new_err(error.message);
    let value = exception.value(py);
    let _ = value.setattr("code", error.code);
    let _ = value.setattr("detail", error.detail);
    let _ = value.setattr("retryable", error.retryable);
    exception
}

fn json_to_python(py: Python<'_>, json: String) -> PyResult<Py<PyAny>> {
    let module = PyModule::import(py, "json")?;
    Ok(module.call_method1("loads", (json,))?.unbind())
}

fn serialize_json(value: &impl Serialize) -> Result<String, BindingError> {
    serde_json::to_string(value).map_err(|error| {
        BindingError::internal(format!("The Core result could not be serialized: {error}"))
    })
}

fn parse_runtime_preference(value: &str) -> Result<RuntimePreference, BindingError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(RuntimePreference::Auto),
        "cpu" => Ok(RuntimePreference::Cpu),
        "gpu" => Ok(RuntimePreference::Gpu),
        _ => Err(BindingError::invalid(
            "runtime_preference must be one of: auto, cpu, gpu.",
        )),
    }
}

fn parse_parse_mode(value: &str) -> Result<DocumentParseMode, BindingError> {
    DocumentParseMode::from_label(value).ok_or_else(|| {
        BindingError::invalid(
            "parse_mode must be one of: specialized, openocr-text, opendoc-hybrid.",
        )
    })
}

fn parse_profile(value: &str) -> Result<RecognitionProfile, BindingError> {
    let compact = value.trim().to_ascii_lowercase().replace(['_', '-'], "");
    match compact.as_str() {
        "formula" => Ok(RecognitionProfile::Formula),
        "croppedformula" => Ok(RecognitionProfile::CroppedFormula),
        "text" => Ok(RecognitionProfile::Text),
        "mixed" => Ok(RecognitionProfile::Mixed),
        "table" => Ok(RecognitionProfile::Table),
        "handwriting" => Ok(RecognitionProfile::Handwriting),
        "formulalayout" => Ok(RecognitionProfile::FormulaLayout),
        _ => Err(BindingError::invalid(
            "profile must be one of: formula, croppedFormula, text, mixed, table, handwriting, formulaLayout.",
        )),
    }
}

fn parse_input_format(value: Option<&str>) -> Result<Option<InputFormat>, BindingError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let compact = value
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', '/', '.'], "");
    let format = match compact.as_str() {
        "png" | "imagepng" => InputFormat::ImagePng,
        "jpg" | "jpeg" | "imagejpeg" => InputFormat::ImageJpeg,
        "webp" | "imagewebp" => InputFormat::ImageWebp,
        "bmp" | "imagebmp" => InputFormat::ImageBmp,
        "tif" | "tiff" | "imagetiff" => InputFormat::ImageTiff,
        "gif" | "imagegif" => InputFormat::ImageGif,
        "svg" | "imagesvg" => InputFormat::ImageSvg,
        "unknown" => InputFormat::Unknown,
        _ => {
            return Err(BindingError::invalid(
                "format_hint must identify png, jpeg, webp, bmp, tiff, gif, svg, or unknown.",
            ));
        }
    };
    Ok(Some(format))
}

fn parse_output_formats(
    formats: Option<Vec<String>>,
) -> Result<Vec<(String, OutputFormat)>, BindingError> {
    let mut seen = BTreeSet::new();
    let mut parsed = Vec::new();
    for value in formats.unwrap_or_default() {
        let key = value.trim().to_ascii_lowercase().replace('-', "_");
        let format = match key.as_str() {
            "latex" => OutputFormat::Latex,
            "latex_display" | "display_latex" => OutputFormat::LatexDisplay,
            "latex_equation" | "equation_latex" => OutputFormat::LatexEquation,
            "typst" => OutputFormat::Typst,
            "markdown_inline" => OutputFormat::MarkdownInline,
            "markdown" | "markdown_block" => OutputFormat::MarkdownBlock,
            "mathml" => OutputFormat::MathML,
            "omml" => OutputFormat::OMML,
            "html" => OutputFormat::Html,
            _ => {
                return Err(BindingError::invalid(format!(
                    "unsupported output format: {value}"
                )));
            }
        };
        if seen.insert(key.clone()) {
            parsed.push((key, format));
        }
    }
    Ok(parsed)
}

fn formula_arguments(
    input: &str,
    output: &str,
    mode: &str,
) -> Result<(FormulaInputFormat, OutputFormat, FormulaConversionMode), BindingError> {
    let input_label = input.trim().to_ascii_lowercase().replace('_', "-");
    let input = FormulaInputFormat::all()
        .iter()
        .copied()
        .find(|format| format.name() == input_label)
        .ok_or_else(|| BindingError::invalid(format!("unknown formula input format: {input}")))?;
    let output = parse_output_formats(Some(vec![output.to_owned()]))?[0].1;
    let mode = match mode.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "strict" => FormulaConversionMode::Strict,
        "best-effort" => FormulaConversionMode::BestEffort,
        _ => return Err(BindingError::invalid("mode must be strict or best-effort.")),
    };
    let capability =
        CapabilityRegistry::formula_conversion(input, output, mode, CapabilityTarget::Native);
    if !capability.available {
        return Err(BindingError {
            code: "UNSUPPORTED_FORMAT".into(),
            message: "The formula conversion route is not supported.".into(),
            detail: capability.unavailable_reason.map(str::to_owned),
            retryable: false,
        });
    }
    Ok((input, output, mode))
}

/// Convert formula strings without loading recognition models or creating a session.
#[pyfunction]
#[pyo3(signature = (content, *, input_format, output_format, mode="strict"))]
fn convert_formula(
    py: Python<'_>,
    content: String,
    input_format: &str,
    output_format: &str,
    mode: &str,
) -> PyResult<String> {
    let (input, output, mode) = formula_arguments(input_format, output_format, mode)
        .map_err(|error| python_error(py, error))?;
    py.detach(move || DocumentConverter::convert_formula_string(&content, input, output, mode))
        .map_err(|error| python_error(py, BindingError::from(ApplicationError::from(error))))
}

/// Return native direction/mode support from the shared executable registry.
#[pyfunction]
fn formula_conversion_capabilities(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let json = serialize_json(&CapabilityRegistry::formula_conversions(
        CapabilityTarget::Native,
    ))
    .map_err(|error| python_error(py, error))?;
    json_to_python(py, json)
}

fn recognition_options(
    parse_mode: Option<&str>,
    timeout_ms: Option<u64>,
    strict: bool,
    include_source_asset: bool,
) -> Result<RecognitionOptions, BindingError> {
    if timeout_ms.is_some_and(|value| value > MAX_TIMEOUT_MS) {
        return Err(BindingError::invalid(format!(
            "timeout_ms must not exceed {MAX_TIMEOUT_MS}."
        )));
    }
    Ok(RecognitionOptions {
        parse_mode: parse_mode.map(parse_parse_mode).transpose()?,
        include_source_asset,
        timeout: timeout_ms.map(Duration::from_millis),
        strict,
    })
}

fn recognize(
    session: &mut RecognitionSession,
    request: RecognitionRequest,
    formats: Vec<(String, OutputFormat)>,
    cancellation: Option<CoreCancellationToken>,
) -> Result<String, BindingError> {
    let result = session
        .recognize_with_control(
            request,
            RecognitionControl {
                progress: None,
                cancellation,
            },
        )
        .map_err(BindingError::from)?;
    recognition_result_json(&result, formats)
}

fn recognition_result_json(
    result: &RecognitionResult,
    formats: Vec<(String, OutputFormat)>,
) -> Result<String, BindingError> {
    let mut value = serde_json::to_value(result).map_err(|error| {
        BindingError::internal(format!(
            "The recognition result could not be serialized: {error}"
        ))
    })?;
    let outputs = formats
        .into_iter()
        .map(|(name, format)| {
            result
                .to_format(format)
                .map(|output| (name, Value::String(output)))
                .map_err(BindingError::from)
        })
        .collect::<Result<Map<String, Value>, BindingError>>()?;
    value
        .as_object_mut()
        .ok_or_else(|| BindingError::internal("The recognition result is not a JSON object."))?
        .insert("outputs".to_string(), Value::Object(outputs));
    serialize_json(&value)
}

/// One independent, process-lifetime recognition session.
#[pyclass(module = "latexsnipper_core._native")]
pub struct Session {
    inner: Mutex<Option<RecognitionSession>>,
}

/// Cloneable cancellation signal observed at safe Core pipeline boundaries.
#[pyclass(module = "latexsnipper_core._native")]
pub struct CancellationToken {
    inner: CoreCancellationToken,
}

#[pymethods]
impl CancellationToken {
    #[new]
    fn new() -> Self {
        Self {
            inner: CoreCancellationToken::new(),
        }
    }

    fn cancel(&self) {
        self.inner.cancel();
    }

    #[getter]
    fn cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
}

impl Session {
    fn from_core(session: RecognitionSession) -> Self {
        Self {
            inner: Mutex::new(Some(session)),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Option<RecognitionSession>>, BindingError> {
        self.inner
            .lock()
            .map_err(|_| BindingError::internal("The Python session lock is poisoned."))
    }

    fn with_session<T>(
        &self,
        operation: impl FnOnce(&mut RecognitionSession) -> Result<T, BindingError>,
    ) -> Result<T, BindingError> {
        let mut session = self.lock()?;
        let session = session.as_mut().ok_or_else(BindingError::closed)?;
        operation(session)
    }

    fn close_core(&self) -> Result<(), BindingError> {
        let mut guard = self.lock()?;
        if let Some(mut session) = guard.take() {
            session.close();
        }
        Ok(())
    }
}

#[pymethods]
impl Session {
    #[new]
    #[pyo3(signature = (
        models_dir,
        *,
        quality_baselines_dir=None,
        provider_smoke_fixture=None,
        runtime_preference="auto",
        parse_mode="specialized",
        max_threads=4
    ))]
    fn new(
        py: Python<'_>,
        models_dir: PathBuf,
        quality_baselines_dir: Option<PathBuf>,
        provider_smoke_fixture: Option<PathBuf>,
        runtime_preference: &str,
        parse_mode: &str,
        max_threads: usize,
    ) -> PyResult<Self> {
        if models_dir.as_os_str().is_empty() {
            return Err(python_error(
                py,
                BindingError::invalid("models_dir must not be empty."),
            ));
        }
        if quality_baselines_dir
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(python_error(
                py,
                BindingError::invalid("quality_baselines_dir must not be empty when provided."),
            ));
        }
        if provider_smoke_fixture
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(python_error(
                py,
                BindingError::invalid("provider_smoke_fixture must not be empty when provided."),
            ));
        }
        if !(1..=MAX_THREADS).contains(&max_threads) {
            return Err(python_error(
                py,
                BindingError::invalid(format!("max_threads must be between 1 and {MAX_THREADS}.")),
            ));
        }
        let runtime_preference = parse_runtime_preference(runtime_preference)
            .map_err(|error| python_error(py, error))?;
        let parse_mode = parse_parse_mode(parse_mode).map_err(|error| python_error(py, error))?;
        let session = py.detach(move || {
            let mut builder = RecognitionSession::builder()
                .models_dir(models_dir)
                .runtime_preference(runtime_preference)
                .parse_mode(parse_mode)
                .max_threads(max_threads);
            if let Some(path) = quality_baselines_dir {
                builder = builder.quality_baselines_dir(path);
            }
            if let Some(path) = provider_smoke_fixture {
                builder = builder.provider_smoke_fixture(path);
            }
            builder.build().map_err(BindingError::from)
        });
        session
            .map(Self::from_core)
            .map_err(|error| python_error(py, error))
    }

    #[getter]
    fn closed(&self, py: Python<'_>) -> PyResult<bool> {
        self.lock()
            .map(|session| session.is_none())
            .map_err(|error| python_error(py, error))
    }

    fn health(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let result = py.detach(|| {
            self.with_session(|session| {
                let report = session.health_check().map_err(BindingError::from)?;
                serialize_json(&report)
            })
        });
        json_to_python(py, result.map_err(|error| python_error(py, error))?)
    }

    fn capabilities(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let result =
            py.detach(|| self.with_session(|session| serialize_json(&session.capabilities())));
        json_to_python(py, result.map_err(|error| python_error(py, error))?)
    }

    #[pyo3(signature = (profile="formula"))]
    fn warmup(&self, py: Python<'_>, profile: &str) -> PyResult<Py<PyAny>> {
        let profile = parse_profile(profile).map_err(|error| python_error(py, error))?;
        let result = py.detach(|| {
            self.with_session(|session| {
                let report = session.warmup(profile).map_err(BindingError::from)?;
                serialize_json(&report)
            })
        });
        json_to_python(py, result.map_err(|error| python_error(py, error))?)
    }

    fn reload_models(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let result = py.detach(|| {
            self.with_session(|session| {
                let report = RecognitionIntegrationApi::reload_models(session)
                    .map_err(BindingError::from)?;
                serialize_json(&report)
            })
        });
        json_to_python(py, result.map_err(|error| python_error(py, error))?)
    }

    #[pyo3(signature = (
        path,
        *,
        profile="formula",
        parse_mode=None,
        timeout_ms=None,
        strict=false,
        include_source_asset=false,
        cancellation=None,
        formats=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn recognize_path(
        &self,
        py: Python<'_>,
        path: PathBuf,
        profile: &str,
        parse_mode: Option<&str>,
        timeout_ms: Option<u64>,
        strict: bool,
        include_source_asset: bool,
        cancellation: Option<PyRef<'_, CancellationToken>>,
        formats: Option<Vec<String>>,
    ) -> PyResult<Py<PyAny>> {
        let profile = parse_profile(profile).map_err(|error| python_error(py, error))?;
        let options = recognition_options(parse_mode, timeout_ms, strict, include_source_asset)
            .map_err(|error| python_error(py, error))?;
        let formats = parse_output_formats(formats).map_err(|error| python_error(py, error))?;
        let cancellation = cancellation.map(|token| token.inner.clone());
        let request = RecognitionRequest::from_path(path)
            .with_profile(profile)
            .with_options(options);
        let result = py.detach(move || {
            self.with_session(|session| recognize(session, request, formats, cancellation))
        });
        json_to_python(py, result.map_err(|error| python_error(py, error))?)
    }

    #[pyo3(signature = (
        data,
        *,
        format_hint=None,
        profile="formula",
        parse_mode=None,
        timeout_ms=None,
        strict=false,
        include_source_asset=false,
        cancellation=None,
        formats=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn recognize_bytes(
        &self,
        py: Python<'_>,
        data: PyBuffer<u8>,
        format_hint: Option<&str>,
        profile: &str,
        parse_mode: Option<&str>,
        timeout_ms: Option<u64>,
        strict: bool,
        include_source_asset: bool,
        cancellation: Option<PyRef<'_, CancellationToken>>,
        formats: Option<Vec<String>>,
    ) -> PyResult<Py<PyAny>> {
        if data.len_bytes() == 0 || data.len_bytes() > MAX_INPUT_BYTES {
            return Err(python_error(
                py,
                BindingError::invalid(format!(
                    "data must contain between 1 and {MAX_INPUT_BYTES} bytes."
                )),
            ));
        }
        let bytes = data.to_vec(py)?;
        let hint = parse_input_format(format_hint).map_err(|error| python_error(py, error))?;
        let profile = parse_profile(profile).map_err(|error| python_error(py, error))?;
        let options = recognition_options(parse_mode, timeout_ms, strict, include_source_asset)
            .map_err(|error| python_error(py, error))?;
        let formats = parse_output_formats(formats).map_err(|error| python_error(py, error))?;
        let cancellation = cancellation.map(|token| token.inner.clone());
        let request = RecognitionRequest::from_bytes(bytes, hint)
            .with_profile(profile)
            .with_options(options);
        let result = py.detach(move || {
            self.with_session(|session| recognize(session, request, formats, cancellation))
        });
        json_to_python(py, result.map_err(|error| python_error(py, error))?)
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.close_core())
            .map_err(|error| python_error(py, error))
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
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

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.close_core();
    }
}

#[pymodule]
fn _native(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(convert_formula, module)?)?;
    module.add_function(wrap_pyfunction!(formula_conversion_capabilities, module)?)?;
    module.add_class::<Session>()?;
    module.add_class::<CancellationToken>()?;
    module.add("LaTeXSnipperError", py.get_type::<LaTeXSnipperError>())?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_arguments_match_the_shared_registry() {
        for &input in FormulaInputFormat::all() {
            for &output in OutputFormat::all() {
                for (label, mode) in [
                    ("strict", FormulaConversionMode::Strict),
                    ("best-effort", FormulaConversionMode::BestEffort),
                ] {
                    let route = CapabilityRegistry::formula_conversion(
                        input,
                        output,
                        mode,
                        CapabilityTarget::Native,
                    );
                    let parsed = formula_arguments(input.name(), output.name(), label);
                    assert_eq!(parsed.is_ok(), route.available);
                    if !route.available {
                        assert_eq!(parsed.unwrap_err().code, "UNSUPPORTED_FORMAT");
                    }
                }
            }
        }
        for (input, output, mode) in [
            ("ole", "omml", "best-effort"),
            ("latex", "pdf", "strict"),
            ("latex", "omml", "lossless"),
        ] {
            assert_eq!(
                formula_arguments(input, output, mode).unwrap_err().code,
                "INVALID_ARGUMENT"
            );
        }
    }

    #[test]
    fn profile_aliases_are_stable() {
        assert_eq!(
            parse_profile("cropped_formula").unwrap(),
            RecognitionProfile::CroppedFormula
        );
        assert_eq!(
            parse_profile("formula-layout").unwrap(),
            RecognitionProfile::FormulaLayout
        );
        assert_eq!(
            parse_profile("unknown").unwrap_err().code,
            "INVALID_ARGUMENT"
        );
    }

    #[test]
    fn output_formats_are_deduplicated_and_validated() {
        let formats = parse_output_formats(Some(vec![
            "latex".to_string(),
            "omml".to_string(),
            "latex".to_string(),
        ]))
        .unwrap();
        assert_eq!(formats.len(), 2);
        assert_eq!(formats[0].0, "latex");
        assert_eq!(formats[1].0, "omml");
        assert!(parse_output_formats(Some(vec!["pdf".to_string()])).is_err());
    }

    #[test]
    fn timeout_is_bounded() {
        assert!(recognition_options(None, Some(MAX_TIMEOUT_MS), false, false).is_ok());
        assert!(recognition_options(None, Some(MAX_TIMEOUT_MS + 1), false, false).is_err());
    }

    #[test]
    fn python_session_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Session>();
    }

    #[test]
    fn cancellation_token_is_cloneable_shared_state() {
        let token = CoreCancellationToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
    }
}
