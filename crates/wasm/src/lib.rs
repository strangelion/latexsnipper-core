use std::sync::Arc;

use js_sys::{Function, Reflect, Uint8Array};
use latexsnipper_ast::{DiagnosticLevel, Document, GeneratedContent};
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_engine::{EngineConfig, RecognizeMode, SnipperEngine};
use latexsnipper_image::PixelFormat;
use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend, SharedModelResolver};
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
use latexsnipper_syntax::markdown::MarkdownRenderer;
use latexsnipper_syntax::typst::TypstRenderer;
use latexsnipper_syntax::{Parser as _, Renderer as _};
use latexsnipper_tract::TractBackend;
use serde::Serialize;
use wasm_bindgen::prelude::*;

mod api;
mod capabilities;
mod error;
mod profiles;
mod state;

use error::{ApiDiagnostic, WasmError, WasmErrorCode, WasmResponse};
use profiles::{validate_profile, ProfileValidation};
use state::{MemoryLimits, STATE};

#[wasm_bindgen]
pub fn init() {
    log::info!("LaTeXSnipper WASM initialized");
}

#[wasm_bindgen]
pub fn api_info_v2() -> JsValue {
    response_to_js(WasmResponse::success(api::ApiInfo::current(), Vec::new()))
}

#[wasm_bindgen]
pub fn capabilities_v2() -> JsValue {
    STATE.with(|cell| {
        let state = cell.borrow();
        response_to_js(WasmResponse::success(
            capabilities::collect(&state.resolver, state.limits(), state.usage()),
            Vec::new(),
        ))
    })
}

#[wasm_bindgen]
pub fn load_model_v2(name: &str, bytes: Vec<u8>, expected_sha256: Option<String>) -> JsValue {
    if let Err(error) = validate_model_artifact(name, &bytes) {
        return error_to_js(error);
    }
    STATE.with(|cell| {
        let result = cell
            .borrow_mut()
            .load(name, bytes, expected_sha256.as_deref());
        result_to_js(result, Vec::new())
    })
}

#[wasm_bindgen]
pub fn unload_model_v2(name: &str) -> JsValue {
    STATE.with(|cell| result_to_js(cell.borrow_mut().unload(name), Vec::new()))
}

#[wasm_bindgen]
pub fn clear_models_v2() -> JsValue {
    STATE.with(|cell| {
        cell.borrow_mut().clear();
        response_to_js(WasmResponse::success(true, Vec::new()))
    })
}

#[wasm_bindgen]
pub fn loaded_models_v2() -> JsValue {
    STATE.with(|cell| response_to_js(WasmResponse::success(cell.borrow().list(), Vec::new())))
}

#[wasm_bindgen]
pub fn model_memory_v2() -> JsValue {
    STATE.with(|cell| {
        let state = cell.borrow();
        response_to_js(WasmResponse::success(
            serde_json::json!({
                "limits": state.limits(),
                "usage": state.usage(),
            }),
            Vec::new(),
        ))
    })
}

#[wasm_bindgen]
pub fn set_model_memory_limits_v2(
    per_artifact_bytes: u64,
    total_model_bytes: u64,
    max_image_pixels: u64,
) -> JsValue {
    STATE.with(|cell| {
        result_to_js(
            cell.borrow_mut().set_limits(MemoryLimits {
                per_artifact_bytes,
                total_model_bytes,
                max_image_pixels,
                profile: "custom",
            }),
            Vec::new(),
        )
    })
}

#[wasm_bindgen]
pub fn set_model_memory_profile_v2(profile: &str) -> JsValue {
    let limits = match profile {
        "balanced" => MemoryLimits::default(),
        "low-memory" | "low_memory" => MemoryLimits::low_memory(),
        _ => {
            return error_to_js(WasmError::new(
                WasmErrorCode::InvalidArgument,
                "Unknown memory profile; use 'balanced' or 'low-memory'",
            ));
        }
    };
    STATE.with(|cell| result_to_js(cell.borrow_mut().set_limits(limits), Vec::new()))
}

#[wasm_bindgen]
pub fn begin_model_update_v2() -> JsValue {
    STATE.with(|cell| result_to_js(cell.borrow_mut().begin_update(), Vec::new()))
}

#[wasm_bindgen]
pub fn commit_model_update_v2() -> JsValue {
    STATE.with(|cell| result_to_js(cell.borrow_mut().commit_update(), Vec::new()))
}

