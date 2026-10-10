//! Shared model-free formula bindings used by full and conversion-only builds.
use crate::api;
use latexsnipper_api_types::{ApiEnvelopeV3, ApiErrorV3};
use latexsnipper_conversion::{
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn api_info_v3() -> JsValue {
    serialize_v3_to_js(&ApiEnvelopeV3::success(
        api::ApiInfoV3::current(),
        Vec::new(),
    ))
}

/// Query model-free formula direction/mode support for the WASM target.
#[wasm_bindgen]
pub fn formula_capabilities_v3() -> JsValue {
    serialize_v3_to_js(&ApiEnvelopeV3::success(
        CapabilityRegistry::formula_conversions(CapabilityTarget::Wasm32UnknownUnknown),
        Vec::new(),
    ))
}

/// Convert a formula string without changing recognition or loaded-model state.
/// Omitted mode defaults to strict; source and reconstruction budgets apply.
#[wasm_bindgen]
pub fn convert_formula_v3(
    content: &str,
    input_format: &str,
    target_format: &str,
    mode: Option<String>,
) -> JsValue {
    let result = convert_formula_value(content, input_format, target_format, mode.as_deref());
    let envelope = match result {
        Ok(data) => ApiEnvelopeV3::success(data, Vec::new()),
        Err(error) => ApiEnvelopeV3::failure(error, Vec::new()),
    };
    serialize_v3_to_js(&envelope)
}

fn convert_formula_value(
    content: &str,
    input_format: &str,
    target_format: &str,
    mode: Option<&str>,
) -> Result<serde_json::Value, ApiErrorV3> {
    let failure = |code: &str, message: &str, detail: Option<String>| ApiErrorV3 {
        code: code.to_owned(),
        message: message.to_owned(),
        recoverable: false,
        details: detail.map(|detail| serde_json::json!({ "detail": detail })),
    };
    let input: FormulaInputFormat = serde_json::from_value(serde_json::json!(input_format
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")))
    .map_err(|_| failure("INVALID_ARGUMENT", "Unknown formula input format.", None))?;
    let output = output_format(&target_format.trim().replace('-', "_"))
        .ok_or_else(|| failure("INVALID_ARGUMENT", "Unknown formula output format.", None))?;
    let mode: FormulaConversionMode = serde_json::from_value(serde_json::json!(mode
        .unwrap_or("strict")
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")))
    .map_err(|_| {
        failure(
            "INVALID_ARGUMENT",
            "Mode must be strict or best-effort.",
            None,
        )
    })?;
    let capability = CapabilityRegistry::formula_conversion(
        input,
        output,
        mode,
        CapabilityTarget::Wasm32UnknownUnknown,
    );
    if !capability.available {
        return Err(failure(
            "UNSUPPORTED_FORMAT",
            "The formula conversion route is not supported.",
            capability.unavailable_reason.map(str::to_owned),
        ));
    }
    let content = DocumentConverter::convert_formula_string(content, input, output, mode).map_err(
        |error| {
            let code = if matches!(
                error,
                latexsnipper_foundation::SnipperError::LimitExceeded(_)
            ) {
                "INPUT_TOO_LARGE"
            } else {
                "CONVERSION_FAILED"
            };
            failure(code, "Formula conversion failed.", Some(error.to_string()))
        },
    )?;
    Ok(serde_json::json!({ "content": content, "capability": capability }))
}

/// Additive bare-formula entry point; legacy `latex` still exports a document.
/// The capability describes the underlying display route; contentKind records
/// the subsequent shape-checked projection, not broader syntax/renderer support.
#[wasm_bindgen]
pub fn convert_formula_fragment_v3(
    content: &str,
    input_format: &str,
    mode: Option<String>,
) -> JsValue {
    let envelope = match convert_formula_fragment_value(content, input_format, mode.as_deref()) {
        Ok(data) => ApiEnvelopeV3::success(data, Vec::new()),
        Err(error) => ApiEnvelopeV3::failure(error, Vec::new()),
    };
    serialize_v3_to_js(&envelope)
}

fn convert_formula_fragment_value(
    content: &str,
    input_format: &str,
    mode: Option<&str>,
) -> Result<serde_json::Value, ApiErrorV3> {
    let mut data = convert_formula_value(content, input_format, "latex_display", mode)?;
    let source = data["content"]
        .as_str()
        .expect("formula result content is a string");
    let fragment = latexsnipper_conversion::formula_fragment::latex_display_to_fragment(source)
        .map_err(|error| ApiErrorV3 {
            code: if matches!(
                error,
                latexsnipper_foundation::SnipperError::LimitExceeded(_)
            ) {
                "INPUT_TOO_LARGE"
            } else {
                "CONVERSION_FAILED"
            }
            .into(),
            message: "Standalone formula projection failed.".into(),
            recoverable: false,
            details: Some(serde_json::json!({"detail": error.to_string()})),
        })?;
    data["content"] = serde_json::json!(fragment);
    data["contentKind"] = serde_json::json!("latex-fragment");
    Ok(data)
}

pub(crate) fn output_format(name: &str) -> Option<OutputFormat> {
    OutputFormat::all()
        .iter()
        .copied()
        .find(|format| format.name().eq_ignore_ascii_case(name))
}

pub(crate) fn serialize_v3_to_js<T: Serialize>(value: &T) -> JsValue {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    value.serialize(&serializer).unwrap_or_else(|error| {
        JsValue::from_str(&format!("WASM v3 response serialization failed: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragment_api_separates_document_display_inline_and_bare_content() {
        let input = "frac(a,b)";
        let mode = Some("best-effort");
        let fragment = convert_formula_fragment_value(input, "typst", mode).unwrap();
        assert_eq!(fragment["content"], "\\frac{a}{b}");
        assert_eq!(fragment["contentKind"], "latex-fragment");
        assert_eq!(fragment["capability"]["output"], "latex_display");
        let document = convert_formula_value(input, "typst", "latex", mode).unwrap();
        assert!(document["content"]
            .as_str()
            .unwrap()
            .contains("\\documentclass"));
        let inline = convert_formula_value(input, "typst", "markdown_inline", mode).unwrap();
        assert_eq!(inline["content"], "$\\frac{a}{b}$");
        assert!(convert_formula_fragment_value(input, "typst", None).is_err());
        assert_eq!(
            convert_formula_fragment_value("\\documentclass{article}x", "latex", mode)
                .unwrap_err()
                .code,
            "CONVERSION_FAILED"
        );
    }
}
