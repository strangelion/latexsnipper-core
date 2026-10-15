//! Shared full/conversion-only exports for the Core editing protocol.
use latexsnipper_api_types::{ApiEnvelopeV3, ApiErrorV3};
use latexsnipper_conversion::formula_completion::{
    FormulaCompletionApplyRequest, FormulaCompletionError, FormulaCompletionRequest,
    FORMULA_COMPLETION_MAX_BYTES, FORMULA_COMPLETION_MAX_CANDIDATES, FORMULA_COMPLETION_MAX_DEPTH,
    FORMULA_COMPLETION_MAX_NODES, FORMULA_COMPLETION_VERSION,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_DATA_BYTES: usize = 1024 * 1024;

fn error(code: &str, message: impl Into<String>) -> ApiErrorV3 {
    ApiErrorV3 {
        code: code.into(),
        message: message.into(),
        recoverable: false,
        details: None,
    }
}
fn completion_error(value: FormulaCompletionError) -> ApiErrorV3 {
    error(
        match value {
            FormulaCompletionError::InvalidCursor => "INVALID_CURSOR",
            FormulaCompletionError::LimitExceeded => "LIMIT_EXCEEDED",
            FormulaCompletionError::StaleContext => "STALE_CONTEXT",
            FormulaCompletionError::UnknownCandidate => "UNKNOWN_CANDIDATE",
        },
        value.to_string(),
    )
}
fn request<T: DeserializeOwned>(source: &str) -> Result<T, ApiErrorV3> {
    if source.len() > MAX_REQUEST_BYTES {
        return Err(error(
            "INPUT_TOO_LARGE",
            "Completion request exceeds 1 MiB.",
        ));
    }
    serde_json::from_str(source).map_err(|_| {
        error(
            "INVALID_JSON",
            "Invalid closed completion request or revision encoding.",
        )
    })
}
fn data(value: impl Serialize) -> Result<Value, ApiErrorV3> {
    let value = serde_json::to_value(value)
        .map_err(|_| error("INTERNAL", "Completion serialization failed."))?;
    let encoded = serde_json::to_vec(&value)
        .map_err(|_| error("INTERNAL", "Completion serialization failed."))?;
    if encoded.len() > MAX_DATA_BYTES {
        return Err(error("OUTPUT_TOO_LARGE", "Completion data exceeds 1 MiB."));
    }
    Ok(value)
}
fn execute(source: &str, apply: bool) -> Result<Value, ApiErrorV3> {
    if apply {
        data(
            request::<FormulaCompletionApplyRequest>(source)?
                .execute()
                .map_err(completion_error)?,
        )
    } else {
        data(
            request::<FormulaCompletionRequest>(source)?
                .execute()
                .map_err(completion_error)?,
        )
    }
}
fn response(result: Result<Value, ApiErrorV3>) -> JsValue {
    let envelope = match result {
        Ok(value) => ApiEnvelopeV3::success(value, Vec::new()),
        Err(value) => ApiEnvelopeV3::failure(value, Vec::new()),
    };
    // Keep null slots identical to the JSONL/C/Python contract, rather than
    // silently turning absent editing metadata into JS undefined.
    envelope
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or_else(|cause| {
            JsValue::from_str(&format!("Completion serialization failed: {cause}"))
        })
}

#[wasm_bindgen]
pub fn formula_completion_capabilities_v3() -> JsValue {
    response(Ok(json!({
        "schemaVersion": FORMULA_COMPLETION_VERSION,
        "inputFormats": ["latex"], "offsetEncoding": "utf-8-bytes",
        "revisionEncoding": "canonical-decimal-u64-string",
        "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES, "maxNodes": FORMULA_COMPLETION_MAX_NODES,
        "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH, "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
        "maxRequestBytes": MAX_REQUEST_BYTES, "maxResultDataBytes": MAX_DATA_BYTES
    })))
}
/// Synchronous model-free query. JSON preserves all revision bits; prefer a worker.
#[wasm_bindgen]
pub fn complete_formula_v3(request_json: &str) -> JsValue {
    response(execute(request_json, false))
}
/// Returns data only; the host owns explicit acceptance, document writes and undo.
#[wasm_bindgen]
pub fn apply_formula_completion_v3(request_json: &str) -> JsValue {
    response(execute(request_json, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wasm_completion_preserves_revision_and_refuses_stale_or_malformed_requests() {
        let mut params = json!({"source":"中\\fr","cursor":6,"inputFormat":"latex","revision":"18446744073709551615"});
        let reply = execute(&params.to_string(), false).unwrap();
        assert_eq!(reply["snapshot"]["revision"], "18446744073709551615");
        params["snapshot"] = reply["snapshot"].clone();
        params["candidateId"] = json!("builtin:frac");
        let applied = execute(&params.to_string(), true).unwrap();
        assert_eq!(applied["source"], "中\\frac{}{}");
        assert_eq!(applied["cursor"], 9);
        params["revision"] = json!("0");
        assert_eq!(
            execute(&params.to_string(), true).unwrap_err().code,
            "STALE_CONTEXT"
        );
        params["revision"] = json!(0);
        assert_eq!(
            execute(&params.to_string(), true).unwrap_err().code,
            "INVALID_JSON"
        );
        assert_eq!(
            execute(&" ".repeat(MAX_REQUEST_BYTES + 1), false)
                .unwrap_err()
                .code,
            "INPUT_TOO_LARGE"
        );
        assert_eq!(
            execute(
                r#"{"source":"\ud800","cursor":0,"revision":"0","inputFormat":"latex"}"#,
                false
            )
            .unwrap_err()
            .code,
            "INVALID_JSON"
        );
    }
}