#[wasm_bindgen]
pub fn rollback_model_update_v2() -> JsValue {
    STATE.with(|cell| result_to_js(cell.borrow_mut().rollback_update(), Vec::new()))
}

#[wasm_bindgen]
pub fn cancel_recognition_v2() -> JsValue {
    STATE.with(|cell| cell.borrow_mut().request_cancellation());
    response_to_js(WasmResponse::success(true, Vec::new()))
}

#[wasm_bindgen]
pub async fn recognize_v2(width: u32, height: u32, pixels: Vec<u8>, mode: String) -> JsValue {
    recognize_internal(width, height, pixels, mode, None).await
}

#[wasm_bindgen]
pub async fn recognize_v2_with_progress(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    mode: String,
    progress: Function,
) -> JsValue {
    recognize_internal(width, height, pixels, mode, Some(progress)).await
}

async fn recognize_internal(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    mode: String,
    progress: Option<Function>,
) -> JsValue {
    if let Err(error) = emit_progress(progress.as_ref(), "validating", 0.05) {
        return error_to_js(error);
    }
    let image = match validate_image(width, height, pixels) {
        Ok(image) => image,
        Err(error) => return error_to_js(error),
    };
    let recognize_mode = match parse_mode(&mode) {
        Ok(mode) => mode,
        Err(error) => return error_to_js(error),
    };

    let (resolver, validation) = match prepare_profile(&mode) {
        Ok(value) => value,
        Err(error) => return error_to_js(error),
    };
    if let Err(error) = emit_progress(progress.as_ref(), "models-ready", 0.15) {
        return error_to_js(error);
    }

    let config = engine_config(&validation);
    let shared: SharedModelResolver = resolver as Arc<dyn latexsnipper_runtime::ModelResolver>;
    let engine =
        SnipperEngine::with_model_resolver(config, Box::new(TractBackend::new(None)), shared);

    if let Err(error) = emit_progress(progress.as_ref(), "inference", 0.25) {
        return error_to_js(error);
    }
    let document = match engine.recognize(image, recognize_mode).await {
        Ok(document) => document,
        Err(error) => {
            return error_to_js(
                WasmError::new(WasmErrorCode::InferenceFailed, error.to_string())
                    .at_stage("inference"),
            );
        }
    };

    let cancelled = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let cancelled = state.cancellation_requested();
        state.reset_cancellation();
        cancelled
    });
    if cancelled {
        return error_to_js(WasmError::recoverable(
            WasmErrorCode::Cancelled,
            "Recognition was cancelled at a cooperative stage boundary",
        ));
    }
    if let Err(error) = emit_progress(progress.as_ref(), "completed", 1.0) {
        return error_to_js(error);
    }

    let diagnostics = document.diagnostics.iter().map(api_diagnostic).collect();
    response_to_js(WasmResponse::success(document, diagnostics))
}

fn prepare_profile(
    mode: &str,
) -> Result<
    (
        Arc<latexsnipper_runtime::MemoryModelResolver>,
        ProfileValidation,
    ),
    WasmError,
> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.cancellation_requested() {
            state.reset_cancellation();
            return Err(WasmError::recoverable(
                WasmErrorCode::Cancelled,
                "Recognition was cancelled before inference started",
            ));
        }
        let canonical = canonical_mode(mode);
        let profile = validate_profile(&state.resolver, &canonical)?;
        if !profile.ready {
            return Err(WasmError::recoverable(
                WasmErrorCode::ModelArtifactMissing,
                format!("Model profile '{}' is not ready", profile.profile),
            )
            .with_details(serde_json::json!({ "missing": profile.missing })));
        }
        state.touch_all();
        Ok((state.resolver.clone(), profile))
    })
}

fn engine_config(profile: &ProfileValidation) -> EngineConfig {
    let mut config = EngineConfig::with_models_dir(std::path::PathBuf::from("/virtual-models"));
    for selected in &profile.variants {
        let Some((category, variant)) = selected.split_once('/') else {
            continue;
        };
        config = match category {
            "formula-det" => config.set_formula_det(variant),
            "formula-rec" => config.set_formula_rec(variant),
            "text-det" => config.set_text_det(variant),
            "text-rec" => config.set_text_rec(variant),
            "table-det" => config.set_table_det(variant),
            "table-struct" => config.set_table_struct(variant),
            _ => config,
        };
    }
    config
}

