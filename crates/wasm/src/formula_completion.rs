//! Shared full/conversion-only exports for the Core editing protocol.
use latexsnipper_api_types::{ApiEnvelopeV3, ApiErrorV3};
use latexsnipper_conversion::formula_completion::{
    fragments::{
        FormulaFragmentCompletionApplyRequest, FormulaFragmentCompletionRequest,
        FORMULA_COMPLETION_MAX_FRAGMENTS, FORMULA_COMPLETION_MAX_FRAGMENT_BYTES,
        FRAGMENT_FORMULA_COMPLETION_VERSION,
    },
    scoped::{
        ScopedFormulaCompletionApplyRequest, ScopedFormulaCompletionError,
        ScopedFormulaCompletionRequest, FORMULA_COMPLETION_ARGUMENT_KINDS,
        FORMULA_COMPLETION_MAX_DEFINITIONS, FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS,
        SCOPED_FORMULA_COMPLETION_VERSION,
    },
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

pub(super) fn scoped_completion_error(value: ScopedFormulaCompletionError) -> ApiErrorV3 {
    error(
        match value {
            ScopedFormulaCompletionError::InvalidCatalog => "INVALID_CATALOG",
            ScopedFormulaCompletionError::InvalidCursor => "INVALID_CURSOR",
            ScopedFormulaCompletionError::LimitExceeded => "LIMIT_EXCEEDED",
            ScopedFormulaCompletionError::StaleContext => "STALE_CONTEXT",
            ScopedFormulaCompletionError::UnknownCandidate => "UNKNOWN_CANDIDATE",
        },
        value.to_string(),
    )
}

fn execute_scoped(source: &str, apply: bool) -> Result<Value, ApiErrorV3> {
    if apply {
        data(
            request::<ScopedFormulaCompletionApplyRequest>(source)?
                .execute()
                .map_err(scoped_completion_error)?,
        )
    } else {
        data(
            request::<ScopedFormulaCompletionRequest>(source)?
                .execute()
                .map_err(scoped_completion_error)?,
        )
    }
}
fn execute_fragments(source: &str, apply: bool) -> Result<Value, ApiErrorV3> {
    if apply {
        data(
            request::<FormulaFragmentCompletionApplyRequest>(source)?
                .execute()
                .map_err(scoped_completion_error)?,
        )
    } else {
        data(
            request::<FormulaFragmentCompletionRequest>(source)?
                .execute()
                .map_err(scoped_completion_error)?,
        )
    }
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

/// Scoped v2 is additive; the v1 capability shape remains unchanged.
#[wasm_bindgen]
pub fn formula_scoped_completion_capabilities_v3() -> JsValue {
    response(Ok(json!({
        "schemaVersion": SCOPED_FORMULA_COMPLETION_VERSION,
        "inputFormats": ["latex"], "offsetEncoding": "utf-8-bytes",
        "revisionEncoding": "canonical-decimal-u64-string",
        "scopeIdentityEncoding": "opaque-lowercase-sha256",
        "definitionRevisionEncoding": "canonical-decimal-u64-string",
        "generationEncoding": "canonical-decimal-u64-string",
        "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES, "maxNodes": FORMULA_COMPLETION_MAX_NODES,
        "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH, "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
        "maxRequestBytes": MAX_REQUEST_BYTES, "maxResultDataBytes": MAX_DATA_BYTES,
        "maxDefinitions": FORMULA_COMPLETION_MAX_DEFINITIONS,
        "maxDefinitionArguments": FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS,
        "argumentKinds": FORMULA_COMPLETION_ARGUMENT_KINDS,
        "maxLeadingOptionalArguments": 1,
        "macroExpansion": false, "persistence": false
    })))
}
/// No registry, macro expansion or IO. Pass only host-authorized definitions.
#[wasm_bindgen]
pub fn complete_formula_scoped_v3(request_json: &str) -> JsValue {
    response(execute_scoped(request_json, false))
}
/// Recheck CURRENT scope, definition content and editor state before acceptance.
#[wasm_bindgen]
pub fn apply_formula_completion_scoped_v3(request_json: &str) -> JsValue {
    response(execute_scoped(request_json, true))
}

/// Additive profile 3: explicit host-authorized fragments, never implicit history.
#[wasm_bindgen]
pub fn formula_fragment_completion_capabilities_v3() -> JsValue {
    response(Ok(json!({
        "schemaVersion": FRAGMENT_FORMULA_COMPLETION_VERSION,
        "inputFormats": ["latex"], "offsetEncoding": "utf-8-bytes",
        "revisionEncoding": "canonical-decimal-u64-string",
        "scopeIdentityEncoding": "opaque-lowercase-sha256",
        "definitionRevisionEncoding": "canonical-decimal-u64-string",
        "fragmentRevisionEncoding": "canonical-decimal-u64-string",
        "generationEncoding": "canonical-decimal-u64-string",
        "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES, "maxNodes": FORMULA_COMPLETION_MAX_NODES,
        "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH, "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
        "maxRequestBytes": MAX_REQUEST_BYTES, "maxResultDataBytes": MAX_DATA_BYTES,
        "maxDefinitions": FORMULA_COMPLETION_MAX_DEFINITIONS,
        "maxDefinitionArguments": FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS,
        "argumentKinds": FORMULA_COMPLETION_ARGUMENT_KINDS, "maxLeadingOptionalArguments": 1,
        "maxFragments": FORMULA_COMPLETION_MAX_FRAGMENTS,
        "maxFragmentBytes": FORMULA_COMPLETION_MAX_FRAGMENT_BYTES,
        "retrievalSource": "explicit-host-catalog", "sensitiveDocumentRequiresHostAllow": true,
        "macroExpansion": false, "persistence": false, "learning": false
    })))
}
/// Policy flags bind snapshots; trusted hosts must authorize retrieval first.
#[wasm_bindgen]
pub fn complete_formula_fragments_v3(request_json: &str) -> JsValue {
    response(execute_fragments(request_json, false))
}
/// No downgrade or retry: recheck the current catalog and policy before editing.
#[wasm_bindgen]
pub fn apply_formula_completion_fragments_v3(request_json: &str) -> JsValue {
    response(execute_fragments(request_json, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_wasm_requests_bind_catalog_scope_and_precise_versions() {
        let mut params = json!({"source":"中\\cust","cursor":8,"inputFormat":"latex","revision":"18446744073709551615",
            "catalog":{"scope":{"userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"generation":"18446744073709551615"},
                "definitions":[{"id":"pair","revision":"18446744073709551615","userSha256":"a".repeat(64),"projectSha256":null,"command":"custompair","arguments":["math","text"]}]}});
        let reply = execute_scoped(&params.to_string(), false).unwrap();
        assert_eq!(
            reply["candidates"][0]["definition"]["revision"],
            "18446744073709551615"
        );
        params["snapshot"] = reply["snapshot"].clone();
        params["candidateId"] = json!("definition:pair");
        assert_eq!(
            execute_scoped(&params.to_string(), true).unwrap()["source"],
            "中\\custompair{}{}"
        );
        params["catalog"]["scope"]["generation"] = json!("0");
        assert_eq!(
            execute_scoped(&params.to_string(), true).unwrap_err().code,
            "STALE_CONTEXT"
        );
        params["catalog"]["scope"]["generation"] = json!(0);
        assert_eq!(
            execute_scoped(&params.to_string(), true).unwrap_err().code,
            "INVALID_JSON"
        );
        assert_eq!(
            execute_scoped(&" ".repeat(MAX_REQUEST_BYTES + 1), false)
                .unwrap_err()
                .code,
            "INPUT_TOO_LARGE"
        );
    }
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