fn validate_image(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> Result<latexsnipper_image::SnipperImage, WasmError> {
    if width == 0 || height == 0 {
        return Err(WasmError::new(
            WasmErrorCode::InvalidImage,
            "Image width and height must be positive",
        ));
    }
    let pixel_count = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| WasmError::new(WasmErrorCode::ImageLimitExceeded, "Image is too large"))?;
    let limit = STATE.with(|cell| cell.borrow().limits().max_image_pixels);
    if pixel_count > limit {
        return Err(WasmError::new(
            WasmErrorCode::ImageLimitExceeded,
            format!("Image has {pixel_count} pixels; configured limit is {limit}"),
        ));
    }
    let expected_len = pixel_count
        .checked_mul(4)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| WasmError::new(WasmErrorCode::ImageLimitExceeded, "Image is too large"))?;
    if pixels.len() != expected_len {
        return Err(WasmError::new(
            WasmErrorCode::InvalidImage,
            format!(
                "RGBA pixel length mismatch: expected {expected_len} bytes, got {}",
                pixels.len()
            ),
        ));
    }
    Ok(latexsnipper_image::SnipperImage::new(
        width,
        height,
        PixelFormat::Rgba,
        pixels,
    ))
}

fn emit_progress(callback: Option<&Function>, stage: &str, progress: f64) -> Result<(), WasmError> {
    let Some(callback) = callback else {
        return Ok(());
    };
    let event = serde_wasm_bindgen::to_value(&serde_json::json!({
        "stage": stage,
        "progress": progress,
    }))
    .map_err(|error| {
        WasmError::new(WasmErrorCode::SerializationFailed, error.to_string()).at_stage("progress")
    })?;
    callback
        .call1(&JsValue::NULL, &event)
        .map_err(|_| {
            WasmError::recoverable(
                WasmErrorCode::InternalError,
                "The progress callback threw an exception",
            )
            .at_stage("progress")
        })
        .map(|_| ())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WasmArtifact {
    format: String,
    mime_type: String,
    suggested_file_name: Option<String>,
    text: Option<String>,
    #[serde(skip)]
    bytes: Option<Vec<u8>>,
    diagnostics: Vec<latexsnipper_ast::Diagnostic>,
    checksum: Option<String>,
    size_bytes: u64,
}

#[wasm_bindgen]
pub fn convert_v2(doc_json: &str, format: &str) -> JsValue {
    let document: Document = match serde_json::from_str(doc_json) {
        Ok(document) => document,
        Err(error) => {
            return error_to_js(
                WasmError::new(WasmErrorCode::InvalidArgument, error.to_string())
                    .at_stage("parse-document"),
            );
        }
    };
    let output_format = match output_format(format) {
        Some(format) => format,
        None => {
            return error_to_js(
                WasmError::new(
                    WasmErrorCode::UnsupportedFormat,
                    format!("Format '{format}' is unavailable in the WASM build"),
                )
                .with_details(serde_json::json!({
                    "available": OutputFormat::all().iter().map(OutputFormat::name).collect::<Vec<_>>()
                })),
            );
        }
    };
    let artifact = match DocumentConverter::new(output_format).convert_artifact(&document) {
        Ok(artifact) => artifact,
        Err(error) => {
            return error_to_js(
                WasmError::new(WasmErrorCode::ConversionFailed, error).at_stage("conversion"),
            );
        }
    };
    let diagnostics: Vec<_> = artifact.diagnostics.iter().map(api_diagnostic).collect();
    let (text, bytes) = match artifact.content {
        Some(GeneratedContent::Text(text)) => (Some(text), None),
        Some(GeneratedContent::Binary(bytes)) => (None, Some(bytes)),
        None => (artifact.text, None),
    };
    let value = WasmArtifact {
        format: artifact.format,
        mime_type: artifact
            .mime_type
            .unwrap_or_else(|| capabilities::mime_type(format).to_string()),
        suggested_file_name: Some(format!("latexsnipper.{}", output_format.extension())),
        text,
        size_bytes: artifact.size_bytes.unwrap_or(0),
        checksum: artifact.checksum_sha256,
        diagnostics: artifact.diagnostics,
        bytes,
    };
    artifact_response_to_js(value, diagnostics)
}

fn artifact_response_to_js(artifact: WasmArtifact, diagnostics: Vec<ApiDiagnostic>) -> JsValue {
    let bytes = artifact
        .bytes
        .as_ref()
        .map(|value| Uint8Array::from(value.as_slice()));
    let response = response_to_js(WasmResponse::success(artifact, diagnostics));
    if let Some(bytes) = bytes {
        if let Ok(data) = Reflect::get(&response, &JsValue::from_str("data")) {
            let _ = Reflect::set(&data, &JsValue::from_str("bytes"), &bytes);
        }
    }
    response
}

#[wasm_bindgen]
pub fn load_model(name: &str, bytes: &[u8]) -> Result<(), JsValue> {
    validate_model_artifact(name, bytes).map_err(wasm_error_to_js_exception)?;
    STATE.with(|cell| {
        cell.borrow_mut()
            .load(name, bytes.to_vec(), None)
            .map(|_| ())
            .map_err(wasm_error_to_js_exception)
    })
}

#[wasm_bindgen]
pub fn is_model_loaded(name: &str) -> bool {
    STATE.with(|cell| cell.borrow().is_loaded(name))
}

#[wasm_bindgen]
pub fn loaded_models() -> String {
    STATE.with(|cell| {
        let names: Vec<_> = cell
            .borrow()
            .list()
            .into_iter()
            .map(|artifact| artifact.name)
            .collect();
        serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Deprecated legacy synchronous recognition is intentionally disabled in browsers.
/// Use `recognize_v2`, which returns a Promise and does not create a nested runtime.
#[wasm_bindgen]
pub fn recognize(
    _width: u32,
    _height: u32,
    _pixels: &[u8],
    _mode: &str,
) -> Result<String, JsValue> {
    Err(JsValue::from_str(
        "Synchronous browser inference is disabled; use recognize_v2",
    ))
}

#[wasm_bindgen]
pub fn parse_latex(latex: &str) -> Result<JsValue, JsValue> {
    let document = LatexParser.parse(latex).map_err(err_to_js)?;
    to_js_value(&document)
}

#[wasm_bindgen]
pub fn render_latex(doc_json: &str) -> Result<String, JsValue> {
    LatexRenderer
        .render(&parse_document(doc_json)?)
        .map_err(err_to_js)
}

#[wasm_bindgen]
pub fn render_typst(doc_json: &str) -> Result<String, JsValue> {
    TypstRenderer
        .render(&parse_document(doc_json)?)
        .map_err(err_to_js)
}

#[wasm_bindgen]
pub fn render_markdown(doc_json: &str) -> Result<String, JsValue> {
    MarkdownRenderer
        .render(&parse_document(doc_json)?)
        .map_err(err_to_js)
}

#[wasm_bindgen]
pub fn convert_document(doc_json: &str, format: &str) -> Result<String, JsValue> {
    let document = parse_document(doc_json)?;
    let format = output_format(format)
        .ok_or_else(|| JsValue::from_str(&format!("Unknown WASM format: {format}")))?;
    DocumentConverter::new(format)
        .convert(&document)
        .map_err(err_to_js)
}

#[wasm_bindgen]
pub fn convert_from_json(doc_json: &str, format: &str) -> Result<String, JsValue> {
    convert_document(doc_json, format)
}

#[wasm_bindgen]
pub fn formula_to_latex(formula_json: &str) -> Result<String, JsValue> {
    let formula: latexsnipper_ast::Formula =
        serde_json::from_str(formula_json).map_err(err_to_js)?;
    Ok(formula.as_latex().to_string())
}

#[wasm_bindgen]
pub fn available_formats() -> String {
    let formats: Vec<_> = OutputFormat::all().iter().map(OutputFormat::name).collect();
    serde_json::to_string(&formats).unwrap_or_else(|_| "[]".to_string())
}

#[wasm_bindgen]
pub fn formula_to_document(latex: &str, format: &str) -> Result<String, JsValue> {
    let document = LatexParser.parse(latex).map_err(err_to_js)?;
    let json = serde_json::to_string(&document).map_err(err_to_js)?;
    convert_document(&json, format)
}

#[wasm_bindgen]
pub fn health_check() -> String {
    "ok".to_string()
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn canonical_mode(mode: &str) -> String {
    match mode.to_ascii_lowercase().as_str() {
        "f" | "formula" => "formula".to_string(),
        "t" | "text" => "text".to_string(),
        "m" | "mixed" => "mixed".to_string(),
        "fl" | "formula_layout" => "formula_layout".to_string(),
        "tbl" | "table" => "table".to_string(),
        "hw" | "handwriting" => "handwriting".to_string(),
        _ => mode.to_ascii_lowercase(),
    }
}

fn parse_mode(mode: &str) -> Result<RecognizeMode, WasmError> {
    match canonical_mode(mode).as_str() {
        "formula" => Ok(RecognizeMode::Formula),
        "text" => Ok(RecognizeMode::Text),
        "mixed" => Ok(RecognizeMode::Mixed),
        "formula_layout" => Ok(RecognizeMode::FormulaLayout),
        "table" => Ok(RecognizeMode::Table),
        "handwriting" => Ok(RecognizeMode::Handwriting),
        _ => Err(WasmError::new(
            WasmErrorCode::UnsupportedMode,
            format!("Unknown recognition mode: {mode}"),
        )),
    }
}

fn output_format(name: &str) -> Option<OutputFormat> {
    OutputFormat::all()
        .iter()
        .copied()
        .find(|format| format.name().eq_ignore_ascii_case(name))
}

fn parse_document(doc_json: &str) -> Result<Document, JsValue> {
    serde_json::from_str(doc_json).map_err(err_to_js)
}

fn validate_model_artifact(name: &str, bytes: &[u8]) -> Result<(), WasmError> {
    if !name.to_ascii_lowercase().ends_with(".onnx") {
        return Ok(());
    }
    let backend = TractBackend::new(None);
    backend
        .create_session(
            &ModelHandle::with_bytes(name, bytes.to_vec()),
            AccelerationMode::Cpu,
        )
        .map(|_| ())
        .map_err(|error| {
            WasmError::new(
                WasmErrorCode::ModelArtifactInvalid,
                format!("Invalid or unsupported ONNX artifact '{name}': {error}"),
            )
            .at_stage("model-validation")
        })
}

fn api_diagnostic(diagnostic: &latexsnipper_ast::Diagnostic) -> ApiDiagnostic {
    ApiDiagnostic {
        level: match diagnostic.level {
            DiagnosticLevel::Info => "info",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Error => "error",
        },
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        stage: Some("recognition".to_string()),
    }
}

fn response_to_js<T: Serialize>(response: WasmResponse<T>) -> JsValue {
    serde_wasm_bindgen::to_value(&response).unwrap_or_else(|error| {
        JsValue::from_str(&format!("WASM response serialization failed: {error}"))
    })
}

fn result_to_js<T: Serialize>(
    result: Result<T, WasmError>,
    diagnostics: Vec<ApiDiagnostic>,
) -> JsValue {
    match result {
        Ok(value) => response_to_js(WasmResponse::success(value, diagnostics)),
        Err(error) => response_to_js(WasmResponse::<()>::failure(error, diagnostics)),
    }
}

fn error_to_js(error: WasmError) -> JsValue {
    response_to_js(WasmResponse::<()>::failure(error, Vec::new()))
}

fn wasm_error_to_js_exception(error: WasmError) -> JsValue {
    serde_json::to_string(&WasmResponse::<()>::failure(error, Vec::new()))
        .map(|value| JsValue::from_str(&value))
        .unwrap_or_else(|_| JsValue::from_str("WASM operation failed"))
}

fn err_to_js<E: std::fmt::Display>(error: E) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(err_to_js)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_registry_drives_public_format_list() {
        let exported: Vec<String> = serde_json::from_str(&available_formats()).unwrap();
        let expected: Vec<_> = OutputFormat::all()
            .iter()
            .map(|format| format.name().to_string())
            .collect();
        assert_eq!(exported, expected);
    }

    #[test]
    fn image_validation_rejects_overflow_and_length_mismatch() {
        let mismatch = validate_image(2, 2, vec![0; 15]).unwrap_err();
        assert_eq!(mismatch.code, WasmErrorCode::InvalidImage);

        let oversized = validate_image(u32::MAX, u32::MAX, Vec::new()).unwrap_err();
        assert_eq!(oversized.code, WasmErrorCode::ImageLimitExceeded);
    }
}
